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

use state::AppState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
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
