//! Transactional native refresh-token persistence.
//!
//! The live OIDC completion path does not call this module yet. It provides the
//! fail-closed Windows Credential Manager replacement primitive needed before
//! refresh tokens may become durable. New values are read back after write; a
//! failed read-back restores the prior credential or removes the new credential.

use std::sync::atomic::{compiler_fence, Ordering};

const MAX_REFRESH_TOKEN_BYTES: usize = 5 * 512;
const REFRESH_TOKEN_TARGET: &str = "Recorder/app.recorder.desktop/saas-refresh-token/v1";

struct SecretBytes(Vec<u8>);

impl SecretBytes {
    fn from_vec(value: Vec<u8>) -> Self {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RefreshTokenWriteOutcome {
    pub(crate) replaced_existing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RefreshTokenReplaceFailure {
    code: &'static str,
    rollback_ok: bool,
}

pub(crate) fn persist_refresh_token(
    refresh_token: &[u8],
) -> Result<RefreshTokenWriteOutcome, String> {
    validate_refresh_token(refresh_token).map_err(|code| {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_persist ok=false code={code} rollback_ok=true"
        );
        "OIDC refresh credential is invalid".to_string()
    })?;

    #[cfg(windows)]
    {
        let result = replace_and_verify_with(
            refresh_token,
            windows_store::read_secret,
            windows_store::write_secret,
            windows_store::delete_secret,
        );
        match result {
            Ok(outcome) => {
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_refresh_persist ok=true replaced_existing={} readback_verified=true rollback_needed=false",
                    outcome.replaced_existing
                );
                Ok(outcome)
            }
            Err(failure) => {
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_refresh_persist ok=false code={} rollback_ok={}",
                    failure.code, failure.rollback_ok
                );
                Err("Unable to persist the OIDC refresh credential securely".to_string())
            }
        }
    }

    #[cfg(not(windows))]
    {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_persist ok=false code=unsupported rollback_ok=true"
        );
        Err("Native refresh credential storage is unavailable on this platform".to_string())
    }
}

fn validate_refresh_token(value: &[u8]) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > MAX_REFRESH_TOKEN_BYTES {
        return Err("size_invalid");
    }
    if value.iter().any(|byte| byte.is_ascii_control()) {
        return Err("text_invalid");
    }
    Ok(())
}

