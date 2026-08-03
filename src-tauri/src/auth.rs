//! Native authentication state and secure storage boundaries for future SaaS login.
//!
//! Refresh tokens must never be persisted in Recorder JSON, localStorage, or logs.
//! On Windows, opaque session bytes are stored as a generic credential in the
//! current user's Windows Credential Manager. OIDC PKCE verifiers remain native,
//! in memory, expire after ten minutes, and are never returned through Tauri.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{compiler_fence, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_CREDENTIAL_BYTES: usize = 5 * 512;
const OIDC_TRANSACTION_TTL_SECONDS: u64 = 10 * 60;
const OIDC_TRANSACTION_TTL: Duration = Duration::from_secs(OIDC_TRANSACTION_TTL_SECONDS);
const OIDC_TOKEN_BYTES: usize = 32;

#[derive(Clone)]
pub struct SecureAuthStore {
    state: Arc<Mutex<AuthState>>,
}

#[derive(Default)]
struct AuthState {
    pending_oidc: Option<PendingOidcTransaction>,
}

struct PendingOidcTransaction {
    state: String,
    nonce: String,
    code_verifier: SecretBytes,
    expires_at_instant: Instant,
    expires_at: DateTime<Utc>,
}

pub(crate) struct OidcExchangeMaterial {
    nonce: SecretBytes,
    code_verifier: SecretBytes,
}

impl OidcExchangeMaterial {
    pub(crate) fn nonce(&self) -> &[u8] {
        self.nonce.expose()
    }

    pub(crate) fn code_verifier(&self) -> &[u8] {
        self.code_verifier.expose()
    }
}

struct SecretBytes(Vec<u8>);

impl SecretBytes {
    fn new(value: String) -> Self {
        Self(value.into_bytes())
    }

    fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        // Best-effort native-memory clearing. Volatile writes and a compiler fence
        // make it harder for the optimiser to remove the overwrite. This does not
        // claim protection from a process already running as the same OS user.
        for byte in &mut self.0 {
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        compiler_fence(Ordering::SeqCst);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OidcConsumeError {
    Missing,
    Expired,
    StateMismatch,
}

impl OidcConsumeError {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Expired => "expired",
            Self::StateMismatch => "state_mismatch",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureAuthStatus {
    pub supported: bool,
    pub signed_in: bool,
    pub storage: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureAuthProbe {
    pub supported: bool,
    pub round_trip_ok: bool,
    pub storage: &'static str,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcAuthorizationPreparation {
    pub state: String,
    pub nonce: String,
    pub code_challenge: String,
    pub code_challenge_method: &'static str,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcTransactionStatus {
    pub pending: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcTransactionProbe {
    pub s256_ready: bool,
    pub state_round_trip_ok: bool,
    pub nonce_retained: bool,
    pub replay_rejected: bool,
    pub verifier_kept_native: bool,
}

impl AuthState {
    fn begin_oidc_transaction(
        &mut self,
        now: Instant,
        now_utc: DateTime<Utc>,
    ) -> OidcAuthorizationPreparation {
        let state = random_protocol_token();
        let nonce = random_protocol_token();
        let verifier = random_protocol_token();
        let code_challenge = pkce_s256_challenge(verifier.as_bytes());
        let expires_at = now_utc + chrono::Duration::seconds(OIDC_TRANSACTION_TTL_SECONDS as i64);

        self.pending_oidc = Some(PendingOidcTransaction {
            state: state.clone(),
            nonce: nonce.clone(),
            code_verifier: SecretBytes::new(verifier),
            expires_at_instant: now + OIDC_TRANSACTION_TTL,
            expires_at,
        });

        OidcAuthorizationPreparation {
            state,
            nonce,
            code_challenge,
            code_challenge_method: "S256",
            expires_at,
        }
    }

    fn oidc_status(&mut self, now: Instant) -> OidcTransactionStatus {
        if self
            .pending_oidc
            .as_ref()
            .is_some_and(|pending| now >= pending.expires_at_instant)
        {
            self.pending_oidc.take();
        }

        match self.pending_oidc.as_ref() {
            Some(pending) => OidcTransactionStatus {
                pending: true,
                expires_at: Some(pending.expires_at),
                expires_in_seconds: pending
                    .expires_at_instant
                    .saturating_duration_since(now)
                    .as_secs(),
            },
            None => OidcTransactionStatus {
                pending: false,
                expires_at: None,
                expires_in_seconds: 0,
            },
        }
    }

    fn cancel_oidc_transaction(&mut self) -> bool {
        self.pending_oidc.take().is_some()
    }

    fn consume_oidc_state(
        &mut self,
        supplied_state: &[u8],
        now: Instant,
    ) -> Result<OidcExchangeMaterial, OidcConsumeError> {
        let Some(pending) = self.pending_oidc.as_ref() else {
            return Err(OidcConsumeError::Missing);
        };

        if now >= pending.expires_at_instant {
            self.pending_oidc.take();
            return Err(OidcConsumeError::Expired);
        }

        if !constant_time_eq(pending.state.as_bytes(), supplied_state) {
            return Err(OidcConsumeError::StateMismatch);
        }

        let Some(pending) = self.pending_oidc.take() else {
            return Err(OidcConsumeError::Missing);
        };
        Ok(OidcExchangeMaterial {
            nonce: SecretBytes::new(pending.nonce),
            code_verifier: pending.code_verifier,
        })
    }
}

impl SecureAuthStore {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(AuthState::default())),
        }
    }

    pub fn status(&self) -> Result<SecureAuthStatus, String> {
        let _operation_guard = self.state.lock();

        #[cfg(windows)]
        {
            let signed_in = windows_store::read_secret(windows_store::CredentialTarget::Session)?
                .is_some_and(|secret| !secret.is_empty());
            eprintln!(
                "[Recorder][AuthHealth] stage=status ok=true supported=true signed_in={signed_in}"
            );
            return Ok(SecureAuthStatus {
                supported: true,
                signed_in,
                storage: "windows-credential-manager",
            });
        }

        #[cfg(not(windows))]
        {
            eprintln!(
                "[Recorder][AuthHealth] stage=status ok=true supported=false signed_in=false"
            );
            Ok(SecureAuthStatus {
                supported: false,
                signed_in: false,
                storage: "unsupported",
            })
        }
    }

    /// Verify the native secure store without handling a real account token.
    /// Random probe bytes use a dedicated credential target and are deleted before
    /// this method returns, including best-effort cleanup after read failures.
    pub fn probe(&self) -> Result<SecureAuthProbe, String> {
        let _operation_guard = self.state.lock();

        #[cfg(windows)]
        {
            let probe = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
            windows_store::write_secret(windows_store::CredentialTarget::Probe, probe.as_bytes())?;

            let read_result = windows_store::read_secret(windows_store::CredentialTarget::Probe);
            let delete_result =
                windows_store::delete_secret(windows_store::CredentialTarget::Probe);

            let stored = read_result?;
            delete_result?;
            let round_trip_ok = stored.as_deref() == Some(probe.as_bytes());
            eprintln!(
                "[Recorder][AuthHealth] stage=probe ok={round_trip_ok} supported=true round_trip={round_trip_ok}"
            );
            if !round_trip_ok {
                return Err("Windows secure-store probe returned different data".to_string());
            }

            return Ok(SecureAuthProbe {
                supported: true,
                round_trip_ok: true,
                storage: "windows-credential-manager",
            });
        }

        #[cfg(not(windows))]
        {
            eprintln!(
                "[Recorder][AuthHealth] stage=probe ok=true supported=false round_trip=false"
            );
            Ok(SecureAuthProbe {
                supported: false,
                round_trip_ok: false,
                storage: "unsupported",
            })
        }
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut auth_state = self.state.lock();
        auth_state.cancel_oidc_transaction();

        #[cfg(windows)]
        {
            windows_store::delete_secret(windows_store::CredentialTarget::Session)?;
            eprintln!("[Recorder][AuthHealth] stage=clear ok=true supported=true");
            return Ok(());
        }

        #[cfg(not(windows))]
        {
            eprintln!("[Recorder][AuthHealth] stage=clear ok=true supported=false");
            Ok(())
        }
    }

    pub fn prepare_oidc_transaction(&self) -> OidcAuthorizationPreparation {
        let mut state = self.state.lock();
        let preparation = state.begin_oidc_transaction(Instant::now(), Utc::now());
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_prepare ok=true pending=true expires_in_seconds={OIDC_TRANSACTION_TTL_SECONDS}"
        );
        preparation
    }

    pub fn oidc_transaction_status(&self) -> OidcTransactionStatus {
        let mut state = self.state.lock();
        let status = state.oidc_status(Instant::now());
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_status ok=true pending={} expires_in_seconds={}",
            status.pending, status.expires_in_seconds
        );
        status
    }

    pub fn cancel_oidc_transaction(&self) {
        let mut state = self.state.lock();
        let had_pending = state.cancel_oidc_transaction();
        eprintln!("[Recorder][AuthHealth] stage=oidc_cancel ok=true had_pending={had_pending}");
    }

