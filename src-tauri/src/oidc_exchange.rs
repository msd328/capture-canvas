//! Provider-aligned native OAuth token exchange contracts and bounded transport.
//!
//! This module prepares the exact public-client form bodies required by the
//! Supabase OAuth 2.1 server, strictly validates bounded success responses, and
//! provides a private HTTPS transport primitive for the future ID-token verifier.
//! It does not establish a signed-in session or persist any token.

use crate::{auth::SecureAuthStore, oidc, oidc_loopback, oidc_token};
use reqwest::{
    header::{HeaderMap, ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    redirect::Policy,
    Client, Response, StatusCode,
};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::fmt;
use std::sync::atomic::{compiler_fence, Ordering};
use std::time::Duration;

const MAX_TOKEN_REQUEST_BYTES: usize = 16 * 1024;
const MAX_TOKEN_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_ACCESS_TOKEN_BYTES: usize = 16 * 1024;
const MAX_REFRESH_TOKEN_BYTES: usize = 16 * 1024;
const MAX_ID_TOKEN_BYTES: usize = 16 * 1024;
const MAX_SCOPE_BYTES: usize = 1_024;
const MAX_CLIENT_ID_BYTES: usize = 512;
const MAX_REDIRECT_URI_BYTES: usize = 8 * 1024;
const MAX_AUTHORIZATION_CODE_BYTES: usize = 4 * 1024;
const MIN_PKCE_VERIFIER_BYTES: usize = 43;
const MAX_PKCE_VERIFIER_BYTES: usize = 128;
const MAX_TOKEN_LIFETIME_SECONDS: u64 = 24 * 60 * 60;
const TOKEN_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TOKEN_READ_TIMEOUT: Duration = Duration::from_secs(5);
const TOKEN_TOTAL_TIMEOUT: Duration = Duration::from_secs(12);

const BUILD_CLIENT_ID: Option<&str> = option_env!("RECORDER_OIDC_CLIENT_ID");
const BUILD_REDIRECT_URI: Option<&str> = option_env!("RECORDER_OIDC_REDIRECT_URI");

const TOKEN_RESPONSE_FIELDS: &[&str] = &[
    "access_token",
    "token_type",
    "expires_in",
    "refresh_token",
    "scope",
    "id_token",
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcExchangeContractStatus {
    pub configured: bool,
    pub public_client: bool,
    pub authorization_code_form_supported: bool,
    pub refresh_token_form_supported: bool,
    pub strict_response_parser: bool,
    pub bounded_https_transport_supported: bool,
    pub redirects_disabled: bool,
    pub runtime_proxy_disabled: bool,
    pub network_exchange_enabled: bool,
    pub identity_validation_enabled: bool,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub connect_timeout_seconds: u64,
    pub read_timeout_seconds: u64,
    pub total_timeout_seconds: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcExchangeProbe {
    pub authorization_code_form_ok: bool,
    pub refresh_token_form_ok: bool,
    pub token_response_ok: bool,
    pub duplicate_field_rejected: bool,
    pub unknown_field_rejected: bool,
    pub oversized_response_rejected: bool,
    pub transport_client_ok: bool,
    pub strict_json_headers_ok: bool,
    pub redirect_response_rejected: bool,
    pub oversized_declared_response_rejected: bool,
    pub secrets_kept_native: bool,
}

struct PublicClientMetadata {
    client_id: &'static str,
    redirect_uri: &'static str,
}

struct SecretBytes(Vec<u8>);

impl SecretBytes {
    fn new(value: String) -> Self {
        Self(value.into_bytes())
    }

    fn from_bytes(value: Vec<u8>) -> Self {
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

struct ParsedTokenResponse {
    access_token: SecretBytes,
    expires_in: u64,
    refresh_token: SecretBytes,
    scope: Vec<String>,
    id_token: SecretBytes,
}

#[allow(dead_code)]
pub(crate) struct UnverifiedOidcExchange {
    tokens: ParsedTokenResponse,
    expected_nonce: SecretBytes,
}

#[allow(dead_code)]
impl UnverifiedOidcExchange {
    pub(crate) fn access_token(&self) -> &[u8] {
        self.tokens.access_token.expose()
    }

    pub(crate) fn expires_in(&self) -> u64 {
        self.tokens.expires_in
    }

    pub(crate) fn refresh_token(&self) -> &[u8] {
        self.tokens.refresh_token.expose()
    }

    pub(crate) fn scopes(&self) -> &[String] {
        &self.tokens.scope
    }

    pub(crate) fn id_token(&self) -> &[u8] {
        self.tokens.id_token.expose()
    }

    pub(crate) fn expected_nonce(&self) -> &[u8] {
        self.expected_nonce.expose()
    }
}

struct RawTokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    refresh_token: String,
    scope: String,
    id_token: String,
}

#[derive(Clone, Copy)]
enum TokenField {
    AccessToken,
    TokenType,
    ExpiresIn,
    RefreshToken,
    Scope,
    IdToken,
}

impl<'de> Deserialize<'de> for TokenField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TokenFieldVisitor;

        impl<'de> Visitor<'de> for TokenFieldVisitor {
            type Value = TokenField;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a supported OAuth token response field")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    "access_token" => Ok(TokenField::AccessToken),
                    "token_type" => Ok(TokenField::TokenType),
                    "expires_in" => Ok(TokenField::ExpiresIn),
                    "refresh_token" => Ok(TokenField::RefreshToken),
                    "scope" => Ok(TokenField::Scope),
                    "id_token" => Ok(TokenField::IdToken),
                    _ => Err(E::unknown_field(value, TOKEN_RESPONSE_FIELDS)),
                }
            }
        }

        deserializer.deserialize_identifier(TokenFieldVisitor)
    }
}

