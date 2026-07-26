//! Screen and window capture.
//!
//! Windows Phase 1 starts with real display enumeration through Win32.
//! Actual frame capture will be added behind this module next.

use crate::recording::types::{DisplayInfo, WindowInfo};

#[cfg(windows)]
mod windows_backend {
    use super::DisplayInfo;
    use std::ffi::c_void;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
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
}

pub fn enumerate_displays() -> Vec<DisplayInfo> {
    #[cfg(windows)]
    {
        return windows_backend::enumerate_displays();
    }

    #[cfg(not(windows))]
    {
        // Native enumeration for macOS/Linux will be implemented when those
        // platforms enter the support matrix. Keep the command shape stable.
        Vec::new()
    }
}

pub fn enumerate_windows() -> Vec<WindowInfo> {
    // Window enumeration is the next Windows-native milestone. Returning an
    // empty list is preferable to presenting fake applications as real ones.
    Vec::new()
}
