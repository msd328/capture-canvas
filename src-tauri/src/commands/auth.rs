use crate::{
    auth::{
        OidcAuthorizationPreparation, OidcTransactionProbe, OidcTransactionStatus,
        SecureAuthProbe, SecureAuthStatus,
    },
    oidc::{OidcAuthorizationRequest, OidcClientStatus},
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

#[tauri::command]
pub async fn get_oidc_client_status() -> Result<OidcClientStatus, String> {
    tauri::async_runtime::spawn_blocking(crate::oidc::client_status)
        .await
        .map_err(|error| worker_error("OIDC client status", error))?
}

#[tauri::command]
pub async fn prepare_oidc_authorization(
    state: State<'_, AppState>,
) -> Result<OidcAuthorizationRequest, String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || crate::oidc::prepare_authorization(&store))
        .await
        .map_err(|error| worker_error("OIDC authorization prepare", error))?
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
    tauri::async_runtime::spawn_blocking(move || store.cancel_oidc_transaction())
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