impl<'de> Deserialize<'de> for RawTokenResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TokenResponseVisitor;

        impl<'de> Visitor<'de> for TokenResponseVisitor {
            type Value = RawTokenResponse;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a strict Supabase OAuth token response")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut access_token = None;
                let mut token_type = None;
                let mut expires_in = None;
                let mut refresh_token = None;
                let mut scope = None;
                let mut id_token = None;

                while let Some(field) = map.next_key::<TokenField>()? {
                    match field {
                        TokenField::AccessToken => {
                            if access_token.is_some() {
                                return Err(de::Error::duplicate_field("access_token"));
                            }
                            access_token = Some(map.next_value()?);
                        }
                        TokenField::TokenType => {
                            if token_type.is_some() {
                                return Err(de::Error::duplicate_field("token_type"));
                            }
                            token_type = Some(map.next_value()?);
                        }
                        TokenField::ExpiresIn => {
                            if expires_in.is_some() {
                                return Err(de::Error::duplicate_field("expires_in"));
                            }
                            expires_in = Some(map.next_value()?);
                        }
                        TokenField::RefreshToken => {
                            if refresh_token.is_some() {
                                return Err(de::Error::duplicate_field("refresh_token"));
                            }
                            refresh_token = Some(map.next_value()?);
                        }
                        TokenField::Scope => {
                            if scope.is_some() {
                                return Err(de::Error::duplicate_field("scope"));
                            }
                            scope = Some(map.next_value()?);
                        }
                        TokenField::IdToken => {
                            if id_token.is_some() {
                                return Err(de::Error::duplicate_field("id_token"));
                            }
                            id_token = Some(map.next_value()?);
                        }
                    }
                }

                Ok(RawTokenResponse {
                    access_token: access_token
                        .ok_or_else(|| de::Error::missing_field("access_token"))?,
                    token_type: token_type.ok_or_else(|| de::Error::missing_field("token_type"))?,
                    expires_in: expires_in.ok_or_else(|| de::Error::missing_field("expires_in"))?,
                    refresh_token: refresh_token
                        .ok_or_else(|| de::Error::missing_field("refresh_token"))?,
                    scope: scope.ok_or_else(|| de::Error::missing_field("scope"))?,
                    id_token: id_token.ok_or_else(|| de::Error::missing_field("id_token"))?,
                })
            }
        }

        deserializer.deserialize_map(TokenResponseVisitor)
    }
}