    pub(crate) fn take_oidc_exchange_material(
        &self,
        supplied_state: &[u8],
    ) -> Result<OidcExchangeMaterial, OidcConsumeError> {
        let mut state = self.state.lock();
        let result = state.consume_oidc_state(supplied_state, Instant::now());
        match result {
            Ok(material) => {
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_pkce_take ok=true nonce_kept_native=true verifier_kept_native=true replay_rejected=true"
                );
                Ok(material)
            }
            Err(error) => {
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_pkce_take ok=false code={}",
                    error.code()
                );
                Err(error)
            }
        }
    }

    /// Exercise the PKCE/state lifecycle without contacting an identity provider
    /// and without mutating a real pending login transaction.
    pub fn probe_oidc_transaction(&self) -> OidcTransactionProbe {
        let _operation_guard = self.state.lock();
        let now = Instant::now();
        let mut probe_state = AuthState::default();
        let preparation = probe_state.begin_oidc_transaction(now, Utc::now());

        let consumed = probe_state.consume_oidc_state(preparation.state.as_bytes(), now);
        let (state_round_trip_ok, nonce_retained, s256_ready) = match consumed.as_ref() {
            Ok(transaction) => (
                true,
                constant_time_eq(transaction.nonce(), preparation.nonce.as_bytes()),
                constant_time_eq(
                    pkce_s256_challenge(transaction.code_verifier()).as_bytes(),
                    preparation.code_challenge.as_bytes(),
                ),
            ),
            Err(_) => (false, false, false),
        };
        let replay_rejected = matches!(
            probe_state.consume_oidc_state(preparation.state.as_bytes(), now),
            Err(OidcConsumeError::Missing)
        );
        let ok = state_round_trip_ok && nonce_retained && s256_ready && replay_rejected;

        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_probe ok={ok} s256={s256_ready} state_round_trip={state_round_trip_ok} nonce_retained={nonce_retained} replay_rejected={replay_rejected} verifier_kept_native=true"
        );

        OidcTransactionProbe {
            s256_ready,
            state_round_trip_ok,
            nonce_retained,
            replay_rejected,
            verifier_kept_native: true,
        }
    }
}

fn random_protocol_token() -> String {
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let mut bytes = [0_u8; OIDC_TOKEN_BYTES];
    bytes[..16].copy_from_slice(first.as_bytes());
    bytes[16..].copy_from_slice(second.as_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_s256_challenge(verifier: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(sha256(verifier))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut difference = 0_u8;
    for (left_byte, right_byte) in left.iter().zip(right.iter()) {
        difference |= left_byte ^ right_byte;
    }
    difference == 0
}

// Compact SHA-256 implementation used only for the RFC 7636 S256 challenge.
// The RFC test vector below guards the interoperable output. No signing,
// password hashing, token verification, or general-purpose crypto uses this code.
fn sha256(input: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    const ROUND: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];

    let bit_length = (input.len() as u64).wrapping_mul(8);
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());

    let mut hash = INITIAL;
    for chunk in message.chunks_exact(64) {
        let mut words = [0_u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..64 {
            let sigma0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let sigma1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(sigma0)
                .wrapping_add(words[index - 7])
                .wrapping_add(sigma1);
        }

        let mut a = hash[0];
        let mut b = hash[1];
        let mut c = hash[2];
        let mut d = hash[3];
        let mut e = hash[4];
        let mut f = hash[5];
        let mut g = hash[6];
        let mut h = hash[7];

        for index in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temporary1 = h
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(ROUND[index])
                .wrapping_add(words[index]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temporary2 = sum0.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temporary1);
            d = c;
            c = b;
            b = a;
            a = temporary1.wrapping_add(temporary2);
        }

        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
        hash[5] = hash[5].wrapping_add(f);
        hash[6] = hash[6].wrapping_add(g);
        hash[7] = hash[7].wrapping_add(h);
    }

    let mut output = [0_u8; 32];
    for (index, value) in hash.iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    output
}

#[cfg(windows)]
mod windows_store {
    use super::MAX_CREDENTIAL_BYTES;
    use std::ffi::c_void;
    use std::ptr;
    use windows::core::{HRESULT, PCWSTR, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS,
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    const HRESULT_NOT_FOUND: HRESULT = HRESULT(0x8007_0490_u32 as i32);

    #[derive(Clone, Copy)]
    pub enum CredentialTarget {
        Session,
        Probe,
    }

    impl CredentialTarget {
        fn value(self) -> &'static str {
            match self {
                Self::Session => "Recorder/app.recorder.desktop/saas-refresh-token/v1",
                Self::Probe => "Recorder/app.recorder.desktop/secure-store-probe/v1",
            }
        }
    }

    struct CredentialBuffer(*mut CREDENTIALW);

