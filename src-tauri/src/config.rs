//! 应用配置管理
//!
//! 处理数据库初始化之前需要读取的配置（如数据库路径）。
//! 配置以 JSON 文件存储在应用安装目录下的 `config.json`。

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tracing::{debug, error, info};

/// 日志文件大小上限：10 MB
pub const DEFAULT_LOG_MAX_SIZE: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatabaseRegistration {
    pub id: String,
    pub name: String,
    pub path: String,
}

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default)]
    pub active_database_id: Option<String>,
    #[serde(default)]
    pub databases: Vec<DatabaseRegistration>,
    /// 自定义数据目录（包含数据库和图片），为 None 时使用默认路径
    #[serde(default)]
    pub data_path: Option<String>,

    /// 是否将日志写入文件（默认 false）
    #[serde(default)]
    pub log_to_file: Option<bool>,

    /// 是否以管理员权限运行（默认 false）
    /// 启用后应用在启动时通过计划任务或 UAC 弹窗自行提权
    #[serde(default)]
    pub run_as_admin: Option<bool>,
}

impl AppConfig {
    /// 从配置文件加载
    pub fn load() -> Self {
        Self::load_from(&get_config_path())
    }

    pub fn load_from(config_path: &Path) -> Self {
        Self::try_load_from(config_path).unwrap_or_default()
    }

    pub fn try_load_from(config_path: &Path) -> Result<Self, String> {
        if !config_path.exists() {
            return Ok(Self::default());
        }
        if config_path.exists() {
            let content = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
            let config = serde_json::from_str(&content).map_err(|e| e.to_string())?;
            debug!("Configuration loaded from {:?}", config_path);
            return Ok(config);
        }
        unreachable!()
    }

    #[cfg(test)]
    pub fn save_to(&self, config_path: &Path) -> Result<(), String> {
        self.save_to_with(config_path, replace_file)
    }

    pub fn update<F>(update: F) -> Result<(), String>
    where
        F: FnOnce(&mut Self),
    {
        Self::update_at(&get_config_path(), update)
    }

    pub fn update_at<F>(config_path: &Path, update: F) -> Result<(), String>
    where
        F: FnOnce(&mut Self),
    {
        let _guard = config_lock().lock().map_err(|e| e.to_string())?;
        let mut config = Self::try_load_from(config_path)?;
        update(&mut config);
        config.write_to_with(config_path, replace_file)
    }

    pub(crate) fn try_update_at<F, T>(config_path: &Path, update: F) -> Result<T, String>
    where
        F: FnOnce(&mut Self) -> Result<T, String>,
    {
        let _guard = config_lock().lock().map_err(|e| e.to_string())?;
        let mut config = Self::try_load_from(config_path)?;
        let result = update(&mut config)?;
        config.write_to_with(config_path, replace_file)?;
        Ok(result)
    }

    #[cfg(test)]
    fn save_to_with<F>(&self, config_path: &Path, replace: F) -> Result<(), String>
    where
        F: FnOnce(&Path, &Path) -> Result<(), String>,
    {
        let _guard = config_lock().lock().map_err(|e| e.to_string())?;
        self.write_to_with(config_path, replace)
    }

    fn write_to_with<F>(&self, config_path: &Path, replace: F) -> Result<(), String>
    where
        F: FnOnce(&Path, &Path) -> Result<(), String>,
    {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;

        let tmp_path = config_path.with_extension(format!(
            "json.{}.{}.tmp",
            std::process::id(),
            CONFIG_TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = File::create(&tmp_path).map_err(|e| e.to_string())?;
            file.write_all(content.as_bytes())
                .map_err(|e| e.to_string())?;
            file.flush().map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            replace(&tmp_path, config_path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp_path);
        }
        result?;

        info!("Configuration saved to {:?}", config_path);
        Ok(())
    }

    pub fn ensure_default_database(&mut self, data_dir: &Path) {
        let normalized = fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
        let matching_id = self.databases.iter().find_map(|db| {
            let path = Path::new(&db.path);
            let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            (path == normalized).then(|| db.id.clone())
        });
        let legacy_id = if let Some(id) = matching_id {
            id
        } else if let Some(default) = self.databases.iter_mut().find(|db| db.id == "default") {
            default.path = data_dir.to_string_lossy().into_owned();
            default.id.clone()
        } else {
            self.databases.push(DatabaseRegistration {
                id: "default".into(),
                name: "默认数据库".into(),
                path: data_dir.to_string_lossy().into_owned(),
            });
            "default".into()
        };
        let active_id = self
            .active_database_id
            .as_ref()
            .filter(|id| self.databases.iter().any(|db| &db.id == *id))
            .cloned()
            .unwrap_or(legacy_id);
        self.active_database_id = Some(active_id.clone());
        self.data_path = self
            .databases
            .iter()
            .find(|db| db.id == active_id)
            .map(|db| db.path.clone());
    }

    /// 获取日志文件路径
    pub fn get_log_path(&self) -> PathBuf {
        self.get_data_dir().join("app.log")
    }

    /// 是否启用文件日志
    pub fn is_log_to_file(&self) -> bool {
        self.log_to_file.unwrap_or(true)
    }

    /// 获取数据目录路径
    pub fn get_data_dir(&self) -> PathBuf {
        if let Some(dir) = self.custom_data_dir() {
            return dir;
        }
        crate::database::get_default_db_path()
            .parent()
            .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf)
    }

