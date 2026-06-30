//! Windows RTF 剪贴板读写

/// 从剪贴板读取 RTF 原始字节（Windows 专用）
///
/// 返回 `Vec<u8>` 而非 `String`，因为 RTF 的 `\binN` 段可能包含非 UTF-8 二进制数据。
/// 调用方在需要 String 时自行决定如何转换（lossy 或 strict）。
#[cfg(target_os = "windows")]
pub fn read_rtf_from_clipboard() -> Option<Vec<u8>> {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatA,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows::core::PCSTR;

    // RAII guard: 确保 CloseClipboard 在任何退出路径都执行
    struct ClipboardGuard;
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard().ok();
            }
        }
    }

    // RAII guard: 确保 GlobalUnlock 在任何退出路径（含 panic）都执行
    struct GlobalUnlockGuard(windows::Win32::Foundation::HGLOBAL);
    impl Drop for GlobalUnlockGuard {
        fn drop(&mut self) {
            unsafe {
                GlobalUnlock(self.0).ok();
            }
        }
    }

    unsafe {
        let cf_rtf = RegisterClipboardFormatA(PCSTR(c"Rich Text Format".as_ptr().cast()));
        if cf_rtf == 0 {
            return None;
        }

        if OpenClipboard(None).is_err() {
            return None;
        }
        let _clip_guard = ClipboardGuard;

        let handle = GetClipboardData(cf_rtf).ok()?;
        let hglobal = windows::Win32::Foundation::HGLOBAL(handle.0);
        if hglobal.is_invalid() {
            return None;
        }

        let ptr = GlobalLock(hglobal) as *const u8;
        if ptr.is_null() {
            return None;
        }
        let _unlock_guard = GlobalUnlockGuard(hglobal);

        // 使用 GlobalSize 确定分配大小，避免扫描 null 字节（\binN 段可含嵌入 null）
        let size = GlobalSize(hglobal) as usize;
        if size == 0 {
            return None;
        }
        let buf = std::slice::from_raw_parts(ptr, size);

        // 去掉尾部的 null 终止符（如有）
        let data = if buf.last() == Some(&0) {
            &buf[..size - 1]
        } else {
            buf
        };

        Some(data.to_vec())
    }
}

#[cfg(not(target_os = "windows"))]
pub fn read_rtf_from_clipboard() -> Option<Vec<u8>> {
    None
}
