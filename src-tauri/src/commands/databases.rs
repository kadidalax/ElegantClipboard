use crate::commands::AppState;
use crate::config::{AppConfig, DatabaseRegistration};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{Emitter, State};

const BUSY: &str = "DATABASE:BUSY";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DatabaseInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActiveDatabaseInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub db_path: String,
    pub images_dir: String,
    pub icons_dir: String,
    pub staged_dir: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActiveDatabaseStats {
    pub id: String,
    pub name: String,
    pub item_count: i64,
    pub db_size: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DatabaseSwitchedPayload {
    pub id: String,
    pub name: String,
}

struct MonitorPauseGuard(crate::clipboard::ClipboardMonitor);

impl MonitorPauseGuard {
    fn new(monitor: &crate::clipboard::ClipboardMonitor) -> Self {
        monitor.pause();
        Self(monitor.clone())
    }
}

impl Drop for MonitorPauseGuard {
    fn drop(&mut self) {
        self.0.resume();
    }
}

fn error(code: &str, message: impl std::fmt::Display) -> String {
    format!("DATABASE:{code}:{message}")
}

fn config_path() -> PathBuf {
    crate::database::get_app_dir().join("config.json")
}

fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        Err(error("INVALID_NAME", "name is empty"))
    } else {
        Ok(name.to_string())
    }
}

fn normalize_existing_dir(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(error("INVALID_PATH", "path must be absolute"));
    }
    let path = std::fs::canonicalize(path).map_err(|e| error("INVALID_PATH", e))?;
    if !path.is_dir() {
        return Err(error("INVALID_PATH", "path is not a directory"));
    }
    Ok(path)
}

fn normalize_new_dir(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(error("INVALID_PATH", "path must be absolute"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| error("INVALID_PATH", "path has no parent"))?;
    let parent = std::fs::canonicalize(parent).map_err(|e| error("INVALID_PATH", e))?;
    let name = path
        .file_name()
        .ok_or_else(|| error("INVALID_PATH", "path has no final component"))?;
    Ok(parent.join(name))
}

pub(crate) fn normalized_path_key(path: &Path) -> String {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|current| current.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    });
    let mut key = path.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    key.make_ascii_lowercase();
    key
}

fn reject_duplicate_path(config: &AppConfig, path: &Path) -> Result<(), String> {
    let path_key = normalized_path_key(path);
    if config
        .databases
        .iter()
        .any(|database| normalized_path_key(Path::new(&database.path)) == path_key)
    {
        Err(error("DUPLICATE_PATH", path.display()))
    } else {
        Ok(())
    }
}

fn next_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn info(registration: &DatabaseRegistration, active_id: Option<&str>) -> DatabaseInfo {
    DatabaseInfo {
        id: registration.id.clone(),
        name: registration.name.clone(),
        path: registration.path.clone(),
        is_active: active_id == Some(registration.id.as_str()),
    }
}

fn list_databases_at(config_path: &Path) -> Result<Vec<DatabaseInfo>, String> {
    let config = AppConfig::try_load_from(config_path).map_err(|e| error("CONFIG_LOAD", e))?;
    Ok(config
        .databases
        .iter()
        .map(|registration| info(registration, config.active_database_id.as_deref()))
        .collect())
}

fn list_databases_from(
    state: &Arc<AppState>,
    config_path: &Path,
) -> Result<Vec<DatabaseInfo>, String> {
    let _guard = state.database_switch.lock();
    list_databases_at(config_path)
}