pub fn status() -> Result<OidcExchangeContractStatus, String> {
    let client = oidc::client_status()?;
    let token = oidc_token::status()?;
    let configured = client.configured && token.configured;
    let bounded_https_transport_supported = build_transport_client().is_ok();
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_exchange_contract ok=true configured={configured} public_client=true bounded_https_transport={bounded_https_transport_supported} redirects_disabled=true runtime_proxy_disabled=true network_exchange=false identity_validation=false"
    );
    Ok(OidcExchangeContractStatus {
        configured,
        public_client: true,
        authorization_code_form_supported: true,
        refresh_token_form_supported: true,
        strict_response_parser: true,
        bounded_https_transport_supported,
        redirects_disabled: true,
        runtime_proxy_disabled: true,
        network_exchange_enabled: false,
        identity_validation_enabled: false,
        max_request_bytes: MAX_TOKEN_REQUEST_BYTES,
        max_response_bytes: MAX_TOKEN_RESPONSE_BYTES,
        connect_timeout_seconds: TOKEN_CONNECT_TIMEOUT.as_secs(),
        read_timeout_seconds: TOKEN_READ_TIMEOUT.as_secs(),
        total_timeout_seconds: TOKEN_TOTAL_TIMEOUT.as_secs(),
    })
}

pub fn probe() -> OidcExchangeProbe {
    let verifier = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let authorization_code_form_ok = build_authorization_code_request(
        b"code value",
        b"recorder-client",
        b"http://127.0.0.1:43829/oidc/callback",
        verifier,
    )
    .is_ok_and(|body| {
        body.windows(b"grant_type=authorization_code".len())
            .any(|window| window == b"grant_type=authorization_code")
            && body
                .windows(b"code=code+value".len())
                .any(|window| window == b"code=code+value")
    });
    let refresh_token_form_ok = build_refresh_token_request(b"refresh.token", b"recorder-client")
        .is_ok_and(|body| {
            body.windows(b"grant_type=refresh_token".len())
                .any(|window| window == b"grant_type=refresh_token")
        });

    let valid = br#"{
        "access_token":"eyJhbGciOiJFUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.c2ln",
        "token_type":"bearer",
        "expires_in":3600,
        "refresh_token":"refresh.token",
        "scope":"openid email profile",
        "id_token":"eyJhbGciOiJFUzI1NiJ9.eyJub25jZSI6InZhbHVlIn0.c2ln"
    }"#;
    let token_response_ok = parse_token_response(valid).is_ok_and(|response| {
        !response.access_token.expose().is_empty()
            && response.expires_in == 3600
            && !response.refresh_token.expose().is_empty()
            && response.scope.iter().any(|scope| scope == "openid")
            && !response.id_token.expose().is_empty()
    });
    let duplicate_field_rejected = parse_token_response(
        br#"{
            "access_token":"a.b.c",
            "access_token":"d.e.f",
            "token_type":"bearer",
            "expires_in":3600,
            "refresh_token":"refresh",
            "scope":"openid",
            "id_token":"g.h.i"
        }"#,
    )
    .is_err();
    let unknown_field_rejected = parse_token_response(
        br#"{
            "access_token":"a.b.c",
            "token_type":"bearer",
            "expires_in":3600,
            "refresh_token":"refresh",
            "scope":"openid",
            "id_token":"g.h.i",
            "unexpected":"value"
        }"#,
    )
    .is_err();
    let oversized_response_rejected =
        parse_token_response(&vec![b'x'; MAX_TOKEN_RESPONSE_BYTES + 1]).is_err();
    let transport_client_ok = build_transport_client().is_ok();

    let mut valid_headers = HeaderMap::new();
    valid_headers.insert(
        CONTENT_TYPE,
        "application/json; charset=utf-8".parse().unwrap(),
    );
    valid_headers.insert(CONTENT_LENGTH, "2".parse().unwrap());
    let strict_json_headers_ok =
        validate_success_response_headers(StatusCode::OK, &valid_headers).is_ok();
    let redirect_response_rejected =
        validate_success_response_headers(StatusCode::FOUND, &valid_headers).is_err();

    let mut oversized_headers = HeaderMap::new();
    oversized_headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
    oversized_headers.insert(
        CONTENT_LENGTH,
        (MAX_TOKEN_RESPONSE_BYTES + 1).to_string().parse().unwrap(),
    );
    let oversized_declared_response_rejected =
        validate_success_response_headers(StatusCode::OK, &oversized_headers).is_err();

    let ok = authorization_code_form_ok
        && refresh_token_form_ok
        && token_response_ok
        && duplicate_field_rejected
        && unknown_field_rejected
        && oversized_response_rejected
        && transport_client_ok
        && strict_json_headers_ok
        && redirect_response_rejected
        && oversized_declared_response_rejected;
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_exchange_probe ok={ok} authorization_form={authorization_code_form_ok} refresh_form={refresh_token_form_ok} token_response={token_response_ok} duplicate_rejected={duplicate_field_rejected} unknown_rejected={unknown_field_rejected} oversized_rejected={oversized_response_rejected} transport_client={transport_client_ok} strict_json_headers={strict_json_headers_ok} redirect_rejected={redirect_response_rejected} declared_oversize_rejected={oversized_declared_response_rejected} secrets_kept_native=true"
    );

    OidcExchangeProbe {
        authorization_code_form_ok,
        refresh_token_form_ok,
        token_response_ok,
        duplicate_field_rejected,
        unknown_field_rejected,
        oversized_response_rejected,
        transport_client_ok,
        strict_json_headers_ok,
        redirect_response_rejected,
        oversized_declared_response_rejected,
        secrets_kept_native: true,
    }
}

