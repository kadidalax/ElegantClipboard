use super::file_clipboard::{self, FileCaptureData};
use super::source_app::{self, SourceAppInfo};
use super::{canonical_url_text, compute_semantic_hash, is_url, semantic_hash_from_text};
use crate::database::{
    ClipboardRepository, ContentType, Database, NewClipboardItem, SettingsRepository,
};
use blake3::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

const DEFAULT_MAX_CONTENT_SIZE: usize = 1_048_576;
const DEFAULT_MAX_IMAGE_SIZE: usize = 50 * 1024 * 1024;
const MAX_PREVIEW_LENGTH: usize = 200;
const DEFAULT_MAX_HISTORY_COUNT: i64 = 0;
const DEFAULT_AUTO_CLEANUP_DAYS: i64 = 30;

pub(crate) fn parse_cf_html_source_url(html: &str) -> Option<String> {
    html.lines()
        .take_while(|line| !line.trim_start().starts_with('<'))
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if !key.trim().eq_ignore_ascii_case("SourceURL") {
                return None;
            }
            let value = value.trim();
            let scheme = value.get(..value.len().min(8))?.to_ascii_lowercase();
            (scheme.starts_with("http://") || scheme.starts_with("https://"))
                .then(|| value.to_string())
        })
}

fn source_file_name_from_title(title: &str, allow_code: bool) -> Option<String> {
    let title = title.trim();
    let title_prefix = title.rsplit_once(" - ").map(|(prefix, _)| prefix.trim());

    title_prefix
        .into_iter()
        .chain([title])
        .find_map(|candidate| {
            let candidate = candidate
                .rsplit_once(" [")
                .filter(|(_, directory)| directory.ends_with(']'))
                .map_or(candidate, |(file_name, _)| file_name.trim());
            if candidate
                .get(..candidate.len().min(8))
                .is_some_and(|prefix| {
                    prefix.eq_ignore_ascii_case("http://")
                        || prefix.eq_ignore_ascii_case("https://")
                })
            {
                return None;
            }
            let name = candidate.rsplit(['\\', '/']).next()?.trim();
            let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
            let document = matches!(
                ext.as_str(),
                "pdf"
                    | "doc"
                    | "docx"
                    | "xls"
                    | "xlsx"
                    | "ppt"
                    | "pptx"
                    | "txt"
                    | "md"
                    | "rtf"
                    | "csv"
                    | "odt"
                    | "ods"
                    | "odp"
            );
            let code = matches!(
                ext.as_str(),
                "rs" | "js"
                    | "jsx"
                    | "ts"
                    | "tsx"
                    | "py"
                    | "go"
                    | "java"
                    | "c"
                    | "h"
                    | "cpp"
                    | "hpp"
                    | "cs"
                    | "html"
                    | "css"
                    | "json"
                    | "yaml"
                    | "yml"
                    | "toml"
                    | "xml"
                    | "sql"
                    | "sh"
            );
            (document || allow_code && code).then(|| name.to_string())
        })
}

fn attach_source_file_name(
    item: &mut NewClipboardItem,
    source_title: Option<&str>,
    source_app_name: Option<&str>,
) {
    if matches!(
        item.content_type,
        ContentType::Text | ContentType::Html | ContentType::Rtf
    ) {
        let app = source_app_name.unwrap_or("").to_ascii_lowercase();
        let editor = [
            "code",
            "studio",
            "editor",
            "idea",
            "intellij",
            "pycharm",
            "webstorm",
            "sublime",
            "notepad",
            "vim",
            "rustrover",
        ]
        .iter()
        .any(|name| app.contains(name));
        item.source_file_name =
            source_title.and_then(|title| source_file_name_from_title(title, editor));
    }
}

/// 通配符匹配（支持 * 和 ?，不区分大小写，O(n) 空间）
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let tlen = text.len();

    let mut prev = vec![false; tlen + 1];
    let mut curr = vec![false; tlen + 1];
    prev[0] = true;

    for &pc in &pattern {
        curr.fill(false);
        if pc == '*' {
            curr[0] = prev[0];
            for j in 0..tlen {
                curr[j + 1] = prev[j + 1] || curr[j];
            }
        } else {
            for (j, &tc) in text.iter().enumerate() {
                if pc == '?' || pc == tc {
                    curr[j + 1] = prev[j];
                }
            }
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[tlen]
}

/// 检查来源应用是否匹配过滤规则
/// 支持通配符模式和普通子串匹配，匹配目标：应用名、进程名、进程路径
fn matches_app_filter(filter: &str, app_name: &str, exe_name: &str, exe_path: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return false;
    }

    if filter.contains('*') || filter.contains('?') {
        wildcard_match(filter, app_name)
            || wildcard_match(filter, exe_name)
            || wildcard_match(filter, exe_path)
    } else {
        let f = filter.to_lowercase();
        app_name.to_lowercase().contains(&f)
            || exe_name.to_lowercase().contains(&f)
            || exe_path.to_lowercase().contains(&f)
    }
}

