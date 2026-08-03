//! Compile-time-pinned OIDC token-exchange and validation metadata.
//!
//! A native browser flow must not start merely because an authorization endpoint is
//! configured. Recorder also requires the HTTPS token endpoint, exact issuer, API
//! audience, and JWKS endpoint that will be used by the next exchange/verification
//! stage. Values are accepted only at compile time and are never read from runtime
//! environment variables, JSON, WebView storage, or user-controlled settings.

use serde::Serialize;
use tauri::Url;

const MAX_ENDPOINT_BYTES: usize = 8 * 1024;
const MAX_AUDIENCE_CHARS: usize = 512;

const BUILD_TOKEN_ENDPOINT: Option<&str> = option_env!("RECORDER_OIDC_TOKEN_ENDPOINT");
const BUILD_ISSUER: Option<&str> = option_env!("RECORDER_OIDC_ISSUER");
const BUILD_AUDIENCE: Option<&str> = option_env!("RECORDER_OIDC_AUDIENCE");
const BUILD_JWKS_URI: Option<&str> = option_env!("RECORDER_OIDC_JWKS_URI");

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcTokenConfigStatus {
    pub configured: bool,
    pub token_endpoint_https: bool,
    pub issuer_https: bool,
    pub audience_configured: bool,
    pub jwks_uri_https: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct OidcTokenConfig {
    pub token_endpoint: Url,
    pub issuer: Url,
    pub audience: String,
    pub jwks_uri: Url,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OidcTokenConfigError {
    code: &'static str,
}

impl OidcTokenConfigError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl OidcTokenConfig {
    fn from_build() -> Result<Option<Self>, OidcTokenConfigError> {
        Self::from_values(
            BUILD_TOKEN_ENDPOINT,
            BUILD_ISSUER,
            BUILD_AUDIENCE,
            BUILD_JWKS_URI,
        )
    }

    fn from_values(
        token_endpoint: Option<&str>,
        issuer: Option<&str>,
        audience: Option<&str>,
        jwks_uri: Option<&str>,
    ) -> Result<Option<Self>, OidcTokenConfigError> {
        let presence = [
            token_endpoint.is_some(),
            issuer.is_some(),
            audience.is_some(),
            jwks_uri.is_some(),
        ];
        if presence.iter().all(|present| !present) {
            return Ok(None);
        }
        if presence.iter().any(|present| !present) {
            return Err(OidcTokenConfigError::new("incomplete"));
        }

        let token_endpoint = required_value(token_endpoint, "token_endpoint_empty")?;
        let issuer = required_value(issuer, "issuer_empty")?;
        let audience = required_value(audience, "audience_empty")?;
        let jwks_uri = required_value(jwks_uri, "jwks_uri_empty")?;

        if audience.chars().count() > MAX_AUDIENCE_CHARS
            || audience
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(OidcTokenConfigError::new("audience_invalid"));
        }

        let token_endpoint = parse_https_endpoint(token_endpoint, "token_endpoint")?;
        let issuer = parse_https_endpoint(issuer, "issuer")?;
        let jwks_uri = parse_https_endpoint(jwks_uri, "jwks_uri")?;

        Ok(Some(Self {
            token_endpoint,
            issuer,
            audience: audience.to_string(),
            jwks_uri,
        }))
    }
}

pub fn status() -> Result<OidcTokenConfigStatus, String> {
    match OidcTokenConfig::from_build() {
        Ok(None) => {
            eprintln!("[Recorder][AuthHealth] stage=oidc_token_config ok=true configured=false");
            Ok(OidcTokenConfigStatus {
                configured: false,
                token_endpoint_https: false,
                issuer_https: false,
                audience_configured: false,
                jwks_uri_https: false,
            })
        }
        Ok(Some(config)) => {
            // Touch every validated field so future refactors cannot accidentally make
            // status independent from the complete trust contract. Values themselves
            // are intentionally never logged or returned to the WebView.
            let token_endpoint_https = config.token_endpoint.scheme() == "https";
            let issuer_https = config.issuer.scheme() == "https";
            let audience_configured = !config.audience.is_empty();
            let jwks_uri_https = config.jwks_uri.scheme() == "https";
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_token_config ok=true configured=true token_endpoint_https={token_endpoint_https} issuer_https={issuer_https} audience_configured={audience_configured} jwks_uri_https={jwks_uri_https}"
            );
            Ok(OidcTokenConfigStatus {
                configured: true,
                token_endpoint_https,
                issuer_https,
                audience_configured,
                jwks_uri_https,
            })
        }
        Err(error) => {
            eprintln!(
                "[Recorder][AuthHealth] stage=oidc_token_config ok=false configured=false code={}",
                error.code
            );
            Err(format!(
                "OIDC token validation configuration is invalid ({})",
                error.code
            ))
        }
    }
}

