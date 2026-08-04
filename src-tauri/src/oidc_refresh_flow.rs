//! Bounded refresh-token exchange and refresh ID-token verification.
//!
//! This path accepts only a subject-bound native refresh credential, posts a
//! public-client refresh request to the compile-time-pinned HTTPS token endpoint,
//! verifies the returned RS256/ES256 ID token, requires subject continuity, and
//! returns native-only candidate tokens. It does not persist or install them.

use crate::{
    oidc_jwks::{resolve_id_token_key, ValidatedJwk},
    oidc_refresh, oidc_token,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::{
    header::{HeaderMap, ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    redirect::Policy,
    Client, Response, StatusCode,
};
use ring::signature;
use serde::Deserialize;
use std::{
    collections::HashSet,
    sync::atomic::{compiler_fence, Ordering},
    time::Duration,
};
use uuid::Uuid;

const BUILD_CLIENT_ID: Option<&str> = option_env!("RECORDER_OIDC_CLIENT_ID");
const MAX_REQUEST_BYTES: usize = 16 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_ACCESS_TOKEN_BYTES: usize = 16 * 1024;
const MAX_ID_TOKEN_BYTES: usize = 16 * 1024;
const MAX_SCOPE_BYTES: usize = 1_024;
const MAX_CLIENT_ID_BYTES: usize = 512;
const MAX_ISSUER_BYTES: usize = 8 * 1024;
const MAX_AUDIENCES: usize = 4;
const MAX_ID_TOKEN_LIFETIME_SECONDS: i64 = 60 * 60;
const CLOCK_SKEW_SECONDS: i64 = 60;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(12);

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

pub(crate) struct VerifiedRefreshExchange {
    subject: Uuid,
    identity_expires_at: i64,
    access_token: SecretBytes,
    refresh_token: SecretBytes,
    expires_in: u64,
}

impl VerifiedRefreshExchange {
    pub(crate) const fn subject(&self) -> Uuid {
        self.subject
    }

    pub(crate) const fn identity_expires_at(&self) -> i64 {
        self.identity_expires_at
    }

    pub(crate) fn access_token(&self) -> &[u8] {
        self.access_token.expose()
    }

    pub(crate) fn refresh_token(&self) -> &[u8] {
        self.refresh_token.expose()
    }

    pub(crate) const fn expires_in(&self) -> u64 {
        self.expires_in
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    refresh_token: String,
    scope: String,
    id_token: String,
}

struct ParsedTokenResponse {
    access_token: SecretBytes,
    expires_in: u64,
    refresh_token: SecretBytes,
    id_token: SecretBytes,
}

#[derive(Deserialize)]
struct RawClaims {
    iss: String,
    sub: String,
    aud: RawAudience,
    exp: i64,
    iat: i64,
    auth_time: i64,
    #[serde(default)]
    nonce: Option<String>,
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

pub(crate) async fn exchange_and_verify(
    credential: &oidc_refresh::RefreshCredential,
) -> Result<VerifiedRefreshExchange, String> {
    let config = oidc_token::require_configured()?;
    let client_id = configured_client_id()?;
    let body = build_request(credential.token(), client_id.as_bytes())?;
    let client = build_client()?;

    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_refresh_request_start ok=true request_bytes={} redirects_disabled=true retries_disabled=true runtime_proxy_disabled=true subject_bound=true",
        body.len()
    );
    let response = client
        .post(config.token_endpoint.as_str())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .header(ACCEPT_ENCODING, "identity")
        .body(body)
        .send()
        .await
        .map_err(|error| transport_error("oidc_refresh_request_send", &error))?;

    if response.url().as_str() != config.token_endpoint.as_str() {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_response_headers ok=false code=endpoint_changed"
        );
        return Err("OIDC refresh response endpoint changed unexpectedly".to_string());
    }
    validate_headers(response.status(), response.headers()).map_err(|code| {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_response_headers ok=false code={code}"
        );
        "OIDC refresh endpoint returned an unacceptable response".to_string()
    })?;

    let response_bytes = SecretBytes::new(read_bounded(response).await?);
    let tokens = parse_response(response_bytes.expose()).map_err(|code| {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_response_parse ok=false code={code}"
        );
        "OIDC refresh endpoint returned an invalid token response".to_string()
    })?;

    let key = resolve_id_token_key(tokens.id_token.expose()).await?;
    verify_signature(tokens.id_token.expose(), &key).map_err(|code| {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_id_signature ok=false code={code}"
        );
        "OIDC refreshed ID token signature is invalid".to_string()
    })?;
    let (subject, identity_expires_at) =
        validate_claims(tokens.id_token.expose(), config.issuer.as_str(), client_id).map_err(
            |code| {
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_refresh_id_claims ok=false code={code}"
                );
                "OIDC refreshed ID token claims are invalid".to_string()
            },
        )?;
    if subject != credential.subject() {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_refresh_subject_continuity ok=false code=subject_mismatch"
        );
        return Err("OIDC refresh identity does not match the stored account".to_string());
    }

    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_refresh_exchange ok=true signature_valid=true claims_valid=true subject_continuity=true nonce_absent=true tokens_persisted=false"
    );
    Ok(VerifiedRefreshExchange {
        subject,
        identity_expires_at,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_in: tokens.expires_in,
    })
}

