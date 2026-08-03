//! Bounded native JWKS retrieval and strict ID-token signing-key selection.
//!
//! This module retrieves only the compile-time-pinned JWKS endpoint, keeps a short
//! in-memory public-key cache, and selects an RS256 or ES256 key by the exact `kid`
//! and `alg` from a strictly parsed ID-token header. It does not verify a signature,
//! establish a session, persist a token, or expose any value to the WebView.

use crate::oidc_token;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::{
    header::{HeaderMap, ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    redirect::Policy,
    Client, Response, StatusCode,
};
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const MAX_JWKS_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_JWKS_KEYS: usize = 8;
const MAX_KID_BYTES: usize = 256;
const MAX_JWT_HEADER_SEGMENT_BYTES: usize = 4 * 1024;
const MAX_JWT_HEADER_JSON_BYTES: usize = 2 * 1024;
const MAX_RSA_COMPONENT_CHARS: usize = 2 * 1024;
const MAX_EC_COMPONENT_CHARS: usize = 128;
const JWKS_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const JWKS_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const JWKS_READ_TIMEOUT: Duration = Duration::from_secs(5);
const JWKS_TOTAL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApprovedIdTokenAlgorithm {
    Rs256,
    Es256,
}

impl ApprovedIdTokenAlgorithm {
    fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "RS256" => Ok(Self::Rs256),
            "ES256" => Ok(Self::Es256),
            _ => Err("algorithm_unapproved"),
        }
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Rs256 => "RS256",
            Self::Es256 => "ES256",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ValidatedJwk {
    Rsa {
        kid: String,
        modulus: Vec<u8>,
        exponent: Vec<u8>,
    },
    EcP256 {
        kid: String,
        x: [u8; 32],
        y: [u8; 32],
    },
}

impl ValidatedJwk {
    pub(crate) fn kid(&self) -> &str {
        match self {
            Self::Rsa { kid, .. } | Self::EcP256 { kid, .. } => kid,
        }
    }

    pub(crate) const fn algorithm(&self) -> ApprovedIdTokenAlgorithm {
        match self {
            Self::Rsa { .. } => ApprovedIdTokenAlgorithm::Rs256,
            Self::EcP256 { .. } => ApprovedIdTokenAlgorithm::Es256,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn rsa_components(&self) -> Option<(&[u8], &[u8])> {
        match self {
            Self::Rsa {
                modulus, exponent, ..
            } => Some((modulus, exponent)),
            Self::EcP256 { .. } => None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn ec_p256_components(&self) -> Option<(&[u8; 32], &[u8; 32])> {
        match self {
            Self::EcP256 { x, y, .. } => Some((x, y)),
            Self::Rsa { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedJwtHeader {
    algorithm: ApprovedIdTokenAlgorithm,
    kid: String,
}

#[derive(Clone)]
struct CachedJwks {
    loaded_at: Instant,
    keys: Vec<ValidatedJwk>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJwks {
    keys: Vec<RawJwk>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJwk {
    kty: String,
    kid: String,
    #[serde(rename = "use")]
    key_use: Option<String>,
    alg: String,
    key_ops: Option<Vec<String>>,
    n: Option<String>,
    e: Option<String>,
    crv: Option<String>,
    x: Option<String>,
    y: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJwtHeader {
    alg: String,
    kid: String,
    typ: Option<String>,
}

fn cache() -> &'static Mutex<Option<CachedJwks>> {
    static CACHE: OnceLock<Mutex<Option<CachedJwks>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Resolve the exact public key declared by a strictly parsed ID-token header.
///
/// The first lookup uses a five-minute in-memory cache. A missing `kid` performs
/// one forced refresh to support provider signing-key rotation, then fails closed.
#[allow(dead_code)]
pub(crate) async fn resolve_id_token_key(id_token: &[u8]) -> Result<ValidatedJwk, String> {
    let header = parse_id_token_header(id_token).map_err(|code| {
        eprintln!("[Recorder][AuthHealth] stage=oidc_id_header ok=false code={code}");
        "OIDC ID token header is invalid".to_string()
    })?;

    let keys = load_jwks(false).await?;
    match select_key(&keys, &header) {
        Ok(key) => {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_jwks_select ok=true cache_refresh=false algorithm={} key_count={}",
                header.algorithm.name(),
                keys.len()
            );
            return Ok(key.clone());
        }
        Err("key_missing") => {}
        Err(code) => {
            eprintln!("[Recorder][AuthHealth] stage=oidc_jwks_select ok=false code={code}");
            return Err("OIDC ID token signing key is invalid".to_string());
        }
    }

    let refreshed = load_jwks(true).await?;
    let key = select_key(&refreshed, &header).map_err(|code| {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_jwks_select ok=false code={code} cache_refresh=true"
        );
        "OIDC ID token signing key was not found".to_string()
    })?;
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_jwks_select ok=true cache_refresh=true algorithm={} key_count={}",
        header.algorithm.name(),
        refreshed.len()
    );
    Ok(key.clone())
}

async fn load_jwks(force_refresh: bool) -> Result<Vec<ValidatedJwk>, String> {
    let mut cached = cache().lock().await;
    if !force_refresh {
        if let Some(current) = cached.as_ref() {
            if current.loaded_at.elapsed() < JWKS_CACHE_TTL {
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_jwks_cache ok=true cache_hit=true key_count={}",
                    current.keys.len()
                );
                return Ok(current.keys.clone());
            }
        }
    }

    let keys = fetch_jwks().await?;
    *cached = Some(CachedJwks {
        loaded_at: Instant::now(),
        keys: keys.clone(),
    });
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_jwks_cache ok=true cache_hit=false force_refresh={force_refresh} key_count={}",
        keys.len()
    );
    Ok(keys)
}

async fn fetch_jwks() -> Result<Vec<ValidatedJwk>, String> {
    let config = oidc_token::require_configured()?;
    let client = build_jwks_client()?;
    let response = client
        .get(config.jwks_uri.as_str())
        .header(ACCEPT, "application/json")
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|error| transport_error("oidc_jwks_request", &error))?;

    if response.url().as_str() != config.jwks_uri.as_str() {
        eprintln!("[Recorder][AuthHealth] stage=oidc_jwks_headers ok=false code=endpoint_changed");
        return Err("OIDC JWKS response origin changed unexpectedly".to_string());
    }
    validate_json_response_headers(response.status(), response.headers()).map_err(|code| {
        eprintln!("[Recorder][AuthHealth] stage=oidc_jwks_headers ok=false code={code}");
        "OIDC JWKS endpoint returned an unacceptable response".to_string()
    })?;
    let bytes = read_bounded_response(response).await?;
    let response_size = bytes.len();
    let keys = parse_jwks(&bytes).map_err(|code| {
        eprintln!("[Recorder][AuthHealth] stage=oidc_jwks_parse ok=false code={code}");
        "OIDC JWKS document is invalid".to_string()
    })?;
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_jwks_fetch ok=true response_bytes={response_size} key_count={}",
        keys.len()
    );
    Ok(keys)
}

fn build_jwks_client() -> Result<Client, String> {
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
        .connect_timeout(JWKS_CONNECT_TIMEOUT)
        .read_timeout(JWKS_READ_TIMEOUT)
        .timeout(JWKS_TOTAL_TIMEOUT)
        .tcp_nodelay(true)
        .user_agent("Recorder/0.1 native-oidc-jwks")
        .build()
        .map_err(|_| "Unable to initialize the bounded OIDC JWKS client".to_string())
}

fn validate_json_response_headers(
    status: StatusCode,
    headers: &HeaderMap,
) -> Result<(), &'static str> {
    if status != StatusCode::OK {
        return Err("http_status");
    }

    let mut content_types = headers.get_all(CONTENT_TYPE).iter();
    let content_type = content_types.next().ok_or("content_type_missing")?;
    if content_types.next().is_some() {
        return Err("content_type_multiple");
    }
    let content_type = content_type.to_str().map_err(|_| "content_type_invalid")?;
    let mut parts = content_type.split(';');
    if !parts
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err("content_type_invalid");
    }
    for parameter in parts {
        if !parameter.trim().eq_ignore_ascii_case("charset=utf-8") {
            return Err("content_type_parameter_unapproved");
        }
    }

    let mut encodings = headers.get_all(CONTENT_ENCODING).iter();
    if let Some(value) = encodings.next() {
        if encodings.next().is_some() {
            return Err("content_encoding_multiple");
        }
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
        if value.parse::<u64>().map_err(|_| "content_length_invalid")?
            > MAX_JWKS_RESPONSE_BYTES as u64
        {
            return Err("content_length_too_large");
        }
    }
    Ok(())
}

async fn read_bounded_response(mut response: Response) -> Result<Vec<u8>, String> {
    let capacity = response
        .content_length()
        .unwrap_or(0)
        .min(MAX_JWKS_RESPONSE_BYTES as u64) as usize;
    let mut bytes = Vec::with_capacity(capacity);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| transport_error("oidc_jwks_read", &error))?
    {
        let next = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| "OIDC JWKS response size overflowed".to_string())?;
        if next > MAX_JWKS_RESPONSE_BYTES {
            eprintln!("[Recorder][AuthHealth] stage=oidc_jwks_read ok=false code=body_too_large");
            return Err("OIDC JWKS response exceeded the configured size limit".to_string());
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
    "OIDC JWKS endpoint communication failed".to_string()
}

fn parse_jwks(bytes: &[u8]) -> Result<Vec<ValidatedJwk>, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_JWKS_RESPONSE_BYTES {
        return Err("size_invalid");
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let raw = RawJwks::deserialize(&mut deserializer).map_err(|_| "json_invalid")?;
    deserializer.end().map_err(|_| "trailing_data")?;
    if raw.keys.is_empty() || raw.keys.len() > MAX_JWKS_KEYS {
        return Err("key_count_invalid");
    }

    let mut kids = HashSet::with_capacity(raw.keys.len());
    let mut validated = Vec::with_capacity(raw.keys.len());
    for raw_key in raw.keys {
        validate_kid(&raw_key.kid)?;
        if !kids.insert(raw_key.kid.clone()) {
            return Err("kid_duplicate");
        }
        validate_key_purpose(&raw_key)?;
        validated.push(validate_jwk(raw_key)?);
    }
    Ok(validated)
}

fn validate_key_purpose(key: &RawJwk) -> Result<(), &'static str> {
    if key.key_use.as_deref().is_some_and(|value| value != "sig") {
        return Err("use_unapproved");
    }
    if let Some(operations) = key.key_ops.as_ref() {
        if operations.is_empty() || operations.len() > 4 {
            return Err("key_ops_invalid");
        }
        let mut seen = HashSet::with_capacity(operations.len());
        for operation in operations {
            if operation != "verify" || !seen.insert(operation) {
                return Err("key_ops_unapproved");
            }
        }
    }
    Ok(())
}

fn validate_jwk(key: RawJwk) -> Result<ValidatedJwk, &'static str> {
    let algorithm = ApprovedIdTokenAlgorithm::parse(&key.alg)?;
    match (key.kty.as_str(), algorithm) {
        ("RSA", ApprovedIdTokenAlgorithm::Rs256) => {
            if key.crv.is_some() || key.x.is_some() || key.y.is_some() {
                return Err("rsa_mixed_parameters");
            }
            let modulus = decode_component(
                key.n.as_deref().ok_or("rsa_modulus_missing")?,
                MAX_RSA_COMPONENT_CHARS,
            )?;
            let exponent = decode_component(
                key.e.as_deref().ok_or("rsa_exponent_missing")?,
                MAX_RSA_COMPONENT_CHARS,
            )?;
            if !(256..=512).contains(&modulus.len()) || exponent.is_empty() || exponent.len() > 8 {
                return Err("rsa_component_size_invalid");
            }
            let exponent_value = exponent.iter().try_fold(0_u64, |value, byte| {
                value.checked_mul(256)?.checked_add(u64::from(*byte))
            });
            if !exponent_value.is_some_and(|value| value >= 3 && value % 2 == 1) {
                return Err("rsa_exponent_invalid");
            }
            Ok(ValidatedJwk::Rsa {
                kid: key.kid,
                modulus,
                exponent,
            })
        }
        ("EC", ApprovedIdTokenAlgorithm::Es256) => {
            if key.n.is_some() || key.e.is_some() || key.crv.as_deref() != Some("P-256") {
                return Err("ec_parameters_invalid");
            }
            let x = decode_component(
                key.x.as_deref().ok_or("ec_x_missing")?,
                MAX_EC_COMPONENT_CHARS,
            )?;
            let y = decode_component(
                key.y.as_deref().ok_or("ec_y_missing")?,
                MAX_EC_COMPONENT_CHARS,
            )?;
            let x: [u8; 32] = x.try_into().map_err(|_| "ec_component_size_invalid")?;
            let y: [u8; 32] = y.try_into().map_err(|_| "ec_component_size_invalid")?;
            Ok(ValidatedJwk::EcP256 { kid: key.kid, x, y })
        }
        _ => Err("key_type_algorithm_mismatch"),
    }
}

fn decode_component(value: &str, maximum_chars: usize) -> Result<Vec<u8>, &'static str> {
    if value.is_empty()
        || value.len() > maximum_chars
        || value.contains('=')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("component_encoding_invalid");
    }
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| "component_encoding_invalid")
}