#[allow(dead_code)]
pub(crate) async fn exchange_authorization_code(
    store: &SecureAuthStore,
) -> Result<UnverifiedOidcExchange, String> {
    let token_config = oidc_token::require_configured()?;
    let public_client = require_public_client_metadata()?;
    let client = build_transport_client()?;
    let grant = oidc_loopback::take_exchange_grant(store)?;
    let request_body = build_authorization_code_request(
        grant.authorization_code(),
        public_client.client_id.as_bytes(),
        public_client.redirect_uri.as_bytes(),
        grant.code_verifier(),
    )?;
    let expected_nonce = SecretBytes::from_bytes(grant.nonce().to_vec());

    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_token_request_start ok=true request_bytes={} redirects_disabled=true retries_disabled=true runtime_proxy_disabled=true",
        request_body.len()
    );
    let response = client
        .post(token_config.token_endpoint.as_str())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .header(ACCEPT_ENCODING, "identity")
        .body(request_body)
        .send()
        .await
        .map_err(|error| transport_error("oidc_token_request_send", &error))?;

    if response.url().as_str() != token_config.token_endpoint.as_str() {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_token_response_headers ok=false code=endpoint_changed"
        );
        return Err("OIDC token endpoint response origin changed unexpectedly".to_string());
    }
    validate_success_response_headers(response.status(), response.headers()).map_err(|code| {
        eprintln!("[Recorder][AuthHealth] stage=oidc_token_response_headers ok=false code={code}");
        "OIDC token endpoint returned an unacceptable response".to_string()
    })?;
    let response_bytes = read_bounded_response(response).await?;
    let response_size = response_bytes.len();
    let tokens = parse_token_response(&response_bytes).map_err(|code| {
        eprintln!("[Recorder][AuthHealth] stage=oidc_token_response_parse ok=false code={code}");
        "OIDC token endpoint returned an invalid token response".to_string()
    })?;

    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_token_exchange ok=true response_bytes={response_size} tokens_persisted=false identity_validated=false"
    );
    Ok(UnverifiedOidcExchange {
        tokens,
        expected_nonce,
    })
}