fn create_database_at(
    state: &Arc<AppState>,
    config_path: &Path,
    name: &str,
    path: &Path,
) -> Result<DatabaseInfo, String> {
    let name = validate_name(name)?;
    let path = normalize_new_dir(path)?;
    let _guard = state.database_switch.lock();
    let config = AppConfig::try_load_from(config_path).map_err(|e| error("CONFIG_LOAD", e))?;
    reject_duplicate_path(&config, &path)?;
    if path.exists() {
        return Err(error("PATH_EXISTS", path.display()));
    }

    std::fs::create_dir(&path).map_err(|e| error("CREATE_FAILED", e))?;
    let registration = DatabaseRegistration {
        id: next_id(),
        name,
        path: path.to_string_lossy().into_owned(),
    };
    let result = (|| {
        drop(
            state
                .db
                .open_active(path.clone())
                .map_err(|e| error("CREATE_FAILED", e))?,
        );
        AppConfig::try_update_at(config_path, |config| {
            reject_duplicate_path(config, &path)?;
            config.databases.push(registration.clone());
            Ok(())
        })
        .map_err(|e| {
            if e.starts_with("DATABASE:") {
                e
            } else {
                error("CONFIG_SAVE", e)
            }
        })?;
        Ok(info(
            &registration,
            AppConfig::try_load_from(config_path)
                .ok()
                .and_then(|config| config.active_database_id)
                .as_deref(),
        ))
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&path);
    }
    result
}

fn add_existing_database_at(
    state: &Arc<AppState>,
    config_path: &Path,
    name: &str,
    path: &Path,
) -> Result<DatabaseInfo, String> {
    let name = validate_name(name)?;
    let path = normalize_existing_dir(path)?;
    if !path.join("clipboard.db").is_file() {
        return Err(error("INVALID_DATABASE", "clipboard.db not found"));
    }
    let _guard = state.database_switch.lock();
    let config = AppConfig::try_load_from(config_path).map_err(|e| error("CONFIG_LOAD", e))?;
    reject_duplicate_path(&config, &path)?;
    drop(
        state
            .db
            .open_active(path.clone())
            .map_err(|e| error("INVALID_DATABASE", e))?,
    );
    let registration = DatabaseRegistration {
        id: next_id(),
        name,
        path: path.to_string_lossy().into_owned(),
    };
    AppConfig::try_update_at(config_path, |config| {
        reject_duplicate_path(config, &path)?;
        config.databases.push(registration.clone());
        Ok(())
    })
    .map_err(|e| {
        if e.starts_with("DATABASE:") {
            e
        } else {
            error("CONFIG_SAVE", e)
        }
    })?;
    Ok(info(
        &registration,
        AppConfig::try_load_from(config_path)
            .ok()
            .and_then(|config| config.active_database_id)
            .as_deref(),
    ))
}

fn rename_database_at(
    state: &Arc<AppState>,
    config_path: &Path,
    id: &str,
    name: &str,
) -> Result<(), String> {
    let name = validate_name(name)?;
    let _guard = state.database_switch.lock();
    AppConfig::try_update_at(config_path, |config| {
        let database = config
            .databases
            .iter_mut()
            .find(|database| database.id == id)
            .ok_or_else(|| error("NOT_FOUND", id))?;
        database.name = name;
        Ok(())
    })
    .map_err(|e| {
        if e.starts_with("DATABASE:") {
            e
        } else {
            error("CONFIG_SAVE", e)
        }
    })
}

fn remove_database_registration_at(
    state: &Arc<AppState>,
    config_path: &Path,
    id: &str,
) -> Result<(), String> {
    let _guard = state.database_switch.lock();
    AppConfig::try_update_at(config_path, |config| {
        if config.active_database_id.as_deref() == Some(id) {
            return Err(error("ACTIVE_DATABASE", id));
        }
        let before = config.databases.len();
        config.databases.retain(|database| database.id != id);
        if config.databases.len() == before {
            return Err(error("NOT_FOUND", id));
        }
        Ok(())
    })
    .map_err(|e| {
        if e.starts_with("DATABASE:") {
            e
        } else {
            error("CONFIG_SAVE", e)
        }
    })
}

fn active_info(state: &Arc<AppState>, registration: &DatabaseRegistration) -> ActiveDatabaseInfo {
    let active = state.db.active_snapshot();
    ActiveDatabaseInfo {
        id: registration.id.clone(),
        name: registration.name.clone(),
        path: active.data_dir.to_string_lossy().into_owned(),
        db_path: active.db_path.to_string_lossy().into_owned(),
        images_dir: active.images_dir.to_string_lossy().into_owned(),
        icons_dir: active.icons_dir.to_string_lossy().into_owned(),
        staged_dir: active.staged_dir.to_string_lossy().into_owned(),
    }
}

