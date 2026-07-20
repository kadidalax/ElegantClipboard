use crate::database::{ClipboardDeleteError, ClipboardItem, ClipboardRepository};
use clipboard_rs::Clipboard as ClipboardTrait;
use std::sync::Arc;
use tauri::State;
use tracing::{debug, info};

use super::{AppState, hide_main_window_if_not_pinned, with_paused_monitor};

const ITEM_LOCKED_ERROR: &str = "CLIPBOARD:ITEM_LOCKED";
const ITEM_NOT_FOUND_ERROR: &str = "CLIPBOARD:ITEM_NOT_FOUND";

fn map_delete_error(error: ClipboardDeleteError) -> String {
    match error {
        ClipboardDeleteError::Locked => ITEM_LOCKED_ERROR.to_string(),
        ClipboardDeleteError::NotFound => ITEM_NOT_FOUND_ERROR.to_string(),
        ClipboardDeleteError::Database(error) => error.to_string(),
    }
}

/// 将 ClipboardItem 内容写入系统剪贴板（保留 HTML/RTF 等格式）
pub(super) fn set_clipboard_content(
    item: &ClipboardItem,
    clipboard: &mut clipboard_rs::ClipboardContext,
) -> Result<(), String> {
    crate::clipboard::format_write::write_item_to_clipboard(item, clipboard)
}

/// 提取以 keyword 首次出现为中心的上下文片段（`...前缀 关键词 后缀...`）。
/// 快速路径 O(n)：整体小写后字节级搜索转字符索引（CJK/ASCII 通用）。
/// 回退路径 O(n*k)：逐字符滑动窗口（处理小写化会改变字节长度的稀有 Unicode）。
fn extract_keyword_context(text: &str, keyword: &str, max_len: usize) -> String {
    let keyword_lower = keyword.to_lowercase();

    let text_lower = text.to_lowercase();
    let keyword_char_pos = if let Some(byte_pos) = text_lower.find(&keyword_lower) {
        let char_idx_in_lower = text_lower[..byte_pos].chars().count();
        let mut ci = text.char_indices().skip(char_idx_in_lower);
        let valid = if let Some((bs, _)) = ci.next() {
            let kw_char_len = keyword_lower.chars().count();
            let be = ci
                .nth(kw_char_len.saturating_sub(1))
                .map_or(text.len(), |(b, _)| b);
            text.get(bs..be)
                .is_some_and(|s| s.to_lowercase() == keyword_lower)
        } else {
            false
        };
        if valid {
            Some(char_idx_in_lower)
        } else {
            find_keyword_char_pos_slow(text, &keyword_lower)
        }
    } else {
        None
    };

    let keyword_char_len = keyword_lower.chars().count();
    let Some(keyword_char_pos) = keyword_char_pos else {
        return text.chars().take(max_len).collect();
    };

    build_context_snippet(text, keyword_char_pos, keyword_char_len, max_len)
}

/// 慢速回退：O(n*k) 滑动窗口定位关键词字符位置（仅用于稀有 Unicode 场景）。
fn find_keyword_char_pos_slow(text: &str, keyword_lower: &str) -> Option<usize> {
    let keyword_char_len = keyword_lower.chars().count();
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
    let n = char_indices.len();
    for i in 0..n {
        if i + keyword_char_len > n {
            break;
        }
        let bs = char_indices[i].0;
        let be = if i + keyword_char_len < n {
            char_indices[i + keyword_char_len].0
        } else {
            text.len()
        };
        if text[bs..be].to_lowercase() == *keyword_lower {
            return Some(i);
        }
    }
    None
}

/// 根据字符级位置信息构建上下文片段。
fn build_context_snippet(
    text: &str,
    keyword_char_pos: usize,
    keyword_char_len: usize,
    max_len: usize,
) -> String {
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
    let text_char_count = char_indices.len();

    let context_before = max_len / 3;
    let start_char = keyword_char_pos.saturating_sub(context_before);
    let end_char =
        (keyword_char_pos + keyword_char_len + max_len - context_before).min(text_char_count);

    if end_char <= start_char {
        return text.chars().take(max_len).collect();
    }

    let byte_start = char_indices[start_char].0;
    let byte_end = if end_char < text_char_count {
        char_indices[end_char].0
    } else {
        text.len()
    };

    let slice = &text[byte_start..byte_end];
    let mut result = String::with_capacity(slice.len() + 6);
    if start_char > 0 {
        result.push_str("...");
    }
    result.push_str(slice);
    if end_char < text_char_count {
        result.push_str("...");
    }
    result
}

