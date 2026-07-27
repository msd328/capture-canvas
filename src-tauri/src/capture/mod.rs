//! Screen and window capture.
//!
//! Win32 remains responsible for enumerating and resolving selectable sources.
//! Actual Windows recording frames now come from Windows.Graphics.Capture/D3D11
//! through the sibling WGC backend rather than FFmpeg gdigrab.

use crate::recording::types::{CaptureKind, CaptureTarget, DisplayInfo, WindowInfo};
use anyhow::Result;

#[cfg(windows)]
mod wgc;
#[cfg(windows)]
pub use wgc::{start_native_video_capture, NativeVideoCapture};

#[derive(Debug, Clone)]
pub struct CaptureSource {
    pub ffmpeg_input: String,
    pub offset_x: Option<i32>,
    pub offset_y: Option<i32>,
    pub width: u32,
    pub height: u32,
}

#[cfg(windows)]
mod windows_backend {
    use super::{CaptureKind, CaptureSource, CaptureTarget, DisplayInfo, WindowInfo};
    use anyhow::{anyhow, Result};
    use std::ffi::c_void;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindow,
        IsWindowVisible,
    };

    const MONITORINFOF_PRIMARY: u32 = 0x00000001;

    #[derive(Debug, Clone)]
    struct MonitorNative { handle: usize, rect: RECT, is_primary: bool }

    unsafe extern "system" fn monitor_native_callback(monitor: HMONITOR, _hdc: HDC, _rect: *mut RECT, data: LPARAM) -> BOOL {
        let monitors = unsafe { &mut *(data.0 as *mut Vec<MonitorNative>) };
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            monitors.push(MonitorNative { handle: monitor.0 as usize, rect: info.rcMonitor, is_primary: (info.dwFlags & MONITORINFOF_PRIMARY) != 0 });
        }
        BOOL(1)
    }

    fn native_monitors() -> Vec<MonitorNative> {
        let mut monitors = Vec::<MonitorNative>::new();
        let ptr = &mut monitors as *mut Vec<MonitorNative> as *mut c_void;
        unsafe { let _ = EnumDisplayMonitors(None, None, Some(monitor_native_callback), LPARAM(ptr as isize)); }
        monitors.sort_by_key(|monitor| !monitor.is_primary);
        monitors
    }

    fn window_title(hwnd: HWND) -> Option<String> {
        let title_len = unsafe { GetWindowTextLengthW(hwnd) };
        if title_len <= 0 { return None; }
        let mut buffer = vec![0u16; title_len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        if copied <= 0 { return None; }
        let title = String::from_utf16_lossy(&buffer[..copied as usize]).trim().to_string();
        (!title.is_empty()).then_some(title)
    }

    /// Windows' GetWindowRect includes invisible resize borders on modern desktop
    /// windows. WGC captures the visible DWM frame instead, so prefer the DWM
    /// extended-frame bounds and retain GetWindowRect only as a compatibility
    /// fallback. This keeps the encoder dimensions aligned with WGC frames.
    fn visible_window_rect(hwnd: HWND) -> Result<RECT> {
        let mut rect = RECT::default();
        let dwm = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut rect as *mut RECT as *mut c_void,
                std::mem::size_of::<RECT>() as u32,
            )
        };
        if dwm.is_ok() && rect.right > rect.left && rect.bottom > rect.top {
            return Ok(rect);
        }

        unsafe { GetWindowRect(hwnd, &mut rect) }
            .map_err(|_| anyhow!("Unable to read selected window bounds"))?;
        Ok(rect)
    }

    unsafe extern "system" fn window_callback(hwnd: HWND, data: LPARAM) -> BOOL {
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() || unsafe { IsIconic(hwnd) }.as_bool() { return BOOL(1); }
        let Some(title) = window_title(hwnd) else { return BOOL(1); };
        if title == "Recorder" || title == "Program Manager" { return BOOL(1); }
        let Ok(rect) = visible_window_rect(hwnd) else { return BOOL(1); };
        let width = (rect.right - rect.left).max(0) as u32;
        let height = (rect.bottom - rect.top).max(0) as u32;
        if width < 100 || height < 100 { return BOOL(1); }
        let windows = unsafe { &mut *(data.0 as *mut Vec<WindowInfo>) };
        windows.push(WindowInfo {
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
        native_monitors().into_iter().enumerate().map(|(idx, monitor)| {
            let width = (monitor.rect.right - monitor.rect.left).max(0) as u32;
            let height = (monitor.rect.bottom - monitor.rect.top).max(0) as u32;
            DisplayInfo {
                id: format!("monitor-{:x}", monitor.handle),
                name: if monitor.is_primary { format!("Display {} (Primary)", idx + 1) } else { format!("Display {}", idx + 1) },
                width, height, is_primary: monitor.is_primary, thumbnail_data_url: None,
            }
        }).collect()
    }

    pub fn enumerate_windows() -> Vec<WindowInfo> {
        let mut windows = Vec::<WindowInfo>::new();
        let ptr = &mut windows as *mut Vec<WindowInfo> as *mut c_void;
        unsafe { let _ = EnumWindows(Some(window_callback), LPARAM(ptr as isize)); }
        windows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        windows
    }

    pub fn resolve_target(target: &CaptureTarget) -> Result<CaptureSource> {
        match target.kind {
            CaptureKind::Display => {
                let raw = target.id.strip_prefix("monitor-").ok_or_else(|| anyhow!("Invalid monitor id"))?;
                let handle = usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid monitor id"))?;
                let monitor = native_monitors().into_iter().find(|monitor| monitor.handle == handle)
                    .ok_or_else(|| anyhow!("Selected display is no longer available"))?;
                let width = (monitor.rect.right - monitor.rect.left).max(0) as u32;
                let height = (monitor.rect.bottom - monitor.rect.top).max(0) as u32;
                Ok(CaptureSource { ffmpeg_input: "desktop".to_string(), offset_x: Some(monitor.rect.left), offset_y: Some(monitor.rect.top), width, height })
            }
            CaptureKind::Window => {
                let raw = target.id.strip_prefix("window-").ok_or_else(|| anyhow!("Invalid window id"))?;
                let value = usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid window id"))?;
                let hwnd = HWND(value as *mut c_void);
                if !unsafe { IsWindow(Some(hwnd)) }.as_bool() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
                    return Err(anyhow!("Selected window is no longer available or visible"));
                }
                if unsafe { IsIconic(hwnd) }.as_bool() {
                    return Err(anyhow!("Selected window is minimized. Restore it before recording."));
                }
                let title = window_title(hwnd).ok_or_else(|| anyhow!("Selected window no longer has a capturable title"))?;
                let rect = visible_window_rect(hwnd)?;
                let width = (rect.right - rect.left).max(0) as u32;
                let height = (rect.bottom - rect.top).max(0) as u32;
                if width < 2 || height < 2 { return Err(anyhow!("Selected window has an invalid capture size")); }
                // Kept only as metadata/debug fallback; real frames now come from WGC via the HWND.
                Ok(CaptureSource { ffmpeg_input: format!("title={title}"), offset_x: None, offset_y: None, width, height })
            }
        }
    }
}

pub fn enumerate_displays() -> Vec<DisplayInfo> {
    #[cfg(windows)] { return windows_backend::enumerate_displays(); }
    #[cfg(not(windows))] { Vec::new() }
}

pub fn enumerate_windows() -> Vec<WindowInfo> {
    #[cfg(windows)] { return windows_backend::enumerate_windows(); }
    #[cfg(not(windows))] { Vec::new() }
}

pub fn resolve_target(target: &CaptureTarget) -> Result<CaptureSource> {
    #[cfg(windows)] { return windows_backend::resolve_target(target); }
    #[cfg(not(windows))] { let _ = target; Err(anyhow::anyhow!("Native capture is not implemented for this operating system yet")) }
}
