//! Verified native OIDC access-session boundary.
//!
//! Initial authorization and persisted refresh restoration both build a completely
//! verified session candidate before entering the commit boundary. The rotated,
//! subject-bound refresh credential is written and read back while the session lock
//! is held; only then is the native-only access session replaced.

use crate::{auth::SecureAuthStore, oidc_exchange, oidc_refresh, oidc_refresh_flow, oidc_verify};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{compiler_fence, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use uuid::Uuid;

const MAX_ACCESS_TOKEN_BYTES: usize = 16 * 1024;
const MAX_ACCESS_SESSION_SECONDS: u64 = 24 * 60 * 60;

struct SecretBytes(Vec<u8>);

impl SecretBytes {
    fn from_slice(value: &[u8]) -> Result<Self, &'static str> {
        if value.is_empty() || value.len() > MAX_ACCESS_TOKEN_BYTES {
            return Err("access_token_size_invalid");
        }
        if value
            .iter()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        {
            return Err("access_token_text_invalid");
        }
        Ok(Self(value.to_vec()))
    }

    fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        compiler_fence(Ordering::SeqCst);
    }
}

struct NativeAccessSession {
    subject: Uuid,
    access_token: SecretBytes,
    expires_at_instant: Instant,
    expires_at: DateTime<Utc>,
    refresh_token_persisted: bool,
}

#[derive(Default)]
struct SessionState {
    active: Option<NativeAccessSession>,
    generation: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeOidcSessionStatus {
    pub active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub access_token_native_only: bool,
    pub refresh_token_persisted: bool,
}

fn runtime() -> &'static Mutex<SessionState> {
    static SESSION: OnceLock<Mutex<SessionState>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(SessionState::default()))
}

pub(crate) async fn establish_verified_session(
    store: &SecureAuthStore,
) -> Result<NativeOidcSessionStatus, String> {
    let expected_generation = generation_snapshot();
    let exchange = oidc_exchange::exchange_authorization_code(store).await?;
    let identity =
        oidc_verify::verify_id_token(exchange.id_token(), exchange.expected_nonce()).await?;
    let now = Instant::now();
    let candidate = prepare_session(
        identity.subject(),
        identity.expires_at(),
        exchange.access_token(),
        exchange.expires_in(),
        now,
        Utc::now(),
    )
    .map_err(|code| session_prepare_error("oidc_session_prepare", code))?;

    commit_session(
        expected_generation,
        candidate,
        identity.subject(),
        exchange.refresh_token(),
        now,
        "oidc_session_commit",
    )
}

pub(crate) async fn restore_verified_session(
    expected_generation: u64,
    credential: &oidc_refresh::RefreshCredential,
) -> Result<NativeOidcSessionStatus, String> {
    let exchange = oidc_refresh_flow::exchange_and_verify(credential).await?;
    let now = Instant::now();
    let candidate = prepare_session(
        exchange.subject(),
        exchange.identity_expires_at(),
        exchange.access_token(),
        exchange.expires_in(),
        now,
        Utc::now(),
    )
    .map_err(|code| session_prepare_error("oidc_refresh_session_prepare", code))?;

    commit_session(
        expected_generation,
        candidate,
        exchange.subject(),
        exchange.refresh_token(),
        now,
        "oidc_refresh_session_commit",
    )
}

fn session_prepare_error(stage: &str, code: &str) -> String {
    eprintln!("[Recorder][AuthHealth] stage={stage} ok=false code={code}");
    "OIDC access session could not be established".to_string()
}