fn require_public_client_metadata() -> Result<PublicClientMetadata, String> {
    let status = oidc::client_status()?;
    if !status.configured {
        return Err("OIDC public client is not configured in this build".to_string());
    }
    let client_id = BUILD_CLIENT_ID
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OIDC client ID is not configured in this build".to_string())?;
    let redirect_uri = BUILD_REDIRECT_URI
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OIDC redirect URI is not configured in this build".to_string())?;
    validate_opaque(client_id.as_bytes(), 1, MAX_CLIENT_ID_BYTES, "client_id")?;
    validate_opaque(
        redirect_uri.as_bytes(),
        1,
        MAX_REDIRECT_URI_BYTES,
        "redirect_uri",
    )?;
    Ok(PublicClientMetadata {
        client_id,
        redirect_uri,
    })
}

fn build_transport_client() -> Result<Client, String> {
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
        .connect_timeout(TOKEN_CONNECT_TIMEOUT)
        .read_timeout(TOKEN_READ_TIMEOUT)
        .timeout(TOKEN_TOTAL_TIMEOUT)
        .tcp_nodelay(true)
        .user_agent("Recorder/0.1 native-oidc-public-client")
        .build()
        .map_err(|_| "Unable to initialize the bounded OIDC HTTPS client".to_string())
}

fn validate_success_response_headers(
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

    let mut content_encodings = headers.get_all(CONTENT_ENCODING).iter();
    if let Some(value) = content_encodings.next() {
        if content_encodings.next().is_some() {
            return Err("content_encoding_multiple");
        }
        let value = value.to_str().map_err(|_| "content_encoding_invalid")?;
        if !value.trim().eq_ignore_ascii_case("identity") {
            return Err("content_encoding_unapproved");
        }
    }

    let declared_lengths: Vec<_> = headers.get_all(CONTENT_LENGTH).iter().collect();
    if declared_lengths.len() > 1 {
        return Err("content_length_multiple");
    }
    if let Some(value) = declared_lengths.first() {
        let value = value.to_str().map_err(|_| "content_length_invalid")?;
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("content_length_invalid");
        }
        let declared = value.parse::<u64>().map_err(|_| "content_length_invalid")?;
        if declared > MAX_TOKEN_RESPONSE_BYTES as u64 {
            return Err("content_length_too_large");
        }
    }
    Ok(())
}

async fn read_bounded_response(mut response: Response) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(response.content_length().unwrap_or(0) as usize);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| transport_error("oidc_token_response_read", &error))?
    {
        let next_size = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| "OIDC token response size overflowed".to_string())?;
        if next_size > MAX_TOKEN_RESPONSE_BYTES {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_token_response_read ok=false code=body_too_large"
            );
            return Err("OIDC token response exceeded the configured size limit".to_string());
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
    "OIDC token endpoint communication failed".to_string()
}

fn build_authorization_code_request(
    authorization_code: &[u8],
    client_id: &[u8],
    redirect_uri: &[u8],
    code_verifier: &[u8],
) -> Result<Vec<u8>, String> {
    validate_opaque(
        authorization_code,
        1,
        MAX_AUTHORIZATION_CODE_BYTES,
        "authorization_code",
    )?;
    validate_opaque(client_id, 1, MAX_CLIENT_ID_BYTES, "client_id")?;
    validate_opaque(redirect_uri, 1, MAX_REDIRECT_URI_BYTES, "redirect_uri")?;
    if !(MIN_PKCE_VERIFIER_BYTES..=MAX_PKCE_VERIFIER_BYTES).contains(&code_verifier.len())
        || !code_verifier
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~'))
    {
        return Err("oidc_exchange_code_verifier_invalid".to_string());
    }

    let mut body = Vec::with_capacity(512);
    append_form_field(&mut body, b"grant_type", b"authorization_code");
    append_form_field(&mut body, b"code", authorization_code);
    append_form_field(&mut body, b"client_id", client_id);
    append_form_field(&mut body, b"redirect_uri", redirect_uri);
    append_form_field(&mut body, b"code_verifier", code_verifier);
    if body.len() > MAX_TOKEN_REQUEST_BYTES {
        return Err("oidc_exchange_request_too_large".to_string());
    }
    Ok(body)
}

