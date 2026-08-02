use crate::{
    auth::{
        OidcAuthorizationPreparation, OidcTransactionProbe, OidcTransactionStatus,
        SecureAuthProbe, SecureAuthStatus,
    },
    oidc::OidcClientStatus,
    oidc_loopback::{OidcCallbackStatus, OidcSignInLaunch},
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
    tauri::async_runtime::spawn_blocking(move || {
        crate::oidc_loopback::cancel_callback();
        store.clear()
    })
    .await
    .map_err(|error| worker_error("clear", error))?
}

#[tauri::command]
pub async fn get_oidc_client_status() -> Result<OidcClientStatus, String> {
    tauri::async_runtime::spawn_blocking(crate::oidc::client_status)
        .await
        .map_err(|error| worker_error("OIDC client status", error))?
}

#[tauri::command]
pub async fn start_oidc_sign_in(
    state: State<'_, AppState>,
) -> Result<OidcSignInLaunch, String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || crate::oidc_loopback::start_sign_in(&store))
        .await
        .map_err(|error| worker_error("OIDC sign-in start", error))?
}

#[tauri::command]
pub async fn get_oidc_callback_status() -> Result<OidcCallbackStatus, String> {
    tauri::async_runtime::spawn_blocking(crate::oidc_loopback::callback_status)
        .await
        .map_err(|error| worker_error("OIDC callback status", error))
}

#[tauri::command]
pub async fn prepare_oidc_transaction(
    state: State<'_, AppState>,
) -> Result<OidcAuthorizationPreparation, String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || store.prepare_oidc_transaction())
        .await
        .map_err(|error| worker_error("OIDC prepare", error))
}

#[tauri::command]
pub async fn get_oidc_transaction_status(
    state: State<'_, AppState>,
) -> Result<OidcTransactionStatus, String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || store.oidc_transaction_status())
        .await
        .map_err(|error| worker_error("OIDC status", error))
}

#[tauri::command]
pub async fn cancel_oidc_transaction(state: State<'_, AppState>) -> Result<(), String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::oidc_loopback::cancel_callback();
        store.cancel_oidc_transaction();
    })
    .await
    .map_err(|error| worker_error("OIDC cancel", error))
}

#[tauri::command]
pub async fn probe_oidc_transaction(
    state: State<'_, AppState>,
) -> Result<OidcTransactionProbe, String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || store.probe_oidc_transaction())
        .await
        .map_err(|error| worker_error("OIDC probe", error))
}