#[cfg(target_os = "windows")]
mod win_keyboard {
    use tracing::info;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
        SendInput,
    };

    pub fn is_key_pressed(vk: u16) -> bool {
        unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
    }

    pub fn send_key(vk: u16, up: bool) {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }

    /// 若用户正按住修饰键则释放，最多重试 20 次（间隔 5ms）。
    pub fn release_if_held(vk: u16) {
        for _ in 0..20 {
            if !is_key_pressed(vk) {
                return;
            }
            send_key(vk, true);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    pub fn log_foreground_window(action: &str) {
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
        let fg = unsafe { GetForegroundWindow() };
        let mut buf = [0u16; 256];
        let len = unsafe { GetWindowTextW(fg, &mut buf) } as usize;
        let title = String::from_utf16_lossy(&buf[..len]);
        info!("{action}: foreground hwnd={:?} title=\"{title}\"", fg.0);
    }
}

/// 使用 Windows SendInput API 模拟 Ctrl+组合键。
/// 先释放用户可能按住的所有修饰键（Alt/Shift/Win），再发送纯净的组合键。
#[cfg(target_os = "windows")]
fn simulate_ctrl_combo(key_vk: u16, action: &str) -> Result<(), String> {
    use win_keyboard::{is_key_pressed, log_foreground_window, release_if_held, send_key};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    log_foreground_window(action);

    release_if_held(VK_MENU.0);
    release_if_held(VK_SHIFT.0);
    release_if_held(VK_LWIN.0);
    release_if_held(VK_RWIN.0);

    let user_ctrl = is_key_pressed(VK_CONTROL.0);
    if !user_ctrl {
        send_key(VK_CONTROL.0, false);
    }
    send_key(key_vk, false);
    std::thread::sleep(std::time::Duration::from_millis(8));
    send_key(key_vk, true);
    if !user_ctrl {
        send_key(VK_CONTROL.0, true);
    }

    Ok(())
}

pub const PASTE_KEY_SETTING: &str = "paste_key";
pub const PASTE_KEY_CTRL_V: &str = "ctrl_v";
pub const PASTE_KEY_SHIFT_INSERT: &str = "shift_insert";

/// 按用户设置模拟粘贴按键（未知值回退为 Ctrl+V）。
#[cfg(target_os = "windows")]
pub fn simulate_paste_by_key(paste_key: &str) -> Result<(), String> {
    if paste_key == PASTE_KEY_SHIFT_INSERT {
        simulate_shift_insert()
    } else {
        simulate_paste()
    }
}

/// 使用 Windows SendInput API 模拟 Ctrl+V 粘贴。
/// 先释放用户可能按住的所有修饰键（Alt/Shift/Win），再发送纯净的 Ctrl+V。
#[cfg(target_os = "windows")]
pub fn simulate_paste() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_V;
    simulate_ctrl_combo(VK_V.0, "simulate_paste")
}

/// 使用 Windows SendInput API 模拟 Shift+Insert 粘贴。
/// 先释放 Ctrl/Alt/Win，保留或补按 Shift 后发送 Insert。
#[cfg(target_os = "windows")]
fn simulate_shift_insert() -> Result<(), String> {
    use win_keyboard::{is_key_pressed, log_foreground_window, release_if_held, send_key};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_CONTROL, VK_INSERT, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    log_foreground_window("simulate_shift_insert");

    release_if_held(VK_MENU.0);
    release_if_held(VK_CONTROL.0);
    release_if_held(VK_LWIN.0);
    release_if_held(VK_RWIN.0);

    let user_shift = is_key_pressed(VK_SHIFT.0);
    if !user_shift {
        send_key(VK_SHIFT.0, false);
    }
    send_key(VK_INSERT.0, false);
    std::thread::sleep(std::time::Duration::from_millis(8));
    send_key(VK_INSERT.0, true);
    if !user_shift {
        send_key(VK_SHIFT.0, true);
    }

    Ok(())
}

/// 使用 Windows SendInput API 模拟 Ctrl+C 复制选中文字。
#[cfg(target_os = "windows")]
pub fn simulate_copy() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_C;
    simulate_ctrl_combo(VK_C.0, "simulate_copy")
}

