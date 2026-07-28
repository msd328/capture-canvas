// Recorder — Tauri backend entrypoint.
//
// This file wires up the Tauri app and registers every command consumed by
// the React frontend (see src/services/desktop.ts for the contract).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod camera;
mod capture;
mod commands;
mod encoding;
mod recording;
mod state;

// Rust 2021 resolves `windows_capture::...` as an external-crate path from
// nested modules. Keep the instrumentation facade in its own internal module,
// re-export its public API at the crate root, and alias this crate as
// `windows_capture`. Existing capture imports then continue to use the same
// paths while the actual dependency remains explicitly named
// `windows_capture_core` in Cargo.toml.
#[cfg(windows)]
#[path = "windows_capture.rs"]
mod windows_capture_facade;
#[cfg(windows)]
pub use windows_capture_facade::{
    capture, d3d11, encoder, frame, graphics_capture_api, monitor, settings, window,
};
#[cfg(windows)]
extern crate self as windows_capture;

use state::AppState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|_| {
            // Media Foundation normally pays a large one-time codec startup cost on
            // the first recording. Exercise the native H.264/AAC path in a background
            // thread while the frontend is loading so Start remains responsive.
            encoding::warm_native_capture_pipeline_async();
            Ok(())
        })
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::devices::list_displays,
            commands::devices::list_windows,
            commands::devices::list_microphones,
            commands::devices::list_cameras,
            commands::devices::system_audio_supported,
            commands::devices::capture_source_preview,
            commands::recording::start_recording,
            commands::recording::pause_recording,
            commands::recording::resume_recording,
            commands::recording::stop_recording,
            commands::library::get_recordings,
            commands::library::get_recording,
            commands::library::delete_recording,
            commands::library::rename_recording,
            commands::library::open_recording_location,
            commands::settings::get_settings,
            commands::settings::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Recorder");
}
