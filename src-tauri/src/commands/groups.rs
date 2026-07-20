use crate::database::{ClipboardRepository, Group, GroupRepository};
use std::sync::Arc;
use tauri::State;
use tracing::debug;

use super::AppState;

#[cfg(test)]
static DELETE_GROUP_TEST_HOOK: parking_lot::Mutex<Option<Arc<dyn Fn() + Send + Sync>>> =
    parking_lot::Mutex::new(None);

/// 获取所有自定义分组（含条目数）
#[tauri::command]
pub async fn get_groups(state: State<'_, Arc<AppState>>) -> Result<Vec<Group>, String> {
    let _operation = state.database_operation.read();
    let repo = GroupRepository::new(&state.db);
    repo.list_with_count().map_err(|e| e.to_string())
}

/// 创建自定义分组，返回完整分组对象
#[tauri::command]
pub async fn create_group(
    state: State<'_, Arc<AppState>>,
    name: String,
    color: Option<String>,
) -> Result<Group, String> {
    let _operation = state.database_operation.read();
    let repo = GroupRepository::new(&state.db);
    repo.create(&name, color.as_deref())
        .map_err(|e| e.to_string())
}

/// 重命名分组
#[tauri::command]
pub async fn rename_group(
    state: State<'_, Arc<AppState>>,
    id: i64,
    name: String,
) -> Result<(), String> {
    let _operation = state.database_operation.read();
    let repo = GroupRepository::new(&state.db);
    repo.rename(id, &name).map_err(|e| e.to_string())
}

/// 更新分组颜色（传 None 清除颜色）
#[tauri::command]
pub async fn update_group_color(
    state: State<'_, Arc<AppState>>,
    id: i64,
    color: Option<String>,
) -> Result<(), String> {
    let _operation = state.database_operation.read();
    let repo = GroupRepository::new(&state.db);
    repo.update_color(id, color.as_deref())
        .map_err(|e| e.to_string())
}

/// 删除分组（ON DELETE CASCADE 自动删除该分组的所有 clipboard_items）
#[tauri::command]
pub async fn delete_group(state: State<'_, Arc<AppState>>, id: i64) -> Result<(), String> {
    delete_group_in(&state.db, id)
}

fn delete_group_in(db: &crate::database::Database, id: i64) -> Result<(), String> {
    let operation = db.operation_lock();
    let _operation = operation.read();
    let clipboard_repo = ClipboardRepository::new(db);
    let image_paths = clipboard_repo
        .get_image_paths_by_group(id)
        .map_err(|e| e.to_string())?;
    let file_payloads = clipboard_repo
        .get_file_payloads_by_group(id)
        .map_err(|e| e.to_string())?;

    #[cfg(test)]
    if let Some(hook) = DELETE_GROUP_TEST_HOOK.lock().clone() {
        hook();
    }

    let repo = GroupRepository::new(db);
    repo.delete(id).map_err(|e| e.to_string())?;

    crate::clipboard::cleanup_deleted_assets(&image_paths, &file_payloads);
    debug!("Deleted group {} ({} image files)", id, image_paths.len());
    Ok(())
}

/// 将条目移动到指定分组（None = 移回默认分组）
#[tauri::command]
pub async fn move_item_to_group(
    state: State<'_, Arc<AppState>>,
    item_id: i64,
    group_id: Option<i64>,
) -> Result<(), String> {
    let _operation = state.database_operation.read();
    let repo = GroupRepository::new(&state.db);
    repo.move_item_to_group(item_id, group_id)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{DELETE_GROUP_TEST_HOOK, delete_group_in};
    use crate::database::{
        ClipboardRepository, ContentType, Database, GroupRepository, NewClipboardItem, QueryOptions,
    };
    use std::sync::{Arc, Barrier, mpsc};

    #[test]
    fn delete_group_holds_operation_guard_through_asset_cleanup() {
        let root = std::env::temp_dir().join(format!(
            "ec-group-operation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db =
            Database::new_with_settings(root.join("old/clipboard.db"), root.join("settings.db"))
                .unwrap();
        let old_path = db.active_snapshot().db_path;
        let group_id = GroupRepository::new(&db)
            .create("old group", None)
            .unwrap()
            .id;
        let old_asset = root.join("old/images/group.png");
        std::fs::create_dir_all(old_asset.parent().unwrap()).unwrap();
        std::fs::write(&old_asset, b"old").unwrap();
        ClipboardRepository::new(&db)
            .insert(NewClipboardItem {
                content_type: ContentType::Text,
                text_content: Some("old".into()),
                content_hash: "old-group".into(),
                semantic_hash: "old-group".into(),
                image_path: Some(old_asset.to_string_lossy().into_owned()),
                group_id: Some(group_id),
                ..Default::default()
            })
            .unwrap();

        let target_db = Database::new_with_settings(
            root.join("new/clipboard.db"),
            root.join("target-settings.db"),
        )
        .unwrap();
        ClipboardRepository::new(&target_db)
            .insert(NewClipboardItem {
                content_type: ContentType::Text,
                text_content: Some("new".into()),
                content_hash: "new-group".into(),
                semantic_hash: "new-group".into(),
                ..Default::default()
            })
            .unwrap();
        let target = db
            .open_active(target_db.active_snapshot().data_dir)
            .unwrap();

        let midpoint = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        *DELETE_GROUP_TEST_HOOK.lock() = Some({
            let midpoint = midpoint.clone();
            let release = release.clone();
            Arc::new(move || {
                midpoint.wait();
                release.wait();
            })
        });

        let delete_db = db.clone();
        let delete = std::thread::spawn(move || delete_group_in(&delete_db, group_id));
        midpoint.wait();
        let switch_db = db.clone();
        let (switched_tx, switched_rx) = mpsc::channel();
        let switch = std::thread::spawn(move || {
            let operation = switch_db.operation_lock();
            let _operation = operation.write();
            drop(switch_db.swap_active(target));
            switched_tx.send(()).unwrap();
        });
        assert!(
            switched_rx
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        release.wait();
        delete.join().unwrap().unwrap();
        switch.join().unwrap();

        let reopened_old =
            Database::new_with_settings(old_path, root.join("reopen-settings.db")).unwrap();
        assert_eq!(
            ClipboardRepository::new(&reopened_old)
                .count(QueryOptions::default())
                .unwrap(),
            0
        );
        assert!(
            GroupRepository::new(&reopened_old)
                .list_with_count()
                .unwrap()
                .is_empty()
        );
        assert!(!old_asset.exists());
        assert_eq!(
            ClipboardRepository::new(&target_db)
                .count(QueryOptions::default())
                .unwrap(),
            1
        );

        *DELETE_GROUP_TEST_HOOK.lock() = None;
        drop(reopened_old);
        drop(target_db);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