fn configured_client_id() -> Result<&'static str, String> {
    let value = BUILD_CLIENT_ID
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OIDC client ID is not configured in this build".to_string())?;
    validate_text(value, 1, MAX_CLIENT_ID_BYTES)
        .map_err(|_| "OIDC client ID configuration is invalid".to_string())?;
    Ok(value)
}

fn build_request(refresh_token: &[u8], client_id: &[u8]) -> Result<Vec<u8>, String> {
    validate_opaque(
        refresh_token,
        1,
        oidc_refresh::MAX_REFRESH_TOKEN_BYTES,
        "refresh_token",
    )?;
    validate_opaque(client_id, 1, MAX_CLIENT_ID_BYTES, "client_id")?;

    let mut body = Vec::with_capacity(256);
    append_field(&mut body, b"grant_type", b"refresh_token");
    append_field(&mut body, b"refresh_token", refresh_token);
    append_field(&mut body, b"client_id", client_id);
    if body.len() > MAX_REQUEST_BYTES {
        return Err("oidc_refresh_request_too_large".to_string());
    }
    Ok(body)
}

fn append_field(body: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    if !body.is_empty() {
        body.push(b'&');
    }
    body.extend_from_slice(name);
    body.push(b'=');
    for byte in value {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                body.push(*byte);
            }
            b' ' => body.push(b'+'),
            _ => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                body.push(b'%');
                body.push(HEX[(*byte >> 4) as usize]);
                body.push(HEX[(*byte & 0x0f) as usize]);
            }
        }
    }
}

fn validate_opaque(
    value: &[u8],
    minimum: usize,
    maximum: usize,
    field: &str,
) -> Result<(), String> {
    if value.len() < minimum
        || value.len() > maximum
        || value.iter().any(|byte| byte.is_ascii_control())
    {
        return Err(format!("oidc_refresh_{field}_invalid"));
    }
    Ok(())
}

fn build_client() -> Result<Client, String> {
    Client::builder()
        .tls_backend_rustls()
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .no_proxy()
        .gzip(false)
        .brotli(false)
        .deflate(false)
        .zstd(false)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .tcp_nodelay(true)
        .user_agent("Recorder/0.1 native-oidc-refresh")
        .build()
        .map_err(|_| "Unable to initialize the bounded OIDC refresh client".to_string())
}

