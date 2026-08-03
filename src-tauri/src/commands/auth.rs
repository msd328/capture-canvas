use crate::{
    auth::{OidcTransactionProbe, SecureAuthProbe, SecureAuthStatus},
    oidc_exchange::{OidcExchangeContractStatus, OidcExchangeProbe},
    oidc_loopback::{OidcCallbackStatus, OidcSignInLaunch},
    state::AppState,
};
use serde::Serialize;
use tauri::State;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcClientReadiness {
    pub configured: bool,
    pub authorization_endpoint_https: bool,
    pub callback_mode: Option<&'static str>,
    pub scope_count: usize,
    pub token_exchange_configured: bool,
    pub token_endpoint_https: bool,
    pub issuer_https: bool,
    pub audience_configured: bool,
    pub jwks_uri_https: bool,
}

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
pub async fn get_oidc_client_status() -> Result<OidcClientReadiness, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let authorization = crate::oidc::client_status()?;
        let token = crate::oidc_token::status()?;
        Ok(OidcClientReadiness {
            configured: authorization.configured && token.configured,
            authorization_endpoint_https: authorization.authorization_endpoint_https,
            callback_mode: authorization.callback_mode,
            scope_count: authorization.scope_count,
            token_exchange_configured: token.configured,
            token_endpoint_https: token.token_endpoint_https,
            issuer_https: token.issuer_https,
            audience_configured: token.audience_configured,
            jwks_uri_https: token.jwks_uri_https,
        })
    })
    .await
    .map_err(|error| worker_error("OIDC client status", error))?
}

#[tauri::command]
pub async fn get_oidc_exchange_contract_status(
) -> Result<OidcExchangeContractStatus, String> {
    tauri::async_runtime::spawn_blocking(crate::oidc_exchange::status)
        .await
        .map_err(|error| worker_error("OIDC exchange contract status", error))?
}

#[tauri::command]
pub async fn start_oidc_sign_in(state: State<'_, AppState>) -> Result<OidcSignInLaunch, String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Do not create a real PKCE transaction or open the browser unless the
        // complete token endpoint/issuer/audience/JWKS trust contract is pinned.
        let _token_config = crate::oidc_token::require_configured()?;
        crate::oidc_loopback::start_sign_in(&store)
    })
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

#[tauri::command]
pub async fn probe_oidc_exchange_contract() -> Result<OidcExchangeProbe, String> {
    tauri::async_runtime::spawn_blocking(crate::oidc_exchange::probe)
        .await
        .map_err(|error| worker_error("OIDC exchange contract probe", error))
}
