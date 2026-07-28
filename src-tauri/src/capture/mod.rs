//! Screen and window capture.
//!
//! Win32 remains responsible for enumerating and resolving selectable sources.
//! Actual Windows recording frames come from Windows.Graphics.Capture/D3D11.
//! The preferred path keeps screen video on D3D11, can mix microphone + system
//! audio into one native AAC track, and composites the webcam directly onto the
//! capture texture. FFmpeg remains only as a compatibility fallback when native
//! Windows capture/encoding cannot be initialized.

use crate::recording::types::{
    CaptureKind, CapturePreview, CaptureTarget, DisplayInfo, WindowInfo,
};
use anyhow::Result;

#[cfg(windows)]
mod metrics;
#[cfg(windows)]
mod preview;
#[cfg(windows)]
mod wgc;
#[cfg(windows)]
mod wgc_gpu;

#[cfg(windows)]
pub enum NativeVideoCapture {
    Gpu {
        capture: wgc_gpu::NativeGpuVideoCapture,
        metrics: metrics::CaptureSessionMetrics,
    },
    Ffmpeg {
        capture: wgc::NativeVideoCapture,
        metrics: metrics::CaptureSessionMetrics,
    },
}

#[cfg(windows)]
impl NativeVideoCapture {
    pub fn captures_system_audio(&self) -> bool {
        match self {
            Self::Gpu { capture, .. } => capture.captures_system_audio(),
            Self::Ffmpeg { .. } => false,
        }
    }

    pub fn stop(self) -> Result<()> {
        let stop_started = std::time::Instant::now();
        match self {
            Self::Gpu { capture, metrics } => {
                let result = capture.stop();
                metrics.log_stopped(stop_started.elapsed(), result.is_ok());
                result
            }
            Self::Ffmpeg { capture, metrics } => {
                let result = capture.stop();
                metrics.log_stopped(stop_started.elapsed(), result.is_ok());
                result
            }
        }
    }
}