fn commit_session(
    expected_generation: u64,
    mut candidate: NativeAccessSession,
    subject: Uuid,
    refresh_token: &[u8],
    now: Instant,
    stage: &str,
) -> Result<NativeOidcSessionStatus, String> {
    let mut state = runtime().lock();
    if state.generation != expected_generation {
        eprintln!(
            "[Recorder][AuthHealth] stage={stage} ok=false code=session_cancelled refresh_write_attempted=false"
        );
        return Err("OIDC access session was cancelled before commit".to_string());
    }

    oidc_refresh::persist_refresh_credential(subject, refresh_token).map_err(|error| {
        eprintln!(
            "[Recorder][AuthHealth] stage={stage} ok=false code=refresh_persist_failed session_installed=false"
        );
        error
    })?;

    candidate.refresh_token_persisted = true;
    state.active = Some(candidate);
    state.generation = state.generation.wrapping_add(1);
    let status = status_from_state(&mut state, now);
    drop(state);

    eprintln!(
        "[Recorder][AuthHealth] stage={stage} ok=true active=true access_token_native_only=true refresh_token_persisted=true subject_bound=true paid_access_granted=false"
    );
    Ok(status)
}

pub(crate) fn generation_snapshot() -> u64 {
    runtime().lock().generation
}

fn prepare_session(
    subject: Uuid,
    identity_expires_at: i64,
    access_token: &[u8],
    expires_in: u64,
    now: Instant,
    now_utc: DateTime<Utc>,
) -> Result<NativeAccessSession, &'static str> {
    if expires_in == 0 || expires_in > MAX_ACCESS_SESSION_SECONDS {
        return Err("access_token_lifetime_invalid");
    }
    let identity_remaining = identity_expires_at.saturating_sub(now_utc.timestamp());
    if identity_remaining <= 0 {
        return Err("identity_expired");
    }
    let lifetime_seconds = expires_in.min(identity_remaining as u64);
    if lifetime_seconds == 0 {
        return Err("session_lifetime_invalid");
    }

    let access_token = SecretBytes::from_slice(access_token)?;
    let lifetime = Duration::from_secs(lifetime_seconds);
    Ok(NativeAccessSession {
        subject,
        access_token,
        expires_at_instant: now + lifetime,
        expires_at: now_utc + chrono::Duration::seconds(lifetime_seconds as i64),
        refresh_token_persisted: false,
    })
}

#[cfg(test)]
fn install_session(
    expected_generation: u64,
    mut session: NativeAccessSession,
    refresh_token_persisted: bool,
    now: Instant,
) -> Result<NativeOidcSessionStatus, &'static str> {
    let mut state = runtime().lock();
    if state.generation != expected_generation {
        return Err("session_cancelled");
    }
    session.refresh_token_persisted = refresh_token_persisted;
    state.active = Some(session);
    state.generation = state.generation.wrapping_add(1);
    Ok(status_from_state(&mut state, now))
}

pub(crate) fn status() -> NativeOidcSessionStatus {
    status_at(Instant::now())
}

fn status_at(now: Instant) -> NativeOidcSessionStatus {
    let mut state = runtime().lock();
    status_from_state(&mut state, now)
}

fn status_from_state(state: &mut SessionState, now: Instant) -> NativeOidcSessionStatus {
    if state
        .active
        .as_ref()
        .is_some_and(|session| now >= session.expires_at_instant)
    {
        state.active.take();
    }
    NativeOidcSessionStatus {
        active: state.active.is_some(),
        expires_at: state.active.as_ref().map(|session| session.expires_at),
        access_token_native_only: true,
        refresh_token_persisted: state
            .active
            .as_ref()
            .is_some_and(|session| session.refresh_token_persisted),
    }
}

pub(crate) fn clear() -> bool {
    let mut state = runtime().lock();
    state.generation = state.generation.wrapping_add(1);
    state.active.take().is_some()
}