/// 获取剪贴板条目（支持可选过滤）
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn get_clipboard_items(
    state: State<'_, Arc<AppState>>,
    search: Option<String>,
    content_type: Option<String>,
    pinned_only: Option<bool>,
    favorite_only: Option<bool>,
    group_id: Option<i64>,
    timeline: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<ClipboardItem>, String> {
    use crate::database::QueryOptions;

    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let search_keyword = search.clone();
    let options = QueryOptions {
        search,
        content_type,
        pinned_only: pinned_only.unwrap_or(false),
        favorite_only: favorite_only.unwrap_or(false),
        group_id,
        timeline: timeline.unwrap_or(false),
        limit,
        offset,
    };
    let mut items = repo.list(options).map_err(|e| e.to_string())?;
    if let Some(ref keyword) = search_keyword {
        let keyword_lower = keyword.to_lowercase();
        for item in &mut items {
            if let Some(ref text) = item.text_content {
                let preview_has_match = item
                    .preview
                    .as_ref()
                    .is_some_and(|p| p.to_lowercase().contains(&keyword_lower));
                if !preview_has_match {
                    item.preview = Some(extract_keyword_context(text, keyword, 200));
                }
            }
            item.text_content = None;
        }
    }
    Ok(items)
}

/// 按 ID 获取剪贴板条目
#[tauri::command]
pub async fn get_clipboard_item(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<Option<ClipboardItem>, String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    repo.get_by_id(id).map_err(|e| e.to_string())
}

/// 获取条目总数
#[tauri::command]
pub async fn get_clipboard_count(
    state: State<'_, Arc<AppState>>,
    content_type: Option<String>,
    pinned_only: Option<bool>,
    favorite_only: Option<bool>,
    group_id: Option<i64>,
) -> Result<i64, String> {
    use crate::database::QueryOptions;

    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let options = QueryOptions {
        content_type,
        pinned_only: pinned_only.unwrap_or(false),
        favorite_only: favorite_only.unwrap_or(false),
        group_id,
        ..Default::default()
    };
    repo.count(options).map_err(|e| e.to_string())
}

/// 切换固定状态
#[tauri::command]
pub async fn toggle_pin(state: State<'_, Arc<AppState>>, id: i64) -> Result<bool, String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let new_state = repo.toggle_pin(id).map_err(|e| e.to_string())?;
    debug!("Toggle pin: id={}, pinned={}", id, new_state);
    Ok(new_state)
}

/// 切换锁定状态
#[tauri::command]
pub async fn toggle_lock(state: State<'_, Arc<AppState>>, id: i64) -> Result<bool, String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let new_state = repo.toggle_lock(id).map_err(|e| e.to_string())?;
    debug!("Toggle lock: id={}, locked={}", id, new_state);
    Ok(new_state)
}

/// 切换收藏状态
#[tauri::command]
pub async fn toggle_favorite(state: State<'_, Arc<AppState>>, id: i64) -> Result<bool, String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let new_state = repo.toggle_favorite(id).map_err(|e| e.to_string())?;
    debug!("Toggle favorite: id={}, favorite={}", id, new_state);
    Ok(new_state)
}

/// 与目标条目交换排序位置
#[tauri::command]
pub async fn move_clipboard_item(
    state: State<'_, Arc<AppState>>,
    from_id: i64,
    to_id: i64,
) -> Result<(), String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    repo.move_item_by_id(from_id, to_id)
        .map_err(|e| e.to_string())?;
    debug!("Moved clipboard item {} to position of {}", from_id, to_id);
    Ok(())
}

/// 与目标收藏条目交换收藏排序位置
#[tauri::command]
pub async fn move_favorite_clipboard_item(
    state: State<'_, Arc<AppState>>,
    from_id: i64,
    to_id: i64,
) -> Result<(), String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    repo.move_favorite_item_by_id(from_id, to_id)
        .map_err(|e| e.to_string())?;
    debug!(
        "Moved favorite clipboard item {} to position of {}",
        from_id, to_id
    );
    Ok(())
}

