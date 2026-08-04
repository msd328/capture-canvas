use crate::{
    auth::{OidcTransactionProbe, SecureAuthProbe, SecureAuthStatus},
    oidc_exchange::{OidcExchangeContractStatus, OidcExchangeProbe},
    oidc_loopback::{OidcCallbackStatus, OidcSignInLaunch},
    oidc_reconcile::NativeOidcSessionOverview,
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
        let memory_session_cleared = crate::oidc_session::clear();
        let refresh_result = crate::oidc_reconcile::clear_persisted_refresh_credential();
        let legacy_result = store.clear();
        let refresh_credential_cleared = refresh_result.is_ok();
        let legacy_store_cleared = legacy_result.is_ok();

        if refresh_credential_cleared && legacy_store_cleared {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_session_clear ok=true memory_session_cleared={memory_session_cleared} refresh_credential_cleared=true legacy_store_cleared=true"
            );
            return Ok(());
        }

        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_session_clear ok=false memory_session_cleared={memory_session_cleared} refresh_credential_cleared={refresh_credential_cleared} legacy_store_cleared={legacy_store_cleared}"
        );
        Err(refresh_result
            .err()
            .or_else(|| legacy_result.err())
            .unwrap_or_else(|| "Unable to clear the native cloud session".to_string()))
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
pub async fn get_oidc_exchange_contract_status() -> Result<OidcExchangeContractStatus, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut status = crate::oidc_exchange::status()?;
        status.network_exchange_enabled = true;
        status.identity_validation_enabled = true;
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_exchange_command_surface ok=true network_exchange=true identity_validation=true webview_token_input=false"
        );
        Ok(status)
    })
    .await
    .map_err(|error| worker_error("OIDC exchange contract status", error))?
}

#[tauri::command]
pub async fn start_oidc_sign_in(state: State<'_, AppState>) -> Result<OidcSignInLaunch, String> {
    let store = state.auth.clone();
    tauri::async_runtime::spawn_blocking(move || {
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
pub async fn complete_oidc_sign_in(
    state: State<'_, AppState>,
) -> Result<NativeOidcSessionOverview, String> {
    let store = state.auth.clone();
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_sign_in_complete_start ok=true webview_token_input=false"
    );
    let result = crate::oidc_session::establish_verified_session(&store).await;
    match result {
        Ok(status) => {
            let overview = crate::oidc_reconcile::reconcile_and_status()?;
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_sign_in_complete ok=true active={} refresh_token_persisted={} restoration_required={} paid_access_granted=false",
                status.active, status.refresh_token_persisted, overview.restoration_required
            );
            Ok(overview)
        }
        Err(error) => {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_sign_in_complete ok=false active=false refresh_token_persisted=false paid_access_granted=false"
            );
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn get_oidc_session_status() -> Result<NativeOidcSessionOverview, String> {
    tauri::async_runtime::spawn_blocking(crate::oidc_reconcile::reconcile_and_status)
        .await
        .map_err(|error| worker_error("OIDC session status", error))?
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
