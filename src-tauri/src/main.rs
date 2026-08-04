// Recorder — Tauri backend entrypoint.
//
// This file wires up the Tauri app and registers every command consumed by
// the React frontend (see src/services/desktop.ts for the contract).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod auth;
mod camera;
mod capture;
mod commands;
mod encoding;
mod oidc;
mod oidc_claims;
mod oidc_exchange;
mod oidc_jwks;
mod oidc_loopback;
mod oidc_reconcile;
mod oidc_refresh;
mod oidc_refresh_flow;
mod oidc_session;
mod oidc_token;
mod oidc_verify;
mod saas_access;
// The bounded WinRT wait macro assigns its timeout flag before returning an
// explicit timeout outcome. Rust reports that macro-local assignment once per
// expansion even though the outcome carries the correct timeout state. Keep the
// expectation scoped to recording code so unrelated unused assignments remain
// visible, and remove it when the macro is converted to a typed helper.
#[cfg_attr(windows, expect(unused_assignments))]
mod recording;
mod security;
mod state;

use state::AppState;

fn main() {
    tauri::Builder::default()
        .setup(|_| {
            // Remove abandoned recorder-owned temporary media without delaying the
            // UI thread. A conservative age gate protects active work from another
            // Recorder process while clearing stale crash/finalizer artifacts.
            security::cleanup_stale_recording_artifacts_async();

            // Media Foundation normally pays a large one-time codec startup cost on
            // the first recording. Exercise the native H.264/AAC path in a background
            // thread while the frontend is loading so Start remains responsive.
            encoding::warm_native_capture_pipeline_async();

            // A v2 subject-bound refresh credential may restore a native-only session
            // through the pinned, bounded and signature-verified refresh flow. The
            // serialized task never grants paid access and never exposes token bytes.
            oidc_reconcile::restore_async();
            Ok(())
        })
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::auth::get_secure_auth_status,
            commands::auth::probe_secure_auth_store,
            commands::auth::clear_secure_auth_session,
            commands::auth::get_oidc_client_status,
            commands::auth::get_oidc_exchange_contract_status,
            commands::auth::start_oidc_sign_in,
            commands::auth::get_oidc_callback_status,
            commands::auth::complete_oidc_sign_in,
            commands::auth::get_oidc_session_status,
            commands::auth::restore_oidc_session,
            commands::auth::cancel_oidc_transaction,
            commands::auth::probe_oidc_transaction,
            commands::auth::probe_oidc_exchange_contract,
            commands::access::get_account_access_status,
            commands::access::refresh_account_access,
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
            commands::library::get_library_snapshot,
            commands::library::wait_for_library_update,
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