fn build_refresh_token_request(refresh_token: &[u8], client_id: &[u8]) -> Result<Vec<u8>, String> {
    validate_opaque(refresh_token, 1, MAX_REFRESH_TOKEN_BYTES, "refresh_token")?;
    validate_opaque(client_id, 1, MAX_CLIENT_ID_BYTES, "client_id")?;

    let mut body = Vec::with_capacity(256);
    append_form_field(&mut body, b"grant_type", b"refresh_token");
    append_form_field(&mut body, b"refresh_token", refresh_token);
    append_form_field(&mut body, b"client_id", client_id);
    if body.len() > MAX_TOKEN_REQUEST_BYTES {
        return Err("oidc_refresh_request_too_large".to_string());
    }
    Ok(body)
}

fn append_form_field(body: &mut Vec<u8>, name: &[u8], value: &[u8]) {
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
        return Err(format!("oidc_exchange_{field}_invalid"));
    }
    Ok(())
}

fn parse_token_response(bytes: &[u8]) -> Result<ParsedTokenResponse, String> {
    if bytes.is_empty() || bytes.len() > MAX_TOKEN_RESPONSE_BYTES {
        return Err("oidc_token_response_size_invalid".to_string());
    }

    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let raw = RawTokenResponse::deserialize(&mut deserializer)
        .map_err(|_| "oidc_token_response_json_invalid".to_string())?;
    deserializer
        .end()
        .map_err(|_| "oidc_token_response_trailing_data".to_string())?;

    if !raw.token_type.eq_ignore_ascii_case("bearer") {
        return Err("oidc_token_response_type_invalid".to_string());
    }
    if raw.expires_in == 0 || raw.expires_in > MAX_TOKEN_LIFETIME_SECONDS {
        return Err("oidc_token_response_lifetime_invalid".to_string());
    }
    validate_token(&raw.access_token, MAX_ACCESS_TOKEN_BYTES, "access_token")?;
    validate_token(&raw.id_token, MAX_ID_TOKEN_BYTES, "id_token")?;
    if !looks_like_jwt(&raw.access_token) || !looks_like_jwt(&raw.id_token) {
        return Err("oidc_token_response_jwt_shape_invalid".to_string());
    }
    validate_opaque(
        raw.refresh_token.as_bytes(),
        1,
        MAX_REFRESH_TOKEN_BYTES,
        "refresh_token",
    )?;

    if raw.scope.is_empty() || raw.scope.len() > MAX_SCOPE_BYTES {
        return Err("oidc_token_response_scope_invalid".to_string());
    }
    let mut scopes = Vec::new();
    for scope in raw.scope.split_ascii_whitespace() {
        if !matches!(scope, "openid" | "email" | "profile" | "phone") {
            return Err("oidc_token_response_scope_unapproved".to_string());
        }
        if scopes.iter().any(|existing| existing == scope) {
            return Err("oidc_token_response_scope_duplicate".to_string());
        }
        scopes.push(scope.to_string());
    }
    if !scopes.iter().any(|scope| scope == "openid") {
        return Err("oidc_token_response_scope_missing_openid".to_string());
    }

    Ok(ParsedTokenResponse {
        access_token: SecretBytes::new(raw.access_token),
        expires_in: raw.expires_in,
        refresh_token: SecretBytes::new(raw.refresh_token),
        scope: scopes,
        id_token: SecretBytes::new(raw.id_token),
    })
}

fn validate_token(value: &str, maximum: usize, field: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > maximum
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(format!("oidc_token_response_{field}_invalid"));
    }
    Ok(())
}