pub(crate) fn require_configured() -> Result<OidcTokenConfig, String> {
    match OidcTokenConfig::from_build() {
        Ok(Some(config)) => Ok(config),
        Ok(None) => Err("OIDC token validation is not configured in this build".to_string()),
        Err(error) => Err(format!(
            "OIDC token validation configuration is invalid ({})",
            error.code
        )),
    }
}

fn required_value<'a>(
    value: Option<&'a str>,
    empty_code: &'static str,
) -> Result<&'a str, OidcTokenConfigError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| OidcTokenConfigError::new(empty_code))
}

fn parse_https_endpoint(raw: &str, field: &'static str) -> Result<Url, OidcTokenConfigError> {
    if raw.len() > MAX_ENDPOINT_BYTES {
        return Err(OidcTokenConfigError::new(match field {
            "token_endpoint" => "token_endpoint_too_large",
            "issuer" => "issuer_too_large",
            _ => "jwks_uri_too_large",
        }));
    }

    let url = Url::parse(raw).map_err(|_| {
        OidcTokenConfigError::new(match field {
            "token_endpoint" => "token_endpoint_parse",
            "issuer" => "issuer_parse",
            _ => "jwks_uri_parse",
        })
    })?;
    if url.scheme() != "https" {
        return Err(OidcTokenConfigError::new(match field {
            "token_endpoint" => "token_endpoint_not_https",
            "issuer" => "issuer_not_https",
            _ => "jwks_uri_not_https",
        }));
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(OidcTokenConfigError::new(match field {
            "token_endpoint" => "token_endpoint_authority",
            "issuer" => "issuer_authority",
            _ => "jwks_uri_authority",
        }));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(OidcTokenConfigError::new(match field {
            "token_endpoint" => "token_endpoint_components",
            "issuer" => "issuer_components",
            _ => "jwks_uri_components",
        }));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_values_leave_token_exchange_disabled() {
        assert!(OidcTokenConfig::from_values(None, None, None, None)
            .expect("absent configuration should be valid")
            .is_none());
    }

    #[test]
    fn partial_token_configuration_is_rejected() {
        assert_eq!(
            OidcTokenConfig::from_values(
                Some("https://identity.example.test/oauth/token"),
                Some("https://identity.example.test/"),
                None,
                Some("https://identity.example.test/.well-known/jwks.json"),
            )
            .expect_err("partial configuration must fail"),
            OidcTokenConfigError::new("incomplete")
        );
    }

    #[test]
    fn insecure_or_mutable_endpoints_are_rejected() {
        assert_eq!(
            OidcTokenConfig::from_values(
                Some("http://identity.example.test/oauth/token"),
                Some("https://identity.example.test/"),
                Some("recorder-api"),
                Some("https://identity.example.test/.well-known/jwks.json"),
            )
            .expect_err("HTTP token endpoint must fail"),
            OidcTokenConfigError::new("token_endpoint_not_https")
        );
        assert_eq!(
            OidcTokenConfig::from_values(
                Some("https://identity.example.test/oauth/token?tenant=runtime"),
                Some("https://identity.example.test/"),
                Some("recorder-api"),
                Some("https://identity.example.test/.well-known/jwks.json"),
            )
            .expect_err("pre-seeded token query must fail"),
            OidcTokenConfigError::new("token_endpoint_components")
        );
    }

    #[test]
    fn audience_cannot_contain_whitespace_or_controls() {
        assert_eq!(
            OidcTokenConfig::from_values(
                Some("https://identity.example.test/oauth/token"),
                Some("https://identity.example.test/"),
                Some("recorder api"),
                Some("https://identity.example.test/.well-known/jwks.json"),
            )
            .expect_err("whitespace audience must fail"),
            OidcTokenConfigError::new("audience_invalid")
        );
    }

    #[test]
    fn complete_https_token_contract_is_accepted() {
        let config = OidcTokenConfig::from_values(
            Some("https://identity.example.test/oauth/token"),
            Some("https://identity.example.test/"),
            Some("https://api.example.test/recorder"),
            Some("https://identity.example.test/.well-known/jwks.json"),
        )
        .expect("complete configuration should be valid")
        .expect("complete configuration should be present");
        assert_eq!(config.token_endpoint.scheme(), "https");
        assert_eq!(config.issuer.scheme(), "https");
        assert_eq!(config.jwks_uri.scheme(), "https");
        assert_eq!(config.audience, "https://api.example.test/recorder");
    }
}