    impl Drop for CredentialBuffer {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CredFree(self.0 as *const c_void) };
            }
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn error_code(error: &windows::core::Error) -> i32 {
        error.code().0
    }

    fn operation_error(stage: &str, error: &windows::core::Error) -> String {
        let code = error_code(error);
        eprintln!("[Recorder][AuthHealth] stage={stage} ok=false supported=true code={code}");
        format!("Windows secure-store operation failed at {stage} (code {code})")
    }

    pub fn write_secret(target: CredentialTarget, secret: &[u8]) -> Result<(), String> {
        if secret.is_empty() || secret.len() > MAX_CREDENTIAL_BYTES {
            return Err(format!(
                "Secure credential must contain 1 to {MAX_CREDENTIAL_BYTES} bytes"
            ));
        }

        let mut target_name = wide_null(target.value());
        let credential = CREDENTIALW {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target_name.as_mut_ptr()),
            Comment: PWSTR::default(),
            LastWritten: Default::default(),
            CredentialBlobSize: secret.len() as u32,
            CredentialBlob: secret.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: PWSTR::default(),
            UserName: PWSTR::default(),
        };

        unsafe { CredWriteW(&credential, 0) }.map_err(|error| operation_error("write", &error))
    }

    pub fn read_secret(target: CredentialTarget) -> Result<Option<Vec<u8>>, String> {
        let target_name = wide_null(target.value());
        let mut raw_credential: *mut CREDENTIALW = ptr::null_mut();
        let result = unsafe {
            CredReadW(
                PCWSTR(target_name.as_ptr()),
                CRED_TYPE_GENERIC,
                None,
                &mut raw_credential,
            )
        };

        if let Err(error) = result {
            if error.code() == HRESULT_NOT_FOUND {
                return Ok(None);
            }
            return Err(operation_error("read", &error));
        }
        if raw_credential.is_null() {
            return Err("Windows secure-store returned an empty credential pointer".to_string());
        }

        let buffer = CredentialBuffer(raw_credential);
        let credential = unsafe { &*buffer.0 };
        let secret_len = credential.CredentialBlobSize as usize;
        if secret_len > MAX_CREDENTIAL_BYTES {
            return Err("Windows secure-store returned an oversized credential".to_string());
        }
        if secret_len == 0 {
            return Ok(Some(Vec::new()));
        }
        if credential.CredentialBlob.is_null() {
            return Err("Windows secure-store returned a missing credential blob".to_string());
        }

        let secret = unsafe {
            std::slice::from_raw_parts(credential.CredentialBlob as *const u8, secret_len).to_vec()
        };
        Ok(Some(secret))
    }

    pub fn delete_secret(target: CredentialTarget) -> Result<(), String> {
        let target_name = wide_null(target.value());
        match unsafe { CredDeleteW(PCWSTR(target_name.as_ptr()), CRED_TYPE_GENERIC, None) } {
            Ok(()) => Ok(()),
            Err(error) if error.code() == HRESULT_NOT_FOUND => Ok(()),
            Err(error) => Err(operation_error("delete", &error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_limit_matches_windows_generic_blob_limit() {
        assert_eq!(MAX_CREDENTIAL_BYTES, 2560);
    }

    #[test]
    fn pkce_s256_matches_rfc_7636_vector() {
        let verifier = b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_s256_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generated_tokens_are_pkce_compatible() {
        for _ in 0..16 {
            let token = random_protocol_token();
            assert_eq!(token.len(), 43);
            assert!(token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'));
        }
    }

    #[test]
    fn valid_state_is_consumed_once_and_replay_is_rejected() {
        let now = Instant::now();
        let mut state = AuthState::default();
        let preparation = state.begin_oidc_transaction(now, Utc::now());

        let consumed = state
            .consume_oidc_state(preparation.state.as_bytes(), now)
            .expect("matching state should consume the transaction");
        assert!(constant_time_eq(consumed.nonce(), preparation.nonce.as_bytes()));
        assert_eq!(
            pkce_s256_challenge(consumed.code_verifier()),
            preparation.code_challenge
        );
        assert!(matches!(
            state.consume_oidc_state(preparation.state.as_bytes(), now),
            Err(OidcConsumeError::Missing)
        ));
    }

    #[test]
    fn mismatched_state_does_not_consume_valid_transaction() {
        let now = Instant::now();
        let mut state = AuthState::default();
        let preparation = state.begin_oidc_transaction(now, Utc::now());

        assert!(matches!(
            state.consume_oidc_state(b"different-state", now),
            Err(OidcConsumeError::StateMismatch)
        ));
        assert!(state.oidc_status(now).pending);
        assert!(state.consume_oidc_state(preparation.state.as_bytes(), now).is_ok());
    }

    #[test]
    fn expired_transaction_is_cleared() {
        let now = Instant::now();
        let mut state = AuthState::default();
        let preparation = state.begin_oidc_transaction(now, Utc::now());

        assert!(matches!(
            state.consume_oidc_state(preparation.state.as_bytes(), now + OIDC_TRANSACTION_TTL),
            Err(OidcConsumeError::Expired)
        ));
        assert!(!state.oidc_status(now + OIDC_TRANSACTION_TTL).pending);
    }
}
