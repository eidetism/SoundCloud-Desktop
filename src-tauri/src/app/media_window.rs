//! An invisible top-level window that owns the media controls (SMTC). The
//! main window is destroyed while the app sits in the tray, and SMTC is tied
//! to the window it was created for, so it gets one that lives as long as
//! the app. Created on the main thread: the app's event loop pumps its
//! messages, so it answers broadcasts like any other window.

use std::ffi::c_void;

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_POPUP,
};

use crate::app::diagnostics;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Create the window; call on the main thread. `None` if Windows refused.
pub fn create() -> Option<*mut c_void> {
    let class = wide("PulseMediaControls");
    let title = wide("Pulse");
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        // Fails harmlessly if the class already exists.
        RegisterClassW(&window_class);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            diagnostics::warn("[MediaControls] owner window was not created");
            None
        } else {
            Some(hwnd)
        }
    }
}
