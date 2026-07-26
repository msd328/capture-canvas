//! Screen and window capture.
//!
//! Windows Phase 1 starts with real source enumeration through Win32.
//! Actual frame capture will be added behind this module next.

use crate::recording::types::{DisplayInfo, WindowInfo};

#[cfg(windows)]
mod windows_backend {
    use super::{DisplayInfo, WindowInfo};
    use std::ffi::c_void;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
    };

    // Win32 MONITORINFOF_PRIMARY is 0x00000001. Defining it locally avoids
    // depending on a constant whose generated location changed between
    // windows-rs versions.
    const MONITORINFOF_PRIMARY: u32 = 0x00000001;

    unsafe extern "system" fn monitor_callback(
        monitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let displays = unsafe { &mut *(data.0 as *mut Vec<DisplayInfo>) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };

        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            let width = (info.rcMonitor.right - info.rcMonitor.left).max(0) as u32;
            let height = (info.rcMonitor.bottom - info.rcMonitor.top).max(0) as u32;
            let is_primary = (info.dwFlags & MONITORINFOF_PRIMARY) != 0;
            let index = displays.len() + 1;

            displays.push(DisplayInfo {
                id: format!("monitor-{:x}", monitor.0 as usize),
                name: if is_primary {
                    format!("Display {index} (Primary)")
                } else {
                    format!("Display {index}")
                },
                width,
                height,
                is_primary,
                thumbnail_data_url: None,
            });
        }

        BOOL(1)
    }

    unsafe extern "system" fn window_callback(hwnd: HWND, data: LPARAM) -> BOOL {
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }

        let title_len = unsafe { GetWindowTextLengthW(hwnd) };
        if title_len <= 0 {
            return BOOL(1);
        }

        let mut title_buffer = vec![0u16; title_len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, &mut title_buffer) };
        if copied <= 0 {
            return BOOL(1);
        }

        let title = String::from_utf16_lossy(&title_buffer[..copied as usize])
            .trim()
            .to_string();
        if title.is_empty() || title == "Recorder" || title == "Program Manager" {
            return BOOL(1);
        }

        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
            return BOOL(1);
        }

        let width = (rect.right - rect.left).max(0) as u32;
        let height = (rect.bottom - rect.top).max(0) as u32;
        // Filter tiny utility/tool windows that are not useful recording targets.
        if width < 100 || height < 100 {
            return BOOL(1);
        }

        let windows = unsafe { &mut *(data.0 as *mut Vec<WindowInfo>) };
        windows.push(WindowInfo {
            // Keep the native HWND in the id so the upcoming capture pipeline can
            // resolve this exact window without changing the frontend contract.
            id: format!("window-{:x}", hwnd.0 as usize),
            name: title,
            app_name: "Windows app".to_string(),
            width,
            height,
            thumbnail_data_url: None,
        });

        BOOL(1)
    }

    pub fn enumerate_displays() -> Vec<DisplayInfo> {
        let mut displays = Vec::<DisplayInfo>::new();
        let ptr = &mut displays as *mut Vec<DisplayInfo> as *mut c_void;

        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(monitor_callback),
                LPARAM(ptr as isize),
            );
        }

        displays.sort_by_key(|display| !display.is_primary);
        displays
    }

    pub fn enumerate_windows() -> Vec<WindowInfo> {
        let mut windows = Vec::<WindowInfo>::new();
        let ptr = &mut windows as *mut Vec<WindowInfo> as *mut c_void;

        unsafe {
            let _ = EnumWindows(Some(window_callback), LPARAM(ptr as isize));
        }

        windows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        windows
    }
}

pub fn enumerate_displays() -> Vec<DisplayInfo> {
    #[cfg(windows)]
    {
        return windows_backend::enumerate_displays();
    }

    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

pub fn enumerate_windows() -> Vec<WindowInfo> {
    #[cfg(windows)]
    {
        return windows_backend::enumerate_windows();
    }

    #[cfg(not(windows))]
    {
        Vec::new()
    }
}