    fn custom_data_dir(&self) -> Option<PathBuf> {
        self.data_path
            .as_ref()
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
    }
}

static CONFIG_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
static CONFIG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
fn config_lock() -> &'static Mutex<()> {
    CONFIG_LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    use windows::core::PCWSTR;

    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|e| e.to_string())
    }
}

/// 获取配置文件路径（固定在安装目录）
fn get_config_path() -> PathBuf {
    crate::database::get_app_dir().join("config.json")
}

/// 将数据从旧路径迁移到新路径
pub fn migrate_data(old_path: &PathBuf, new_path: &PathBuf) -> Result<MigrationResult, String> {
    info!("Migrating data: {:?} -> {:?}", old_path, new_path);

    fs::create_dir_all(new_path).map_err(|e| format!("创建新目录失败: {e}"))?;

    let mut result = MigrationResult::default();
    let old_db = old_path.join("clipboard.db");
    let new_db = new_path.join("clipboard.db");
    let db_present = copy_path_if_exists(&old_db, &new_db, &mut result);
    for suffix in ["-wal", "-shm"] {
        copy_path_if_exists(
            &old_path.join(format!("clipboard.db{suffix}")),
            &new_path.join(format!("clipboard.db{suffix}")),
            &mut result,
        );
    }
    result.db_migrated = db_present && new_db.is_file();

    for directory in ["images", "icons", "staged"] {
        let source = old_path.join(directory);
        let target = new_path.join(directory);
        let present = copy_path_if_exists(&source, &target, &mut result);
        if directory == "images" {
            result.images_migrated = present && target.is_dir();
        }
    }

    info!(
        "Migration complete: {} files, {} bytes",
        result.files_copied, result.bytes_copied
    );
    Ok(result)
}

fn copy_path_if_exists(src: &Path, dst: &Path, result: &mut MigrationResult) -> bool {
    let metadata = match fs::symlink_metadata(src) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => {
            result
                .errors
                .push(format!("Failed to inspect {src:?}: {error}"));
            return false;
        }
    };
    if metadata.is_dir() {
        copy_dir_recursive(src, dst, result);
    } else if metadata.is_file() {
        copy_file(src, dst, result);
    } else {
        result
            .errors
            .push(format!("Unsupported resource type: {src:?}"));
    }
    true
}

fn copy_file(src: &Path, dst: &Path, result: &mut MigrationResult) {
    match fs::copy(src, dst) {
        Ok(bytes) => {
            info!("Copied {:?} ({} bytes)", src, bytes);
            result.files_copied += 1;
            result.bytes_copied += bytes;
        }
        Err(error) => {
            error!("Failed to copy {:?}: {}", src, error);
            result
                .errors
                .push(format!("Failed to copy {src:?}: {error}"));
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path, result: &mut MigrationResult) {
    if let Err(error) = fs::create_dir_all(dst) {
        result
            .errors
            .push(format!("Failed to create {dst:?}: {error}"));
        return;
    }
    let entries = match fs::read_dir(src) {
        Ok(entries) => entries,
        Err(error) => {
            result
                .errors
                .push(format!("Failed to read {src:?}: {error}"));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                result
                    .errors
                    .push(format!("Failed to read entry in {src:?}: {error}"));
                continue;
            }
        };
        let old_file = entry.path();
        let new_file = dst.join(entry.file_name());
        match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => {
                copy_dir_recursive(&old_file, &new_file, result);
            }
            Ok(file_type) if file_type.is_file() => copy_file(&old_file, &new_file, result),
            Ok(_) => result
                .errors
                .push(format!("Unsupported resource type: {old_file:?}")),
            Err(error) => result
                .errors
                .push(format!("Failed to inspect {old_file:?}: {error}")),
        }
    }
}