fn parse_id_token_header(id_token: &[u8]) -> Result<ValidatedJwtHeader, &'static str> {
    let token = std::str::from_utf8(id_token).map_err(|_| "token_utf8_invalid")?;
    if token.is_empty()
        || token
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("token_text_invalid");
    }
    let mut segments = token.split('.');
    let header = segments.next().ok_or("segment_count_invalid")?;
    let payload = segments.next().ok_or("segment_count_invalid")?;
    let signature = segments.next().ok_or("segment_count_invalid")?;
    if segments.next().is_some() || payload.is_empty() || signature.is_empty() {
        return Err("segment_count_invalid");
    }
    if header.is_empty() || header.len() > MAX_JWT_HEADER_SEGMENT_BYTES {
        return Err("header_size_invalid");
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(header)
        .map_err(|_| "header_encoding_invalid")?;
    if decoded.is_empty() || decoded.len() > MAX_JWT_HEADER_JSON_BYTES {
        return Err("header_size_invalid");
    }

    let mut deserializer = serde_json::Deserializer::from_slice(&decoded);
    let raw = RawJwtHeader::deserialize(&mut deserializer).map_err(|_| "header_json_invalid")?;
    deserializer.end().map_err(|_| "header_trailing_data")?;
    if raw
        .typ
        .as_deref()
        .is_some_and(|value| !value.eq_ignore_ascii_case("JWT"))
    {
        return Err("header_type_invalid");
    }
    validate_kid(&raw.kid)?;
    Ok(ValidatedJwtHeader {
        algorithm: ApprovedIdTokenAlgorithm::parse(&raw.alg)?,
        kid: raw.kid,
    })
}