/// 粘贴后置顶：将条目移到非置顶区最前面（sort_order 设为全表最大值 + 1）
#[tauri::command]
pub async fn bump_item_to_top(state: State<'_, Arc<AppState>>, id: i64) -> Result<(), String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    repo.bump_to_top(id).map_err(|e| e.to_string())?;
    debug!("Bumped clipboard item {} to top", id);
    Ok(())
}

fn delete_clipboard_item_in_repo(
    repo: &ClipboardRepository,
    id: i64,
) -> Result<ClipboardItem, String> {
    repo.delete_unlocked(id).map_err(map_delete_error)
}

fn batch_delete_clipboard_items_in_repo(
    repo: &ClipboardRepository,
    ids: &[i64],
) -> Result<(i64, Vec<String>, Vec<String>), String> {
    repo.batch_delete(ids).map_err(map_delete_error)
}

fn delete_empty_text_item_in_repo(repo: &ClipboardRepository, id: i64) -> Result<(), String> {
    let item = delete_clipboard_item_in_repo(repo, id)?;
    let payloads: Vec<String> = item.file_payload.map(|p| vec![p]).unwrap_or_default();
    crate::clipboard::cleanup_deleted_assets(
        &item.image_path.map(|p| vec![p]).unwrap_or_default(),
        &payloads,
    );
    Ok(())
}

#[cfg(test)]
static CLEAR_ALL_HISTORY_TEST_HOOK: parking_lot::Mutex<Option<Arc<dyn Fn() + Send + Sync>>> =
    parking_lot::Mutex::new(None);

#[cfg(test)]
static CLEAR_HISTORY_TEST_HOOK: parking_lot::Mutex<Option<Arc<dyn Fn() + Send + Sync>>> =
    parking_lot::Mutex::new(None);

#[cfg(test)]
fn run_clear_test_hook(hook: &parking_lot::Mutex<Option<Arc<dyn Fn() + Send + Sync>>>) {
    if let Some(hook) = hook.lock().clone() {
        hook();
    }
}

/// 删除剪贴板条目（同时删除关联图片文件）
#[tauri::command]
pub async fn delete_clipboard_item(state: State<'_, Arc<AppState>>, id: i64) -> Result<(), String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);

    let item = delete_clipboard_item_in_repo(&repo, id)?;
    let payloads: Vec<String> = item.file_payload.map(|p| vec![p]).unwrap_or_default();
    crate::clipboard::cleanup_deleted_assets(
        &item.image_path.map(|p| vec![p]).unwrap_or_default(),
        &payloads,
    );
    debug!(
        "Deleted clipboard item: id={}, type={}",
        id, item.content_type
    );

    Ok(())
}

/// 批量删除剪贴板条目（同时删除关联图片文件）
#[tauri::command]
pub async fn batch_delete_clipboard_items(
    state: State<'_, Arc<AppState>>,
    ids: Vec<i64>,
) -> Result<i64, String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let (deleted, image_paths, file_payloads) = batch_delete_clipboard_items_in_repo(&repo, &ids)?;
    crate::clipboard::cleanup_deleted_assets(&image_paths, &file_payloads);
    debug!("Batch deleted {} clipboard items", deleted);
    Ok(deleted)
}

/// 清空所有历史（包括置顶/收藏/锁定，同时删除图片文件）
#[tauri::command]
pub async fn clear_all_history(state: State<'_, Arc<AppState>>) -> Result<i64, String> {
    clear_all_history_in(&state.db)
}

fn clear_all_history_in(db: &crate::database::Database) -> Result<i64, String> {
    use tracing::info;

    let operation = db.operation_lock();
    let _operation = operation.read();
    let repo = ClipboardRepository::new(db);
    let image_paths = repo.get_all_image_paths().unwrap_or_default();
    let file_payloads = repo.get_all_file_payloads().unwrap_or_default();
    #[cfg(test)]
    run_clear_test_hook(&CLEAR_ALL_HISTORY_TEST_HOOK);
    let deleted = repo.clear_all().map_err(|e| e.to_string())?;
    crate::clipboard::cleanup_deleted_assets(&image_paths, &file_payloads);
    db.vacuum().ok();

    info!(
        "Cleared all {} clipboard items ({} image files)",
        deleted,
        image_paths.len()
    );
    Ok(deleted)
}

/// 清空所有非固定/非收藏/非锁定历史（同时删除图片文件），按分组
#[tauri::command]
pub async fn clear_history(
    state: State<'_, Arc<AppState>>,
    group_id: Option<i64>,
    content_type: Option<String>,
) -> Result<i64, String> {
    clear_history_in(&state.db, group_id, content_type.as_deref())
}

