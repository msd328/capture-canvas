//! Persisted refresh-credential reconciliation and startup restoration.
//!
//! Only a subject-bound v2 credential may enter the refresh flow. Restoration is
//! serialized, cancellation-generation protected, and never treats durable storage
//! alone as authentication. Legacy raw credentials remain visible for cleanup but
//! are not exchanged automatically.

use crate::{
    oidc_refresh,
    oidc_session::{self, NativeOidcSessionStatus},
};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::OnceLock;
use tokio::sync::Mutex as AsyncMutex;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeOidcSessionOverview {
    pub active: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub access_token_native_only: bool,
    pub refresh_token_persisted: bool,
    pub reconciliation_complete: bool,
    pub refresh_credential_present: bool,
    pub legacy_refresh_credential_present: bool,
    pub restoration_required: bool,
    pub restoration_attempted: bool,
    pub restoration_failed: bool,
    pub automatic_restoration_enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RestorationState {
    attempted: bool,
    failed: bool,
}

fn restoration_state() -> &'static Mutex<RestorationState> {
    static STATE: OnceLock<Mutex<RestorationState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(RestorationState::default()))
}

fn restoration_lock() -> &'static AsyncMutex<()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(()))
}

pub(crate) fn reconcile_and_status() -> Result<NativeOidcSessionOverview, String> {
    let session = oidc_session::status();
    let stored = oidc_refresh::load_refresh_credential_state()?;
    let state = *restoration_state().lock();
    let overview = overview(
        session,
        stored.credential.is_some(),
        stored.legacy_present,
        state,
    );
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_startup_reconcile ok=true active={} refresh_present={} legacy_present={} restoration_required={} restoration_attempted={} restoration_failed={} automatic_refresh=true paid_access_granted=false",
        overview.active,
        overview.refresh_credential_present,
        overview.legacy_refresh_credential_present,
        overview.restoration_required,
        overview.restoration_attempted,
        overview.restoration_failed
    );
    Ok(overview)
}

pub(crate) async fn restore_persisted_session() -> Result<NativeOidcSessionOverview, String> {
    let _guard = restoration_lock().lock().await;
    if oidc_session::status().active {
        return reconcile_and_status();
    }

    let expected_generation = oidc_session::generation_snapshot();
    let stored = oidc_refresh::load_refresh_credential_state()?;
    let Some(credential) = stored.credential else {
        return reconcile_and_status();
    };

    {
        let mut state = restoration_state().lock();
        state.attempted = true;
        state.failed = false;
    }
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_startup_restore_start ok=true subject_bound=true legacy_credential=false webview_token_input=false paid_access_granted=false"
    );

    match oidc_session::restore_verified_session(expected_generation, &credential).await {
        Ok(_) => {
            {
                let mut state = restoration_state().lock();
                state.failed = false;
            }
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_startup_restore ok=true active=true refresh_rotated=true subject_continuity=true paid_access_granted=false"
            );
            reconcile_and_status()
        }
        Err(error) => {
            let active = oidc_session::status().active;
            let credential_still_present = oidc_refresh::load_refresh_credential_state()
                .map(|state| state.credential.is_some())
                .unwrap_or(true);
            {
                let mut state = restoration_state().lock();
                if active || !credential_still_present {
                    *state = RestorationState::default();
                } else {
                    state.attempted = true;
                    state.failed = true;
                }
            }
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_startup_restore ok=false active={active} refresh_rotated=false credential_still_present={credential_still_present} paid_access_granted=false"
            );
            if active {
                return reconcile_and_status();
            }
            Err(error)
        }
    }
}

pub(crate) fn restore_async() {
    tauri::async_runtime::spawn(async {
        if let Err(error) = restore_persisted_session().await {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_startup_restore_background ok=false code=restore_failed"
            );
            drop(error);
        }
    });
}

pub(crate) fn reset_restoration_state() {
    *restoration_state().lock() = RestorationState::default();
}

fn overview(
    session: NativeOidcSessionStatus,
    current_present: bool,
    legacy_present: bool,
    restoration: RestorationState,
) -> NativeOidcSessionOverview {
    NativeOidcSessionOverview {
        active: session.active,
        expires_at: session.expires_at,
        access_token_native_only: session.access_token_native_only,
        refresh_token_persisted: session.refresh_token_persisted,
        reconciliation_complete: true,
        refresh_credential_present: current_present || legacy_present,
        legacy_refresh_credential_present: legacy_present,
        restoration_required: !session.active && current_present,
        restoration_attempted: restoration.attempted,
        restoration_failed: restoration.failed,
        automatic_restoration_enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inactive_session() -> NativeOidcSessionStatus {
        NativeOidcSessionStatus {
            active: false,
            expires_at: None,
            access_token_native_only: true,
            refresh_token_persisted: false,
        }
    }

    #[test]
    fn subject_bound_refresh_requires_restoration_but_is_not_a_session() {
        let status = overview(
            inactive_session(),
            true,
            false,
            RestorationState::default(),
        );
        assert!(!status.active);
        assert!(status.refresh_credential_present);
        assert!(status.restoration_required);
        assert!(status.automatic_restoration_enabled);
    }

    #[test]
    fn legacy_raw_refresh_is_visible_but_never_restorable() {
        let status = overview(
            inactive_session(),
            false,
            true,
            RestorationState::default(),
        );
        assert!(status.refresh_credential_present);
        assert!(status.legacy_refresh_credential_present);
        assert!(!status.restoration_required);
    }

    #[test]
    fn failed_restore_is_non_secret_status_only() {
        let status = overview(
            inactive_session(),
            true,
            false,
            RestorationState {
                attempted: true,
                failed: true,
            },
        );
        assert!(status.restoration_attempted);
        assert!(status.restoration_failed);
        let serialized = serde_json::to_string(&status).unwrap();
        assert!(!serialized.contains("refresh.token"));
    }
}