fn get_active_database_info_from(
    state: &Arc<AppState>,
    config_path: &Path,
) -> Result<ActiveDatabaseInfo, String> {
    let _guard = state.database_switch.lock();
    let config = AppConfig::try_load_from(config_path).map_err(|e| error("CONFIG_LOAD", e))?;
    let id = config
        .active_database_id
        .as_deref()
        .ok_or_else(|| error("NOT_FOUND", "active database"))?;
    let registration = config
        .databases
        .iter()
        .find(|database| database.id == id)
        .ok_or_else(|| error("NOT_FOUND", id))?;
    let registered_path =
        std::fs::canonicalize(&registration.path).map_err(|e| error("INCONSISTENT", e))?;
    if registered_path != state.db.active_snapshot().data_dir {
        return Err(error("INCONSISTENT", "active id/path mismatch"));
    }
    Ok(active_info(state, registration))
}

fn active_database_stats_from(
    state: &Arc<AppState>,
    config_path: &Path,
) -> Result<ActiveDatabaseStats, String> {
    let _switch = state.database_switch.lock();
    let _operation = state.database_operation.read();
    let config = AppConfig::try_load_from(config_path).map_err(|e| error("CONFIG_LOAD", e))?;
    let id = config
        .active_database_id
        .as_deref()
        .ok_or_else(|| error("NOT_FOUND", "active database"))?;
    let registration = config
        .databases
        .iter()
        .find(|database| database.id == id)
        .ok_or_else(|| error("NOT_FOUND", id))?;
    let active = state.db.active_snapshot();
    let registered_path =
        std::fs::canonicalize(&registration.path).map_err(|e| error("INCONSISTENT", e))?;
    if registered_path != active.data_dir {
        return Err(error("INCONSISTENT", "active id/path mismatch"));
    }
    let item_count = crate::database::ClipboardRepository::new(&state.db)
        .count(crate::database::QueryOptions::default())
        .map_err(|e| error("STATS", e))?;
    let db_size = ["clipboard.db", "clipboard.db-wal", "clipboard.db-shm"]
        .iter()
        .map(|name| std::fs::metadata(active.data_dir.join(name)).map_or(0, |meta| meta.len()))
        .sum();

    Ok(ActiveDatabaseStats {
        id: registration.id.clone(),
        name: registration.name.clone(),
        item_count,
        db_size,
    })
}

fn switch_database_with<P, E>(
    state: &Arc<AppState>,
    config_path: &Path,
    id: &str,
    persist: P,
    emit: E,
) -> Result<ActiveDatabaseInfo, String>
where
    P: FnOnce(&DatabaseRegistration) -> Result<(), String>,
    E: FnOnce(&DatabaseSwitchedPayload) -> Result<(), String>,
{
    let _switch = state.database_switch.lock();
    let config = AppConfig::try_load_from(config_path).map_err(|e| error("CONFIG_LOAD", e))?;
    let current = config.active_database_id.as_deref();
    if current == Some(id) {
        return Err(error("ALREADY_ACTIVE", id));
    }
    let target = config
        .databases
        .iter()
        .find(|database| database.id == id)
        .cloned()
        .ok_or_else(|| error("NOT_FOUND", id))?;
    let _sync = crate::webdav::try_begin_sync_session().map_err(|_| BUSY.to_string())?;
    if crate::webdav::has_active_media_sync() {
        return Err(BUSY.to_string());
    }
    let _operation = state.database_operation.write();
    let _pause = MonitorPauseGuard::new(&state.monitor);
    let data_dir =
        normalize_existing_dir(Path::new(&target.path)).map_err(|e| error("OPEN_FAILED", e))?;
    if !data_dir.join("clipboard.db").is_file() {
        return Err(error("OPEN_FAILED", "clipboard.db not found"));
    }
    let opened = state
        .db
        .open_active(data_dir)
        .map_err(|e| error("OPEN_FAILED", e))?;
    persist(&target).map_err(|e| error("CONFIG_SAVE", e))?;
    drop(state.db.swap_active(opened));
    *state.active_group_id.lock() = None;
    let payload = DatabaseSwitchedPayload {
        id: target.id.clone(),
        name: target.name.clone(),
    };
    if let Err(e) = emit(&payload) {
        tracing::warn!("Failed to emit database-switched: {e}");
    }
    Ok(active_info(state, &target))
}