fn clear_history_in(
    db: &crate::database::Database,
    group_id: Option<i64>,
    content_type: Option<&str>,
) -> Result<i64, String> {
    use tracing::info;

    let operation = db.operation_lock();
    let _operation = operation.read();
    let repo = ClipboardRepository::new(db);
    #[cfg(test)]
    run_clear_test_hook(&CLEAR_HISTORY_TEST_HOOK);
    let (deleted, image_paths, file_payloads) = repo
        .clear_history(group_id, content_type)
        .map_err(|e| e.to_string())?;
    crate::clipboard::cleanup_deleted_assets(&image_paths, &file_payloads);

    info!(
        "Cleared {} clipboard items ({} image files) (group: {:?}, content_type: {:?})",
        deleted,
        image_paths.len(),
        group_id,
        content_type
    );
    Ok(deleted)
}

/// 设置当前活动分组（None = 默认分组）
#[tauri::command]
pub async fn set_active_group(
    state: State<'_, Arc<AppState>>,
    group_id: Option<i64>,
) -> Result<(), String> {
    let _operation = state.database_operation.read();
    *state.active_group_id.lock() = group_id;
    debug!("Active group set to: {:?}", group_id);
    Ok(())
}

/// 更新剪贴板条目的文本内容，内容为空时删除并返回 true
#[tauri::command]
pub async fn update_text_content(
    state: State<'_, Arc<AppState>>,
    id: i64,
    new_text: String,
) -> Result<bool, String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    if new_text.trim().is_empty() {
        delete_empty_text_item_in_repo(&repo, id)?;
        debug!("Deleted empty item {}", id);
        Ok(true)
    } else {
        if let Ok(Some(item)) = repo.get_by_id(id) {
            let payloads: Vec<String> = item.file_payload.map(|p| vec![p]).unwrap_or_default();
            repo.update_text_content(id, &new_text)
                .map_err(|e| e.to_string())?;
            if !payloads.is_empty() {
                crate::clipboard::cleanup_deleted_assets(&[], &payloads);
            }
        } else {
            repo.update_text_content(id, &new_text)
                .map_err(|e| e.to_string())?;
        }
        debug!("Updated text content for item {}", id);
        Ok(false)
    }
}

/// 将条目复制到系统剪贴板
#[tauri::command]
pub async fn copy_to_clipboard(state: State<'_, Arc<AppState>>, id: i64) -> Result<(), String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let item = repo
        .get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;

    let result = with_paused_monitor(&state, || {
        let mut clipboard = clipboard_rs::ClipboardContext::new()
            .map_err(|e| format!("Failed to access clipboard: {e}"))?;
        set_clipboard_content(&item, &mut clipboard)?;
        debug!("Copied item {} to clipboard", id);
        Ok(())
    });
    if let Err(ref e) = result {
        tracing::warn!(id, error = %e, content_type = %item.content_type, "copy_to_clipboard failed");
    }
    result
}

/// 直接粘贴剪贴板条目（写入系统剪贴板 → 隐藏窗口 → 模拟 Ctrl+V）
#[tauri::command]
pub async fn paste_content(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
    close_window: Option<bool>,
) -> Result<(), String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let item = repo
        .get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;

    paste_item_to_active_window(&state, &app, &item, close_window.unwrap_or(true))?;
    debug!("Pasted item {} to active window", id);
    Ok(())
}

/// 以纯文本粘贴条目内容（去除 html/rtf 格式）
#[tauri::command]
pub async fn paste_content_as_plain(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
    close_window: Option<bool>,
) -> Result<(), String> {
    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let item = repo
        .get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;

    let text = crate::clipboard::format_write::item_plain_text(&item)?;

    paste_plain_text_to_active_window(&state, &app, &text, close_window.unwrap_or(true))?;
    debug!("Pasted item {} as plain text", id);
    Ok(())
}

/// 将任意文本直接粘贴到当前活动窗口（用于表情、片段等功能）
#[tauri::command]
pub async fn paste_text_direct(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    text: String,
) -> Result<(), String> {
    paste_plain_text_to_active_window(&state, &app, &text, true)?;
    debug!("Pasted direct text ({} chars)", text.len());
    Ok(())
}

