//! Native secure storage boundary for future SaaS authentication.
//!
//! Refresh tokens must never be persisted in Recorder JSON, localStorage, or logs.
//! On Windows, opaque secret bytes are stored as a generic credential in the
//! current user's Windows Credential Manager. The public Tauri surface exposes
//! only non-secret status, a random round-trip probe, and deletion.

use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;

const MAX_CREDENTIAL_BYTES: usize = 5 * 512;

#[derive(Clone, Debug)]
pub struct SecureAuthStore {
    operation_lock: Arc<Mutex<()>>,
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

impl SecureAuthStore {
    pub fn new() -> Self {
        Self {
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn status(&self) -> Result<SecureAuthStatus, String> {
        let _operation_guard = self.operation_lock.lock();

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
        let _operation_guard = self.operation_lock.lock();

        #[cfg(windows)]
        {
            let probe = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
            windows_store::write_secret(
                windows_store::CredentialTarget::Probe,
                probe.as_bytes(),
            )?;

            let read_result = windows_store::read_secret(windows_store::CredentialTarget::Probe);
            let delete_result = windows_store::delete_secret(windows_store::CredentialTarget::Probe);

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
        let _operation_guard = self.operation_lock.lock();

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
        eprintln!(
            "[Recorder][AuthHealth] stage={stage} ok=false supported=true code={code}"
        );
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

        unsafe { CredWriteW(&credential, 0) }
            .map_err(|error| operation_error("write", &error))
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
            std::slice::from_raw_parts(credential.CredentialBlob as *const u8, secret_len)
                .to_vec()
        };
        Ok(Some(secret))
    }

    pub fn delete_secret(target: CredentialTarget) -> Result<(), String> {
        let target_name = wide_null(target.value());
        match unsafe {
            CredDeleteW(
                PCWSTR(target_name.as_ptr()),
                CRED_TYPE_GENERIC,
                None,
            )
        } {
            Ok(()) => Ok(()),
            Err(error) if error.code() == HRESULT_NOT_FOUND => Ok(()),
            Err(error) => Err(operation_error("delete", &error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MAX_CREDENTIAL_BYTES;

    #[test]
    fn credential_limit_matches_windows_generic_blob_limit() {
        assert_eq!(MAX_CREDENTIAL_BYTES, 2560);
    }
}
