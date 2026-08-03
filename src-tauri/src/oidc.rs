//! Compile-time-pinned OIDC client configuration and authorization URL construction.
//!
//! Provider endpoints and public client metadata are intentionally supplied at build
//! time. Runtime environment variables, frontend settings, JSON files, and WebView
//! storage cannot redirect Recorder authentication to an attacker-controlled origin.
//! This module does not open a browser or exchange authorization codes yet.

use crate::auth::{OidcAuthorizationPreparation, SecureAuthStore};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::net::IpAddr;
use tauri::Url;

const DEFAULT_SCOPES: &str = "openid profile email offline_access";
const MAX_AUTHORIZATION_URL_BYTES: usize = 8 * 1024;
const MAX_CLIENT_ID_CHARS: usize = 512;
const MAX_SCOPE_COUNT: usize = 16;
const MAX_SCOPE_CHARS: usize = 64;
const MAX_SCOPE_SET_CHARS: usize = 1_024;

const BUILD_AUTHORIZATION_ENDPOINT: Option<&str> =
    option_env!("RECORDER_OIDC_AUTHORIZATION_ENDPOINT");
const BUILD_CLIENT_ID: Option<&str> = option_env!("RECORDER_OIDC_CLIENT_ID");
const BUILD_REDIRECT_URI: Option<&str> = option_env!("RECORDER_OIDC_REDIRECT_URI");
const BUILD_SCOPES: Option<&str> = option_env!("RECORDER_OIDC_SCOPES");

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcClientStatus {
    pub configured: bool,
    pub authorization_endpoint_https: bool,
    pub callback_mode: Option<&'static str>,
    pub scope_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcAuthorizationRequest {
    pub authorization_url: String,
    pub expires_at: DateTime<Utc>,
    pub callback_mode: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallbackMode {
    CustomScheme,
    Loopback,
}

impl CallbackMode {
    fn label(self) -> &'static str {
        match self {
            Self::CustomScheme => "custom-scheme",
            Self::Loopback => "loopback",
        }
    }
}

#[derive(Clone, Debug)]
struct OidcClientConfig {
    authorization_endpoint: Url,
    client_id: String,
    redirect_uri: Url,
    callback_mode: CallbackMode,
    scopes: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OidcConfigError {
    code: &'static str,
}

impl OidcConfigError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl OidcClientConfig {
    fn from_build() -> Result<Option<Self>, OidcConfigError> {
        Self::from_values(
            BUILD_AUTHORIZATION_ENDPOINT,
            BUILD_CLIENT_ID,
            BUILD_REDIRECT_URI,
            BUILD_SCOPES,
        )
    }

    fn from_values(
        authorization_endpoint: Option<&str>,
        client_id: Option<&str>,
        redirect_uri: Option<&str>,
        scopes: Option<&str>,
    ) -> Result<Option<Self>, OidcConfigError> {
        let required_presence = [
            authorization_endpoint.is_some(),
            client_id.is_some(),
            redirect_uri.is_some(),
        ];
        if required_presence.iter().all(|present| !present) {
            return Ok(None);
        }
        if required_presence.iter().any(|present| !present) {
            return Err(OidcConfigError::new("incomplete"));
        }

        let authorization_endpoint = authorization_endpoint
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| OidcConfigError::new("authorization_endpoint_empty"))?;
        let client_id = client_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| OidcConfigError::new("client_id_empty"))?;
        let redirect_uri = redirect_uri
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| OidcConfigError::new("redirect_uri_empty"))?;

        if client_id.chars().count() > MAX_CLIENT_ID_CHARS
            || client_id.chars().any(char::is_control)
        {
            return Err(OidcConfigError::new("client_id_invalid"));
        }

        let authorization_endpoint = Url::parse(authorization_endpoint)
            .map_err(|_| OidcConfigError::new("authorization_endpoint_parse"))?;
        validate_authorization_endpoint(&authorization_endpoint)?;

        let redirect_uri =
            Url::parse(redirect_uri).map_err(|_| OidcConfigError::new("redirect_uri_parse"))?;
        let callback_mode = validate_redirect_uri(&redirect_uri)?;
        let scopes = parse_scopes(scopes.unwrap_or(DEFAULT_SCOPES))?;

        Ok(Some(Self {
            authorization_endpoint,
            client_id: client_id.to_string(),
            redirect_uri,
            callback_mode,
            scopes,
        }))
    }

    fn build_authorization_url(
        &self,
        preparation: &OidcAuthorizationPreparation,
    ) -> Result<String, OidcConfigError> {
        let mut authorization_url = self.authorization_endpoint.clone();
        {
            let mut query = authorization_url.query_pairs_mut();
            query.append_pair("response_type", "code");
            query.append_pair("response_mode", "query");
            query.append_pair("client_id", &self.client_id);
            query.append_pair("redirect_uri", self.redirect_uri.as_str());
            query.append_pair("scope", &self.scopes.join(" "));
            query.append_pair("state", &preparation.state);
            query.append_pair("nonce", &preparation.nonce);
            query.append_pair("code_challenge", &preparation.code_challenge);
            query.append_pair("code_challenge_method", preparation.code_challenge_method);
        }

        let value = authorization_url.to_string();
        if value.len() > MAX_AUTHORIZATION_URL_BYTES {
            return Err(OidcConfigError::new("authorization_url_too_large"));
        }
        Ok(value)
    }
}