#[allow(dead_code)]
pub(crate) fn with_access_token<T>(
    operation: impl FnOnce(Uuid, &[u8]) -> Result<T, String>,
) -> Result<T, String> {
    let mut state = runtime().lock();
    let now = Instant::now();
    if state
        .active
        .as_ref()
        .is_some_and(|session| now >= session.expires_at_instant)
    {
        state.active.take();
    }
    let session = state
        .active
        .as_ref()
        .ok_or_else(|| "No verified native access session is active".to_string())?;
    operation(session.subject, session.access_token.expose())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_guard() -> parking_lot::MutexGuard<'static, ()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK.get_or_init(|| Mutex::new(())).lock()
    }

    fn reset() {
        clear();
    }

    fn candidate(
        subject: Uuid,
        identity_lifetime: i64,
        token_lifetime: u64,
        now: Instant,
        now_utc: DateTime<Utc>,
    ) -> Result<NativeAccessSession, &'static str> {
        prepare_session(
            subject,
            now_utc.timestamp() + identity_lifetime,
            b"header.payload.signature",
            token_lifetime,
            now,
            now_utc,
        )
    }

    #[test]
    fn verified_session_is_native_only_and_expires_at_the_shorter_deadline() {
        let _guard = test_guard();
        reset();
        let now = Instant::now();
        let now_utc = Utc::now();
        let subject = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let status = install_session(
            generation_snapshot(),
            candidate(subject, 120, 3600, now, now_utc).unwrap(),
            true,
            now,
        )
        .unwrap();
        assert!(status.active);
        assert!(status.access_token_native_only);
        assert!(status.refresh_token_persisted);
        assert_eq!(
            status.expires_at,
            Some(now_utc + chrono::Duration::seconds(120))
        );
        assert_eq!(
            with_access_token(|stored_subject, token| {
                assert_eq!(stored_subject, subject);
                assert_eq!(token, b"header.payload.signature");
                Ok(())
            }),
            Ok(())
        );
        let serialized = serde_json::to_string(&status).unwrap();
        assert!(!serialized.contains("11111111-1111-4111-8111-111111111111"));
        assert!(!serialized.contains("header.payload.signature"));
        assert!(!status_at(now + Duration::from_secs(121)).active);
    }

    #[test]
    fn successful_session_commit_invalidates_concurrent_candidates() {
        let _guard = test_guard();
        reset();
        let expected_generation = generation_snapshot();
        let now = Instant::now();
        let now_utc = Utc::now();
        let first = candidate(Uuid::new_v4(), 300, 300, now, now_utc).unwrap();
        let second = candidate(Uuid::new_v4(), 300, 300, now, now_utc).unwrap();

        install_session(expected_generation, first, true, now).unwrap();
        assert!(matches!(
            install_session(expected_generation, second, true, now),
            Err("session_cancelled")
        ));
    }

    #[test]
    fn session_candidate_is_fully_validated_before_commit() {
        let _guard = test_guard();
        reset();
        let now = Instant::now();
        let now_utc = Utc::now();
        let subject = Uuid::new_v4();
        assert!(matches!(
            prepare_session(subject, now_utc.timestamp(), b"a.b.c", 60, now, now_utc),
            Err("identity_expired")
        ));
        assert!(matches!(
            prepare_session(
                subject,
                now_utc.timestamp() + 60,
                b"bad token",
                60,
                now,
                now_utc,
            ),
            Err("access_token_text_invalid")
        ));
        assert!(!status().active);
    }

    #[test]
    fn clearing_drops_the_active_native_session() {
        let _guard = test_guard();
        reset();
        let now = Instant::now();
        let now_utc = Utc::now();
        install_session(
            generation_snapshot(),
            candidate(Uuid::new_v4(), 300, 300, now, now_utc).unwrap(),
            true,
            now,
        )
        .unwrap();
        assert!(clear());
        assert!(!clear());
        assert!(with_access_token(|_, _| Ok(())).is_err());
    }

    #[test]
    fn clear_during_exchange_prevents_late_session_commit() {
        let _guard = test_guard();
        reset();
        let expected_generation = generation_snapshot();
        let now = Instant::now();
        let now_utc = Utc::now();
        let prepared = candidate(Uuid::new_v4(), 300, 300, now, now_utc).unwrap();
        clear();

        assert!(matches!(
            install_session(expected_generation, prepared, true, now),
            Err("session_cancelled")
        ));
        assert!(!status().active);
    }
}