fn looks_like_jwt(value: &str) -> bool {
    let mut segments = value.split('.');
    let valid_segment = |segment: &str| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    };
    let first = segments.next().is_some_and(valid_segment);
    let second = segments.next().is_some_and(valid_segment);
    let third = segments.next().is_some_and(valid_segment);
    first && second && third && segments.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_code_form_is_bounded_and_percent_encoded() {
        let verifier = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let body = build_authorization_code_request(
            b"code value/+",
            b"desktop client",
            b"http://127.0.0.1:43829/oidc/callback",
            verifier,
        )
        .expect("valid request should build");
        let body = String::from_utf8(body).expect("form should remain ASCII");
        assert!(body.contains("grant_type=authorization_code"));
        assert!(body.contains("code=code+value%2F%2B"));
        assert!(body.contains("client_id=desktop+client"));
        assert!(body.contains("code_verifier=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));
        assert!(body.len() <= MAX_TOKEN_REQUEST_BYTES);
    }

    #[test]
    fn invalid_pkce_verifier_is_rejected() {
        assert!(build_authorization_code_request(
            b"code",
            b"client",
            b"http://127.0.0.1:43829/oidc/callback",
            b"too-short",
        )
        .is_err());
    }

    #[test]
    fn refresh_form_contains_no_client_secret() {
        let body = build_refresh_token_request(b"refresh.token", b"desktop-client")
            .expect("refresh request should build");
        let body = String::from_utf8(body).expect("form should remain ASCII");
        assert_eq!(
            body,
            "grant_type=refresh_token&refresh_token=refresh.token&client_id=desktop-client"
        );
        assert!(!body.contains("client_secret"));
    }

    #[test]
    fn strict_token_response_accepts_documented_supabase_shape() {
        let response = parse_token_response(
            br#"{
                "access_token":"a.b.c",
                "token_type":"bearer",
                "expires_in":3600,
                "refresh_token":"refresh.token",
                "scope":"openid email profile",
                "id_token":"d.e.f"
            }"#,
        )
        .expect("documented token response should parse");
        assert_eq!(response.expires_in, 3600);
        assert_eq!(
            response.scope,
            vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string()
            ]
        );
    }

    #[test]
    fn duplicate_and_unknown_fields_are_rejected() {
        assert!(parse_token_response(
            br#"{
                "access_token":"a.b.c",
                "access_token":"d.e.f",
                "token_type":"bearer",
                "expires_in":3600,
                "refresh_token":"refresh",
                "scope":"openid",
                "id_token":"g.h.i"
            }"#,
        )
        .is_err());
        assert!(parse_token_response(
            br#"{
                "access_token":"a.b.c",
                "token_type":"bearer",
                "expires_in":3600,
                "refresh_token":"refresh",
                "scope":"openid",
                "id_token":"g.h.i",
                "user":{}
            }"#,
        )
        .is_err());
    }

    #[test]
    fn unsupported_scope_and_oversized_response_are_rejected() {
        assert!(parse_token_response(
            br#"{
                "access_token":"a.b.c",
                "token_type":"bearer",
                "expires_in":3600,
                "refresh_token":"refresh",
                "scope":"openid offline_access",
                "id_token":"g.h.i"
            }"#,
        )
        .is_err());
        assert!(parse_token_response(&vec![b'x'; MAX_TOKEN_RESPONSE_BYTES + 1]).is_err());
    }

    #[test]
    fn strict_response_headers_reject_redirects_and_oversized_lengths() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            "application/json; charset=utf-8".parse().unwrap(),
        );
        headers.insert(CONTENT_LENGTH, "2".parse().unwrap());
        assert!(validate_success_response_headers(StatusCode::OK, &headers).is_ok());
        assert!(validate_success_response_headers(StatusCode::FOUND, &headers).is_err());

        headers.insert(
            CONTENT_LENGTH,
            (MAX_TOKEN_RESPONSE_BYTES + 1).to_string().parse().unwrap(),
        );
        assert!(validate_success_response_headers(StatusCode::OK, &headers).is_err());
    }

    #[test]
    fn response_content_type_allows_only_json_and_utf8() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "text/html".parse().unwrap());
        assert!(validate_success_response_headers(StatusCode::OK, &headers).is_err());

        headers.insert(
            CONTENT_TYPE,
            "application/json; charset=iso-8859-1".parse().unwrap(),
        );
        assert!(validate_success_response_headers(StatusCode::OK, &headers).is_err());

        headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
        assert!(validate_success_response_headers(StatusCode::OK, &headers).is_ok());
    }

    #[test]
    fn bounded_transport_client_builds_without_network_access() {
        assert!(build_transport_client().is_ok());
    }
}
