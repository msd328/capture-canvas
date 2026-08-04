//! Subject-bound Windows refresh-credential storage.
//!
//! The v2 binary envelope binds the opaque refresh token to the UUID subject
//! verified during sign-in. Writes are read back before success and roll back on
//! mismatch. Raw v1 credentials are detected for cleanup but never auto-restored.

use std::sync::atomic::{compiler_fence, Ordering};
use uuid::Uuid;

const MAX_CREDENTIAL_BYTES: usize = 5 * 512;
const MAGIC: &[u8] = b"RECORDER-OIDC-REFRESH\0\x02";
const SUBJECT_BYTES: usize = 16;
pub(crate) const MAX_REFRESH_TOKEN_BYTES: usize = MAX_CREDENTIAL_BYTES - MAGIC.len() - SUBJECT_BYTES;
const TARGET_V2: &str = "Recorder/app.recorder.desktop/saas-refresh-token/v2";
const TARGET_V1: &str = "Recorder/app.recorder.desktop/saas-refresh-token/v1";

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

pub(crate) struct RefreshCredential {
    subject: Uuid,
    token: SecretBytes,
}

impl RefreshCredential {
    pub(crate) const fn subject(&self) -> Uuid {
        self.subject
    }

    pub(crate) fn token(&self) -> &[u8] {
        self.token.expose()
    }
}

pub(crate) struct RefreshCredentialState {
    pub(crate) credential: Option<RefreshCredential>,
    pub(crate) legacy_present: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RefreshCredentialWriteOutcome {
    pub(crate) replaced_existing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReplaceFailure {
    code: &'static str,
    rollback_ok: bool,
}

pub(crate) fn persist_refresh_credential(
    subject: Uuid,
    refresh_token: &[u8],
) -> Result<RefreshCredentialWriteOutcome, String> {
    let encoded = encode(subject, refresh_token).map_err(|code| {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_persist ok=false code={code} rollback_ok=true subject_bound=false"
        );
        "OIDC refresh credential is invalid".to_string()
    })?;

    #[cfg(windows)]
    {
        windows_store::delete(TARGET_V1).map_err(|error| {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_refresh_persist ok=false code=legacy_delete_failed rollback_ok=true subject_bound=false"
            );
            error
        })?;
        let result = replace_with(
            encoded.expose(),
            || windows_store::read(TARGET_V2),
            |value| windows_store::write(TARGET_V2, value),
            || windows_store::delete(TARGET_V2),
        );
        match result {
            Ok(outcome) => {
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_refresh_persist ok=true replaced_existing={} readback_verified=true subject_bound=true version=2",
                    outcome.replaced_existing
                );
                Ok(outcome)
            }
            Err(failure) => {
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_refresh_persist ok=false code={} rollback_ok={} subject_bound=true version=2",
                    failure.code, failure.rollback_ok
                );
                Err("Unable to persist the OIDC refresh credential securely".to_string())
            }
        }
    }

    #[cfg(not(windows))]
    {
        drop(encoded);
        Err("Native refresh credential storage is unavailable on this platform".to_string())
    }
}

pub(crate) fn load_refresh_credential_state() -> Result<RefreshCredentialState, String> {
    #[cfg(windows)]
    {
        let legacy = windows_store::read(TARGET_V1)?.map(SecretBytes::new);
        let legacy_present = legacy.is_some();
        let current = windows_store::read(TARGET_V2)?;
        let credential = current
            .map(SecretBytes::new)
            .map(decode)
            .transpose()
            .map_err(|code| {
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_refresh_load ok=false code={code} subject_bound=false version=2"
                );
                "Stored OIDC refresh credential is invalid".to_string()
            })?;
        drop(legacy);
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_load ok=true current_present={} legacy_present={} subject_bound={}",
            credential.is_some(),
            legacy_present,
            credential.is_some()
        );
        Ok(RefreshCredentialState {
            credential,
            legacy_present,
        })
    }

    #[cfg(not(windows))]
    {
        Ok(RefreshCredentialState {
            credential: None,
            legacy_present: false,
        })
    }
}