fn replace_and_verify_with<R, W, D>(
    new_secret: &[u8],
    mut read: R,
    mut write: W,
    mut delete: D,
) -> Result<RefreshTokenWriteOutcome, RefreshTokenReplaceFailure>
where
    R: FnMut() -> Result<Option<Vec<u8>>, String>,
    W: FnMut(&[u8]) -> Result<(), String>,
    D: FnMut() -> Result<(), String>,
{
    validate_refresh_token(new_secret).map_err(|code| RefreshTokenReplaceFailure {
        code,
        rollback_ok: true,
    })?;

    let previous = read()
        .map_err(|_| RefreshTokenReplaceFailure {
            code: "existing_read_failed",
            rollback_ok: true,
        })?
        .filter(|value| !value.is_empty())
        .map(SecretBytes::from_vec);
    let replaced_existing = previous.is_some();

    write(new_secret).map_err(|_| RefreshTokenReplaceFailure {
        code: "write_failed",
        rollback_ok: true,
    })?;

    let (readback_matches, failure_code) = match read() {
        Ok(Some(value)) => {
            let value = SecretBytes::from_vec(value);
            (
                constant_time_eq(value.expose(), new_secret),
                "readback_mismatch",
            )
        }
        Ok(None) => (false, "readback_missing"),
        Err(_) => (false, "readback_failed"),
    };
    if readback_matches {
        return Ok(RefreshTokenWriteOutcome { replaced_existing });
    }

    let rollback_ok = match previous.as_ref() {
        Some(value) => write(value.expose()).is_ok(),
        None => delete().is_ok(),
    };
    Err(RefreshTokenReplaceFailure {
        code: failure_code,
        rollback_ok,
    })
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

#[cfg(windows)]
mod windows_store {
    use super::{MAX_REFRESH_TOKEN_BYTES, REFRESH_TOKEN_TARGET};
    use std::ffi::c_void;
    use std::ptr;
    use windows::core::{HRESULT, PCWSTR, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS,
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
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
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_store_{stage} ok=false code={code}"
        );
        format!("Windows refresh credential operation failed at {stage} (code {code})")
    }

    pub(super) fn write_secret(secret: &[u8]) -> Result<(), String> {
        if secret.is_empty() || secret.len() > MAX_REFRESH_TOKEN_BYTES {
            return Err("Refresh credential size is invalid".to_string());
        }

        let mut target_name = wide_null(REFRESH_TOKEN_TARGET);
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

        let secret = unsafe {
            std::slice::from_raw_parts(credential.CredentialBlob as *const u8, secret_len).to_vec()
        };
        Ok(Some(secret))
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
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[test]
    fn refresh_token_bounds_match_windows_generic_credential_limit() {
        assert_eq!(MAX_REFRESH_TOKEN_BYTES, 2560);
        assert_eq!(validate_refresh_token(b"refresh.token"), Ok(()));
        assert_eq!(validate_refresh_token(b""), Err("size_invalid"));
        assert_eq!(
            validate_refresh_token(&vec![b'x'; MAX_REFRESH_TOKEN_BYTES + 1]),
            Err("size_invalid")
        );
        assert_eq!(validate_refresh_token(b"bad\nvalue"), Err("text_invalid"));
    }

    #[test]
    fn successful_replacement_is_read_back_and_reports_previous_state() {
        let stored = Rc::new(RefCell::new(Some(b"old.refresh".to_vec())));
        let read_store = stored.clone();
        let write_store = stored.clone();
        let delete_store = stored.clone();

        let outcome = replace_and_verify_with(
            b"new.refresh",
            move || Ok(read_store.borrow().clone()),
            move |value| {
                *write_store.borrow_mut() = Some(value.to_vec());
                Ok(())
            },
            move || {
                *delete_store.borrow_mut() = None;
                Ok(())
            },
        )
        .unwrap();

        assert!(outcome.replaced_existing);
        assert_eq!(stored.borrow().as_deref(), Some(b"new.refresh".as_slice()));
    }

    #[test]
    fn failed_readback_restores_the_previous_credential() {
        let stored = Rc::new(RefCell::new(Some(b"old.refresh".to_vec())));
        let reads = Rc::new(Cell::new(0_usize));
        let read_store = stored.clone();
        let read_count = reads.clone();
        let write_store = stored.clone();
        let delete_store = stored.clone();

        let result = replace_and_verify_with(
            b"new.refresh",
            move || {
                let count = read_count.get();
                read_count.set(count + 1);
                if count == 0 {
                    Ok(read_store.borrow().clone())
                } else {
                    Ok(Some(b"corrupted".to_vec()))
                }
            },
            move |value| {
                *write_store.borrow_mut() = Some(value.to_vec());
                Ok(())
            },
            move || {
                *delete_store.borrow_mut() = None;
                Ok(())
            },
        );

        assert_eq!(
            result,
            Err(RefreshTokenReplaceFailure {
                code: "readback_mismatch",
                rollback_ok: true,
            })
        );
        assert_eq!(stored.borrow().as_deref(), Some(b"old.refresh".as_slice()));
    }

    #[test]
    fn initial_write_failure_does_not_modify_the_previous_credential() {
        let stored = Rc::new(RefCell::new(Some(b"old.refresh".to_vec())));
        let read_store = stored.clone();
        let delete_store = stored.clone();

        let result = replace_and_verify_with(
            b"new.refresh",
            move || Ok(read_store.borrow().clone()),
            |_| Err("write failed".to_string()),
            move || {
                *delete_store.borrow_mut() = None;
                Ok(())
            },
        );

        assert_eq!(
            result,
            Err(RefreshTokenReplaceFailure {
                code: "write_failed",
                rollback_ok: true,
            })
        );
        assert_eq!(stored.borrow().as_deref(), Some(b"old.refresh".as_slice()));
    }
}
