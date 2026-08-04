//! Native authenticated access-state reader for the paid desktop boundary.
//!
//! The access token is copied only inside native Rust, sent to one compile-time-
//! pinned `/api/v1/me/access` endpoint, and zeroed when the request scope ends.
//! The WebView receives only a short-lived normalized access status. Recording
//! commands are not unlocked by this module yet.

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use reqwest::{
    header::{
        HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CACHE_CONTROL,
        CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE,
    },
    redirect::Policy,
    Client, Response, StatusCode,
};
use ring::{constant_time, digest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::atomic::{compiler_fence, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::Url;
use uuid::Uuid;

const ACCESS_PATH: &str = "/api/v1/me/access";
const MAX_ENDPOINT_BYTES: usize = 8 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024;
const MAX_ACCESS_TOKEN_BYTES: usize = 16 * 1024;
const MAX_ENTITLEMENTS: usize = 64;
const MAX_FEATURE_KEY_BYTES: usize = 64;
const ACCESS_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const ACCESS_READ_TIMEOUT: Duration = Duration::from_secs(5);
const ACCESS_TOTAL_TIMEOUT: Duration = Duration::from_secs(10);
const ACCESS_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const TOKEN_FINGERPRINT_BYTES: usize = 32;
const BUILD_ACCESS_ENDPOINT: Option<&str> = option_env!("RECORDER_SAAS_ACCESS_ENDPOINT");

struct SecretBytes(Vec<u8>);

impl SecretBytes {
    fn from_slice(value: &[u8]) -> Result<Self, &'static str> {
        if value.is_empty() || value.len() > MAX_ACCESS_TOKEN_BYTES {
            return Err("access_token_size_invalid");
        }
        if !value.is_ascii()
            || value
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Active,
    Disabled,
    DeletionPending,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BillingProvider {
    Stripe,
    Paypal,
    Phonepe,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Pending,
    Active,
    GracePeriod,
    PastDue,
    Suspended,
    Cancelled,
    Expired,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeSubscriptionAccess {
    pub provider: BillingProvider,
    pub status: SubscriptionStatus,
    pub current_period_end: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeAccountAccessStatus {
    pub configured: bool,
    pub endpoint_https: bool,
    pub loopback_development: bool,
    pub checked: bool,
    pub authenticated: bool,
    pub account_status: Option<AccountStatus>,
    pub subscription: Option<NativeSubscriptionAccess>,
    pub entitlement_count: usize,
    pub full_access: bool,
    pub checked_at: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub access_token_native_only: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawAccessResponse {
    user_id: Uuid,
    authenticated: bool,
    account_status: AccountStatus,
    subscription: Option<NativeSubscriptionAccess>,
    entitlements: Vec<String>,
    full_access: bool,
}

#[derive(Clone, Debug)]
struct AccessEndpointConfig {
    endpoint: Url,
    endpoint_https: bool,
    loopback_development: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AccessConfigError {
    code: &'static str,
}

impl AccessConfigError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl AccessEndpointConfig {
    fn from_build() -> Result<Option<Self>, AccessConfigError> {
        Self::from_value(BUILD_ACCESS_ENDPOINT, cfg!(debug_assertions))
    }

    fn from_value(
        endpoint: Option<&str>,
        debug_build: bool,
    ) -> Result<Option<Self>, AccessConfigError> {
        let Some(endpoint) = endpoint else {
            return Ok(None);
        };
        let endpoint = endpoint.trim();
        if endpoint.is_empty() {
            return Err(AccessConfigError::new("endpoint_empty"));
        }
        if endpoint.len() > MAX_ENDPOINT_BYTES {
            return Err(AccessConfigError::new("endpoint_too_large"));
        }

        let endpoint =
            Url::parse(endpoint).map_err(|_| AccessConfigError::new("endpoint_parse"))?;
        if endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
        {
            return Err(AccessConfigError::new("endpoint_authority"));
        }
        if endpoint.path() != ACCESS_PATH
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(AccessConfigError::new("endpoint_components"));
        }

        let endpoint_https = endpoint.scheme() == "https";
        let loopback_development = endpoint.scheme() == "http"
            && debug_build
            && endpoint.port().is_some()
            && endpoint
                .host_str()
                .and_then(|host| host.parse::<IpAddr>().ok())
                .is_some_and(|address| address.is_loopback());
        if !endpoint_https && !loopback_development {
            return Err(AccessConfigError::new("endpoint_transport"));
        }

        Ok(Some(Self {
            endpoint,
            endpoint_https,
            loopback_development,
        }))
    }
}

struct CachedAccess {
    subject: Uuid,
    token_fingerprint: [u8; TOKEN_FINGERPRINT_BYTES],
    expires_at_instant: Instant,
    status: NativeAccountAccessStatus,
}

#[derive(Default)]
struct AccessState {
    cached: Option<CachedAccess>,
}

fn runtime() -> &'static Mutex<AccessState> {
    static ACCESS: OnceLock<Mutex<AccessState>> = OnceLock::new();
    ACCESS.get_or_init(|| Mutex::new(AccessState::default()))
}

pub(crate) fn clear() -> bool {
    runtime().lock().cached.take().is_some()
}

pub(crate) fn status() -> Result<NativeAccountAccessStatus, String> {
    let config = configured_status()?;
    if !config.configured {
        clear();
        return Ok(config);
    }

    let current = crate::oidc_session::with_access_token(|subject, token| {
        let fingerprint = token_fingerprint(token);
        let mut state = runtime().lock();
        if state.cached.as_ref().is_some_and(|cached| {
            cached.subject != subject
                || constant_time::verify_slices_are_equal(
                    &cached.token_fingerprint,
                    &fingerprint,
                )
                .is_err()
                || Instant::now() >= cached.expires_at_instant
        }) {
            state.cached.take();
        }
        Ok(state.cached.as_ref().map(|cached| cached.status.clone()))
    });

    match current {
        Ok(Some(status)) => Ok(status),
        Ok(None) | Err(_) => {
            clear();
            Ok(config)
        }
    }
}

pub(crate) async fn refresh() -> Result<NativeAccountAccessStatus, String> {
    let config = require_configured()?;
    clear();
    let client = build_client()?;
    let (subject, access_token, fingerprint) =
        crate::oidc_session::with_access_token(|subject, token| {
            let token = SecretBytes::from_slice(token)
                .map_err(|_| "Native access token is invalid".to_string())?;
            let fingerprint = token_fingerprint(token.expose());
            Ok((subject, token, fingerprint))
        })?;

    let mut authorization = Vec::with_capacity(7 + access_token.expose().len());
    authorization.extend_from_slice(b"Bearer ");
    authorization.extend_from_slice(access_token.expose());
    let authorization = SecretBytes(authorization);
    let mut authorization_header = HeaderValue::from_bytes(authorization.expose())
        .map_err(|_| "Native access token could not be encoded safely".to_string())?;
    authorization_header.set_sensitive(true);

    eprintln!(
        "[Recorder][SaasHealth] stage=desktop_me_access_start ok=true token_native_only=true redirects_disabled=true retries_disabled=true runtime_proxy_disabled=true"
    );
    let response = client
        .get(config.endpoint.as_str())
        .header(AUTHORIZATION, authorization_header)
        .header(ACCEPT, "application/json")
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|error| transport_error("desktop_me_access_send", &error))?;

    if response.url().as_str() != config.endpoint.as_str() {
        eprintln!(
            "[Recorder][SaasHealth] stage=desktop_me_access_headers ok=false code=endpoint_changed"
        );
        return Err("The account-access response endpoint changed unexpectedly".to_string());
    }
    let response_status = response.status();
    validate_response_headers(response_status, response.headers()).map_err(|code| {
        eprintln!(
            "[Recorder][SaasHealth] stage=desktop_me_access_headers ok=false code={code}"
        );
        if response_status == StatusCode::UNAUTHORIZED {
            "The native access session was rejected and must be refreshed".to_string()
        } else {
            "The account-access endpoint returned an unacceptable response".to_string()
        }
    })?;

    let bytes = read_bounded_response(response).await?;
    let response_bytes = bytes.len();
    let validated = parse_response(&bytes, subject).map_err(|code| {
        eprintln!("[Recorder][SaasHealth] stage=desktop_me_access_parse ok=false code={code}");
        "The account-access endpoint returned an invalid response".to_string()
    })?;
    let checked_at = Utc::now();
    let valid_until = checked_at + chrono::Duration::seconds(ACCESS_CACHE_TTL.as_secs() as i64);
    let status = NativeAccountAccessStatus {
        configured: true,
        endpoint_https: config.endpoint_https,
        loopback_development: config.loopback_development,
        checked: true,
        authenticated: true,
        account_status: Some(validated.account_status),
        subscription: validated.subscription,
        entitlement_count: validated.entitlements.len(),
        full_access: validated.full_access,
        checked_at: Some(checked_at),
        valid_until: Some(valid_until),
        access_token_native_only: true,
    };

    let committed = crate::oidc_session::with_access_token(|current_subject, current_token| {
        if current_subject != subject
            || constant_time::verify_slices_are_equal(
                &token_fingerprint(current_token),
                &fingerprint,
            )
            .is_err()
        {
            return Err("Native access session changed during the access check".to_string());
        }
        runtime().lock().cached = Some(CachedAccess {
            subject,
            token_fingerprint: fingerprint,
            expires_at_instant: Instant::now() + ACCESS_CACHE_TTL,
            status: status.clone(),
        });
        Ok(status)
    })?;

    eprintln!(
        "[Recorder][SaasHealth] stage=desktop_me_access ok=true response_bytes={response_bytes} full_access={} entitlement_count={} cache_seconds={} token_native_only=true",
        committed.full_access,
        committed.entitlement_count,
        ACCESS_CACHE_TTL.as_secs()
    );
    Ok(committed)
}

fn configured_status() -> Result<NativeAccountAccessStatus, String> {
    match AccessEndpointConfig::from_build() {
        Ok(Some(config)) => Ok(empty_status(
            true,
            config.endpoint_https,
            config.loopback_development,
        )),
        Ok(None) => Ok(empty_status(false, false, false)),
        Err(error) => {
            eprintln!(
                "[Recorder][SaasHealth] stage=desktop_me_access_config ok=false code={}",
                error.code
            );
            Err(format!(
                "Desktop account-access configuration is invalid ({})",
                error.code
            ))
        }
    }
}

fn require_configured() -> Result<AccessEndpointConfig, String> {
    match AccessEndpointConfig::from_build() {
        Ok(Some(config)) => Ok(config),
        Ok(None) => Err("Desktop account access is not configured in this build".to_string()),
        Err(error) => Err(format!(
            "Desktop account-access configuration is invalid ({})",
            error.code
        )),
    }
}

fn empty_status(
    configured: bool,
    endpoint_https: bool,
    loopback_development: bool,
) -> NativeAccountAccessStatus {
    NativeAccountAccessStatus {
        configured,
        endpoint_https,
        loopback_development,
        checked: false,
        authenticated: false,
        account_status: None,
        subscription: None,
        entitlement_count: 0,
        full_access: false,
        checked_at: None,
        valid_until: None,
        access_token_native_only: true,
    }
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
        .connect_timeout(ACCESS_CONNECT_TIMEOUT)
        .read_timeout(ACCESS_READ_TIMEOUT)
        .timeout(ACCESS_TOTAL_TIMEOUT)
        .tcp_nodelay(true)
        .user_agent("Recorder/0.1 native-access-client")
        .build()
        .map_err(|_| "Unable to initialize the bounded account-access client".to_string())
}

fn validate_response_headers(
    status: StatusCode,
    headers: &HeaderMap,
) -> Result<(), &'static str> {
    if status != StatusCode::OK {
        return Err(if status == StatusCode::UNAUTHORIZED {
            "not_authenticated"
        } else {
            "http_status"
        });
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
    if let Some(encoding) = encodings.next() {
        if encodings.next().is_some() {
            return Err("content_encoding_multiple");
        }
        if !encoding
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
    if let Some(length) = lengths.first() {
        let length = length.to_str().map_err(|_| "content_length_invalid")?;
        if length.is_empty() || !length.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("content_length_invalid");
        }
        if length
            .parse::<u64>()
            .map_err(|_| "content_length_invalid")?
            > MAX_RESPONSE_BYTES as u64
        {
            return Err("content_length_too_large");
        }
    }

    let no_store = headers.get_all(CACHE_CONTROL).iter().any(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|directive| directive.trim().eq_ignore_ascii_case("no-store"))
        })
    });
    if !no_store {
        return Err("cache_control_missing_no_store");
    }
    Ok(())
}