/// 合并粘贴：将多条记录的文本内容合并后粘贴
#[tauri::command]
pub async fn merge_paste_content(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    ids: Vec<i64>,
    separator: Option<String>,
) -> Result<(), String> {
    if ids.is_empty() {
        return Err("No items selected".to_string());
    }

    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let sep = separator.as_deref().unwrap_or("\n");

    let mut items: Vec<ClipboardItem> = Vec::with_capacity(ids.len());
    for id in &ids {
        let item = repo
            .get_by_id(*id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Item {id} not found"))?;
        items.push(item);
    }

    with_paused_monitor(&state, || {
        let mut clipboard = clipboard_rs::ClipboardContext::new()
            .map_err(|e| format!("Failed to access clipboard: {e}"))?;
        crate::clipboard::merge_paste::merge_items_to_clipboard(&items, sep, &mut clipboard)?;

        super::hide_preview_windows(&app);
        hide_main_window_if_not_pinned(&app);

        std::thread::sleep(std::time::Duration::from_millis(50));
        super::run_simulate_paste_with_sound(&app)?;
        super::hide_preview_windows(&app);

        debug!("Merge pasted {} items", items.len());
        Ok(())
    })
}

/// 粘贴快速槽位（1-9）对应条目到活动窗口。
pub fn quick_paste_by_slot(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    slot: u8,
) -> Result<(), String> {
    if !(1..=10).contains(&slot) {
        return Err("Quick paste slot must be between 1 and 10".to_string());
    }

    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let active_group = *state.active_group_id.lock();
    let item = repo
        .get_by_position((slot - 1) as usize, active_group)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No clipboard item available for slot {slot}"))?;

    paste_item_to_active_window(state, app, &item, true)?;
    debug!("Quick pasted slot {} with item {}", slot, item.id);
    Ok(())
}

/// 粘贴收藏快速槽位（1-9）对应条目到活动窗口。
pub fn quick_paste_favorite_by_slot(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    slot: u8,
) -> Result<(), String> {
    if !(1..=10).contains(&slot) {
        return Err("收藏槽位必须在 1-10 之间".to_string());
    }

    let _operation = state.database_operation.read();
    let repo = ClipboardRepository::new(&state.db);
    let active_group = *state.active_group_id.lock();
    let item = repo
        .get_favorite_by_position((slot - 1) as usize, active_group)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("收藏槽位 {slot} 没有可用的收藏条目"))?;

    paste_item_to_active_window(state, app, &item, true)?;
    debug!("Quick pasted favorite slot {} with item {}", slot, item.id);
    Ok(())
}

