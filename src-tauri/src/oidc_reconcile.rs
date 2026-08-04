//! Persisted refresh-credential reconciliation for native OIDC sessions.
//!
//! A durable refresh credential is not itself an authenticated session. This module
//! inspects only Recorder's dedicated Windows Credential Manager target and publishes
//! non-secret restart state. Automatic network refresh remains a separate boundary.

use crate::oidc_session::{self, NativeOidcSessionStatus};
use serde::Serialize;
use std::sync::atomic::{compiler_fence, Ordering};

const MAX_REFRESH_TOKEN_BYTES: usize = 5 * 512;
const REFRESH_TOKEN_TARGET: &str = "Recorder/app.recorder.desktop/saas-refresh-token/v1";

struct SecretBytes(Vec<u8>);

impl SecretBytes {
    fn new(value: Vec<u8>) -> Self {
        Self(value)
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

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeOidcSessionOverview {
    pub active: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub access_token_native_only: bool,
    pub refresh_token_persisted: bool,
    pub reconciliation_complete: bool,
    pub refresh_credential_present: bool,
    pub restoration_required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RefreshCredentialInspection {
    present: bool,
    usable: bool,
}

pub(crate) fn reconcile_and_status() -> Result<NativeOidcSessionOverview, String> {
    let session = oidc_session::status();
    let inspection = inspect_persisted_refresh_credential()?;
    let restoration_required = !session.active && inspection.present && inspection.usable;
    let overview = overview(session, inspection, restoration_required);
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_startup_reconcile ok=true active={} refresh_present={} refresh_usable={} restoration_required={} automatic_refresh=false paid_access_granted=false",
        overview.active,
        overview.refresh_credential_present,
        inspection.usable,
        overview.restoration_required
    );
    Ok(overview)
}

pub(crate) fn reconcile_async() {
    let _ = std::thread::Builder::new()
        .name("recorder-oidc-reconcile".to_string())
        .spawn(|| {
            if let Err(error) = reconcile_and_status() {
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_startup_reconcile ok=false code=credential_read_failed automatic_refresh=false"
                );
                drop(error);
            }
        });
}

pub(crate) fn clear_persisted_refresh_credential() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let previous = windows_store::read_secret()?.map(SecretBytes::new);
        let present = previous.is_some();
        windows_store::delete_secret()?;
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_clear ok=true previously_present={present}"
        );
        Ok(present)
    }

    #[cfg(not(windows))]
    {
        eprintln!("[Recorder][AuthHealth] stage=oidc_refresh_clear ok=false code=unsupported");
        Err("Native refresh credential storage is unavailable on this platform".to_string())
    }
}

fn overview(
    session: NativeOidcSessionStatus,
    inspection: RefreshCredentialInspection,
    restoration_required: bool,
) -> NativeOidcSessionOverview {
    NativeOidcSessionOverview {
        active: session.active,
        expires_at: session.expires_at,
        access_token_native_only: session.access_token_native_only,
        refresh_token_persisted: session.refresh_token_persisted,
        reconciliation_complete: true,
        refresh_credential_present: inspection.present,
        restoration_required,
    }
}

fn validate_refresh_token(value: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REFRESH_TOKEN_BYTES
        && !value.iter().any(|byte| byte.is_ascii_control())
}

fn inspect_persisted_refresh_credential() -> Result<RefreshCredentialInspection, String> {
    #[cfg(windows)]
    {
        let secret = windows_store::read_secret()?.map(SecretBytes::new);
        Ok(match secret.as_ref() {
            Some(secret) => RefreshCredentialInspection {
                present: true,
                usable: validate_refresh_token(secret.expose()),
            },
            None => RefreshCredentialInspection {
                present: false,
                usable: false,
            },
        })
    }

    #[cfg(not(windows))]
    {
        Ok(RefreshCredentialInspection {
            present: false,
            usable: false,
        })
    }
}

#[cfg(windows)]
mod windows_store {
    use super::{MAX_REFRESH_TOKEN_BYTES, REFRESH_TOKEN_TARGET};
    use std::ffi::c_void;
    use std::ptr;
    use windows::core::{HRESULT, PCWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    const HRESULT_NOT_FOUND: HRESULT = HRESULT(0x8007_0490_u32 as i32);

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

    fn operation_error(stage: &str, error: &windows::core::Error) -> String {
        let code = error.code().0;
        eprintln!("[Recorder][AuthHealth] stage=oidc_reconcile_store_{stage} ok=false code={code}");
        format!("Windows refresh credential operation failed at {stage} (code {code})")
    }

    pub(super) fn read_secret() -> Result<Option<Vec<u8>>, String> {
        let target_name = wide_null(REFRESH_TOKEN_TARGET);
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
            return Err("Windows refresh credential returned an empty pointer".to_string());
        }

        let buffer = CredentialBuffer(raw_credential);
        let credential = unsafe { &*buffer.0 };
        let secret_len = credential.CredentialBlobSize as usize;
        if secret_len > MAX_REFRESH_TOKEN_BYTES {
            return Err("Windows refresh credential is oversized".to_string());
        }
        if secret_len == 0 {
            return Ok(Some(Vec::new()));
        }
        if credential.CredentialBlob.is_null() {
            return Err("Windows refresh credential blob is missing".to_string());
        }
        Ok(Some(unsafe {
            std::slice::from_raw_parts(credential.CredentialBlob as *const u8, secret_len).to_vec()
        }))
    }

    pub(super) fn delete_secret() -> Result<(), String> {
        let target_name = wide_null(REFRESH_TOKEN_TARGET);
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

    fn inactive_session() -> NativeOidcSessionStatus {
        NativeOidcSessionStatus {
            active: false,
            expires_at: None,
            access_token_native_only: true,
            refresh_token_persisted: false,
        }
    }

    #[test]
    fn persisted_refresh_requires_restoration_but_does_not_create_session() {
        let status = overview(
            inactive_session(),
            RefreshCredentialInspection {
                present: true,
                usable: true,
            },
            true,
        );
        assert!(!status.active);
        assert!(status.refresh_credential_present);
        assert!(status.restoration_required);
        assert!(status.reconciliation_complete);
    }

    #[test]
    fn malformed_or_missing_refresh_does_not_claim_restorable_session() {
        assert!(validate_refresh_token(b"valid.refresh"));
        assert!(!validate_refresh_token(b""));
        assert!(!validate_refresh_token(b"bad\nrefresh"));
        assert!(!validate_refresh_token(&vec![
            b'x';
            MAX_REFRESH_TOKEN_BYTES + 1
        ]));
    }
}
