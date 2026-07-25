//! Screen and window capture.
//!
//! Backends per OS:
//!   * macOS 12.3+  — ScreenCaptureKit (SCStream)
//!   * Windows 10+ — Windows.Graphics.Capture + DXGI Desktop Duplication
//!   * Linux       — PipeWire + xdg-desktop-portal
//!
//! For Phase 1 we ship stubs returning representative sample data so the
//! source-picker UI can be developed against real command shapes.

use crate::recording::types::{DisplayInfo, WindowInfo};

pub fn enumerate_displays() -> Vec<DisplayInfo> {
    // TODO(native): query the OS. Below is placeholder data.
    vec![
        DisplayInfo {
            id: "display-1".into(),
            name: "Built-in Display".into(),
            width: 2560,
            height: 1600,
            is_primary: true,
            thumbnail_data_url: None,
        },
        DisplayInfo {
            id: "display-2".into(),
            name: "External Monitor".into(),
            width: 3840,
            height: 2160,
            is_primary: false,
            thumbnail_data_url: None,
        },
    ]
}

pub fn enumerate_windows() -> Vec<WindowInfo> {
    // TODO(native): enumerate windows with per-OS APIs, filter out the
    // Recorder controls window itself.
    vec![
        WindowInfo {
            id: "win-1".into(),
            name: "Design Review — Figma".into(),
            app_name: "Figma".into(),
            width: 1440,
            height: 900,
            thumbnail_data_url: None,
        },
        WindowInfo {
            id: "win-2".into(),
            name: "index.tsx — VS Code".into(),
            app_name: "VS Code".into(),
            width: 1600,
            height: 1000,
            thumbnail_data_url: None,
        },
    ]
}