pub fn client_status() -> Result<OidcClientStatus, String> {
    match OidcClientConfig::from_build() {
        Ok(None) => {
            eprintln!("[Recorder][AuthHealth] stage=oidc_client_config ok=true configured=false");
            Ok(OidcClientStatus {
                configured: false,
                authorization_endpoint_https: false,
                callback_mode: None,
                scope_count: 0,
            })
        }
        Ok(Some(config)) => {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_client_config ok=true configured=true callback_mode={} scope_count={}",
                config.callback_mode.label(),
                config.scopes.len()
            );
            Ok(OidcClientStatus {
                configured: true,
                authorization_endpoint_https: true,
                callback_mode: Some(config.callback_mode.label()),
                scope_count: config.scopes.len(),
            })
        }
        Err(error) => {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_client_config ok=false configured=false code={}",
                error.code
            );
            Err(format!(
                "OIDC client configuration is invalid ({})",
                error.code
            ))
        }
    }
}

pub fn prepare_authorization(store: &SecureAuthStore) -> Result<OidcAuthorizationRequest, String> {
    let config = match OidcClientConfig::from_build() {
        Ok(Some(config)) => config,
        Ok(None) => {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_authorization_prepare ok=false code=not_configured"
            );
            return Err("OIDC client is not configured in this build".to_string());
        }
        Err(error) => {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_authorization_prepare ok=false code={}",
                error.code
            );
            return Err(format!(
                "OIDC client configuration is invalid ({})",
                error.code
            ));
        }
    };

    let preparation = store.prepare_oidc_transaction();
    let authorization_url = match config.build_authorization_url(&preparation) {
        Ok(url) => url,
        Err(error) => {
            store.cancel_oidc_transaction();
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_authorization_prepare ok=false code={}",
                error.code
            );
            return Err(format!(
                "Unable to prepare the OIDC authorization request ({})",
                error.code
            ));
        }
    };

    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_authorization_prepare ok=true callback_mode={} url_bytes={} verifier_kept_native=true",
        config.callback_mode.label(),
        authorization_url.len()
    );
    Ok(OidcAuthorizationRequest {
        authorization_url,
        expires_at: preparation.expires_at,
        callback_mode: config.callback_mode.label(),
    })
}

