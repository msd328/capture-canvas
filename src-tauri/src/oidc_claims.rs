//! Strict native OpenID Connect ID-token claim parsing and validation.
//!
//! This module validates the standards-critical ID-token payload fields against
//! Recorder's compile-time-pinned issuer, public client ID, authorization nonce and
//! current time. It does not verify the JWS signature, establish a session, persist a
//! token, or expose any identity data to the WebView. The result therefore remains
//! explicitly unverified until a maintained RS256/ES256 verifier accepts the complete
//! signed token.

use crate::oidc_token;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::atomic::{compiler_fence, Ordering};
use uuid::Uuid;

const BUILD_CLIENT_ID: Option<&str> = option_env!("RECORDER_OIDC_CLIENT_ID");

const MAX_ID_TOKEN_BYTES: usize = 16 * 1024;
const MAX_PAYLOAD_SEGMENT_BYTES: usize = 12 * 1024;
const MAX_PAYLOAD_JSON_BYTES: usize = 8 * 1024;
const MAX_ISSUER_BYTES: usize = 8 * 1024;
const MAX_CLIENT_ID_BYTES: usize = 512;
const MAX_NONCE_BYTES: usize = 512;
const MAX_AUDIENCES: usize = 4;
const CLOCK_SKEW_SECONDS: i64 = 60;
const MAX_ID_TOKEN_LIFETIME_SECONDS: i64 = 60 * 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnverifiedIdTokenClaims {
    subject: Uuid,
    issued_at: i64,
    expires_at: i64,
    authenticated_at: i64,
}

#[allow(dead_code)]
impl UnverifiedIdTokenClaims {
    pub(crate) const fn subject(&self) -> Uuid {
        self.subject
    }

    pub(crate) const fn issued_at(&self) -> i64 {
        self.issued_at
    }

    pub(crate) const fn expires_at(&self) -> i64 {
        self.expires_at
    }

    pub(crate) const fn authenticated_at(&self) -> i64 {
        self.authenticated_at
    }
}

struct SecretBytes(Vec<u8>);

impl SecretBytes {
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

#[derive(Deserialize)]
struct RawIdTokenClaims {
    iss: String,
    sub: String,
    aud: RawAudience,
    exp: i64,
    iat: i64,
    auth_time: i64,
    nonce: String,
    #[serde(default)]
    nbf: Option<i64>,
    #[serde(default)]
    azp: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawAudience {
    Single(String),
    Multiple(Vec<String>),
}

/// Parse and validate ID-token claims without treating them as authenticated.
///
/// The token signature must be verified over the original compact JWS before this
/// result can be promoted to a native identity or used to persist a session.
#[allow(dead_code)]
pub(crate) fn inspect_unverified_id_token(
    id_token: &[u8],
    expected_nonce: &[u8],
) -> Result<UnverifiedIdTokenClaims, String> {
    let config = oidc_token::require_configured()?;
    let client_id = configured_client_id()?;
    let now = chrono::Utc::now().timestamp();
    validate_claims_at(
        id_token,
        expected_nonce,
        config.issuer.as_str(),
        client_id,
        now,
    )
    .map_err(|code| {
        eprintln!("[Recorder][AuthHealth] stage=oidc_id_claims ok=false code={code}");
        "OIDC ID token claims are invalid".to_string()
    })
}

fn configured_client_id() -> Result<&'static str, String> {
    let client_id = BUILD_CLIENT_ID
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OIDC client ID is not configured in this build".to_string())?;
    validate_text(client_id, 1, MAX_CLIENT_ID_BYTES)
        .map_err(|_| "OIDC client ID configuration is invalid".to_string())?;
    Ok(client_id)
}

fn validate_claims_at(
    id_token: &[u8],
    expected_nonce: &[u8],
    expected_issuer: &str,
    expected_client_id: &str,
    now: i64,
) -> Result<UnverifiedIdTokenClaims, &'static str> {
    if id_token.is_empty() || id_token.len() > MAX_ID_TOKEN_BYTES {
        return Err("token_size_invalid");
    }
    validate_text(expected_issuer, 1, MAX_ISSUER_BYTES)?;
    validate_text(expected_client_id, 1, MAX_CLIENT_ID_BYTES)?;
    if expected_nonce.is_empty() || expected_nonce.len() > MAX_NONCE_BYTES {
        return Err("expected_nonce_invalid");
    }

    let payload = decode_payload(id_token)?;
    let mut deserializer = serde_json::Deserializer::from_slice(payload.expose());
    let raw = RawIdTokenClaims::deserialize(&mut deserializer).map_err(|_| "json_invalid")?;
    deserializer.end().map_err(|_| "trailing_data")?;

    validate_text(&raw.iss, 1, MAX_ISSUER_BYTES)?;
    if raw.iss != expected_issuer {
        return Err("issuer_mismatch");
    }

    validate_audience(&raw.aud, raw.azp.as_deref(), expected_client_id)?;

