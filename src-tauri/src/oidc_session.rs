//! Verified native OIDC access-session boundary.
//!
//! A session may be installed only after the bounded authorization-code exchange
//! and RS256/ES256 ID-token verification both succeed. Access-token bytes remain
//! native-only and in memory. Refresh-token persistence, rotation, revocation and
//! desktop entitlement enforcement are deliberately not implemented here.

use crate::{auth::SecureAuthStore, oidc_exchange, oidc_verify};
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

/// Exchange and verify the one-time callback grant, then install a native session.
///
/// The generation snapshot prevents a concurrent clear/logout operation from being
/// undone by a network request that completes later.
pub(crate) async fn establish_verified_session(
    store: &SecureAuthStore,
) -> Result<NativeOidcSessionStatus, String> {
    let expected_generation = generation();
    let exchange = oidc_exchange::exchange_authorization_code(store).await?;
    let identity =
        oidc_verify::verify_id_token(exchange.id_token(), exchange.expected_nonce()).await?;
    let status = install_session(
        expected_generation,
        identity.subject(),
        identity.expires_at(),
        exchange.access_token(),
        exchange.expires_in(),
        Instant::now(),
        Utc::now(),
    )
    .map_err(|code| {
        eprintln!("[Recorder][AuthHealth] stage=oidc_session_install ok=false code={code}");
        "OIDC access session could not be established".to_string()
    })?;

    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_session_install ok=true active=true access_token_native_only=true refresh_token_persisted=false"
    );
    Ok(status)
}

fn generation() -> u64 {
    runtime().lock().generation
}

fn install_session(
    expected_generation: u64,
    subject: Uuid,
    identity_expires_at: i64,
    access_token: &[u8],
    expires_in: u64,
    now: Instant,
    now_utc: DateTime<Utc>,
) -> Result<NativeOidcSessionStatus, &'static str> {
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
    let expires_at = now_utc + chrono::Duration::seconds(lifetime_seconds as i64);
    let mut state = runtime().lock();
    if state.generation != expected_generation {
        return Err("session_cancelled");
    }
    state.active = Some(NativeAccessSession {
        subject,
        access_token,
        expires_at_instant: now + lifetime,
        expires_at,
    });
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
        refresh_token_persisted: false,
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

    #[test]
    fn verified_session_is_native_only_and_expires_at_the_shorter_deadline() {
        let _guard = test_guard();
        reset();
        let now = Instant::now();
        let now_utc = Utc::now();
        let subject = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let status = install_session(
            generation(),
            subject,
            now_utc.timestamp() + 120,
            b"header.payload.signature",
            3600,
            now,
            now_utc,
        )
        .unwrap();
        assert!(status.active);
        assert!(status.access_token_native_only);
        assert!(!status.refresh_token_persisted);
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
    fn invalid_or_expired_session_inputs_fail_closed() {
        let _guard = test_guard();
        reset();
        let now = Instant::now();
        let now_utc = Utc::now();
        let subject = Uuid::new_v4();
        let current_generation = generation();
        assert_eq!(
            install_session(
                current_generation,
                subject,
                now_utc.timestamp(),
                b"a.b.c",
                60,
                now,
                now_utc,
            ),
            Err("identity_expired")
        );
        assert_eq!(
            install_session(
                current_generation,
                subject,
                now_utc.timestamp() + 60,
                b"bad token",
                60,
                now,
                now_utc,
            ),
            Err("access_token_text_invalid")
        );
        assert!(!status().active);
    }

    #[test]
    fn clearing_drops_the_active_native_session() {
        let _guard = test_guard();
        reset();
        let now = Instant::now();
        let now_utc = Utc::now();
        install_session(
            generation(),
            Uuid::new_v4(),
            now_utc.timestamp() + 300,
            b"a.b.c",
            300,
            now,
            now_utc,
        )
        .unwrap();
        assert!(clear());
        assert!(!clear());
        assert!(with_access_token(|_, _| Ok(())).is_err());
    }

    #[test]
    fn clear_during_exchange_prevents_late_session_install() {
        let _guard = test_guard();
        reset();
        let expected_generation = generation();
        let now = Instant::now();
        let now_utc = Utc::now();
        clear();

        assert_eq!(
            install_session(
                expected_generation,
                Uuid::new_v4(),
                now_utc.timestamp() + 300,
                b"a.b.c",
                300,
                now,
                now_utc,
            ),
            Err("session_cancelled")
        );
        assert!(!status().active);
    }
}