fn validate_authorization_endpoint(url: &Url) -> Result<(), OidcConfigError> {
    if url.scheme() != "https" {
        return Err(OidcConfigError::new("authorization_endpoint_not_https"));
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(OidcConfigError::new("authorization_endpoint_authority"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(OidcConfigError::new("authorization_endpoint_components"));
    }
    Ok(())
}

fn validate_redirect_uri(url: &Url) -> Result<CallbackMode, OidcConfigError> {
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(OidcConfigError::new("redirect_uri_components"));
    }

    if url.scheme() == "capture-canvas"
        && url.host_str() == Some("auth")
        && url.path() == "/callback"
        && url.port().is_none()
    {
        return Ok(CallbackMode::CustomScheme);
    }

    if url.scheme() == "http" && url.path() == "/oidc/callback" && url.port().is_some() {
        let loopback = url
            .host_str()
            .and_then(|host| host.parse::<IpAddr>().ok())
            .is_some_and(|address| address.is_loopback());
        if loopback {
            return Ok(CallbackMode::Loopback);
        }
    }

    Err(OidcConfigError::new("redirect_uri_not_native"))
}

fn parse_scopes(raw: &str) -> Result<Vec<String>, OidcConfigError> {
    let mut scopes = Vec::new();
    for scope in raw.split_ascii_whitespace() {
        if scope.len() > MAX_SCOPE_CHARS || !scope.bytes().all(is_valid_scope_byte) {
            return Err(OidcConfigError::new("scope_invalid"));
        }
        if !scopes.iter().any(|existing| existing == scope) {
            scopes.push(scope.to_string());
        }
    }

    if scopes.is_empty() || scopes.len() > MAX_SCOPE_COUNT {
        return Err(OidcConfigError::new("scope_count"));
    }
    if !scopes.iter().any(|scope| scope == "openid") {
        return Err(OidcConfigError::new("scope_missing_openid"));
    }
    if scopes.join(" ").len() > MAX_SCOPE_SET_CHARS {
        return Err(OidcConfigError::new("scope_set_too_large"));
    }
    Ok(scopes)
}

fn is_valid_scope_byte(byte: u8) -> bool {
    byte == 0x21 || (0x23..=0x5b).contains(&byte) || (0x5d..=0x7e).contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_client() -> OidcClientConfig {
        OidcClientConfig::from_values(
            Some("https://identity.example.test/oauth2/authorize"),
            Some("recorder desktop client"),
            Some("capture-canvas://auth/callback"),
            Some("openid profile email offline_access"),
        )
        .expect("test OIDC config should be valid")
        .expect("test OIDC config should be present")
    }

    fn preparation() -> OidcAuthorizationPreparation {
        OidcAuthorizationPreparation {
            state: "state-value".to_string(),
            nonce: "nonce-value".to_string(),
            code_challenge: "challenge-value".to_string(),
            code_challenge_method: "S256",
            expires_at: Utc::now(),
        }
    }

    #[test]
    fn absent_build_values_leave_cloud_auth_disabled() {
        assert!(OidcClientConfig::from_values(None, None, None, None)
            .expect("absent config should be valid")
            .is_none());
    }

    #[test]
    fn partial_configuration_is_rejected() {
        assert_eq!(
            OidcClientConfig::from_values(
                Some("https://identity.example.test/authorize"),
                None,
                Some("capture-canvas://auth/callback"),
                None,
            )
            .expect_err("partial config must fail"),
            OidcConfigError::new("incomplete")
        );
    }

    #[test]
    fn insecure_authorization_endpoint_is_rejected() {
        assert_eq!(
            OidcClientConfig::from_values(
                Some("http://identity.example.test/authorize"),
                Some("client"),
                Some("capture-canvas://auth/callback"),
                None,
            )
            .expect_err("insecure endpoint must fail"),
            OidcConfigError::new("authorization_endpoint_not_https")
        );
    }

    #[test]
    fn authorization_endpoint_cannot_preseed_query_parameters() {
        assert_eq!(
            OidcClientConfig::from_values(
                Some("https://identity.example.test/authorize?prompt=none"),
                Some("client"),
                Some("capture-canvas://auth/callback"),
                None,
            )
            .expect_err("preseeded query must fail"),
            OidcConfigError::new("authorization_endpoint_components")
        );
    }

    #[test]
    fn non_loopback_http_redirect_is_rejected() {
        assert_eq!(
            OidcClientConfig::from_values(
                Some("https://identity.example.test/authorize"),
                Some("client"),
                Some("http://192.168.1.20:43829/oidc/callback"),
                None,
            )
            .expect_err("non-loopback redirect must fail"),
            OidcConfigError::new("redirect_uri_not_native")
        );
    }

    #[test]
    fn loopback_redirect_requires_a_fixed_port_and_path() {
        assert!(OidcClientConfig::from_values(
            Some("https://identity.example.test/authorize"),
            Some("client"),
            Some("http://127.0.0.1:43829/oidc/callback"),
            None,
        )
        .expect("loopback redirect should be valid")
        .is_some());
    }

    #[test]
    fn openid_scope_is_mandatory() {
        assert_eq!(
            OidcClientConfig::from_values(
                Some("https://identity.example.test/authorize"),
                Some("client"),
                Some("capture-canvas://auth/callback"),
                Some("profile email"),
            )
            .expect_err("openid scope must be required"),
            OidcConfigError::new("scope_missing_openid")
        );
    }

    #[test]
    fn authorization_url_contains_standard_parameters_and_no_verifier() {
        let config = configured_client();
        let authorization_url = config
            .build_authorization_url(&preparation())
            .expect("authorization URL should build");
        let parsed = Url::parse(&authorization_url).expect("authorization URL should parse");
        let parameters: std::collections::HashMap<_, _> =
            parsed.query_pairs().into_owned().collect();

        assert_eq!(
            parameters.get("response_type").map(String::as_str),
            Some("code")
        );
        assert_eq!(
            parameters.get("response_mode").map(String::as_str),
            Some("query")
        );
        assert_eq!(
            parameters.get("client_id").map(String::as_str),
            Some("recorder desktop client")
        );
        assert_eq!(
            parameters.get("redirect_uri").map(String::as_str),
            Some("capture-canvas://auth/callback")
        );
        assert_eq!(
            parameters.get("state").map(String::as_str),
            Some("state-value")
        );
        assert_eq!(
            parameters.get("nonce").map(String::as_str),
            Some("nonce-value")
        );
        assert_eq!(
            parameters.get("code_challenge").map(String::as_str),
            Some("challenge-value")
        );
        assert_eq!(
            parameters.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert!(!parameters.contains_key("code_verifier"));
    }
}