fn validate_headers(status: StatusCode, headers: &HeaderMap) -> Result<(), &'static str> {
    if status != StatusCode::OK {
        return Err("http_status");
    }

    let content_types: Vec<_> = headers.get_all(CONTENT_TYPE).iter().collect();
    if content_types.len() != 1 {
        return Err("content_type_count");
    }
    let content_type = content_types[0]
        .to_str()
        .map_err(|_| "content_type_invalid")?;
    let mut parts = content_type.split(';');
    if !parts.next().is_some_and(|value| {
        value.trim().eq_ignore_ascii_case("application/json")
    }) {
        return Err("content_type_invalid");
    }
    if parts.any(|value| !value.trim().eq_ignore_ascii_case("charset=utf-8")) {
        return Err("content_type_parameter_unapproved");
    }

    let encodings: Vec<_> = headers.get_all(CONTENT_ENCODING).iter().collect();
    if encodings.len() > 1 {
        return Err("content_encoding_multiple");
    }
    if let Some(value) = encodings.first() {
        if !value
            .to_str()
            .map_err(|_| "content_encoding_invalid")?
            .trim()
            .eq_ignore_ascii_case("identity")
        {
            return Err("content_encoding_unapproved");
        }
    }

    let lengths: Vec<_> = headers.get_all(CONTENT_LENGTH).iter().collect();
    if lengths.len() > 1 {
        return Err("content_length_multiple");
    }
    if let Some(value) = lengths.first() {
        let value = value.to_str().map_err(|_| "content_length_invalid")?;
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("content_length_invalid");
        }
        let declared = value
            .parse::<u64>()
            .map_err(|_| "content_length_invalid")?;
        if declared > MAX_RESPONSE_BYTES as u64 {
            return Err("content_length_too_large");
        }
    }
    Ok(())
}

async fn read_bounded(mut response: Response) -> Result<Vec<u8>, String> {
    let capacity = response.content_length().unwrap_or(0).min(MAX_RESPONSE_BYTES as u64) as usize;
    let mut bytes = Vec::with_capacity(capacity);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| transport_error("oidc_refresh_response_read", &error))?
    {
        let next_size = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| "OIDC refresh response size overflowed".to_string())?;
        if next_size > MAX_RESPONSE_BYTES {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_refresh_response_read ok=false code=body_too_large"
            );
            return Err("OIDC refresh response exceeded the configured size limit".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn transport_error(stage: &str, error: &reqwest::Error) -> String {
    let code = if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_request() {
        "request"
    } else if error.is_body() {
        "body"
    } else {
        "transport"
    };
    eprintln!("[Recorder][AuthHealth] stage={stage} ok=false code={code}");
    "OIDC refresh endpoint communication failed".to_string()
}

fn parse_response(bytes: &[u8]) -> Result<ParsedTokenResponse, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_RESPONSE_BYTES {
        return Err("response_size_invalid");
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let raw = RawTokenResponse::deserialize(&mut deserializer)
        .map_err(|_| "response_json_invalid")?;
    deserializer
        .end()
        .map_err(|_| "response_trailing_data")?;

    if !raw.token_type.eq_ignore_ascii_case("bearer") {
        return Err("token_type_invalid");
    }
    if raw.expires_in == 0 || raw.expires_in > 24 * 60 * 60 {
        return Err("token_lifetime_invalid");
    }
    validate_token_text(&raw.access_token, MAX_ACCESS_TOKEN_BYTES)?;
    validate_token_text(&raw.id_token, MAX_ID_TOKEN_BYTES)?;
    validate_token_text(&raw.refresh_token, oidc_refresh::MAX_REFRESH_TOKEN_BYTES)?;
    if !looks_like_jwt(&raw.access_token) || !looks_like_jwt(&raw.id_token) {
        return Err("jwt_shape_invalid");
    }
    validate_scopes(&raw.scope)?;

    Ok(ParsedTokenResponse {
        access_token: SecretBytes::new(raw.access_token.into_bytes()),
        expires_in: raw.expires_in,
        refresh_token: SecretBytes::new(raw.refresh_token.into_bytes()),
        id_token: SecretBytes::new(raw.id_token.into_bytes()),
    })
}

fn validate_token_text(value: &str, maximum: usize) -> Result<(), &'static str> {
    if value.is_empty()
        || value.len() > maximum
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("token_text_invalid");
    }
    Ok(())
}