fn validate_kid(kid: &str) -> Result<(), &'static str> {
    if kid.is_empty()
        || kid.len() > MAX_KID_BYTES
        || !kid.is_ascii()
        || kid
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("kid_invalid");
    }
    Ok(())
}

fn select_key<'a>(
    keys: &'a [ValidatedJwk],
    header: &ValidatedJwtHeader,
) -> Result<&'a ValidatedJwk, &'static str> {
    let Some(key) = keys.iter().find(|key| key.kid() == header.kid) else {
        return Err("key_missing");
    };
    if key.algorithm() != header.algorithm {
        return Err("key_algorithm_mismatch");
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rsa_key(kid: &str) -> String {
        let modulus = URL_SAFE_NO_PAD.encode(vec![0x81; 256]);
        format!(
            r#"{{"kty":"RSA","kid":"{kid}","use":"sig","alg":"RS256","key_ops":["verify"],"n":"{modulus}","e":"AQAB","crv":null,"x":null,"y":null}}"#
        )
    }

    fn ec_key(kid: &str) -> String {
        let x = URL_SAFE_NO_PAD.encode([0x11; 32]);
        let y = URL_SAFE_NO_PAD.encode([0x22; 32]);
        format!(
            r#"{{"kty":"EC","kid":"{kid}","use":"sig","alg":"ES256","key_ops":["verify"],"n":null,"e":null,"crv":"P-256","x":"{x}","y":"{y}"}}"#
        )
    }

    fn id_token(algorithm: &str, kid: &str) -> Vec<u8> {
        let header = URL_SAFE_NO_PAD.encode(format!(
            r#"{{"alg":"{algorithm}","kid":"{kid}","typ":"JWT"}}"#
        ));
        format!("{header}.e30.c2ln").into_bytes()
    }

    #[test]
    fn strict_jwks_accepts_bounded_rsa_and_p256_keys() {
        let document = format!(
            r#"{{"keys":[{},{}]}}"#,
            rsa_key("rsa-key"),
            ec_key("ec-key")
        );
        let keys = parse_jwks(document.as_bytes()).expect("valid JWKS should parse");
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].algorithm(), ApprovedIdTokenAlgorithm::Rs256);
        assert_eq!(keys[1].algorithm(), ApprovedIdTokenAlgorithm::Es256);
    }

    #[test]
    fn duplicate_kids_and_unapproved_algorithms_are_rejected() {
        let duplicate = format!(r#"{{"keys":[{},{}]}}"#, rsa_key("same"), ec_key("same"));
        assert_eq!(parse_jwks(duplicate.as_bytes()), Err("kid_duplicate"));

        let hs = rsa_key("rsa-key").replace("RS256", "HS256");
        let document = format!(r#"{{"keys":[{hs}]}}"#);
        assert_eq!(parse_jwks(document.as_bytes()), Err("algorithm_unapproved"));
    }

    #[test]
    fn unknown_fields_and_mixed_key_parameters_are_rejected() {
        let unknown = format!(r#"{{"keys":[{}],"unexpected":true}}"#, rsa_key("rsa-key"));
        assert_eq!(parse_jwks(unknown.as_bytes()), Err("json_invalid"));

        let mixed = rsa_key("rsa-key").replace("\"crv\":null", "\"crv\":\"P-256\"");
        let document = format!(r#"{{"keys":[{mixed}]}}"#);
        assert_eq!(parse_jwks(document.as_bytes()), Err("rsa_mixed_parameters"));
    }

    #[test]
    fn strict_header_selects_only_exact_kid_and_algorithm() {
        let document = format!(
            r#"{{"keys":[{},{}]}}"#,
            rsa_key("rsa-key"),
            ec_key("ec-key")
        );
        let keys = parse_jwks(document.as_bytes()).expect("valid JWKS should parse");

        let rsa_header = parse_id_token_header(&id_token("RS256", "rsa-key"))
            .expect("valid RSA header should parse");
        assert_eq!(
            select_key(&keys, &rsa_header)
                .expect("RSA key should resolve")
                .algorithm(),
            ApprovedIdTokenAlgorithm::Rs256
        );

        let mismatch = parse_id_token_header(&id_token("ES256", "rsa-key"))
            .expect("valid ES header should parse");
        assert_eq!(select_key(&keys, &mismatch), Err("key_algorithm_mismatch"));
    }

    #[test]
    fn duplicate_or_extended_jwt_headers_are_rejected() {
        let duplicate =
            URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","alg":"ES256","kid":"key","typ":"JWT"}"#);
        assert_eq!(
            parse_id_token_header(format!("{duplicate}.e30.c2ln").as_bytes()),
            Err("header_json_invalid")
        );

        let extended = URL_SAFE_NO_PAD
            .encode(br#"{"alg":"RS256","kid":"key","typ":"JWT","jku":"https://evil.test"}"#);
        assert_eq!(
            parse_id_token_header(format!("{extended}.e30.c2ln").as_bytes()),
            Err("header_json_invalid")
        );
    }
}