    let subject = Uuid::parse_str(&raw.sub).map_err(|_| "subject_invalid")?;
    if raw.sub != subject.hyphenated().to_string() {
        return Err("subject_not_canonical");
    }

    validate_text(&raw.nonce, 1, MAX_NONCE_BYTES)?;
    if !constant_time_eq(raw.nonce.as_bytes(), expected_nonce) {
        return Err("nonce_mismatch");
    }

    validate_timestamps(&raw, now)?;

    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_id_claims ok=true subject_valid=true issuer_valid=true audience_valid=true nonce_valid=true time_valid=true signature_validated=false"
    );
    Ok(UnverifiedIdTokenClaims {
        subject,
        issued_at: raw.iat,
        expires_at: raw.exp,
        authenticated_at: raw.auth_time,
    })
}

fn decode_payload(id_token: &[u8]) -> Result<SecretBytes, &'static str> {
    let token = std::str::from_utf8(id_token).map_err(|_| "token_utf8_invalid")?;
    if token
        .bytes()
        .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("token_text_invalid");
    }

    let mut segments = token.split('.');
    let header = segments.next().ok_or("segment_count_invalid")?;
    let payload = segments.next().ok_or("segment_count_invalid")?;
    let signature = segments.next().ok_or("segment_count_invalid")?;
    if segments.next().is_some() || header.is_empty() || payload.is_empty() || signature.is_empty()
    {
        return Err("segment_count_invalid");
    }
    if payload.len() > MAX_PAYLOAD_SEGMENT_BYTES
        || payload.contains('=')
        || !payload
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("payload_encoding_invalid");
    }

    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| "payload_encoding_invalid")?;
    if decoded.is_empty() || decoded.len() > MAX_PAYLOAD_JSON_BYTES {
        return Err("payload_size_invalid");
    }
    Ok(SecretBytes(decoded))
}

fn validate_audience(
    audience: &RawAudience,
    authorized_party: Option<&str>,
    expected_client_id: &str,
) -> Result<(), &'static str> {
    let mut values = Vec::new();
    match audience {
        RawAudience::Single(value) => values.push(value.as_str()),
        RawAudience::Multiple(items) => {
            if items.is_empty() || items.len() > MAX_AUDIENCES {
                return Err("audience_count_invalid");
            }
            values.extend(items.iter().map(String::as_str));
        }
    }

    let mut unique = HashSet::with_capacity(values.len());
    for value in &values {
        validate_text(value, 1, MAX_CLIENT_ID_BYTES)?;
        if !unique.insert(*value) {
            return Err("audience_duplicate");
        }
    }
    if !values.iter().any(|value| *value == expected_client_id) {
        return Err("audience_mismatch");
    }

    if let Some(value) = authorized_party {
        validate_text(value, 1, MAX_CLIENT_ID_BYTES)?;
        if value != expected_client_id {
            return Err("authorized_party_mismatch");
        }
    } else if values.len() > 1 {
        return Err("authorized_party_missing");
    }
    Ok(())
}

fn validate_timestamps(raw: &RawIdTokenClaims, now: i64) -> Result<(), &'static str> {
    if raw.exp <= 0 || raw.iat <= 0 || raw.auth_time <= 0 {
        return Err("timestamp_nonpositive");
    }
    if raw.exp <= raw.iat || raw.exp.saturating_sub(raw.iat) > MAX_ID_TOKEN_LIFETIME_SECONDS {
        return Err("lifetime_invalid");
    }
    if raw.exp <= now.saturating_sub(CLOCK_SKEW_SECONDS) {
        return Err("expired");
    }
    if raw.iat > now.saturating_add(CLOCK_SKEW_SECONDS) {
        return Err("issued_in_future");
    }
    if raw.auth_time > now.saturating_add(CLOCK_SKEW_SECONDS)
        || raw.auth_time > raw.iat.saturating_add(CLOCK_SKEW_SECONDS)
    {
        return Err("auth_time_invalid");
    }
    if let Some(not_before) = raw.nbf {
        if not_before <= 0 || not_before > raw.exp {
            return Err("not_before_invalid");
        }
        if not_before > now.saturating_add(CLOCK_SKEW_SECONDS) {
            return Err("not_yet_valid");
        }
    }
    Ok(())
}