fn validate_scopes(scope: &str) -> Result<(), &'static str> {
    if scope.is_empty() || scope.len() > MAX_SCOPE_BYTES {
        return Err("scope_invalid");
    }
    let mut seen = HashSet::new();
    for value in scope.split_ascii_whitespace() {
        if !matches!(value, "openid" | "email" | "profile" | "phone") {
            return Err("scope_unapproved");
        }
        if !seen.insert(value) {
            return Err("scope_duplicate");
        }
    }
    if !seen.contains("openid") {
        return Err("scope_missing_openid");
    }
    Ok(())
}

fn looks_like_jwt(value: &str) -> bool {
    let mut parts = value.split('.');
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    };
    parts.next().is_some_and(valid)
        && parts.next().is_some_and(valid)
        && parts.next().is_some_and(valid)
        && parts.next().is_none()
}

fn verify_signature(token: &[u8], key: &ValidatedJwk) -> Result<(), &'static str> {
    let (signing_input, signature_bytes) = split_signature(token)?;
    match key {
        ValidatedJwk::Rsa {
            modulus, exponent, ..
        } => {
            if signature_bytes.len() != modulus.len() {
                return Err("rsa_signature_size_invalid");
            }
            signature::RsaPublicKeyComponents {
                n: modulus.as_slice(),
                e: exponent.as_slice(),
            }
            .verify(
                &signature::RSA_PKCS1_2048_8192_SHA256,
                signing_input,
                signature_bytes.as_slice(),
            )
            .map_err(|_| "signature_mismatch")
        }
        ValidatedJwk::EcP256 { x, y, .. } => {
            if signature_bytes.len() != 64 {
                return Err("es256_signature_size_invalid");
            }
            let mut public_key = [0_u8; 65];
            public_key[0] = 0x04;
            public_key[1..33].copy_from_slice(x);
            public_key[33..].copy_from_slice(y);
            signature::UnparsedPublicKey::new(
                &signature::ECDSA_P256_SHA256_FIXED,
                public_key.as_slice(),
            )
            .verify(signing_input, signature_bytes.as_slice())
            .map_err(|_| "signature_mismatch")
        }
    }
}

fn split_signature(token: &[u8]) -> Result<(&[u8], Vec<u8>), &'static str> {
    if token.is_empty()
        || token.len() > MAX_ID_TOKEN_BYTES
        || token
            .iter()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("token_invalid");
    }
    let first = token
        .iter()
        .position(|byte| *byte == b'.')
        .ok_or("segment_count_invalid")?;
    let second = token[first + 1..]
        .iter()
        .position(|byte| *byte == b'.')
        .map(|value| first + 1 + value)
        .ok_or("segment_count_invalid")?;
    if first == 0
        || second == first + 1
        || second + 1 >= token.len()
        || token[second + 1..].contains(&b'.')
    {
        return Err("segment_count_invalid");
    }
    let encoded = &token[second + 1..];
    if encoded.contains(&b'=')
        || !encoded
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'_')
    {
        return Err("signature_encoding_invalid");
    }
    let signature = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "signature_encoding_invalid")?;
    if signature.is_empty() || signature.len() > 512 {
        return Err("signature_size_invalid");
    }
    Ok((&token[..second], signature))
}

