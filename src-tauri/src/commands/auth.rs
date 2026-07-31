use crate::{
    auth::{SecureAuthProbe, SecureAuthStatus},
    state::AppState,
};
use tauri::State;

fn worker_error(operation: &str, error: impl std::fmt::Display) -> String {
    format!("Recorder secure-auth {operation} worker failed: {error}")
}

#[tauri::command]
pub async fn get_secure_auth_status(
    state: State<'_, AppState>,
) -> Result<SecureAuthStatus, String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || store.status())
        .await
        .map_err(|error| worker_error("status", error))?
}

#[tauri::command]
pub async fn probe_secure_auth_store(
    state: State<'_, AppState>,
) -> Result<SecureAuthProbe, String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || store.probe())
        .await
        .map_err(|error| worker_error("probe", error))?
}

#[tauri::command]
pub async fn clear_secure_auth_session(state: State<'_, AppState>) -> Result<(), String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || store.clear())
        .await
        .map_err(|error| worker_error("clear", error))?
}