#[cfg(windows)]
pub fn start_native_video_capture(
    config: &crate::recording::types::RecordingConfig,
    target: &CaptureTarget,
    width: u32,
    height: u32,
    output_path: &std::path::Path,
) -> Result<NativeVideoCapture> {
    let camera = config.camera_id.is_some();
    let microphone = config.microphone_id.is_some();
    let system_audio = config.system_audio;
    let fps = config.fps.clamp(1, 60);

    // The first crop implementation uses the already-stable WGC latest-frame
    // backend. The selected rectangle is applied before FFmpeg receives the frame,
    // so the output contains only the requested area. Full-source recordings stay
    // on the preferred D3D11/Media Foundation path.
    if config.crop_region.is_none() {
        let native_started = std::time::Instant::now();
        match wgc_gpu::start_native_gpu_video_capture(config, target, width, height, output_path) {
            Ok(capture) => {
                let metrics = metrics::CaptureSessionMetrics::new(
                    metrics::CaptureBackend::NativeGpu,
                    output_path,
                    native_started.elapsed(),
                    width,
                    height,
                    fps,
                    camera,
                    microphone,
                    system_audio,
                );
                metrics.log_started();

                if camera && microphone && system_audio {
                    eprintln!("[Recorder] Native WGC/D3D11 H.264 + camera + mixed microphone/system-audio active");
                } else if microphone && system_audio {
                    eprintln!("[Recorder] Native WGC/D3D11 H.264 + mixed microphone/system-audio active");
                } else if camera && system_audio {
                    eprintln!("[Recorder] Native WGC/D3D11 H.264 + camera + system-audio active");
                } else if camera && microphone {
                    eprintln!("[Recorder] Native WGC/D3D11 H.264 + camera + microphone active");
                } else if camera {
                    eprintln!("[Recorder] Native WGC/D3D11 H.264 + camera active");
                } else if system_audio {
                    eprintln!("[Recorder] Native WGC/D3D11 Windows H.264 + system-audio encoder active");
                } else if microphone {
                    eprintln!("[Recorder] Native WGC/D3D11 Windows H.264 + microphone encoder active");
                } else {
                    eprintln!("[Recorder] Native WGC/D3D11 Windows H.264 encoder active");
                }
                return Ok(NativeVideoCapture::Gpu { capture, metrics });
            }
            Err(native_error) => {
                eprintln!(
                    "[Recorder][Health] native_init_failed_ms={} error={native_error}",
                    native_started.elapsed().as_millis(),
                );
                eprintln!(
                    "[Recorder] Native Windows encoder unavailable ({native_error}); falling back to FFmpeg"
                );
                // Native MediaTranscoder setup can create the destination before a
                // later initialization error. Clear a partial file before fallback.
                let _ = std::fs::remove_file(output_path);
            }
        }
    } else {
        eprintln!("[Recorder] Selected-area recording active through WGC crop compatibility backend");
    }

    let fallback_started = std::time::Instant::now();
    let capture = wgc::start_native_video_capture(config, target, width, height, output_path)?;
    let metrics = metrics::CaptureSessionMetrics::new(
        metrics::CaptureBackend::FfmpegFallback,
        output_path,
        fallback_started.elapsed(),
        width,
        height,
        fps,
        camera,
        microphone,
        system_audio,
    );
    metrics.log_started();
    Ok(NativeVideoCapture::Ffmpeg { capture, metrics })
}

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
    use windows::Win32::Graphics::Dwm::{
        DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
    };
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindow,
        IsWindowVisible,
    };

    const MONITORINFOF_PRIMARY: u32 = 0x00000001;

    #[derive(Debug, Clone)]
    struct MonitorNative {
        handle: usize,
        rect: RECT,
        is_primary: bool,
    }

    unsafe extern "system" fn monitor_native_callback(
        monitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let monitors = unsafe { &mut *(data.0 as *mut Vec<MonitorNative>) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            monitors.push(MonitorNative {
                handle: monitor.0 as usize,
                rect: info.rcMonitor,
                is_primary: (info.dwFlags & MONITORINFOF_PRIMARY) != 0,
            });
        }
        BOOL(1)
    }

    fn native_monitors() -> Vec<MonitorNative> {
        let mut monitors = Vec::<MonitorNative>::new();
        let ptr = &mut monitors as *mut Vec<MonitorNative> as *mut c_void;
        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(monitor_native_callback),
                LPARAM(ptr as isize),
            );
        }
        monitors.sort_by_key(|monitor| !monitor.is_primary);
        monitors
    }

    fn window_title(hwnd: HWND) -> Option<String> {
        let title_len = unsafe { GetWindowTextLengthW(hwnd) };
        if title_len <= 0 {
            return None;
        }
        let mut buffer = vec![0u16; title_len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        if copied <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&buffer[..copied as usize])
            .trim()
            .to_string();
        (!title.is_empty()).then_some(title)
    }

    fn is_window_cloaked(hwnd: HWND) -> bool {
        let mut cloaked = 0u32;
        unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut u32 as *mut c_void,
                std::mem::size_of::<u32>() as u32,
            )
        }
        .is_ok()
            && cloaked != 0
    }

    fn is_capture_candidate(hwnd: HWND) -> bool {
        if unsafe { IsIconic(hwnd) }.as_bool() || is_window_cloaked(hwnd) {
            return false;
        }

        // windows-capture applies the same basic suitability checks used by the
        // WGC backend: visible, top-level, not a tool window, and not owned by the
        // Recorder process itself. DWM cloaking above additionally removes windows
        // parked on another virtual desktop or retained only for background use.
        windows_capture::window::Window::from_raw_hwnd(hwnd.0).is_valid()
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
        if !is_capture_candidate(hwnd) {
            return BOOL(1);
        }
        let Some(title) = window_title(hwnd) else {
            return BOOL(1);
        };
        if title == "Recorder" || title == "Program Manager" {
            return BOOL(1);
        }
        let Ok(rect) = visible_window_rect(hwnd) else {
            return BOOL(1);
        };
        let width = (rect.right - rect.left).max(0) as u32;
        let height = (rect.bottom - rect.top).max(0) as u32;
        if width < 100 || height < 100 {
            return BOOL(1);
        }
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
        native_monitors()
            .into_iter()
            .enumerate()
            .map(|(idx, monitor)| {
                let width = (monitor.rect.right - monitor.rect.left).max(0) as u32;
                let height = (monitor.rect.bottom - monitor.rect.top).max(0) as u32;
                DisplayInfo {
                    id: format!("monitor-{:x}", monitor.handle),
                    name: if monitor.is_primary {
                        format!("Display {} (Primary)", idx + 1)
                    } else {
                        format!("Display {}", idx + 1)
                    },
                    width,
                    height,
                    is_primary: monitor.is_primary,
                    thumbnail_data_url: None,
                }
            })
            .collect()
    }

    pub fn enumerate_windows() -> Vec<WindowInfo> {
        let mut windows = Vec::<WindowInfo>::new();
        let ptr = &mut windows as *mut Vec<WindowInfo> as *mut c_void;
        unsafe {
            let _ = EnumWindows(Some(window_callback), LPARAM(ptr as isize));
        }
        windows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        windows.dedup_by(|a, b| a.id == b.id);
        windows
    }

    pub fn resolve_target(target: &CaptureTarget) -> Result<CaptureSource> {
        match target.kind {
            CaptureKind::Display => {
                let raw = target
                    .id
                    .strip_prefix("monitor-")
                    .ok_or_else(|| anyhow!("Invalid monitor id"))?;
                let handle =
                    usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid monitor id"))?;
                let monitor = native_monitors()
                    .into_iter()
                    .find(|monitor| monitor.handle == handle)
                    .ok_or_else(|| anyhow!("Selected display is no longer available"))?;
                let width = (monitor.rect.right - monitor.rect.left).max(0) as u32;
                let height = (monitor.rect.bottom - monitor.rect.top).max(0) as u32;
                Ok(CaptureSource {
                    ffmpeg_input: "desktop".to_string(),
                    offset_x: Some(monitor.rect.left),
                    offset_y: Some(monitor.rect.top),
                    width,
                    height,
                })
            }
            CaptureKind::Window => {
                let raw = target
                    .id
                    .strip_prefix("window-")
                    .ok_or_else(|| anyhow!("Invalid window id"))?;
                let value =
                    usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid window id"))?;
                let hwnd = HWND(value as *mut c_void);
                if !unsafe { IsWindow(Some(hwnd)) }.as_bool()
                    || !unsafe { IsWindowVisible(hwnd) }.as_bool()
                    || !is_capture_candidate(hwnd)
                {
                    return Err(anyhow!(
                        "Selected window is no longer available or capturable"
                    ));
                }
                let title = window_title(hwnd)
                    .ok_or_else(|| anyhow!("Selected window no longer has a capturable title"))?;
                let rect = visible_window_rect(hwnd)?;
                let width = (rect.right - rect.left).max(0) as u32;
                let height = (rect.bottom - rect.top).max(0) as u32;
                if width < 2 || height < 2 {
                    return Err(anyhow!("Selected window has an invalid capture size"));
                }
                // Kept only as metadata/debug fallback; real frames now come from WGC via the HWND.
                Ok(CaptureSource {
                    ffmpeg_input: format!("title={title}"),
                    offset_x: None,
                    offset_y: None,
                    width,
                    height,
                })
            }
        }
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

pub fn resolve_target(target: &CaptureTarget) -> Result<CaptureSource> {
    #[cfg(windows)]
    {
        return windows_backend::resolve_target(target);
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err(anyhow::anyhow!(
            "Native capture is not implemented for this operating system yet"
        ))
    }
}

pub fn capture_source_preview(target: &CaptureTarget) -> Result<CapturePreview> {
    #[cfg(windows)]
    {
        return preview::capture_source_preview(target);
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err(anyhow::anyhow!(
            "Source previews are not implemented for this operating system yet"
        ))
    }
}