fn validate_claims(
    token: &[u8],
    issuer: &str,
    client_id: &str,
) -> Result<(Uuid, i64), &'static str> {
    validate_text(issuer, 1, MAX_ISSUER_BYTES)?;
    validate_text(client_id, 1, MAX_CLIENT_ID_BYTES)?;
    let payload = decode_payload(token)?;
    let mut deserializer = serde_json::Deserializer::from_slice(payload.expose());
    let raw = RawClaims::deserialize(&mut deserializer).map_err(|_| "claims_json_invalid")?;
    deserializer
        .end()
        .map_err(|_| "claims_trailing_data")?;

    validate_text(&raw.iss, 1, MAX_ISSUER_BYTES)?;
    if raw.iss != issuer {
        return Err("issuer_mismatch");
    }
    validate_audience(&raw.aud, raw.azp.as_deref(), client_id)?;
    let subject = Uuid::parse_str(&raw.sub).map_err(|_| "subject_invalid")?;
    if raw.sub != subject.hyphenated().to_string() {
        return Err("subject_not_canonical");
    }
    if raw.nonce.is_some() {
        return Err("refresh_nonce_present");
    }
    validate_times(&raw, chrono::Utc::now().timestamp())?;
    Ok((subject, raw.exp))
}

fn decode_payload(token: &[u8]) -> Result<SecretBytes, &'static str> {
    let token = std::str::from_utf8(token).map_err(|_| "token_utf8_invalid")?;
    let mut parts = token.split('.');
    let header = parts.next().ok_or("segment_count_invalid")?;
    let payload = parts.next().ok_or("segment_count_invalid")?;
    let signature = parts.next().ok_or("segment_count_invalid")?;
    if parts.next().is_some()
        || header.is_empty()
        || payload.is_empty()
        || signature.is_empty()
        || payload.contains('=')
        || !payload
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("segment_count_invalid");
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| "payload_encoding_invalid")?;
    if bytes.is_empty() || bytes.len() > 8 * 1024 {
        return Err("payload_size_invalid");
    }
    Ok(SecretBytes::new(bytes))
}

fn validate_audience(
    audience: &RawAudience,
    authorized_party: Option<&str>,
    client_id: &str,
) -> Result<(), &'static str> {
    let values: Vec<&str> = match audience {
        RawAudience::Single(value) => vec![value],
        RawAudience::Multiple(values)
            if !values.is_empty() && values.len() <= MAX_AUDIENCES =>
        {
            values.iter().map(String::as_str).collect()
        }
        RawAudience::Multiple(_) => return Err("audience_count_invalid"),
    };

    let mut unique = HashSet::with_capacity(values.len());
    for value in &values {
        validate_text(value, 1, MAX_CLIENT_ID_BYTES)?;
        if !unique.insert(*value) {
            return Err("audience_duplicate");
        }
    }
    if !values.iter().any(|value| *value == client_id) {
        return Err("audience_mismatch");
    }
    if values.len() > 1 && authorized_party != Some(client_id) {
        return Err("authorized_party_mismatch");
    }
    if authorized_party.is_some_and(|value| value != client_id) {
        return Err("authorized_party_mismatch");
    }
    Ok(())
}

fn validate_times(raw: &RawClaims, now: i64) -> Result<(), &'static str> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn refresh_form_is_public_client_only() {
        let body = String::from_utf8(
            build_request(b"refresh.token", b"desktop-client").unwrap(),
        )
        .unwrap();
        assert_eq!(
            body,
            "grant_type=refresh_token&refresh_token=refresh.token&client_id=desktop-client"
        );
        assert!(!body.contains("client_secret"));
    }

    #[test]
    fn refreshed_claims_require_nonce_absence() {
        let issuer = "https://identity.example.test/auth/v1";
        let client = "desktop-client";
        let subject = "11111111-1111-4111-8111-111111111111";
        let now = chrono::Utc::now().timestamp();
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256","kid":"key"}"#);
        let make = |nonce: Option<&str>| {
            let mut payload = json!({
                "iss": issuer,
                "sub": subject,
                "aud": client,
                "exp": now + 600,
                "iat": now,
                "auth_time": now - 10
            });
            if let Some(value) = nonce {
                payload["nonce"] = json!(value);
            }
            let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
            format!("{header}.{payload}.c2ln").into_bytes()
        };
        assert!(validate_claims(&make(None), issuer, client).is_ok());
        assert_eq!(
            validate_claims(&make(Some("old-nonce")), issuer, client),
            Err("refresh_nonce_present")
        );
    }
}