/// 数据迁移结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationResult {
    pub db_migrated: bool,
    pub images_migrated: bool,
    pub files_copied: usize,
    pub bytes_copied: u64,
    pub errors: Vec<String>,
}

impl MigrationResult {
    pub fn success(&self) -> bool {
        self.errors.is_empty() && self.db_migrated
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn registry_roundtrip_and_legacy_defaults() {
        let legacy: AppConfig = serde_json::from_str(r#"{"data_path":"D:/clip"}"#).unwrap();
        assert!(legacy.databases.is_empty());
        assert_eq!(legacy.active_database_id, None);

        let dir = std::env::temp_dir().join(format!("elegant-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let mut config = legacy;
        config.log_to_file = Some(true);
        config.run_as_admin = Some(false);
        config.ensure_default_database(Path::new("D:/clip"));
        config.save_to(&path).unwrap();
        config.save_to(&path).unwrap();
        assert_eq!(AppConfig::load_from(&path), config);
        assert!(
            !fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn normalize_keeps_active_valid_and_avoids_duplicate_paths() {
        let mut config = AppConfig {
            active_database_id: Some("other".into()),
            databases: vec![
                DatabaseRegistration {
                    id: "existing".into(),
                    name: "已有".into(),
                    path: "D:/clip".into(),
                },
                DatabaseRegistration {
                    id: "other".into(),
                    name: "其他".into(),
                    path: "D:/else".into(),
                },
            ],
            ..Default::default()
        };
        config.ensure_default_database(Path::new("D:/clip"));
        assert_eq!(config.active_database_id.as_deref(), Some("other"));
        assert_eq!(config.databases.len(), 2);

        config.ensure_default_database(Path::new("D:/other"));
        assert!(config.databases.iter().any(|db| db.id == "default"));
        assert!(config.databases.iter().any(|db| db.id == "existing"));
        assert_eq!(
            config
                .databases
                .iter()
                .filter(|db| Path::new(&db.path) == Path::new("D:/clip"))
                .count(),
            1
        );
    }

    #[test]
    fn normalize_preserves_registered_active_database_as_source_of_truth() {
        let mut config = AppConfig {
            active_database_id: Some("other".into()),
            databases: vec![
                DatabaseRegistration {
                    id: "legacy".into(),
                    name: "旧路径".into(),
                    path: "D:/legacy".into(),
                },
                DatabaseRegistration {
                    id: "other".into(),
                    name: "活动库".into(),
                    path: "D:/active".into(),
                },
            ],
            data_path: Some("D:/legacy".into()),
            ..Default::default()
        };
        config.ensure_default_database(Path::new("D:/legacy"));
        assert_eq!(config.active_database_id.as_deref(), Some("other"));
        assert_eq!(config.data_path.as_deref(), Some("D:/active"));
    }

    #[test]
    fn failed_atomic_replace_preserves_old_config_and_cleans_tmp() {
        let dir = std::env::temp_dir().join(format!("elegant-config-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, "old").unwrap();
        let result = AppConfig::default().save_to_with(&path, |_, _| Err("replace failed".into()));
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert!(
            !fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_json_is_reported_and_never_normalized_over() {
        let dir =
            std::env::temp_dir().join(format!("elegant-config-invalid-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, "{broken").unwrap();
        assert!(AppConfig::try_load_from(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{broken");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_saves_leave_valid_json_and_no_temp_files() {
        let dir = std::env::temp_dir().join(format!("elegant-config-race-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    AppConfig {
                        data_path: Some(format!("D:/{i}")),
                        ..Default::default()
                    }
                    .save_to(&path)
                    .unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert!(AppConfig::try_load_from(&path).is_ok());
        assert!(
            !fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_updates_preserve_changes_to_different_fields() {
        let dir =
            std::env::temp_dir().join(format!("elegant-config-update-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        AppConfig::default().save_to(&path).unwrap();
        let a = {
            let path = path.clone();
            std::thread::spawn(move || {
                AppConfig::update_at(&path, |c| c.log_to_file = Some(true)).unwrap()
            })
        };
        let b = {
            let path = path.clone();
            std::thread::spawn(move || {
                AppConfig::update_at(&path, |c| c.run_as_admin = Some(true)).unwrap()
            })
        };
        a.join().unwrap();
        b.join().unwrap();
        let config = AppConfig::try_load_from(&path).unwrap();
        assert_eq!(config.log_to_file, Some(true));
        assert_eq!(config.run_as_admin, Some(true));
        fs::remove_dir_all(dir).unwrap();
    }
}