pub(crate) fn clear_persisted_refresh_credentials() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let current = windows_store::read(TARGET_V2)?.map(SecretBytes::new);
        let legacy = windows_store::read(TARGET_V1)?.map(SecretBytes::new);
        let present = current.is_some() || legacy.is_some();
        let current_result = windows_store::delete(TARGET_V2);
        let legacy_result = windows_store::delete(TARGET_V1);
        drop(current);
        drop(legacy);
        if current_result.is_ok() && legacy_result.is_ok() {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_refresh_clear ok=true previously_present={present}"
            );
            return Ok(present);
        }
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_clear ok=false current_cleared={} legacy_cleared={}",
            current_result.is_ok(),
            legacy_result.is_ok()
        );
        Err(current_result
            .err()
            .or_else(|| legacy_result.err())
            .unwrap_or_else(|| "Unable to clear native refresh credentials".to_string()))
    }

    #[cfg(not(windows))]
    {
        Err("Native refresh credential storage is unavailable on this platform".to_string())
    }
}

fn encode(subject: Uuid, token: &[u8]) -> Result<SecretBytes, &'static str> {
    validate_token(token)?;
    let mut value = Vec::with_capacity(MAGIC.len() + SUBJECT_BYTES + token.len());
    value.extend_from_slice(MAGIC);
    value.extend_from_slice(subject.as_bytes());
    value.extend_from_slice(token);
    Ok(SecretBytes::new(value))
}

fn decode(value: SecretBytes) -> Result<RefreshCredential, &'static str> {
    let bytes = value.expose();
    let subject_start = MAGIC.len();
    let subject_end = subject_start + SUBJECT_BYTES;
    if bytes.len() <= subject_end
        || bytes.len() > MAX_CREDENTIAL_BYTES
        || !bytes.starts_with(MAGIC)
    {
        return Err("envelope_invalid");
    }
    let subject = Uuid::from_slice(&bytes[subject_start..subject_end])
        .map_err(|_| "subject_invalid")?;
    validate_token(&bytes[subject_end..])?;
    let token = SecretBytes::new(bytes[subject_end..].to_vec());
    drop(value);
    Ok(RefreshCredential { subject, token })
}

fn validate_token(value: &[u8]) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > MAX_REFRESH_TOKEN_BYTES {
        return Err("token_size_invalid");
    }
    if value
        .iter()
        .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("token_text_invalid");
    }
    Ok(())
}

fn replace_with<R, W, D>(
    new_secret: &[u8],
    mut read: R,
    mut write: W,
    mut delete: D,
) -> Result<RefreshCredentialWriteOutcome, ReplaceFailure>
where
    R: FnMut() -> Result<Option<Vec<u8>>, String>,
    W: FnMut(&[u8]) -> Result<(), String>,
    D: FnMut() -> Result<(), String>,
{
    let previous = read()
        .map_err(|_| ReplaceFailure {
            code: "existing_read_failed",
            rollback_ok: true,
        })?
        .filter(|value| !value.is_empty())
        .map(SecretBytes::new);
    let replaced_existing = previous.is_some();
    write(new_secret).map_err(|_| ReplaceFailure {
        code: "write_failed",
        rollback_ok: true,
    })?;
    let readback_ok = read()
        .ok()
        .flatten()
        .map(SecretBytes::new)
        .is_some_and(|value| constant_time_eq(value.expose(), new_secret));
    if readback_ok {
        return Ok(RefreshCredentialWriteOutcome { replaced_existing });
    }
    let rollback_ok = match previous.as_ref() {
        Some(value) => write(value.expose()).is_ok(),
        None => delete().is_ok(),
    };
    Err(ReplaceFailure {
        code: "readback_failed",
        rollback_ok,
    })
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let maximum = left.len().max(right.len());
    let mut difference = left.len() ^ right.len();
    for index in 0..maximum {
        difference |= usize::from(
            left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0),
        );
    }
    difference == 0
}