fn validate_text(value: &str, minimum: usize, maximum: usize) -> Result<(), &'static str> {
    if value.len() < minimum
        || value.len() > maximum
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("text_invalid");
    }
    Ok(())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let maximum = left.len().max(right.len());
    let mut difference = left.len() ^ right.len();
    for index in 0..maximum {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ISSUER: &str = "https://identity.example.test/auth/v1";
    const CLIENT_ID: &str = "recorder-desktop-client";
    const SUBJECT: &str = "11111111-1111-4111-8111-111111111111";
    const NONCE: &[u8] = b"expected-authorization-nonce";
    const NOW: i64 = 1_800_000_000;

    fn token(payload: serde_json::Value) -> Vec<u8> {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256","kid":"key","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        format!("{header}.{payload}.c2lnbmF0dXJl").into_bytes()
    }

    fn valid_payload() -> serde_json::Value {
        json!({
            "iss": ISSUER,
            "sub": SUBJECT,
            "aud": CLIENT_ID,
            "exp": NOW + 3_600,
            "iat": NOW,
            "auth_time": NOW - 10,
            "nonce": std::str::from_utf8(NONCE).unwrap(),
            "email": "person@example.test",
            "email_verified": true
        })
    }

    #[test]
    fn valid_supabase_id_token_claims_are_accepted_but_remain_unverified() {
        let claims = validate_claims_at(&token(valid_payload()), NONCE, ISSUER, CLIENT_ID, NOW)
            .expect("valid claims should pass");
        assert_eq!(claims.subject().to_string(), SUBJECT);
        assert_eq!(claims.issued_at(), NOW);
        assert_eq!(claims.expires_at(), NOW + 3_600);
        assert_eq!(claims.authenticated_at(), NOW - 10);
    }

    #[test]
    fn issuer_audience_subject_and_nonce_mismatches_are_rejected() {
        let mut payload = valid_payload();
        payload["iss"] = json!("https://attacker.example.test/auth/v1");
        assert_eq!(
            validate_claims_at(&token(payload), NONCE, ISSUER, CLIENT_ID, NOW),
            Err("issuer_mismatch")
        );

        let mut payload = valid_payload();
        payload["aud"] = json!("other-client");
        assert_eq!(
            validate_claims_at(&token(payload), NONCE, ISSUER, CLIENT_ID, NOW),
            Err("audience_mismatch")
        );

        let mut payload = valid_payload();
        payload["sub"] = json!("11111111111141118111111111111111");
        assert_eq!(
            validate_claims_at(&token(payload), NONCE, ISSUER, CLIENT_ID, NOW),
            Err("subject_not_canonical")
        );

        let mut payload = valid_payload();
        payload["nonce"] = json!("different-nonce");
        assert_eq!(
            validate_claims_at(&token(payload), NONCE, ISSUER, CLIENT_ID, NOW),
            Err("nonce_mismatch")
        );
    }

    #[test]
    fn expired_future_and_excessive_lifetime_tokens_are_rejected() {
        let mut payload = valid_payload();
        payload["iat"] = json!(NOW - 3_600);
        payload["auth_time"] = json!(NOW - 3_610);
        payload["exp"] = json!(NOW - CLOCK_SKEW_SECONDS);
        assert_eq!(
            validate_claims_at(&token(payload), NONCE, ISSUER, CLIENT_ID, NOW),
            Err("expired")
        );

        let mut payload = valid_payload();
        payload["iat"] = json!(NOW + CLOCK_SKEW_SECONDS + 1);
        payload["exp"] = json!(NOW + CLOCK_SKEW_SECONDS + 1 + 3_600);
        assert_eq!(
            validate_claims_at(&token(payload), NONCE, ISSUER, CLIENT_ID, NOW),
            Err("issued_in_future")
        );

        let mut payload = valid_payload();
        payload["exp"] = json!(NOW + MAX_ID_TOKEN_LIFETIME_SECONDS + 1);
        assert_eq!(
            validate_claims_at(&token(payload), NONCE, ISSUER, CLIENT_ID, NOW),
            Err("lifetime_invalid")
        );
    }

    #[test]
    fn multiple_audiences_require_matching_authorized_party() {
        let mut payload = valid_payload();
        payload["aud"] = json!([CLIENT_ID, "secondary-client"]);
        assert_eq!(
            validate_claims_at(&token(payload.clone()), NONCE, ISSUER, CLIENT_ID, NOW),
            Err("authorized_party_missing")
        );

        payload["azp"] = json!(CLIENT_ID);
        assert!(validate_claims_at(&token(payload), NONCE, ISSUER, CLIENT_ID, NOW).is_ok());
    }

    #[test]
    fn duplicate_critical_claims_and_malformed_compact_tokens_are_rejected() {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256","kid":"key"}"#);
        let duplicate = URL_SAFE_NO_PAD.encode(format!(
            r#"{{"iss":"{ISSUER}","iss":"{ISSUER}","sub":"{SUBJECT}","aud":"{CLIENT_ID}","exp":{},"iat":{NOW},"auth_time":{},"nonce":"{}"}}"#,
            NOW + 3_600,
            NOW - 10,
            std::str::from_utf8(NONCE).unwrap()
        ));
        let duplicate = format!("{header}.{duplicate}.c2ln").into_bytes();
        assert_eq!(
            validate_claims_at(&duplicate, NONCE, ISSUER, CLIENT_ID, NOW),
            Err("json_invalid")
        );
        assert_eq!(
            validate_claims_at(b"a.b", NONCE, ISSUER, CLIENT_ID, NOW),
            Err("segment_count_invalid")
        );
    }

    #[test]
    fn nonce_comparison_rejects_length_and_value_changes() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"samf"));
        assert!(!constant_time_eq(b"same", b"same-longer"));
    }
}