async fn read_bounded_response(mut response: Response) -> Result<Vec<u8>, String> {
    let capacity = response
        .content_length()
        .unwrap_or(0)
        .min(MAX_RESPONSE_BYTES as u64) as usize;
    let mut bytes = Vec::with_capacity(capacity);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| transport_error("desktop_me_access_read", &error))?
    {
        let next_size = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| "Account-access response size overflowed".to_string())?;
        if next_size > MAX_RESPONSE_BYTES {
            eprintln!(
                "[Recorder][SaasHealth] stage=desktop_me_access_read ok=false code=body_too_large"
            );
            return Err("Account-access response exceeded the size limit".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn parse_response(bytes: &[u8], expected_subject: Uuid) -> Result<RawAccessResponse, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_RESPONSE_BYTES {
        return Err("response_size_invalid");
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let response = RawAccessResponse::deserialize(&mut deserializer)
        .map_err(|_| "response_json_invalid")?;
    deserializer
        .end()
        .map_err(|_| "response_trailing_data")?;

    if !response.authenticated {
        return Err("authenticated_false");
    }
    if response.user_id != expected_subject {
        return Err("subject_mismatch");
    }
    if response.entitlements.len() > MAX_ENTITLEMENTS {
        return Err("entitlement_count_invalid");
    }

    let mut seen = BTreeSet::new();
    for feature in &response.entitlements {
        if !valid_feature_key(feature) {
            return Err("entitlement_key_invalid");
        }
        if !seen.insert(feature.as_str()) {
            return Err("entitlement_duplicate");
        }
    }
    let expected_full_access = response.account_status == AccountStatus::Active
        && seen.contains("desktop_full_access");
    if response.full_access != expected_full_access {
        return Err("full_access_mismatch");
    }
    Ok(response)
}

fn valid_feature_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    (3..=MAX_FEATURE_KEY_BYTES).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

fn token_fingerprint(value: &[u8]) -> [u8; TOKEN_FINGERPRINT_BYTES] {
    let value = digest::digest(&digest::SHA256, value);
    let mut fingerprint = [0_u8; TOKEN_FINGERPRINT_BYTES];
    fingerprint.copy_from_slice(value.as_ref());
    fingerprint
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
    eprintln!("[Recorder][SaasHealth] stage={stage} ok=false code={code}");
    "Account-access endpoint communication failed".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER_ID: &str = "11111111-1111-4111-8111-111111111111";

    fn valid_response(full_access: bool, entitlements: &str) -> Vec<u8> {
        format!(
            r#"{{"userId":"{USER_ID}","authenticated":true,"accountStatus":"active","subscription":{{"provider":"stripe","status":"active","currentPeriodEnd":"2026-09-01T00:00:00Z"}},"entitlements":{entitlements},"fullAccess":{full_access}}}"#
        )
        .into_bytes()
    }

    #[test]
    fn access_endpoint_is_compile_time_pinned_and_https() {
        assert!(AccessEndpointConfig::from_value(None, false)
            .expect("absent endpoint should be valid")
            .is_none());
        assert_eq!(
            AccessEndpointConfig::from_value(
                Some("http://api.example.test/api/v1/me/access"),
                false,
            )
            .expect_err("production HTTP must fail"),
            AccessConfigError::new("endpoint_transport")
        );
        assert!(AccessEndpointConfig::from_value(
            Some("https://api.example.test/api/v1/me/access"),
            false,
        )
        .expect("HTTPS endpoint should be valid")
        .is_some());
    }

    #[test]
    fn numeric_loopback_http_is_debug_only() {
        assert!(AccessEndpointConfig::from_value(
            Some("http://127.0.0.1:8080/api/v1/me/access"),
            true,
        )
        .expect("debug loopback should be valid")
        .is_some());
        assert!(AccessEndpointConfig::from_value(
            Some("http://localhost:8080/api/v1/me/access"),
            true,
        )
        .is_err());
    }

    #[test]
    fn endpoint_path_query_and_fragment_are_fixed() {
        for endpoint in [
            "https://api.example.test/api/v1/me",
            "https://api.example.test/api/v1/me/access?user=1",
            "https://api.example.test/api/v1/me/access#fragment",
        ] {
            assert!(AccessEndpointConfig::from_value(Some(endpoint), false).is_err());
        }
    }

    #[test]
    fn normalized_access_response_is_strict_and_subject_bound() {
        let subject = Uuid::parse_str(USER_ID).unwrap();
        let response = parse_response(
            &valid_response(true, r#"["desktop_full_access"]"#),
            subject,
        )
        .expect("valid response should parse");
        assert!(response.full_access);
        assert_eq!(response.entitlements.len(), 1);
        assert!(parse_response(
            &valid_response(true, r#"["desktop_full_access"]"#),
            Uuid::new_v4(),
        )
        .is_err());
    }

    #[test]
    fn entitlement_and_full_access_invariants_fail_closed() {
        let subject = Uuid::parse_str(USER_ID).unwrap();
        assert!(parse_response(
            &valid_response(true, r#"["desktop_full_access","desktop_full_access"]"#),
            subject,
        )
        .is_err());
        assert!(parse_response(&valid_response(true, r#"["library_read"]"#), subject).is_err());
        assert!(parse_response(
            &valid_response(false, r#"["desktop_full_access"]"#),
            subject,
        )
        .is_err());
    }

    #[test]
    fn response_headers_require_json_bounds_and_no_store() {
        let mut valid = HeaderMap::new();
        valid.insert(
            CONTENT_TYPE,
            "application/json; charset=utf-8".parse().unwrap(),
        );
        valid.insert(CONTENT_LENGTH, "128".parse().unwrap());
        valid.insert(CACHE_CONTROL, "no-store".parse().unwrap());
        assert!(validate_response_headers(StatusCode::OK, &valid).is_ok());
        assert!(validate_response_headers(StatusCode::FOUND, &valid).is_err());

        valid.remove(CACHE_CONTROL);
        assert!(validate_response_headers(StatusCode::OK, &valid).is_err());
        valid.insert(CACHE_CONTROL, "no-store".parse().unwrap());
        valid.insert(
            CONTENT_LENGTH,
            (MAX_RESPONSE_BYTES + 1).to_string().parse().unwrap(),
        );
        assert!(validate_response_headers(StatusCode::OK, &valid).is_err());
    }

    #[test]
    fn webview_status_never_contains_subject_or_token() {
        let status = NativeAccountAccessStatus {
            configured: true,
            endpoint_https: true,
            loopback_development: false,
            checked: true,
            authenticated: true,
            account_status: Some(AccountStatus::Active),
            subscription: None,
            entitlement_count: 1,
            full_access: true,
            checked_at: Some(Utc::now()),
            valid_until: Some(Utc::now()),
            access_token_native_only: true,
        };
        let serialized = serde_json::to_string(&status).unwrap();
        assert!(!serialized.contains(USER_ID));
        assert!(!serialized.contains("header.payload.signature"));
        assert!(!serialized.contains("desktop_full_access"));
    }
}