#[cfg(windows)]
mod windows_store {
    use super::MAX_CREDENTIAL_BYTES;
    use std::{ffi::c_void, ptr};
    use windows::core::{HRESULT, PCWSTR, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS,
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    const NOT_FOUND: HRESULT = HRESULT(0x8007_0490_u32 as i32);

    struct Buffer(*mut CREDENTIALW);

    impl Drop for Buffer {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CredFree(self.0 as *const c_void) };
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn operation_error(stage: &str, error: &windows::core::Error) -> String {
        let code = error.code().0;
        eprintln!("[Recorder][AuthHealth] stage=oidc_refresh_store_{stage} ok=false code={code}");
        format!("Windows refresh credential operation failed at {stage} (code {code})")
    }

    pub(super) fn write(target: &str, secret: &[u8]) -> Result<(), String> {
        if secret.is_empty() || secret.len() > MAX_CREDENTIAL_BYTES {
            return Err("Refresh credential size is invalid".to_string());
        }
        let mut target = wide(target);
        let credential = CREDENTIALW {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
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

    pub(super) fn read(target: &str) -> Result<Option<Vec<u8>>, String> {
        let target = wide(target);
        let mut raw = ptr::null_mut();
        if let Err(error) =
            unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut raw) }
        {
            if error.code() == NOT_FOUND {
                return Ok(None);
            }
            return Err(operation_error("read", &error));
        }
        if raw.is_null() {
            return Err("Windows refresh credential returned an empty pointer".to_string());
        }
        let buffer = Buffer(raw);
        let credential = unsafe { &*buffer.0 };
        let size = credential.CredentialBlobSize as usize;
        if size > MAX_CREDENTIAL_BYTES {
            return Err("Windows refresh credential is oversized".to_string());
        }
        if size == 0 {
            return Ok(Some(Vec::new()));
        }
        if credential.CredentialBlob.is_null() {
            return Err("Windows refresh credential blob is missing".to_string());
        }
        Ok(Some(unsafe {
            std::slice::from_raw_parts(credential.CredentialBlob as *const u8, size).to_vec()
        }))
    }

    pub(super) fn delete(target: &str) -> Result<(), String> {
        let target = wide(target);
        match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
            Ok(()) => Ok(()),
            Err(error) if error.code() == NOT_FOUND => Ok(()),
            Err(error) => Err(operation_error("delete", &error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    #[test]
    fn envelope_round_trips_subject_and_token() {
        let subject = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let decoded = decode(encode(subject, b"refresh.token").unwrap()).unwrap();
        assert_eq!(decoded.subject(), subject);
        assert_eq!(decoded.token(), b"refresh.token");
        assert!(decode(SecretBytes::new(b"refresh.token".to_vec())).is_err());
    }

    #[test]
    fn replacement_rolls_back_on_readback_failure() {
        let stored = Rc::new(RefCell::new(Some(b"old".to_vec())));
        let reads = Rc::new(Cell::new(0));
        let result = replace_with(
            b"new",
            {
                let stored = stored.clone();
                let reads = reads.clone();
                move || {
                    let n = reads.get();
                    reads.set(n + 1);
                    if n == 0 {
                        Ok(stored.borrow().clone())
                    } else {
                        Ok(Some(b"bad".to_vec()))
                    }
                }
            },
            {
                let stored = stored.clone();
                move |value| {
                    *stored.borrow_mut() = Some(value.to_vec());
                    Ok(())
                }
            },
            {
                let stored = stored.clone();
                move || {
                    *stored.borrow_mut() = None;
                    Ok(())
                }
            },
        );
        assert!(matches!(
            result,
            Err(ReplaceFailure {
                rollback_ok: true,
                ..
            })
        ));
        assert_eq!(stored.borrow().as_deref(), Some(b"old".as_slice()));
    }
}