#[tauri::command]
pub fn list_databases(state: State<'_, Arc<AppState>>) -> Result<Vec<DatabaseInfo>, String> {
    list_databases_from(&state, &config_path())
}

#[tauri::command]
pub fn create_database(
    state: State<'_, Arc<AppState>>,
    name: String,
    path: String,
) -> Result<DatabaseInfo, String> {
    create_database_at(&state, &config_path(), &name, Path::new(&path))
}

#[tauri::command]
pub fn add_existing_database(
    state: State<'_, Arc<AppState>>,
    name: String,
    path: String,
) -> Result<DatabaseInfo, String> {
    add_existing_database_at(&state, &config_path(), &name, Path::new(&path))
}

#[tauri::command]
pub fn rename_database(
    state: State<'_, Arc<AppState>>,
    id: String,
    name: String,
) -> Result<(), String> {
    rename_database_at(&state, &config_path(), &id, &name)
}

#[tauri::command]
pub fn remove_database_registration(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    remove_database_registration_at(&state, &config_path(), &id)
}

#[tauri::command]
pub fn switch_database(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<ActiveDatabaseInfo, String> {
    let config_path = config_path();
    switch_database_with(
        &state,
        &config_path,
        &id,
        |target| {
            AppConfig::try_update_at(&config_path, |config| {
                if !config
                    .databases
                    .iter()
                    .any(|database| database.id == target.id)
                {
                    return Err(error("NOT_FOUND", &target.id));
                }
                config.active_database_id = Some(target.id.clone());
                config.data_path = Some(target.path.clone());
                Ok(())
            })
        },
        |payload| {
            app.emit("database-switched", payload)
                .map_err(|e| e.to_string())
        },
    )
}

#[tauri::command]
pub fn get_active_database_info(
    state: State<'_, Arc<AppState>>,
) -> Result<ActiveDatabaseInfo, String> {
    get_active_database_info_from(&state, &config_path())
}

#[tauri::command]
pub fn get_active_database_stats(
    state: State<'_, Arc<AppState>>,
) -> Result<ActiveDatabaseStats, String> {
    active_database_stats_from(&state, &config_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::ClipboardMonitor;
    use crate::commands::{AppState, PositionCache};
    use crate::config::{AppConfig, DatabaseRegistration};
    use crate::database::{ClipboardRepository, Database, GroupRepository, NewClipboardItem};
    use parking_lot::Mutex;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    static TEST_LOCK: &Mutex<()> = &crate::webdav::SYNC_SESSION_TEST_LOCK;

    fn temp_root(label: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ec-registry-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn state_and_config(root: &Path) -> (Arc<AppState>, PathBuf) {
        let first = root.join("first");
        let db = Database::new_with_settings(first.join("clipboard.db"), root.join("settings.db"))
            .unwrap();
        let first = db.active_snapshot().data_dir;
        let database_operation = db.operation_lock();
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
            database_operation,
        });
        let config_path = root.join("config.json");
        AppConfig {
            active_database_id: Some("default".into()),
            databases: vec![DatabaseRegistration {
                id: "default".into(),
                name: "默认数据库".into(),
                path: first.to_string_lossy().into_owned(),
            }],
            data_path: Some(first.to_string_lossy().into_owned()),
            ..Default::default()
        }
        .save_to(&config_path)
        .unwrap();
        (state, config_path)
    }

    #[test]
    fn registry_create_add_rename_remove_without_deleting_files() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("crud");
        let (state, config_path) = state_and_config(&root);
        let created_dir = root.join("created");
        let created = create_database_at(&state, &config_path, " 新库 ", &created_dir).unwrap();
        assert!(created_dir.join("clipboard.db").exists());
        assert_eq!(created.name, "新库");

        let external = root.join("external");
        Database::new_with_settings(
            external.join("clipboard.db"),
            root.join("external-settings.db"),
        )
        .unwrap();
        let added = add_existing_database_at(&state, &config_path, "已有", &external).unwrap();
        rename_database_at(&state, &config_path, &added.id, "重命名").unwrap();
        remove_database_registration_at(&state, &config_path, &added.id).unwrap();
        assert!(external.join("clipboard.db").exists());
        assert_eq!(list_databases_at(&config_path).unwrap().len(), 2);
        assert!(remove_database_registration_at(&state, &config_path, "default").is_err());
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn registry_rejects_duplicate_path_empty_name_and_invalid_existing_database() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("invalid");
        let (state, config_path) = state_and_config(&root);
        let active = state.db.active_snapshot().data_dir;
        assert!(
            add_existing_database_at(&state, &config_path, "重复", &active)
                .unwrap_err()
                .starts_with("DATABASE:DUPLICATE_PATH")
        );
        assert!(
            create_database_at(&state, &config_path, " ", &root.join("blank"))
                .unwrap_err()
                .starts_with("DATABASE:INVALID_NAME")
        );
        let invalid = root.join("invalid-db");
        std::fs::create_dir_all(&invalid).unwrap();
        std::fs::write(invalid.join("clipboard.db"), b"not sqlite").unwrap();
        assert!(
            add_existing_database_at(&state, &config_path, "坏库", &invalid)
                .unwrap_err()
                .starts_with("DATABASE:INVALID_DATABASE")
        );
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn registry_rejects_duplicate_path_with_different_ascii_case() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("duplicate-case");
        let (state, config_path) = state_and_config(&root);
        create_database_at(&state, &config_path, "原路径", &root.join("CaseDb")).unwrap();

        assert!(
            create_database_at(&state, &config_path, "大小写变体", &root.join("casedb"))
                .unwrap_err()
                .starts_with("DATABASE:DUPLICATE_PATH")
        );

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn normalized_path_key_is_ascii_case_insensitive() {
        let root = temp_root("normalized-case");
        std::fs::create_dir_all(root.join("Images")).unwrap();

        assert_eq!(
            normalized_path_key(&root.join("Images")),
            normalized_path_key(&root.join("images"))
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generated_database_ids_are_uuid_v4() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("uuid-id");
        let (state, config_path) = state_and_config(&root);
        let created = create_database_at(&state, &config_path, "UUID", &root.join("uuid")).unwrap();
        let id = uuid::Uuid::parse_str(&created.id).unwrap();
        assert_eq!(id.get_version_num(), 4);

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_database_stats_returns_identity_count_and_db_size() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("active-stats");
        let (state, config_path) = state_and_config(&root);
        ClipboardRepository::new(&state.db)
            .insert(NewClipboardItem {
                text_content: Some("stats".into()),
                content_hash: "stats-hash".into(),
                semantic_hash: "stats-hash".into(),
                ..Default::default()
            })
            .unwrap();

        let stats = active_database_stats_from(&state, &config_path).unwrap();

        assert_eq!(stats.id, "default");
        assert_eq!(stats.name, "默认数据库");
        assert_eq!(stats.item_count, 1);
        assert!(stats.db_size > 0);

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn switch_save_failure_or_open_failure_keeps_old_database_and_resumes_monitor() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("rollback");
        let (state, config_path) = state_and_config(&root);
        let target =
            create_database_at(&state, &config_path, "目标", &root.join("target")).unwrap();
        let old_path = state.db.active_snapshot().db_path;
        let error = switch_database_with(
            &state,
            &config_path,
            &target.id,
            |_| Err("save failed".into()),
            |_| Ok(()),
        )
        .unwrap_err();
        assert!(error.starts_with("DATABASE:CONFIG_SAVE"));
        assert_eq!(state.db.active_snapshot().db_path, old_path);
        assert!(!state.monitor.is_paused());

        AppConfig::update_at(&config_path, |config| {
            config.databases.push(DatabaseRegistration {
                id: "broken".into(),
                name: "损坏".into(),
                path: root.join("broken").to_string_lossy().into_owned(),
            });
        })
        .unwrap();
        std::fs::create_dir_all(root.join("broken")).unwrap();
        std::fs::write(root.join("broken/clipboard.db"), b"broken").unwrap();
        assert!(
            switch_database_with(&state, &config_path, "broken", |_| Ok(()), |_| Ok(()))
                .unwrap_err()
                .starts_with("DATABASE:OPEN_FAILED")
        );
        assert_eq!(state.db.active_snapshot().db_path, old_path);
        assert!(!state.monitor.is_paused());
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn switch_updates_active_info_group_and_payload_but_ignores_emit_failure() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("payload");
        let (state, config_path) = state_and_config(&root);
        let target =
            create_database_at(&state, &config_path, "目标", &root.join("target")).unwrap();
        GroupRepository::new(&state.db).create("old", None).unwrap();
        *state.active_group_id.lock() = Some(1);
        let payload = Arc::new(Mutex::new(None));
        let captured = payload.clone();
        let updated = AppConfig::try_load_from(&config_path).unwrap();
        let target_info = switch_database_with(
            &state,
            &config_path,
            &target.id,
            |registration| {
                let mut updated = updated;
                updated.active_database_id = Some(registration.id.clone());
                updated.data_path = Some(registration.path.clone());
                updated.save_to(&config_path)
            },
            |event| {
                *captured.lock() = Some(event.clone());
                Err("listener failed".into())
            },
        )
        .unwrap();
        assert_eq!(target_info.id, target.id);
        assert_eq!(*state.active_group_id.lock(), None);
        assert_eq!(payload.lock().as_ref().unwrap().name, "目标");
        assert_eq!(
            get_active_database_info_from(&state, &config_path)
                .unwrap()
                .id,
            target.id
        );
        assert!(!state.monitor.is_paused());
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn switch_waits_when_database_operation_is_active() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("busy");
        let (state, config_path) = state_and_config(&root);
        let target =
            create_database_at(&state, &config_path, "目标", &root.join("target")).unwrap();
        let operation = state.database_operation.read();
        let switch_state = state.clone();
        let switch_path = config_path.clone();
        let switch_id = target.id.clone();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let (paused_tx, paused_rx) = std::sync::mpsc::channel();
        let (continue_tx, continue_rx) = std::sync::mpsc::channel();
        let switch = std::thread::spawn(move || {
            let monitor = switch_state.monitor.clone();
            result_tx
                .send(switch_database_with(
                    &switch_state,
                    &switch_path,
                    &switch_id,
                    |_| {
                        paused_tx.send(monitor.is_paused()).unwrap();
                        continue_rx.recv().unwrap();
                        Ok(())
                    },
                    |_| Ok(()),
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
        assert!(paused_rx.recv().unwrap());
        continue_tx.send(()).unwrap();
        result_rx.recv().unwrap().unwrap();
        switch.join().unwrap();
        assert!(!state.monitor.is_paused());
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
        assert!(!paused_while_waiting);
    }

    #[test]
    fn concurrent_switch_serializes_and_reloads_latest_config() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("concurrent");
        let (state, config_path) = state_and_config(&root);
        let target =
            create_database_at(&state, &config_path, "目标", &root.join("target")).unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let state = state.clone();
                let config_path = config_path.clone();
                let id = target.id.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    switch_database_with(
                        &state,
                        &config_path,
                        &id,
                        |registration| {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            AppConfig::try_update_at(&config_path, |config| {
                                config.active_database_id = Some(registration.id.clone());
                                config.data_path = Some(registration.path.clone());
                                Ok(())
                            })
                        },
                        |_| Ok(()),
                    )
                })
            })
            .collect();
        barrier.wait();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| {
                    result
                        .as_ref()
                        .err()
                        .is_some_and(|error| error.starts_with("DATABASE:ALREADY_ACTIVE:"))
                })
                .count(),
            1
        );
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_switches_load_config_only_after_acquiring_switch_mutex() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("fresh-switch-config");
        let (state, config_path) = state_and_config(&root);
        let target =
            create_database_at(&state, &config_path, "目标", &root.join("target")).unwrap();
        let (saved_tx, saved_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first_state = state.clone();
        let first_path = config_path.clone();
        let first_id = target.id.clone();
        let first = std::thread::spawn(move || {
            switch_database_with(
                &first_state,
                &first_path,
                &first_id,
                |registration| {
                    AppConfig::try_update_at(&first_path, |config| {
                        config.active_database_id = Some(registration.id.clone());
                        config.data_path = Some(registration.path.clone());
                        Ok(())
                    })?;
                    saved_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                },
                |_| Ok(()),
            )
        });
        saved_rx.recv().unwrap();

        let second_state = state.clone();
        let second_path = config_path.clone();
        let second_id = target.id.clone();
        let (second_tx, second_rx) = std::sync::mpsc::channel();
        let second = std::thread::spawn(move || {
            let result = switch_database_with(
                &second_state,
                &second_path,
                &second_id,
                |registration| {
                    AppConfig::try_update_at(&second_path, |config| {
                        config.active_database_id = Some(registration.id.clone());
                        config.data_path = Some(registration.path.clone());
                        Ok(())
                    })
                },
                |_| Ok(()),
            );
            second_tx.send(result).unwrap();
        });
        assert!(
            second_rx
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        release_tx.send(()).unwrap();
        first.join().unwrap().unwrap();
        let second_error = second_rx.recv().unwrap().unwrap_err();
        assert!(second_error.starts_with("DATABASE:ALREADY_ACTIVE:"));
        second.join().unwrap();

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adding_existing_database_runs_migrations() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("migration");
        let (state, config_path) = state_and_config(&root);
        let legacy = root.join("legacy");
        std::fs::create_dir_all(&legacy).unwrap();
        let conn = rusqlite::Connection::open(legacy.join("clipboard.db")).unwrap();
        conn.execute_batch(crate::database::SCHEMA_SQL).unwrap();
        conn.execute_batch("ALTER TABLE clipboard_items DROP COLUMN file_payload;")
            .unwrap();
        drop(conn);

        add_existing_database_at(&state, &config_path, "旧库", &legacy).unwrap();
        let conn = rusqlite::Connection::open(legacy.join("clipboard.db")).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('clipboard_items') WHERE name='file_payload'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
        drop(conn);
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_info_and_list_never_observe_config_before_active_swap() {
        let _serial = TEST_LOCK.lock();
        let root = temp_root("consistent-read");
        let (state, config_path) = state_and_config(&root);
        let target =
            create_database_at(&state, &config_path, "目标", &root.join("target")).unwrap();
        let updated = AppConfig::try_load_from(&config_path).unwrap();
        let (saved_tx, saved_rx) = std::sync::mpsc::channel();
        let (continue_tx, continue_rx) = std::sync::mpsc::channel();
        let switch_state = state.clone();
        let switch_path = config_path.clone();
        let switch_id = target.id.clone();
        let switch_thread = std::thread::spawn(move || {
            switch_database_with(
                &switch_state,
                &switch_path,
                &switch_id,
                |registration| {
                    let mut updated = updated;
                    updated.active_database_id = Some(registration.id.clone());
                    updated.data_path = Some(registration.path.clone());
                    updated.save_to(&switch_path)?;
                    saved_tx.send(()).unwrap();
                    continue_rx.recv().unwrap();
                    Ok(())
                },
                |_| Ok(()),
            )
        });
        saved_rx.recv().unwrap();

        let reader_state = state.clone();
        let reader_path = config_path.clone();
        let (reader_tx, reader_rx) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            reader_tx
                .send((
                    get_active_database_info_from(&reader_state, &reader_path),
                    list_databases_from(&reader_state, &reader_path),
                ))
                .unwrap();
        });
        let observed_early = reader_rx
            .recv_timeout(std::time::Duration::from_millis(50))
            .ok();
        let was_early = observed_early.is_some();
        continue_tx.send(()).unwrap();
        switch_thread.join().unwrap().unwrap();
        let (active, list) = observed_early.unwrap_or_else(|| reader_rx.recv().unwrap());
        reader.join().unwrap();

        let active = active.unwrap();
        let list = list.unwrap();
        assert!(!was_early);
        assert_eq!(active.id, target.id);
        assert_eq!(PathBuf::from(active.path), PathBuf::from(&target.path));
        let listed = list.iter().find(|database| database.is_active).unwrap();
        assert_eq!(listed.id, target.id);
        assert_eq!(listed.path, target.path);
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }
}