/// 公共粘贴执行：写剪贴板 → 隐藏窗口 → 模拟粘贴
fn execute_paste_flow<F>(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    close_window: bool,
    log_label: &str,
    write_fn: F,
) -> Result<(), String>
where
    F: FnOnce(&mut clipboard_rs::ClipboardContext) -> Result<(), String>,
{
    with_paused_monitor(state, || {
        let mut clipboard = clipboard_rs::ClipboardContext::new()
            .map_err(|e| format!("Failed to access clipboard: {e}"))?;
        write_fn(&mut clipboard)?;
        debug!("{log_label}: clipboard set ok");

        super::hide_preview_windows(app);

        if close_window {
            hide_main_window_if_not_pinned(app);
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
        super::run_simulate_paste_with_sound(app)?;

        super::hide_preview_windows(app);

        debug!("{log_label}: simulate_paste ok");
        Ok(())
    })
}

fn paste_item_to_active_window(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    item: &ClipboardItem,
    close_window: bool,
) -> Result<(), String> {
    info!("paste_item: id={}, close_window={}", item.id, close_window);
    execute_paste_flow(state, app, close_window, "paste_item", |clipboard| {
        set_clipboard_content(item, clipboard)
    })
}

/// 纯文本粘贴：写剪贴板 → 隐藏窗口 → 模拟 Ctrl+V
fn paste_plain_text_to_active_window(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    text: &str,
    close_window: bool,
) -> Result<(), String> {
    info!(
        "paste_plain_text: len={}, close_window={}",
        text.len(),
        close_window
    );
    let text = text.to_string();
    execute_paste_flow(
        state,
        app,
        close_window,
        "paste_plain_text",
        move |clipboard| {
            clipboard
                .set_text(text)
                .map_err(|e| format!("Failed to set clipboard text: {e}"))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CLEAR_ALL_HISTORY_TEST_HOOK, CLEAR_HISTORY_TEST_HOOK, ITEM_LOCKED_ERROR,
        batch_delete_clipboard_items_in_repo, clear_all_history_in, clear_history_in,
        delete_clipboard_item_in_repo, delete_empty_text_item_in_repo, extract_keyword_context,
    };
    use crate::database::{
        ClipboardRepository, ContentType, Database, NewClipboardItem, QueryOptions,
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Barrier, mpsc};

    fn temp_repo() -> ClipboardRepository {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "elegant_clipboard_command_test_{}_{}.db",
            std::process::id(),
            n
        ));
        let db = Database::new(path).unwrap();
        ClipboardRepository::new(&db)
    }

    fn text_item(text: &str, is_locked: bool) -> NewClipboardItem {
        NewClipboardItem {
            content_type: ContentType::Text,
            text_content: Some(text.to_string()),
            preview: Some(text.to_string()),
            content_hash: text.to_string(),
            semantic_hash: text.to_string(),
            is_locked,
            ..Default::default()
        }
    }

    fn temp_asset(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "elegant_clipboard_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"asset").unwrap();
        path
    }

    fn assert_history_clear_blocks_switch(clear_all: bool) {
        let root = std::env::temp_dir().join(format!(
            "ec-command-operation-{}-{}-{}",
            if clear_all { "all" } else { "history" },
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
        let old_repo = ClipboardRepository::new(&db);
        let old_asset = root.join("old/images/old.png");
        std::fs::create_dir_all(old_asset.parent().unwrap()).unwrap();
        std::fs::write(&old_asset, b"old").unwrap();
        let mut old_item = text_item("old", false);
        old_item.image_path = Some(old_asset.to_string_lossy().into_owned());
        old_repo.insert(old_item).unwrap();

        let target_db = Database::new_with_settings(
            root.join("new/clipboard.db"),
            root.join("target-settings.db"),
        )
        .unwrap();
        let target_repo = ClipboardRepository::new(&target_db);
        target_repo.insert(text_item("new", false)).unwrap();
        let target = db
            .open_active(target_db.active_snapshot().data_dir)
            .unwrap();

        let midpoint = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let hook: Arc<dyn Fn() + Send + Sync> = {
            let midpoint = midpoint.clone();
            let release = release.clone();
            Arc::new(move || {
                midpoint.wait();
                release.wait();
            })
        };
        if clear_all {
            *CLEAR_ALL_HISTORY_TEST_HOOK.lock() = Some(hook);
        } else {
            *CLEAR_HISTORY_TEST_HOOK.lock() = Some(hook);
        }

        let clear_db = db.clone();
        let clear = std::thread::spawn(move || {
            if clear_all {
                clear_all_history_in(&clear_db)
            } else {
                clear_history_in(&clear_db, None, None)
            }
        });
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
        assert_eq!(clear.join().unwrap().unwrap(), 1);
        switch.join().unwrap();

        let reopened_old =
            Database::new_with_settings(old_path, root.join("reopen-settings.db")).unwrap();
        assert_eq!(
            ClipboardRepository::new(&reopened_old)
                .count(QueryOptions::default())
                .unwrap(),
            0
        );
        assert!(!old_asset.exists());
        assert_eq!(target_repo.count(QueryOptions::default()).unwrap(), 1);

        *CLEAR_ALL_HISTORY_TEST_HOOK.lock() = None;
        *CLEAR_HISTORY_TEST_HOOK.lock() = None;
        drop(target_repo);
        drop(target_db);
        drop(reopened_old);
        drop(old_repo);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clear_all_history_holds_operation_guard_through_asset_cleanup() {
        assert_history_clear_blocks_switch(true);
    }

    #[test]
    fn clear_history_holds_operation_guard_through_asset_cleanup() {
        assert_history_clear_blocks_switch(false);
    }

    #[test]
    fn locked_item_delete_is_rejected_until_unlocked() {
        let repo = temp_repo();
        let id = repo.insert(text_item("locked-delete", true)).unwrap();

        assert_eq!(
            delete_clipboard_item_in_repo(&repo, id).unwrap_err(),
            ITEM_LOCKED_ERROR
        );
        assert!(repo.get_by_id(id).unwrap().is_some());

        repo.toggle_lock(id).unwrap();
        delete_clipboard_item_in_repo(&repo, id).unwrap();
        assert!(repo.get_by_id(id).unwrap().is_none());
    }

    #[test]
    fn batch_delete_with_locked_item_deletes_nothing() {
        let repo = temp_repo();
        let normal_id = repo.insert(text_item("batch-normal", false)).unwrap();
        let locked_id = repo.insert(text_item("batch-locked", true)).unwrap();

        assert_eq!(
            batch_delete_clipboard_items_in_repo(&repo, &[normal_id, locked_id]).unwrap_err(),
            ITEM_LOCKED_ERROR
        );
        assert!(repo.get_by_id(normal_id).unwrap().is_some());
        assert!(repo.get_by_id(locked_id).unwrap().is_some());
    }

    #[test]
    fn empty_text_edit_rejects_locked_item_without_cleaning_assets() {
        let repo = temp_repo();
        let asset = temp_asset("locked_empty_edit");
        let mut item = text_item("locked-empty", true);
        item.image_path = Some(asset.to_string_lossy().into_owned());
        let id = repo.insert(item).unwrap();

        assert_eq!(
            delete_empty_text_item_in_repo(&repo, id).unwrap_err(),
            ITEM_LOCKED_ERROR
        );
        assert!(repo.get_by_id(id).unwrap().is_some());
        assert!(asset.exists());
        std::fs::remove_file(asset).unwrap();
    }

    #[test]
    fn empty_text_edit_deletes_unlocked_item_and_cleans_assets() {
        let repo = temp_repo();
        let asset = temp_asset("unlocked_empty_edit");
        let mut item = text_item("unlocked-empty", false);
        item.image_path = Some(asset.to_string_lossy().into_owned());
        let id = repo.insert(item).unwrap();

        delete_empty_text_item_in_repo(&repo, id).unwrap();

        assert!(repo.get_by_id(id).unwrap().is_none());
        assert!(!asset.exists());
    }

    #[test]
    fn keyword_at_start() {
        let result = extract_keyword_context("hello world foo bar", "hello", 20);
        assert!(result.contains("hello"), "result: {result}");
    }

    #[test]
    fn keyword_in_middle() {
        let result = extract_keyword_context("aaa bbb ccc ddd eee", "ccc", 15);
        assert!(result.contains("ccc"), "result: {result}");
    }

    #[test]
    fn keyword_at_end() {
        let result = extract_keyword_context("foo bar baz qux", "qux", 15);
        assert!(result.contains("qux"), "result: {result}");
    }

    #[test]
    fn keyword_not_found_returns_prefix() {
        let text = "abcdefghijklmnop";
        let result = extract_keyword_context(text, "xyz", 5);
        assert_eq!(result, text.chars().take(5).collect::<String>());
    }

    #[test]
    fn case_insensitive() {
        let result = extract_keyword_context("Hello World", "hello", 20);
        assert!(result.contains("Hello"), "result: {result}");
    }

    #[test]
    fn cjk_keyword() {
        let result = extract_keyword_context("这是一段中文文本用于测试", "中文", 10);
        assert!(result.contains("中文"), "result: {result}");
    }

    #[test]
    fn cjk_text_with_emoji() {
        let result = extract_keyword_context("测试 🎉 emoji 关键词搜索", "关键词", 15);
        assert!(result.contains("关键词"), "result: {result}");
    }

    #[test]
    fn empty_keyword_returns_prefix() {
        let text = "hello world";
        let result = extract_keyword_context(text, "", 5);
        // 空关键词视为未找到，返回截断前缀（可能带省略号）
        assert!(result.starts_with("hell"), "result: {result}");
    }

    #[test]
    fn empty_text() {
        let result = extract_keyword_context("", "keyword", 10);
        assert_eq!(result, "");
    }

    #[test]
    fn max_len_larger_than_text() {
        let result = extract_keyword_context("short", "short", 100);
        assert_eq!(result, "short");
    }

    #[test]
    fn unicode_boundary_safety() {
        // 多字节字符不应 panic
        let result = extract_keyword_context("émoji 🎉 test", "🎉", 20);
        assert!(result.contains("🎉"), "result: {result}");
    }
}
