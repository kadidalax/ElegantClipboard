use crate::commands::AppState;
use crate::config::{self, AppConfig};
use crate::database;
use crate::utils::format_size;

fn chrono_timestamp() -> String {
    chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    dir: &std::path::Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    use std::io::Write;

    if !dir.exists() || !dir.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
            zip.start_file(&name, options).map_err(|e| e.to_string())?;
            let buf = std::fs::read(&path).map_err(|e| format!("读取 {path:?} 失败: {e}"))?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn write_settings_zip<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    repo: &database::SettingsRepository,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    use std::io::Write;
    zip.start_file("settings.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(
        &serde_json::to_vec_pretty(&repo.get_all().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn validate_backup_zip<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<Option<std::collections::HashMap<String, String>>, String> {
    use std::io::Read;
    let mut has_db = false;
    let mut settings = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let Some(path) = sanitize_zip_relative_path(&name) else {
            return Err(format!("ZIP 包含不安全路径: {name}"));
        };
        let allowed = path == std::path::Path::new("clipboard.db")
            || path == std::path::Path::new("settings.json")
            || ["images", "icons", "staged"]
                .iter()
                .any(|root| path.starts_with(root));
        if !allowed {
            return Err(format!("ZIP 包含不允许的条目: {name}"));
        }
        if path == std::path::Path::new("clipboard.db") {
            has_db = true;
        }
        if path == std::path::Path::new("settings.json") {
            let mut data = Vec::new();
            entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
            settings = Some(
                serde_json::from_slice(&data).map_err(|e| format!("无效的 settings.json: {e}"))?,
            );
        }
    }
    if !has_db {
        return Err("ZIP 文件中未找到 clipboard.db，不是有效的备份文件".into());
    }
    Ok(settings)
}

/// 检测并应用待导入的 staging 数据库文件（clipboard.db.import → clipboard.db）。
fn sanitize_zip_relative_path(name: &str) -> Option<std::path::PathBuf> {
    use std::path::{Component, Path, PathBuf};

    let raw = Path::new(name);
    if raw.is_absolute() {
        return None;
    }

    let mut clean = PathBuf::new();
    for component in raw.components() {
        match component {
            Component::Normal(seg) => clean.push(seg),
            Component::CurDir => {}
            // 拒绝根/前缀/父目录防止路径穿越
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => return None,
        }
    }

    if clean.as_os_str().is_empty() {
        return None;
    }

    Some(clean)
}

pub(crate) fn apply_pending_import(db_path: &std::path::Path) {
    use std::fs;

    let staging = db_path.with_extension("db.import");
    if !staging.exists() {
        return;
    }

    tracing::info!("Detected pending import: {:?}", staging);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let Some(db_dir) = db_path.parent() else {
        return;
    };

    // 原子替换：先备份原库 → rename staging → 失败则回滚
    let backup = db_dir.join("clipboard.db.bak");
    let backup_wal = db_dir.join("clipboard.db.bak-wal");
    let backup_shm = db_dir.join("clipboard.db.bak-shm");

    for attempt in 1..=10 {
        // 1. 将原库 rename 到 backup（原子操作，不会丢数据）
        let mut backed_up = true;
        for (src_ext, dst_ext) in &[("", ".bak"), ("-wal", ".bak-wal"), ("-shm", ".bak-shm")] {
            let src = db_dir.join(format!("clipboard.db{src_ext}"));
            let dst = db_dir.join(format!("clipboard.db{dst_ext}"));
            if src.exists() && fs::rename(&src, &dst).is_err() {
                backed_up = false;
                break;
            }
        }

        if !backed_up {
            // 备份失败，回滚已备份的文件
            for (src_ext, dst_ext) in &[("", ".bak"), ("-wal", ".bak-wal"), ("-shm", ".bak-shm")] {
                let src = db_dir.join(format!("clipboard.db{dst_ext}"));
                let dst = db_dir.join(format!("clipboard.db{src_ext}"));
                let _ = fs::rename(&src, &dst);
            }
            if attempt < 10 {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            continue;
        }

        // 2. rename staging 到 db_path
        if fs::rename(&staging, db_path).is_ok() {
            tracing::info!("Import staging applied (attempt {attempt})");
            // 成功，删除 backup
            let _ = fs::remove_file(&backup);
            let _ = fs::remove_file(&backup_wal);
            let _ = fs::remove_file(&backup_shm);
            return;
        }

        // 3. rename 失败，回滚原库
        tracing::warn!("Rename staging failed (attempt {attempt}), rolling back");
        for (src_ext, dst_ext) in &[("", ".bak"), ("-wal", ".bak-wal"), ("-shm", ".bak-shm")] {
            let src = db_dir.join(format!("clipboard.db{dst_ext}"));
            let dst = db_dir.join(format!("clipboard.db{src_ext}"));
            let _ = fs::rename(&src, &dst);
        }

        if attempt < 10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    // 所有尝试都失败，尝试 copy fallback（原库已回滚，不会丢失）
    tracing::error!("Rename failed after 10 attempts, trying copy fallback");
    if fs::copy(&staging, db_path).is_ok() {
        let _ = fs::remove_file(&staging);
        tracing::info!("Import applied via copy fallback");
    } else {
        tracing::error!("Import staging completely failed, original database preserved");
    }
}

#[tauri::command]
pub fn get_default_data_path(state: tauri::State<'_, std::sync::Arc<AppState>>) -> String {
    get_default_data_path_from(&state)
}

fn get_default_data_path_from(state: &std::sync::Arc<AppState>) -> String {
    state
        .db
        .active_snapshot()
        .data_dir
        .to_string_lossy()
        .into_owned()
}

#[tauri::command]
pub fn get_original_default_path() -> String {
    database::get_default_db_path()
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[tauri::command]
pub fn check_path_has_data(path: String) -> bool {
    let p = std::path::PathBuf::from(&path);
    p.join("clipboard.db").exists()
}

#[tauri::command]
pub fn set_data_path(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    path: String,
) -> Result<(), String> {
    migrate_active_database_at(
        &state,
        &database::get_app_dir().join("config.json"),
        std::path::Path::new(&path),
    )
    .map(|_| ())
}

#[tauri::command]
pub fn migrate_data_to_path(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    new_path: String,
) -> Result<config::MigrationResult, String> {
    migrate_active_database_at(
        &state,
        &database::get_app_dir().join("config.json"),
        std::path::Path::new(&new_path),
    )
}

struct MigrationMonitorPause(crate::clipboard::ClipboardMonitor);

impl MigrationMonitorPause {
    fn new(monitor: &crate::clipboard::ClipboardMonitor) -> Self {
        monitor.pause();
        Self(monitor.clone())
    }
}

impl Drop for MigrationMonitorPause {
    fn drop(&mut self) {
        self.0.resume();
    }
}

fn normalize_migration_target(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    if !path.is_absolute() {
        return Err("DATABASE:INVALID_PATH:path must be absolute".into());
    }
    if path.exists() {
        return std::fs::canonicalize(path).map_err(|e| format!("DATABASE:INVALID_PATH:{e}"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "DATABASE:INVALID_PATH:path has no parent".to_string())?;
    let parent = std::fs::canonicalize(parent).map_err(|e| format!("DATABASE:INVALID_PATH:{e}"))?;
    let name = path
        .file_name()
        .ok_or_else(|| "DATABASE:INVALID_PATH:path has no final component".to_string())?;
    Ok(parent.join(name))
}

fn validate_migration_target(path: &std::path::Path) -> Result<bool, String> {
    match std::fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("DATABASE:INVALID_PATH:{error}")),
        Ok(metadata) if !metadata.is_dir() => {
            Err(format!("DATABASE:TARGET_NOT_EMPTY:{}", path.display()))
        }
        Ok(_) => {
            let mut entries = std::fs::read_dir(path)
                .map_err(|error| format!("DATABASE:INVALID_PATH:{error}"))?;
            match entries.next() {
                None => Ok(true),
                Some(Ok(_)) => Err(format!("DATABASE:TARGET_NOT_EMPTY:{}", path.display())),
                Some(Err(error)) => Err(format!("DATABASE:INVALID_PATH:{error}")),
            }
        }
    }
}

fn validate_migration_resource_target(
    source: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), String> {
    fn same_or_descendant(target: &str, parent: &str) -> bool {
        target == parent
            || target
                .strip_prefix(parent)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    let target_key = super::databases::normalized_path_key(target);
    let source_key = super::databases::normalized_path_key(source);
    if target_key == source_key
        || ["images", "icons", "staged"].iter().any(|directory| {
            same_or_descendant(
                &target_key,
                &super::databases::normalized_path_key(&source.join(directory)),
            )
        })
    {
        Err("DATABASE:INVALID_TARGET".into())
    } else {
        Ok(())
    }
}

fn migrate_active_database_at(
    state: &std::sync::Arc<AppState>,
    config_path: &std::path::Path,
    new_path: &std::path::Path,
) -> Result<config::MigrationResult, String> {
    let new_path = normalize_migration_target(new_path)?;
    let _switch = state.database_switch.lock();
    let config =
        AppConfig::try_load_from(config_path).map_err(|e| format!("DATABASE:CONFIG_LOAD:{e}"))?;
    let active_id = config
        .active_database_id
        .clone()
        .ok_or_else(|| "DATABASE:NOT_FOUND:active database".to_string())?;
    let registration = config
        .databases
        .iter()
        .find(|database| database.id == active_id)
        .ok_or_else(|| format!("DATABASE:NOT_FOUND:{active_id}"))?;
    let new_path_key = super::databases::normalized_path_key(&new_path);
    if config.databases.iter().any(|database| {
        database.id != active_id
            && super::databases::normalized_path_key(std::path::Path::new(&database.path))
                == new_path_key
    }) {
        return Err(format!("DATABASE:DUPLICATE_PATH:{}", new_path.display()));
    }
    let source = state.db.active_snapshot();
    let registered_source = std::fs::canonicalize(&registration.path)
        .map_err(|e| format!("DATABASE:INCONSISTENT:{e}"))?;
    if registered_source != source.data_dir {
        return Err("DATABASE:INCONSISTENT:active id/path mismatch".into());
    }
    validate_migration_resource_target(&source.data_dir, &new_path)?;
    let target_existed = validate_migration_target(&new_path)?;

    let _sync = crate::webdav::try_begin_sync_session().map_err(|_| "DATABASE:BUSY".to_string())?;
    let _operation = state.database_operation.write();
    let _pause = MigrationMonitorPause::new(&state.monitor);

    let mut result = match config::migrate_data(&source.data_dir, &new_path) {
        Ok(result) => result,
        Err(error) => {
            if !target_existed {
                let _ = std::fs::remove_dir_all(&new_path);
            }
            return Err(error);
        }
    };
    if !result.success() {
        if !target_existed && let Err(error) = std::fs::remove_dir_all(&new_path) {
            result
                .errors
                .push(format!("Failed to clean migration target: {error}"));
        }
        return Ok(result);
    }
    let new_path =
        std::fs::canonicalize(&new_path).map_err(|e| format!("DATABASE:OPEN_FAILED:{e}"))?;
    let opened = state
        .db
        .open_active(new_path.clone())
        .map_err(|e| format!("DATABASE:OPEN_FAILED:{e}"))?;
    AppConfig::try_update_at(config_path, |config| {
        if config.active_database_id.as_deref() != Some(active_id.as_str()) {
            return Err("DATABASE:INCONSISTENT:active database changed".into());
        }
        let registration = config
            .databases
            .iter_mut()
            .find(|database| database.id == active_id)
            .ok_or_else(|| format!("DATABASE:NOT_FOUND:{active_id}"))?;
        registration.path = new_path.to_string_lossy().into_owned();
        config.data_path = Some(registration.path.clone());
        Ok(())
    })
    .map_err(|e| {
        if e.starts_with("DATABASE:") {
            e
        } else {
            format!("DATABASE:CONFIG_SAVE:{e}")
        }
    })?;
    drop(state.db.swap_active(opened));
    Ok(result)
}

#[tauri::command]
pub async fn export_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<String, String> {
    use std::fs::{self, File};
    use std::io::Write;
    use tauri_plugin_dialog::DialogExt;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    let operation = state.db.operation_lock();
    let _operation = operation.read();
    let data_dir = state.db.active_snapshot().data_dir;

    let export_db = data_dir.join("clipboard.db.export");
    {
        let src_conn = state.db.write_connection();
        let src_conn = src_conn.lock();
        let _ = fs::remove_file(&export_db);
        let mut dst_conn =
            rusqlite::Connection::open(&export_db).map_err(|e| format!("创建备份文件失败: {e}"))?;
        let backup = rusqlite::backup::Backup::new(&src_conn, &mut dst_conn)
            .map_err(|e| format!("初始化备份失败: {e}"))?;
        backup
            .run_to_completion(100, std::time::Duration::from_millis(0), None)
            .map_err(|e| format!("执行备份失败: {e}"))?;
    }

    let timestamp = chrono_timestamp();
    let default_name = format!("ElegantClipboard_backup_{timestamp}.zip");
    let dest = app
        .dialog()
        .file()
        .set_title("导出数据")
        .set_file_name(&default_name)
        .add_filter("ZIP 压缩文件", &["zip"])
        .blocking_save_file();

    let dest_path = if let Some(p) = dest {
        p.to_string()
    } else {
        let _ = fs::remove_file(&export_db);
        return Err("用户取消了导出".to_string());
    };

    let file = File::create(&dest_path).map_err(|e| format!("创建文件失败: {e}"))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("clipboard.db", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(&fs::read(&export_db).map_err(|e| format!("读取数据库副本失败: {e}"))?)
        .map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&export_db);

    write_settings_zip(
        &mut zip,
        &database::SettingsRepository::new(&state.db),
        options,
    )?;

    add_dir_to_zip(&mut zip, &data_dir.join("images"), "images", options)?;

    add_dir_to_zip(&mut zip, &data_dir.join("icons"), "icons", options)?;

    add_dir_to_zip(&mut zip, &data_dir.join("staged"), "staged", options)?;

    zip.finish().map_err(|e| e.to_string())?;

    let size = fs::metadata(&dest_path).map_or(0, |m| m.len());
    Ok(format!("导出成功 ({})", format_size(size)))
}

#[tauri::command]
pub async fn import_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<String, String> {
    use std::fs::{self, File};
    use std::io::Read;
    use tauri_plugin_dialog::DialogExt;

    let operation = state.db.operation_lock();
    let _operation = operation.read();
    let data_dir = state.db.active_snapshot().data_dir;

    let src = app
        .dialog()
        .file()
        .set_title("导入数据")
        .add_filter("ZIP 压缩文件", &["zip"])
        .blocking_pick_file();

    let src_path = match src {
        Some(p) => p.to_string(),
        None => return Err("用户取消了导入".to_string()),
    };

    let file = File::open(&src_path).map_err(|e| format!("打开文件失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("无效的 ZIP 文件: {e}"))?;

    let imported_settings = validate_backup_zip(&mut archive)?;

    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let mut files_extracted = 0u32;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();

        let rel_path = sanitize_zip_relative_path(&name).expect("archive validated");

        // 跳过临时数据库文件，仅导入 clipboard.db 和资产目录
        if rel_path.ends_with("clipboard.db-wal") || rel_path.ends_with("clipboard.db-shm") {
            continue;
        }
        if rel_path == std::path::Path::new("settings.json") {
            continue;
        }
        let out_path = if rel_path == std::path::Path::new("clipboard.db") {
            data_dir.join("clipboard.db.import")
        } else {
            data_dir.join(&rel_path)
        };

        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            fs::write(&out_path, &buf).map_err(|e| format!("写入 {name} 失败: {e}"))?;
            files_extracted += 1;
        }
    }
    if let Some(settings) = imported_settings {
        database::SettingsRepository::new(&state.db)
            .replace_all(&settings)
            .map_err(|e| e.to_string())?;
    }

    Ok(format!(
        "导入成功，共恢复 {files_extracted} 个文件，应用即将重启"
    ))
}

#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    crate::admin_launch::perform_restart(&app);
}

#[cfg(test)]
mod tests {
    use super::{
        get_default_data_path_from, migrate_active_database_at, sanitize_zip_relative_path,
        validate_backup_zip, validate_migration_resource_target, write_settings_zip,
    };
    use crate::clipboard::ClipboardMonitor;
    use crate::commands::{AppState, PositionCache};
    use crate::config::{AppConfig, DatabaseRegistration};
    use crate::database::{
        ClipboardRepository, ContentType, Database, NewClipboardItem, QueryOptions,
        SettingsRepository,
    };
    use parking_lot::Mutex;
    use std::io::{Cursor, Write};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn migration_state(root: &Path) -> (Arc<AppState>, PathBuf, PathBuf) {
        let source = root.join("source");
        let db = Database::new_with_settings(source.join("clipboard.db"), root.join("settings.db"))
            .unwrap();
        let source = db.active_snapshot().data_dir;
        let operation = db.operation_lock();
        let monitor = ClipboardMonitor::new();
        let state = Arc::new(AppState {
            active_group_id: monitor.active_group_id(),
            db,
            monitor,
            position_cache: Arc::new(Mutex::new(PositionCache {
                position_mode: crate::positioning::PositionMode::FollowCursor,
                persist_window_size: true,
                window_width: None,
                window_height: None,
                window_x: None,
                window_y: None,
            })),
            database_switch: Mutex::new(()),
            database_operation: operation,
        });
        let config_path = root.join("config.json");
        AppConfig {
            active_database_id: Some("other".into()),
            databases: vec![
                DatabaseRegistration {
                    id: "default".into(),
                    name: "默认".into(),
                    path: root.join("default").to_string_lossy().into_owned(),
                },
                DatabaseRegistration {
                    id: "other".into(),
                    name: "当前".into(),
                    path: source.to_string_lossy().into_owned(),
                },
            ],
            data_path: Some(source.to_string_lossy().into_owned()),
            ..Default::default()
        }
        .save_to(&config_path)
        .unwrap();
        (state, config_path, source)
    }

    #[test]
    fn migrating_non_default_active_database_updates_registry_and_live_context() {
        let _serial = crate::webdav::SYNC_SESSION_TEST_LOCK.lock();
        let root = std::env::temp_dir().join(format!(
            "ec-active-migrate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (state, config_path, source) = migration_state(&root);
        let repo = ClipboardRepository::new(&state.db);
        repo.insert(NewClipboardItem {
            content_type: ContentType::Text,
            text_content: Some("migrated".into()),
            content_hash: "migrated".into(),
            semantic_hash: "migrated".into(),
            ..Default::default()
        })
        .unwrap();
        std::fs::create_dir_all(source.join("images/nested")).unwrap();
        std::fs::write(source.join("images/nested/asset.png"), b"asset").unwrap();
        std::fs::create_dir_all(source.join("icons")).unwrap();
        std::fs::write(source.join("icons/app.png"), b"icon").unwrap();
        std::fs::create_dir_all(source.join("staged/files")).unwrap();
        std::fs::write(source.join("staged/files/item.txt"), b"staged").unwrap();
        let target = root.join("target");

        let result = migrate_active_database_at(&state, &config_path, &target).unwrap();
        assert!(result.success());
        let target = std::fs::canonicalize(target).unwrap();
        assert_eq!(state.db.active_snapshot().data_dir, target);
        assert_eq!(repo.count(QueryOptions::default()).unwrap(), 1);
        assert!(target.join("images/nested/asset.png").exists());
        assert!(target.join("icons/app.png").exists());
        assert!(target.join("staged/files/item.txt").exists());
        assert!(source.join("clipboard.db").exists());
        assert!(!state.monitor.is_paused());

        let config = AppConfig::try_load_from(&config_path).unwrap();
        assert_eq!(config.active_database_id.as_deref(), Some("other"));
        assert_eq!(
            config.data_path.as_deref(),
            Some(target.to_string_lossy().as_ref())
        );
        assert_eq!(
            config
                .databases
                .iter()
                .find(|database| database.id == "other")
                .unwrap()
                .path,
            target.to_string_lossy()
        );
        assert_ne!(
            config
                .databases
                .iter()
                .find(|database| database.id == "default")
                .unwrap()
                .path,
            target.to_string_lossy()
        );

        let reopened = Database::new_with_settings(
            PathBuf::from(
                &config
                    .databases
                    .iter()
                    .find(|database| database.id == "other")
                    .unwrap()
                    .path,
            )
            .join("clipboard.db"),
            root.join("reopen-settings.db"),
        )
        .unwrap();
        assert_eq!(
            ClipboardRepository::new(&reopened)
                .count(QueryOptions::default())
                .unwrap(),
            1
        );
        drop(reopened);
        drop(repo);
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_rejects_path_registered_to_another_database_without_writing() {
        let _serial = crate::webdav::SYNC_SESSION_TEST_LOCK.lock();
        let root = std::env::temp_dir().join(format!(
            "ec-active-migrate-duplicate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (state, config_path, source) = migration_state(&root);
        let active_repo = ClipboardRepository::new(&state.db);
        let active_item_id = active_repo
            .insert(NewClipboardItem {
                content_type: ContentType::Text,
                text_content: Some("active".into()),
                content_hash: "active".into(),
                semantic_hash: "active".into(),
                ..Default::default()
            })
            .unwrap();

        let registered = Database::new_with_settings(
            root.join("default/clipboard.db"),
            root.join("default-settings.db"),
        )
        .unwrap();
        let registered_repo = ClipboardRepository::new(&registered);
        let registered_item_id = registered_repo
            .insert(NewClipboardItem {
                content_type: ContentType::Text,
                text_content: Some("registered".into()),
                content_hash: "registered".into(),
                semantic_hash: "registered".into(),
                ..Default::default()
            })
            .unwrap();
        let target = registered.active_snapshot().data_dir;

        let error = migrate_active_database_at(&state, &config_path, &target).unwrap_err();
        assert!(error.starts_with("DATABASE:DUPLICATE_PATH:"));
        assert_eq!(state.db.active_snapshot().data_dir, source);
        assert!(
            active_repo
                .get_by_id(active_item_id)
                .unwrap()
                .unwrap()
                .text_content
                .as_deref()
                == Some("active")
        );
        assert!(
            registered_repo
                .get_by_id(registered_item_id)
                .unwrap()
                .unwrap()
                .text_content
                .as_deref()
                == Some("registered")
        );

        drop(registered_repo);
        drop(registered);
        drop(active_repo);
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_rejects_nonempty_unregistered_target_before_copying() {
        let _serial = crate::webdav::SYNC_SESSION_TEST_LOCK.lock();
        let root = std::env::temp_dir().join(format!(
            "ec-active-migrate-nonempty-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (state, config_path, source) = migration_state(&root);
        let target = root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"keep").unwrap();

        let error = migrate_active_database_at(&state, &config_path, &target).unwrap_err();
        assert!(error.starts_with("DATABASE:TARGET_NOT_EMPTY:"));
        assert_eq!(state.db.active_snapshot().data_dir, source);
        assert_eq!(std::fs::read(target.join("keep.txt")).unwrap(), b"keep");

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn resource_copy_failure_does_not_update_config_or_switch() {
        use std::os::windows::fs::OpenOptionsExt;

        let _serial = crate::webdav::SYNC_SESSION_TEST_LOCK.lock();
        let root = std::env::temp_dir().join(format!(
            "ec-active-migrate-resource-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (state, config_path, source) = migration_state(&root);
        let locked_path = source.join("images/nested/locked.png");
        std::fs::create_dir_all(locked_path.parent().unwrap()).unwrap();
        std::fs::write(&locked_path, b"locked").unwrap();
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked_path)
            .unwrap();
        let target = root.join("target");

        let result = migrate_active_database_at(&state, &config_path, &target).unwrap();
        assert!(!result.success());
        assert!(!result.errors.is_empty());
        assert_eq!(state.db.active_snapshot().data_dir, source);
        let config = AppConfig::try_load_from(&config_path).unwrap();
        assert_eq!(config.active_database_id.as_deref(), Some("other"));
        assert_eq!(
            config
                .databases
                .iter()
                .find(|database| database.id == "other")
                .unwrap()
                .path,
            source.to_string_lossy()
        );

        drop(locked);
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_rejects_target_inside_source_resources_without_writing() {
        let _serial = crate::webdav::SYNC_SESSION_TEST_LOCK.lock();
        let root = std::env::temp_dir().join(format!(
            "ec-active-migrate-recursive-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (state, config_path, source) = migration_state(&root);
        std::fs::create_dir_all(source.join("images")).unwrap();
        std::fs::write(source.join("images/original.png"), b"original").unwrap();
        let target = source.join("images/new");

        let before = std::fs::read_dir(source.join("images")).unwrap().count();
        assert_eq!(
            migrate_active_database_at(&state, &config_path, &target).unwrap_err(),
            "DATABASE:INVALID_TARGET"
        );
        assert_eq!(
            std::fs::read_dir(source.join("images")).unwrap().count(),
            before
        );
        assert!(!target.exists());
        assert_eq!(state.db.active_snapshot().data_dir, source);

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_resource_target_requires_path_component_boundary() {
        let root = std::env::temp_dir().join("ec-migration-resource-boundary");
        let source = root.join("source");

        assert!(validate_migration_resource_target(&source, &source.join("images2/new")).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn migration_rejects_case_variant_target_inside_source_resources() {
        let _serial = crate::webdav::SYNC_SESSION_TEST_LOCK.lock();
        let root = std::env::temp_dir().join(format!(
            "ec-active-migrate-recursive-case-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (state, config_path, source) = migration_state(&root);
        std::fs::create_dir_all(source.join("Images")).unwrap();
        std::fs::write(source.join("Images/original.png"), b"original").unwrap();
        let target = source.join("images/new");
        let before = std::fs::read_dir(source.join("Images")).unwrap().count();

        assert_eq!(
            migrate_active_database_at(&state, &config_path, &target).unwrap_err(),
            "DATABASE:INVALID_TARGET"
        );
        assert_eq!(
            std::fs::read_dir(source.join("Images")).unwrap().count(),
            before
        );
        assert!(!target.exists());
        assert_eq!(state.db.active_snapshot().data_dir, source);

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_keeps_monitor_running_while_waiting_for_database_operation() {
        let _serial = crate::webdav::SYNC_SESSION_TEST_LOCK.lock();
        let root = std::env::temp_dir().join(format!(
            "ec-active-migrate-busy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (state, config_path, _) = migration_state(&root);
        let operation = state.database_operation.read();
        let migration_state = state.clone();
        let target = root.join("target");
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let migration = std::thread::spawn(move || {
            result_tx
                .send(migrate_active_database_at(
                    &migration_state,
                    &config_path,
                    &target,
                ))
                .unwrap();
        });

        assert!(
            result_rx
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        let paused_while_waiting = state.monitor.is_paused();
        drop(operation);
        assert!(result_rx.recv().unwrap().unwrap().success());
        migration.join().unwrap();
        assert!(!state.monitor.is_paused());
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
        assert!(!paused_while_waiting);
    }

    #[test]
    fn default_data_path_reads_only_complete_active_snapshots_during_switches() {
        let root = std::env::temp_dir().join(format!(
            "ec-default-path-switch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (state, _, old_path) = migration_state(&root);
        let new_active = state.db.open_active(root.join("new")).unwrap();
        let new_path = new_active.data_dir.clone();
        let old_active = state.db.active_snapshot();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let writer_state = state.clone();
        let writer_barrier = barrier.clone();
        let writer = std::thread::spawn(move || {
            writer_barrier.wait();
            for index in 0..1_000 {
                writer_state.db.swap_active(if index % 2 == 0 {
                    new_active.clone()
                } else {
                    old_active.clone()
                });
            }
        });

        barrier.wait();
        for _ in 0..1_000 {
            let observed = PathBuf::from(get_default_data_path_from(&state));
            assert!(observed == old_path || observed == new_path);
        }
        writer.join().unwrap();

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn normal_relative_path() {
        assert_eq!(
            sanitize_zip_relative_path("images/screenshot.png"),
            Some(PathBuf::from("images/screenshot.png"))
        );
    }

    #[test]
    fn simple_filename() {
        assert_eq!(
            sanitize_zip_relative_path("clipboard.db"),
            Some(PathBuf::from("clipboard.db"))
        );
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        assert_eq!(sanitize_zip_relative_path("../etc/passwd"), None);
        assert_eq!(sanitize_zip_relative_path("images/../../secret"), None);
    }

    #[test]
    fn rejects_absolute_path() {
        assert_eq!(sanitize_zip_relative_path("/etc/passwd"), None);
        assert_eq!(sanitize_zip_relative_path("C:\\Windows\\system32"), None);
    }

    #[test]
    fn rejects_empty_path() {
        assert_eq!(sanitize_zip_relative_path(""), None);
    }

    #[test]
    fn rejects_dot_only() {
        assert_eq!(sanitize_zip_relative_path("."), None);
        assert_eq!(sanitize_zip_relative_path("./"), None);
    }

    #[test]
    fn strips_current_dir_prefix() {
        assert_eq!(
            sanitize_zip_relative_path("./images/test.png"),
            Some(PathBuf::from("images/test.png"))
        );
    }

    #[test]
    fn nested_path_ok() {
        assert_eq!(
            sanitize_zip_relative_path("a/b/c/d.txt"),
            Some(PathBuf::from("a/b/c/d.txt"))
        );
    }

    #[test]
    fn settings_zip_roundtrip_and_legacy_zip_compatibility() {
        let dir = std::env::temp_dir().join(format!("elegant-zip-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db =
            Database::new_with_settings(dir.join("clipboard.db"), dir.join("settings.db")).unwrap();
        let repo = SettingsRepository::new(&db);
        repo.set("theme", "dark").unwrap();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("clipboard.db", options).unwrap();
        zip.write_all(b"db").unwrap();
        write_settings_zip(&mut zip, &repo, options).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        repo.set("theme", "light").unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let settings = validate_backup_zip(&mut archive).unwrap().unwrap();
        repo.replace_all(&settings).unwrap();
        assert_eq!(repo.get("theme").unwrap().as_deref(), Some("dark"));

        let mut legacy = zip::ZipWriter::new(Cursor::new(Vec::new()));
        legacy.start_file("clipboard.db", options).unwrap();
        legacy.write_all(b"db").unwrap();
        let mut archive =
            zip::ZipArchive::new(Cursor::new(legacy.finish().unwrap().into_inner())).unwrap();
        repo.set("theme", "current").unwrap();
        assert!(validate_backup_zip(&mut archive).unwrap().is_none());
        assert_eq!(repo.get("theme").unwrap().as_deref(), Some("current"));
        drop(repo);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_unknown_root_entries_before_writing_or_committing_settings() {
        let dir =
            std::env::temp_dir().join(format!("elegant-zip-malicious-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db =
            Database::new_with_settings(dir.join("clipboard.db"), dir.join("settings.db")).unwrap();
        let repo = SettingsRepository::new(&db);
        repo.set("theme", "current").unwrap();
        for bad_name in ["config.json", "settings.db"] {
            let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("settings.json", options).unwrap();
            zip.write_all(br#"{"theme":"evil"}"#).unwrap();
            zip.start_file("clipboard.db", options).unwrap();
            zip.write_all(b"db").unwrap();
            zip.start_file(bad_name, options).unwrap();
            zip.write_all(b"evil").unwrap();
            let mut archive =
                zip::ZipArchive::new(Cursor::new(zip.finish().unwrap().into_inner())).unwrap();
            assert!(validate_backup_zip(&mut archive).is_err());
            assert_eq!(repo.get("theme").unwrap().as_deref(), Some("current"));
            if bad_name == "config.json" {
                assert!(!dir.join(bad_name).exists());
            }
        }
        drop(repo);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