/// 按字符边界截断超长内容
fn truncate_content(content: String, max_size: usize, content_type: &str) -> String {
    if max_size > 0 && content.len() > max_size {
        warn!(
            "{} content truncated from {} to {} bytes",
            content_type,
            content.len(),
            max_size
        );
        content
            .char_indices()
            .take_while(|(i, _)| *i < max_size)
            .map(|(_, c)| c)
            .collect()
    } else {
        content
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImageCapture {
    pub temp_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub byte_size: usize,
    /// 伴侣文件：原始 CF_DIB 数据（用于 Photoshop 等专业软件粘贴兼容）
    pub dib_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum ClipboardContent {
    Text(String),
    Html {
        html: String,
        text: Option<String>,
        rtf: Option<String>,
        source_url: Option<String>,
    },
    Rtf {
        rtf: String,
        text: Option<String>,
    },
    ImageFile(ImageCapture),
    Files(FileCaptureData),
}
pub(crate) fn cleanup_capture_content(content: &ClipboardContent) {
    if let ClipboardContent::ImageFile(capture) = content {
        if let Err(e) = std::fs::remove_file(&capture.temp_path) {
            debug!(
                "Failed to remove capture temp file {:?}: {}",
                capture.temp_path, e
            );
        }
        if let Some(ref dib_path) = capture.dib_path {
            let _ = std::fs::remove_file(dib_path);
        }
    }
}

impl Drop for ClipboardContent {
    fn drop(&mut self) {
        cleanup_capture_content(self);
    }
}

#[derive(Debug, Clone)]
struct ContentHashes {
    content_hash: String,
    semantic_hash: String,
}

/// 处理剪贴板内容所需的设置，批量读取避免多次数据库查询
struct ProcessSettings {
    max_content_size: usize,
    dedup_strategy: String,
    text_dedup_mode: String,
    max_history_count: i64,
    auto_cleanup_days: i64,
}

/// 剪贴板变更热路径所需的设置，单次批量查询
#[derive(Debug, Clone)]
pub struct ClipChangeSettings {
    app_filter_enabled: bool,
    app_filter_list: Option<String>,
    app_filter_mode: String,
    pub max_image_bytes: usize,
}

impl Default for ClipChangeSettings {
    fn default() -> Self {
        Self {
            app_filter_enabled: false,
            app_filter_list: None,
            app_filter_mode: "blacklist".to_string(),
            max_image_bytes: DEFAULT_MAX_IMAGE_SIZE,
        }
    }
}

impl ClipChangeSettings {
    /// 检查来源应用是否应被过滤（无 DB 查询）
    pub fn is_source_app_excluded(&self, source: &Option<SourceAppInfo>) -> bool {
        let Some(source) = source else {
            return false;
        };

        if !self.app_filter_enabled {
            return false;
        }

        let filter_list = match self.app_filter_list.as_deref() {
            Some(s) => s,
            None => return false,
        };

        let exe_name = std::path::Path::new(&source.exe_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let matches = filter_list.split(',').any(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return false;
            }
            matches_app_filter(entry, &source.app_name, exe_name, &source.exe_path)
        });

        match self.app_filter_mode.as_str() {
            "whitelist" => !matches,
            _ => matches,
        }
    }
}

pub struct ClipboardHandler {
    db: Database,
    repository: ClipboardRepository,
    settings_repo: SettingsRepository,
    /// 内存级去重：最近一次成功处理的内容哈希，防止快速连续事件绕过 DB dedup
    last_content_hash: parking_lot::Mutex<Option<(PathBuf, String)>>,
}

impl ClipboardHandler {
    pub fn new(db: &Database) -> Self {
        let paths = db.active_snapshot();
        std::fs::create_dir_all(&paths.images_dir).ok();
        std::fs::create_dir_all(&paths.icons_dir).ok();

        Self {
            db: db.clone(),
            repository: ClipboardRepository::new(db),
            settings_repo: SettingsRepository::new(db),
            last_content_hash: parking_lot::Mutex::new(None),
        }
    }

    pub(crate) fn active_paths(&self) -> crate::database::ActiveDatabase {
        self.db.active_snapshot()
    }

    /// 批量读取处理所需的全部设置，单次数据库查询
    fn get_process_settings(&self) -> ProcessSettings {
        let keys = [
            "max_content_size_kb",
            "dedup_strategy",
            "text_dedup_mode",
            "max_history_count",
            "auto_cleanup_days",
        ];
        let batch = self.settings_repo.get_batch(&keys);

        let max_content_size = batch
            .get("max_content_size_kb")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse::<usize>().ok())
            .map_or(DEFAULT_MAX_CONTENT_SIZE, |kb| kb * 1024);

        let dedup_strategy = batch
            .get("dedup_strategy")
            .and_then(|v| v.as_deref())
            .unwrap_or("move_to_top")
            .to_string();

        let text_dedup_mode = batch
            .get("text_dedup_mode")
            .and_then(|v| v.as_deref())
            .unwrap_or("semantic")
            .to_string();

        let max_history_count = batch
            .get("max_history_count")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(DEFAULT_MAX_HISTORY_COUNT);

        let auto_cleanup_days = batch
            .get("auto_cleanup_days")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(DEFAULT_AUTO_CLEANUP_DAYS);

        ProcessSettings {
            max_content_size,
            dedup_strategy,
            text_dedup_mode,
            max_history_count,
            auto_cleanup_days,
        }
    }

    /// 批量读取剪贴板变更热路径所需的全部设置，单次数据库查询
    pub(crate) fn get_clip_change_settings(&self) -> ClipChangeSettings {
        let keys = [
            "app_filter_enabled",
            "app_filter_list",
            "app_filter_mode",
            "max_image_size_kb",
        ];
        let batch = self.settings_repo.get_batch(&keys);

        let app_filter_enabled = batch
            .get("app_filter_enabled")
            .and_then(|v| v.as_deref())
            .is_some_and(|v| v == "true");

        let app_filter_list = batch
            .get("app_filter_list")
            .and_then(|v| v.as_deref())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let app_filter_mode = batch
            .get("app_filter_mode")
            .and_then(|v| v.as_deref())
            .unwrap_or("blacklist")
            .to_string();

        let max_image_bytes = batch
            .get("max_image_size_kb")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse::<usize>().ok())
            .map_or(DEFAULT_MAX_IMAGE_SIZE, |kb| kb.saturating_mul(1024));

        ClipChangeSettings {
            app_filter_enabled,
            app_filter_list,
            app_filter_mode,
            max_image_bytes,
        }
    }

    /// 检查内容类型是否被允许监听
    /// 读取 `monitor_types` 设置（逗号分隔，如 "text,html,rtf,image,files,url"）
    /// 默认全部允许
    pub fn is_content_type_allowed(&self, content: &ClipboardContent) -> bool {
        let allowed = self.settings_repo.get("monitor_types").ok().flatten();

        // 无设置或空字符串 → 全部允许
        let allowed = match allowed {
            Some(ref s) if !s.is_empty() => s,
            _ => return true,
        };

        let content_type = match content {
            ClipboardContent::Text(text) if is_url(text) => "url",
            ClipboardContent::Text(_) => "text",
            ClipboardContent::Html { .. } => "html",
            ClipboardContent::Rtf { .. } => "rtf",
            ClipboardContent::ImageFile(_) => "image",
            ClipboardContent::Files(_) => "files",
        };

        if content_type == "url" && !allowed.split(',').any(|t| t.trim() == "url") {
            return false;
        }

        allowed.split(',').any(|t| t.trim() == content_type)
    }

    /// 处理剪贴板内容，去重后存入数据库
    #[cfg(test)]
    pub fn process(
        &self,
        content: ClipboardContent,
        source: Option<SourceAppInfo>,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, String> {
        let operation = self.db.operation_lock();
        let _operation = operation.read();
        self.process_locked(content, source, group_id)
    }

    pub(crate) fn process_for_database(
        &self,
        expected_db_path: &Path,
        content: ClipboardContent,
        source: Option<SourceAppInfo>,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, String> {
        let operation = self.db.operation_lock();
        let _operation = operation.read();
        if self.active_paths().db_path != expected_db_path {
            cleanup_capture_content(&content);
            return Ok(None);
        }
        self.process_locked(content, source, group_id)
    }

    fn process_locked(
        &self,
        content: ClipboardContent,
        source: Option<SourceAppInfo>,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, String> {
        // 批量读取所有设置，单次数据库查询替代 5-6 次独立查询
        let settings = self.get_process_settings();
        let max_content_size = settings.max_content_size;

        // max_content_size 仅限制文本类内容
        if max_content_size > 0 {
            let is_text_content = Self::is_text_like_content(&content);
            if is_text_content {
                let content_size = self.get_content_size(&content);
                if content_size > max_content_size {
                    warn!(
                        "Content size {} bytes exceeds max {} bytes, skipping",
                        content_size, max_content_size
                    );
                    return Ok(None);
                }
            }
        }

        let (source_app_name, source_app_icon, source_title) = match source {
            Some(ref info) => {
                let paths = self.active_paths();
                std::fs::create_dir_all(&paths.icons_dir).ok();
                let icon_path = source_app::extract_and_cache_icon(
                    &info.exe_path,
                    &paths.icons_dir,
                    &info.icon_cache_key,
                );
                (
                    Some(info.app_name.clone()),
                    icon_path,
                    info.source_title.clone(),
                )
            }
            None => (None, None, None),
        };

        let hashes = self.calculate_hashes(&content)?;
        let dedup = &settings.dedup_strategy;
        let text_like = Self::is_text_like_content(&content);
        let text_dedup_mode = &settings.text_dedup_mode;
        let text_use_strict = text_like && text_dedup_mode == "strict";

        // 内存级去重：防止快速连续事件（如 Zen 浏览器 1ms 内两次事件）绕过 DB dedup
        // 仅在 dedup 策略不是 always_new 时生效
        if dedup != "always_new" {
            let db_path = self.active_paths().db_path;
            let mut last = self.last_content_hash.lock();
            if last.as_ref().is_some_and(|(last_db_path, last_hash)| {
                last_db_path == &db_path && last_hash == &hashes.content_hash
            }) {
                debug!("Content hash matches last processed, skipping (memory dedup)");
                return Ok(None);
            }
            *last = Some((db_path, hashes.content_hash.clone()));
        }

        if dedup != "always_new"
            && if text_like {
                if text_use_strict {
                    self.repository
                        .exists_by_hash(&hashes.content_hash, group_id)
                } else {
                    self.repository
                        .exists_by_semantic_hash(&hashes.semantic_hash, group_id)
                }
            } else {
                self.repository
                    .exists_by_hash(&hashes.content_hash, group_id)
            }
            .map_err(|e| e.to_string())?
        {
            if dedup.as_str() == "ignore" {
                debug!("Content already exists, ignoring (dedup=ignore)");
                return Ok(None);
            } else {
                // move_to_top: 更新访问时间并置顶；HTML/RTF 同时刷新富文本字段
                debug!("Content already exists, updating access time (dedup=move_to_top)");
                let id = if text_like {
                    if text_use_strict {
                        self.repository
                            .touch_by_hash(&hashes.content_hash, group_id)
                    } else {
                        self.repository
                            .touch_by_semantic_hash(&hashes.semantic_hash, group_id)
                    }
                } else {
                    self.repository
                        .touch_by_hash(&hashes.content_hash, group_id)
                }
                .map_err(|e| e.to_string())?;

                if let (Some(id), true) = (
                    id,
                    matches!(
                        content,
                        ClipboardContent::Html { .. } | ClipboardContent::Rtf { .. }
                    ),
                ) {
                    let mut refreshed = match &content {
                        ClipboardContent::Html {
                            html,
                            text,
                            rtf,
                            source_url,
                        } => self.process_html(
                            html.clone(),
                            text.clone(),
                            rtf.clone(),
                            source_url.clone(),
                            &hashes,
                            max_content_size,
                        )?,
                        ClipboardContent::Rtf { rtf, text } => {
                            self.process_rtf(rtf.clone(), text.clone(), &hashes, max_content_size)?
                        }
                        _ => unreachable!(),
                    };
                    refreshed.source_app_name = source_app_name.clone();
                    refreshed.source_app_icon = source_app_icon.clone();
                    refreshed.source_title = source_title.clone();
                    attach_source_file_name(
                        &mut refreshed,
                        source_title.as_deref(),
                        source_app_name.as_deref(),
                    );
                    self.repository
                        .refresh_rich_fields(id, &refreshed)
                        .map_err(|e| e.to_string())?;
                }

                if let Some(id) = id
                    && let ClipboardContent::Text(text) = &content
                {
                    let mut refreshed = NewClipboardItem {
                        content_type: if is_url(text) {
                            ContentType::Url
                        } else {
                            ContentType::Text
                        },
                        source_app_name: source_app_name.clone(),
                        source_app_icon: source_app_icon.clone(),
                        source_title: source_title.clone(),
                        ..Default::default()
                    };
                    attach_source_file_name(
                        &mut refreshed,
                        source_title.as_deref(),
                        source_app_name.as_deref(),
                    );
                    self.repository
                        .refresh_source_metadata(id, &refreshed)
                        .map_err(|e| e.to_string())?;
                }

                return Ok(id);
            }
        }

        // 安全地从 ClipboardContent（impl Drop）中取出内部值
        // 通过 ManuallyDrop 阻止 Drop 运行，process_image_file 已自行处理临时文件
        let mut item = {
            let mut content = std::mem::ManuallyDrop::new(content);
            // SAFETY: 逐字段 take 所有权，ManuallyDrop 阻止外层 Drop
            match &mut *content {
                ClipboardContent::Text(text) => {
                    let text = std::mem::take(text);
                    self.process_text(text, &hashes, max_content_size)?
                }
                ClipboardContent::Html {
                    html,
                    text,
                    rtf,
                    source_url,
                } => {
                    let html = std::mem::take(html);
                    let text = std::mem::take(text);
                    let rtf = std::mem::take(rtf);
                    let source_url = std::mem::take(source_url);
                    self.process_html(html, text, rtf, source_url, &hashes, max_content_size)?
                }
                ClipboardContent::Rtf { rtf, text } => {
                    let rtf = std::mem::take(rtf);
                    let text = std::mem::take(text);
                    self.process_rtf(rtf, text, &hashes, max_content_size)?
                }
                ClipboardContent::ImageFile(capture) => {
                    let capture = std::mem::take(capture);
                    self.process_image_file(capture, &hashes)?
                }
                ClipboardContent::Files(files) => {
                    let files = std::mem::take(files);
                    // staging 使用 file_clipboard 内置 50MB 上限，与文本长度限制无关
                    self.process_files(files, &hashes, 0)?
                }
            }
        };

        attach_source_file_name(
            &mut item,
            source_title.as_deref(),
            source_app_name.as_deref(),
        );
        item.source_app_name = source_app_name;
        item.source_app_icon = source_app_icon;
        item.source_title = source_title;
        item.group_id = group_id;

        let log_type = format!("{:?}", item.content_type);
        let log_size = item.byte_size;
        let log_source = item
            .source_app_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        let id = self.repository.insert(item).map_err(|e| e.to_string())?;
        info!(
            "Stored clipboard item: id={}, type={}, size={} bytes, source={}",
            id, log_type, log_size, log_source
        );

        // 执行最大历史数限制，清理旧图片
        if settings.max_history_count > 0 {
            match self
                .repository
                .enforce_max_count(settings.max_history_count, group_id)
            {
                Ok((deleted, image_paths, file_payloads)) => {
                    super::cleanup_deleted_assets(&image_paths, &file_payloads);
                    if deleted > 0 {
                        debug!("Enforced max count: removed {} old items", deleted);
                    }
                }
                Err(e) => warn!("Failed to enforce max history count: {}", e),
            }
        }

        // 自动清理超过指定天数的旧记录
        if settings.auto_cleanup_days > 0 {
            match self
                .repository
                .delete_older_than(settings.auto_cleanup_days, group_id)
            {
                Ok((deleted, image_paths, file_payloads)) => {
                    super::cleanup_deleted_assets(&image_paths, &file_payloads);
                    if deleted > 0 {
                        info!(
                            "Auto-cleanup: removed {} items older than {} days",
                            deleted, settings.auto_cleanup_days
                        );
                    }
                }
                Err(e) => warn!("Failed to auto-cleanup old items: {}", e),
            }
        }

        Ok(Some(id))
    }

    fn get_content_size(&self, content: &ClipboardContent) -> usize {
        match content {
            ClipboardContent::Text(text) => text.len(),
            ClipboardContent::Html { html, .. } => html.len(),
            ClipboardContent::Rtf { rtf, .. } => rtf.len(),
            ClipboardContent::ImageFile(capture) => capture.byte_size,
            ClipboardContent::Files(files) => {
                files.paths.iter().map(std::string::String::len).sum()
            }
        }
    }

    fn is_text_like_content(content: &ClipboardContent) -> bool {
        matches!(
            content,
            ClipboardContent::Text(_)
                | ClipboardContent::Html { .. }
                | ClipboardContent::Rtf { .. }
        )
    }

    fn calculate_hashes(&self, content: &ClipboardContent) -> Result<ContentHashes, String> {
        let content_hash = self.calculate_hash(content)?;
        let semantic_hash = match content {
            ClipboardContent::Text(text) => {
                if is_url(text) {
                    content_hash.clone()
                } else {
                    semantic_hash_from_text(text).unwrap_or_else(|| content_hash.clone())
                }
            }
            ClipboardContent::Html { text, .. } => {
                compute_semantic_hash("html", text.as_deref(), &content_hash)
            }
            ClipboardContent::Rtf { text, .. } => {
                compute_semantic_hash("rtf", text.as_deref(), &content_hash)
            }
            ClipboardContent::ImageFile(_) => content_hash.clone(),
            ClipboardContent::Files(_) => content_hash.clone(),
        };

        Ok(ContentHashes {
            content_hash,
            semantic_hash,
        })
    }

    fn calculate_hash(&self, content: &ClipboardContent) -> Result<String, String> {
        let mut hasher = Hasher::new();

        match content {
            ClipboardContent::Text(text) => {
                if let Some(canonical) = canonical_url_text(text) {
                    hasher.update(b"url:");
                    hasher.update(canonical.as_bytes());
                } else {
                    hasher.update(b"text:");
                    hasher.update(text.as_bytes());
                }
            }
            ClipboardContent::Html { html, .. } => {
                hasher.update(b"html:");
                hasher.update(html.as_bytes());
            }
            ClipboardContent::Rtf { rtf, .. } => {
                hasher.update(b"rtf:");
                hasher.update(rtf.as_bytes());
            }
            ClipboardContent::ImageFile(capture) => {
                hasher.update(b"image:");
                hash_file_into_hasher(&mut hasher, &capture.temp_path)?;
            }
            ClipboardContent::Files(files) => {
                hasher.update(b"files:");
                for file in &files.paths {
                    hasher.update(file.as_bytes());
                    hasher.update(b"|");
                }
                if let Some(ref raw) = files.hdrop_raw {
                    hasher.update(b"hdrop:");
                    hasher.update(raw);
                }
            }
        }

        Ok(hasher.finalize().to_hex().to_string())
    }

    fn process_text(
        &self,
        text: String,
        hashes: &ContentHashes,
        max_size: usize,
    ) -> Result<NewClipboardItem, String> {
        let is_url = canonical_url_text(&text).is_some();
        let text = if is_url {
            text.trim().to_string()
        } else {
            text
        };
        let char_count = Some(text.chars().count() as i64);
        let preview = Self::create_preview(&text);
        let content_type = if is_url {
            ContentType::Url
        } else {
            ContentType::Text
        };
        let text_content = truncate_content(text, max_size, "Text");
        let byte_size = text_content.len() as i64;

        Ok(NewClipboardItem {
            content_type,
            text_content: Some(text_content),
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            char_count,
            ..Default::default()
        })
    }

    fn process_html(
        &self,
        html: String,
        text: Option<String>,
        rtf: Option<String>,
        source_url: Option<String>,
        hashes: &ContentHashes,
        max_size: usize,
    ) -> Result<NewClipboardItem, String> {
        let preview = text
            .as_ref()
            .map_or_else(|| Self::create_preview(&html), |t| Self::create_preview(t));
        let html_content = truncate_content(html, max_size, "HTML");
        let byte_size = html_content.len() as i64;
        let rtf_content = rtf.map(|r| super::rtf_storage::truncate_rtf_storage(r, max_size));

        let char_count = text.as_ref().map(|t| t.chars().count() as i64);

        Ok(NewClipboardItem {
            content_type: ContentType::Html,
            text_content: text,
            html_content: Some(html_content),
            rtf_content,
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            char_count,
            source_url,
            ..Default::default()
        })
    }

    fn process_rtf(
        &self,
        rtf: String,
        text: Option<String>,
        hashes: &ContentHashes,
        max_size: usize,
    ) -> Result<NewClipboardItem, String> {
        let preview = text
            .as_ref()
            .map_or_else(|| "[RTF Content]".to_string(), |t| Self::create_preview(t));
        let rtf_content = super::rtf_storage::truncate_rtf_storage(rtf, max_size);
        let byte_size = rtf_content.len() as i64;

        let char_count = text.as_ref().map(|t| t.chars().count() as i64);

        Ok(NewClipboardItem {
            content_type: ContentType::Rtf,
            text_content: text,
            rtf_content: Some(rtf_content),
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            char_count,
            ..Default::default()
        })
    }

    /// 处理图片：将 watcher 写入的临时 PNG rename 到 hash 命名路径，同时处理 DIB 伴侣文件
    fn process_image_file(
        &self,
        capture: ImageCapture,
        hashes: &ContentHashes,
    ) -> Result<NewClipboardItem, String> {
        let byte_size = capture.byte_size as i64;
        let image_width = i64::from(capture.width);
        let image_height = i64::from(capture.height);

        let paths = self.active_paths();
        std::fs::create_dir_all(&paths.images_dir).ok();
        let filename = format!("{}.png", &hashes.content_hash[..32]);
        let image_path = paths.images_dir.join(&filename);
        let image_path_str = image_path.to_string_lossy().to_string();

        debug!(
            "Processing image: {}x{}, {} bytes, hash={}",
            image_width,
            image_height,
            byte_size,
            &hashes.content_hash[..32]
        );

        if image_path.exists() {
            let _ = std::fs::remove_file(&capture.temp_path);
        } else if let Err(e) = std::fs::rename(&capture.temp_path, &image_path) {
            let _ = std::fs::remove_file(&capture.temp_path);
            return Err(format!("Failed to save image: {e}"));
        }
        debug!("Saved image to {:?}", image_path);

        // 处理 DIB 伴侣文件
        if let Some(ref dib_temp) = capture.dib_path {
            let dib_filename = format!("{}.dib", &hashes.content_hash[..32]);
            let dib_target = paths.images_dir.join(&dib_filename);
            if dib_target.exists() {
                let _ = std::fs::remove_file(dib_temp);
            } else if std::fs::rename(dib_temp, &dib_target).is_ok() {
                debug!("Saved DIB companion to {:?}", dib_target);
            }
        }

        Ok(NewClipboardItem {
            content_type: ContentType::Image,
            image_path: Some(image_path_str),
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some("[图片]".to_string()),
            byte_size,
            image_width: Some(image_width),
            image_height: Some(image_height),
            ..Default::default()
        })
    }

    fn process_files(
        &self,
        capture: FileCaptureData,
        hashes: &ContentHashes,
        max_stage_bytes: usize,
    ) -> Result<NewClipboardItem, String> {
        use std::path::Path;
        debug!("Processing {} file(s)", capture.paths.len());

        let byte_size: i64 = capture
            .paths
            .iter()
            .filter_map(|f| {
                let path = Path::new(f);
                if path.is_file() {
                    std::fs::metadata(path).ok().map(|m| m.len() as i64)
                } else {
                    None
                }
            })
            .sum();

        let preview = if capture.paths.len() == 1 {
            capture.paths[0].clone()
        } else if capture.paths.is_empty() {
            "[文件]".to_string()
        } else {
            format!("{} files", capture.paths.len())
        };

        let staged_dir = self
            .active_paths()
            .staged_dir
            .join(&hashes.content_hash[..8.min(hashes.content_hash.len())]);
        let payload = file_clipboard::build_payload(&capture, &staged_dir, max_stage_bytes as u64);
        let file_payload = Some(file_clipboard::encode_payload(&payload));

        Ok(NewClipboardItem {
            content_type: ContentType::Files,
            file_paths: Some(capture.paths),
            file_payload,
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            ..Default::default()
        })
    }

    fn create_preview(text: &str) -> String {
        let trimmed = text.trim();
        if let Some((idx, _)) = trimmed.char_indices().nth(MAX_PREVIEW_LENGTH) {
            format!("{}...", &trimmed[..idx])
        } else {
            trimmed.to_string()
        }
    }
}

fn hash_file_into_hasher(hasher: &mut Hasher, path: &Path) -> Result<(), String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open image file for hash: {e}"))?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Failed to read image file for hash: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(())
}

/// 清理 images/captures 目录中的残留临时文件
pub fn cleanup_stale_capture_files(capture_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(capture_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "tmp") {
            std::fs::remove_file(path).ok();
        }
    }
}

#[cfg(test)]
mod source_metadata_tests {
    use super::*;

    fn temp_handler(label: &str) -> (PathBuf, Database, ClipboardHandler) {
        let root = std::env::temp_dir().join(format!(
            "ec-handler-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(root.join("first/clipboard.db")).unwrap();
        let handler = ClipboardHandler::new(&db);
        (root, db, handler)
    }

    #[test]
    fn memory_dedup_is_scoped_to_active_database() {
        let (root, db, handler) = temp_handler("dedup-switch");
        assert!(
            handler
                .process(ClipboardContent::Text("same".into()), None, None)
                .unwrap()
                .is_some()
        );
        let target = db.open_active(root.join("second")).unwrap();
        db.swap_active(target);
        assert!(
            handler
                .process(ClipboardContent::Text("same".into()), None, None)
                .unwrap()
                .is_some()
        );
        drop(handler);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn memory_dedup_still_skips_consecutive_hash_in_same_database() {
        let (root, db, handler) = temp_handler("dedup-same");
        assert!(
            handler
                .process(ClipboardContent::Text("same".into()), None, None)
                .unwrap()
                .is_some()
        );
        assert!(
            handler
                .process(ClipboardContent::Text("same".into()), None, None)
                .unwrap()
                .is_none()
        );
        drop(handler);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn long_lived_handler_uses_active_database_resource_paths() {
        let root = std::env::temp_dir().join(format!(
            "ec-handler-switch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(root.join("first/clipboard.db")).unwrap();
        let handler = ClipboardHandler::new(&db);
        let target = db.open_active(root.join("second")).unwrap();
        db.swap_active(target);
        let paths = handler.active_paths();
        assert_eq!(
            paths.images_dir,
            std::fs::canonicalize(root.join("second"))
                .unwrap()
                .join("images")
        );
        drop(paths);
        drop(handler);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_http_source_url_from_cf_html_header() {
        assert_eq!(
            parse_cf_html_source_url(
                "Version:1.0\r\nSourceURL:https://example.com/page?q=1\r\n<html>"
            ),
            Some("https://example.com/page?q=1".to_string())
        );
        assert_eq!(
            parse_cf_html_source_url("Version:1.0\nSourceURL: http://example.com/a\n<html>"),
            Some("http://example.com/a".to_string())
        );
        assert_eq!(
            parse_cf_html_source_url("Version:1.0\nSourceURL:HTTPS://example.com/a\n<html>"),
            Some("HTTPS://example.com/a".to_string())
        );
        assert_eq!(
            parse_cf_html_source_url(
                "Version:1.0\nStartHTML:00000040\n<html>\nSourceURL:https://evil.example\n"
            ),
            None
        );
    }

    #[test]
    fn preserves_source_url_when_html_body_has_already_been_extracted() {
        let db_path = std::env::temp_dir().join(format!(
            "ec_source_html_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(db_path).unwrap();
        let handler = ClipboardHandler::new(&db);
        let hashes = ContentHashes {
            content_hash: "content".to_string(),
            semantic_hash: "semantic".to_string(),
        };
        let raw_cf_html = "Version:1.0\r\nStartHTML:00000100\r\nEndHTML:00000140\r\nSourceURL:https://example.com/source\r\n<html>body</html>";
        let item = handler
            .process_html(
                "<html><body>fragment only</body></html>".to_string(),
                None,
                None,
                parse_cf_html_source_url(raw_cf_html),
                &hashes,
                DEFAULT_MAX_CONTENT_SIZE,
            )
            .unwrap();
        assert_eq!(
            item.source_url.as_deref(),
            Some("https://example.com/source")
        );
    }

    #[test]
    fn rejects_missing_or_non_http_cf_html_source_url() {
        assert_eq!(parse_cf_html_source_url("Version:1.0\n<html>"), None);
        assert_eq!(
            parse_cf_html_source_url("SourceURL:file:///C:/secret.txt\n<html>"),
            None
        );
        assert_eq!(
            parse_cf_html_source_url("SourceURL:javascript:alert(1)\n<html>"),
            None
        );
    }

    #[test]
    fn conservatively_extracts_file_name_from_window_title() {
        assert_eq!(
            source_file_name_from_title(
                "C:\\Users\\Administrator\\Desktop\\report.docx - Microsoft Word",
                false,
            ),
            Some("report.docx".to_string()),
        );
        assert_eq!(
            source_file_name_from_title(
                "plan - final.txt [C:\\Users\\Administrator\\Desktop] - Notepad3",
                true,
            ),
            Some("plan - final.txt".to_string()),
        );
        assert_eq!(
            source_file_name_from_title(
                "新建文本文档.txt [C:\\Users\\Administrator\\Desktop] - Notepad3 · 管理员权限",
                true,
            ),
            Some("新建文本文档.txt".to_string()),
        );
        assert_eq!(
            source_file_name_from_title("report.final.pdf - Microsoft Edge", false),
            Some("report.final.pdf".to_string())
        );
        assert_eq!(source_file_name_from_title("Inbox - Mail", false), None);
        assert_eq!(
            source_file_name_from_title("example.com - Google Chrome", false),
            None
        );
        for name in [
            "notes.txt",
            "deck.pptx",
            "report.docx",
            "main.rs",
            "app.tsx",
        ] {
            assert_eq!(
                source_file_name_from_title(&format!("{name} - Editor"), true),
                Some(name.to_string())
            );
        }
        let mut browser_item = NewClipboardItem::default();
        attach_source_file_name(
            &mut browser_item,
            Some("docs.rs - Browser"),
            Some("Google Chrome"),
        );
        assert_eq!(browser_item.source_file_name, None);

        let mut editor_item = NewClipboardItem::default();
        attach_source_file_name(
            &mut editor_item,
            Some("main.rs - Visual Studio Code"),
            Some("Visual Studio Code"),
        );
        assert_eq!(editor_item.source_file_name.as_deref(), Some("main.rs"));
    }

    #[test]
    fn source_file_name_is_only_attached_to_text_like_items() {
        for content_type in [ContentType::Files, ContentType::Image] {
            let mut item = NewClipboardItem {
                content_type,
                ..Default::default()
            };
            attach_source_file_name(&mut item, Some("report.docx - App"), Some("App"));
            assert_eq!(item.source_file_name, None);
        }

        for content_type in [ContentType::Text, ContentType::Html, ContentType::Rtf] {
            let mut item = NewClipboardItem {
                content_type,
                ..Default::default()
            };
            attach_source_file_name(&mut item, Some("report.docx - App"), Some("App"));
            assert_eq!(item.source_file_name.as_deref(), Some("report.docx"));
        }
    }

    #[test]
    fn semantic_rich_text_refresh_updates_all_source_metadata() {
        let db_path = std::env::temp_dir().join(format!(
            "ec_source_refresh_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(db_path).unwrap();
        let handler = ClipboardHandler::new(&db);
        let source = |title: &str| SourceAppInfo {
            app_name: "Visual Studio Code".to_string(),
            exe_path: "missing.exe".to_string(),
            icon_cache_key: "missing".to_string(),
            source_title: Some(title.to_string()),
        };
        let content = |html: &str, url: &str| ClipboardContent::Html {
            html: html.to_string(),
            text: Some("same text".to_string()),
            rtf: None,
            source_url: Some(url.to_string()),
        };

        let first_id = handler
            .process(
                content("<b>first</b>", "https://first.example"),
                Some(source("first.docx - Visual Studio Code")),
                None,
            )
            .unwrap()
            .unwrap();
        let second_id = handler
            .process(
                content("<i>second</i>", "https://second.example"),
                Some(source("second.docx - Visual Studio Code")),
                None,
            )
            .unwrap()
            .unwrap();
        assert_eq!(first_id, second_id);

        let conn = db.read_connection();
        let conn = conn.lock();
        let values: (String, String, String, String) = conn
            .query_row(
                "SELECT html_content, source_url, source_title, source_file_name FROM clipboard_items WHERE id = ?1",
                [first_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            values,
            (
                "<i>second</i>".to_string(),
                "https://second.example".to_string(),
                "second.docx - Visual Studio Code".to_string(),
                "second.docx".to_string(),
            )
        );
    }

    #[test]
    fn repeated_plain_text_refreshes_source_metadata() {
        let db_path = std::env::temp_dir().join(format!(
            "ec_plain_source_refresh_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(db_path).unwrap();
        let source = |title: &str| SourceAppInfo {
            app_name: "Notepad3".to_string(),
            exe_path: "missing.exe".to_string(),
            icon_cache_key: "missing".to_string(),
            source_title: Some(title.to_string()),
        };

        let first_id = ClipboardHandler::new(&db)
            .process(
                ClipboardContent::Text("same text".to_string()),
                Some(source("first.txt [C:\\Old] - Notepad3")),
                None,
            )
            .unwrap()
            .unwrap();
        let second_id = ClipboardHandler::new(&db)
            .process(
                ClipboardContent::Text("same text".to_string()),
                Some(source("second.txt [C:\\New] - Notepad3")),
                None,
            )
            .unwrap()
            .unwrap();
        assert_eq!(first_id, second_id);

        let conn = db.read_connection();
        let conn = conn.lock();
        let values: (String, String) = conn
            .query_row(
                "SELECT source_title, source_file_name FROM clipboard_items WHERE id = ?1",
                [first_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            values,
            (
                "second.txt [C:\\New] - Notepad3".to_string(),
                "second.txt".to_string(),
            )
        );
    }

    #[test]
    fn repeated_url_does_not_infer_a_file_name_from_browser_title() {
        let db_path = std::env::temp_dir().join(format!(
            "ec_url_source_refresh_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(db_path).unwrap();
        let source = |title: &str| SourceAppInfo {
            app_name: "Google Chrome".to_string(),
            exe_path: "missing.exe".to_string(),
            icon_cache_key: "missing".to_string(),
            source_title: Some(title.to_string()),
        };

        for title in ["first page - Google Chrome", "report.pdf - Google Chrome"] {
            ClipboardHandler::new(&db)
                .process(
                    ClipboardContent::Text("https://example.com/report.pdf".to_string()),
                    Some(source(title)),
                    None,
                )
                .unwrap()
                .unwrap();
        }

        let conn = db.read_connection();
        let conn = conn.lock();
        let file_name: Option<String> = conn
            .query_row(
                "SELECT source_file_name FROM clipboard_items LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(file_name, None);
    }
}
