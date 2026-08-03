//! Native browser launch and bounded loopback callback interception for OIDC.
//!
//! The WebView never receives the authorization URL or authorization code through
//! this surface. Only compile-time-pinned loopback callbacks are supported here;
//! custom-scheme callbacks remain disabled until installer registration and
//! single-instance dispatch are implemented.

mod browser;
mod http;
mod runtime;

use crate::{auth::SecureAuthStore, oidc};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::time::{Duration, Instant};
use tauri::Url;

const BUILD_REDIRECT_URI: Option<&str> = option_env!("RECORDER_OIDC_REDIRECT_URI");
const CALLBACK_PATH: &str = "/oidc/callback";
pub(super) const CALLBACK_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const MAX_INVALID_CALLBACKS: usize = 8;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcSignInLaunch {
    pub launched: bool,
    pub callback_mode: &'static str,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcCallbackStatus {
    pub stage: &'static str,
    pub pending: bool,
    pub code_received: bool,
    pub provider_error: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub(super) struct LoopbackEndpoint {
    pub address: SocketAddr,
    pub path: String,
}

pub fn start_sign_in(store: &SecureAuthStore) -> Result<OidcSignInLaunch, String> {
    let endpoint = configured_loopback_endpoint()?;
    let listener = TcpListener::bind(endpoint.address).map_err(|error| {
        eprintln!(
            "[Recorder][AuthHealth] stage=oidc_callback_bind ok=false code={}",
            error.raw_os_error().unwrap_or_default()
        );
        "Unable to reserve the configured loopback sign-in callback port".to_string()
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "Unable to configure the loopback sign-in listener".to_string())?;

    let authorization = oidc::prepare_authorization(store)?;
    if authorization.callback_mode != "loopback" {
        store.cancel_oidc_transaction();
        return Err(
            "This build uses a custom-scheme callback; installer registration is not implemented"
                .to_string(),
        );
    }

    let expected_state = extract_unique_query_parameter(&authorization.authorization_url, "state")
        .ok_or_else(|| {
            store.cancel_oidc_transaction();
            "Prepared authorization request did not contain a unique state value".to_string()
        })?;
    let generation = runtime::begin(expected_state, authorization.expires_at);
    let worker_store = store.clone();
    let worker_endpoint = endpoint.clone();
    std::thread::Builder::new()
        .name("recorder-oidc-loopback".to_string())
        .spawn(move || run_listener(listener, worker_endpoint, generation, worker_store))
        .map_err(|_| {
            runtime::fail(generation);
            store.cancel_oidc_transaction();
            "Unable to start the loopback sign-in listener".to_string()
        })?;

    if let Err(error) = browser::open(&authorization.authorization_url) {
        runtime::fail(generation);
        store.cancel_oidc_transaction();
        return Err(error);
    }

    eprintln!("[Recorder][AuthHealth] stage=oidc_browser_launch ok=true callback_mode=loopback");
    Ok(OidcSignInLaunch {
        launched: true,
        callback_mode: "loopback",
        expires_at: authorization.expires_at,
    })
}

pub fn callback_status() -> OidcCallbackStatus {
    let status = runtime::status();
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_callback_status ok=true callback_stage={} pending={} code_received={} provider_error={}",
        status.stage, status.pending, status.code_received, status.provider_error
    );
    status
}

pub fn cancel_callback() -> bool {
    let had_pending = runtime::cancel();
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_callback_cancel ok=true had_pending={had_pending}"
    );
    had_pending
}

fn configured_loopback_endpoint() -> Result<LoopbackEndpoint, String> {
    let raw = BUILD_REDIRECT_URI
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OIDC redirect URI is not configured in this build".to_string())?;
    parse_loopback_endpoint(raw)
}

fn parse_loopback_endpoint(raw: &str) -> Result<LoopbackEndpoint, String> {
    let url = Url::parse(raw).map_err(|_| "OIDC redirect URI is invalid".to_string())?;
    if url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != CALLBACK_PATH
    {
        return Err(
            "OIDC browser launch currently requires the fixed loopback callback path".to_string(),
        );
    }

    let port = url
        .port()
        .ok_or_else(|| "OIDC loopback callback requires a fixed port".to_string())?;
    let address = url
        .host_str()
        .and_then(|host| {
            host.trim_matches(|character| character == '[' || character == ']')
                .parse::<IpAddr>()
                .ok()
        })
        .filter(|address| address.is_loopback())
        .ok_or_else(|| "OIDC callback host must be an IP loopback address".to_string())?;

    Ok(LoopbackEndpoint {
        address: SocketAddr::new(address, port),
        path: CALLBACK_PATH.to_string(),
    })
}

fn run_listener(
    listener: TcpListener,
    endpoint: LoopbackEndpoint,
    generation: u64,
    store: SecureAuthStore,
) {
    let started = Instant::now();
    let mut invalid_callbacks = 0usize;
    eprintln!("[Recorder][AuthHealth] stage=oidc_callback_listen ok=true callback_mode=loopback");

    while started.elapsed() < CALLBACK_TIMEOUT && runtime::is_active(generation) {
        match listener.accept() {
            Ok((stream, peer)) => {
                if !peer.ip().is_loopback() {
                    invalid_callbacks = invalid_callbacks.saturating_add(1);
                } else {
                    match http::handle_connection(stream, &endpoint, generation) {
                        Ok(http::ConnectionOutcome::Complete) => {
                            if !runtime::status().code_received {
                                store.cancel_oidc_transaction();
                            }
                            return;
                        }
                        Ok(http::ConnectionOutcome::Continue) | Err(_) => {
                            invalid_callbacks = invalid_callbacks.saturating_add(1);
                        }
                    }
                }

                if invalid_callbacks >= MAX_INVALID_CALLBACKS {
                    runtime::fail(generation);
                    store.cancel_oidc_transaction();
                    eprintln!(
                        "[Recorder][AuthHealth] stage=oidc_callback_listen ok=false code=invalid_attempt_limit"
                    );
                    return;
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(error) => {
                runtime::fail(generation);
                store.cancel_oidc_transaction();
                eprintln!(
                    "[Recorder][AuthHealth] stage=oidc_callback_accept ok=false code={}",
                    error.raw_os_error().unwrap_or_default()
                );
                return;
            }
        }
    }

    if runtime::is_active(generation) {
        runtime::timeout(generation);
        store.cancel_oidc_transaction();
        eprintln!("[Recorder][AuthHealth] stage=oidc_callback_timeout ok=false");
    }
}

fn extract_unique_query_parameter(url: &str, name: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let mut values = parsed
        .query_pairs()
        .filter(|(key, _)| key.as_ref() == name)
        .map(|(_, value)| value.into_owned());
    let first = values.next()?;
    if values.next().is_some() {
        return None;
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_endpoint_requires_numeric_loopback_and_fixed_path() {
        assert!(parse_loopback_endpoint("http://127.0.0.1:43829/oidc/callback").is_ok());
        assert!(parse_loopback_endpoint("http://[::1]:43829/oidc/callback").is_ok());
        assert!(parse_loopback_endpoint("http://192.168.1.20:43829/oidc/callback").is_err());
        assert!(parse_loopback_endpoint("http://localhost:43829/oidc/callback").is_err());
        assert!(parse_loopback_endpoint("http://127.0.0.1:43829/other").is_err());
    }
}
