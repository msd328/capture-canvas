use super::runtime::{self, AcceptError};
use super::LoopbackEndpoint;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream};
use std::time::Duration;
use tauri::Url;

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const MAX_REQUEST_TARGET_BYTES: usize = 8 * 1024;
const MAX_CALLBACK_PARAMETERS: usize = 16;
const MAX_PARAMETER_NAME_BYTES: usize = 64;
const MAX_STATE_BYTES: usize = 512;
const MAX_AUTHORIZATION_CODE_BYTES: usize = 4 * 1024;
const MAX_PROVIDER_ERROR_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
enum CallbackPayload {
    AuthorizationCode { state: String, code: String },
    ProviderError { state: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConnectionOutcome {
    Continue,
    Complete,
}

pub(super) fn handle_connection(
    mut stream: TcpStream,
    endpoint: &LoopbackEndpoint,
    generation: u64,
) -> Result<ConnectionOutcome, String> {
    stream
        .set_read_timeout(Some(CONNECTION_TIMEOUT))
        .map_err(|_| "Unable to bound callback request reading".to_string())?;
    stream
        .set_write_timeout(Some(CONNECTION_TIMEOUT))
        .map_err(|_| "Unable to bound callback response writing".to_string())?;

    let request = read_http_headers(&mut stream)?;
    let payload = match parse_callback_request(&request, endpoint) {
        Ok(payload) => payload,
        Err(error) => {
            let _ = write_browser_response(
                &mut stream,
                400,
                "Sign-in response rejected",
                "Return to Recorder and start sign-in again.",
            );
            return Err(error);
        }
    };

    match payload {
        CallbackPayload::AuthorizationCode { state, code } => {
            match runtime::accept_code(generation, state, code) {
                Ok(()) => {
                    write_browser_response(
                        &mut stream,
                        200,
                        "Sign-in response received",
                        "Return to Recorder. Token exchange remains disabled in this build.",
                    )?;
                    eprintln!(
                        "[Recorder][AuthHealth] stage=oidc_callback_code ok=true code_received=true verifier_kept_native=true"
                    );
                    Ok(ConnectionOutcome::Complete)
                }
                Err(AcceptError::StateMismatch) => {
                    write_browser_response(
                        &mut stream,
                        400,
                        "Sign-in response rejected",
                        "Return to Recorder and start sign-in again.",
                    )?;
                    eprintln!(
                        "[Recorder][AuthHealth] stage=oidc_callback_state ok=false code=mismatch"
                    );
                    Ok(ConnectionOutcome::Continue)
                }
                Err(AcceptError::NotActive) => {
                    write_browser_response(
                        &mut stream,
                        409,
                        "Sign-in response expired",
                        "Return to Recorder and start sign-in again.",
                    )?;
                    Ok(ConnectionOutcome::Complete)
                }
            }
        }
        CallbackPayload::ProviderError { state } => {
            match runtime::accept_provider_error(generation, state) {
                Ok(()) => {
                    write_browser_response(
                        &mut stream,
                        200,
                        "Sign-in was not completed",
                        "Return to Recorder to try again.",
                    )?;
                    eprintln!("[Recorder][AuthHealth] stage=oidc_callback_provider_error ok=true");
                    Ok(ConnectionOutcome::Complete)
                }
                Err(AcceptError::StateMismatch) => {
                    write_browser_response(
                        &mut stream,
                        400,
                        "Sign-in response rejected",
                        "Return to Recorder and start sign-in again.",
                    )?;
                    Ok(ConnectionOutcome::Continue)
                }
                Err(AcceptError::NotActive) => {
                    write_browser_response(
                        &mut stream,
                        409,
                        "Sign-in response expired",
                        "Return to Recorder and start sign-in again.",
                    )?;
                    Ok(ConnectionOutcome::Complete)
                }
            }
        }
    }
}

fn read_http_headers(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut request = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|_| "Unable to read the callback request".to_string())?;
        if read == 0 {
            return Err("Callback connection closed before headers completed".to_string());
        }
        request.extend_from_slice(&buffer[..read]);
        if request.len() > MAX_HTTP_HEADER_BYTES {
            return Err("Callback request headers were too large".to_string());
        }
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(request);
        }
    }
}

fn parse_callback_request(
    request: &[u8],
    endpoint: &LoopbackEndpoint,
) -> Result<CallbackPayload, String> {
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "Callback request headers were incomplete".to_string())?;
    let headers = std::str::from_utf8(&request[..header_end])
        .map_err(|_| "Callback request headers were not UTF-8".to_string())?;
    let mut lines = headers.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "Callback request line was missing".to_string())?;
    let mut request_parts = request_line.split(' ');
    let method = request_parts.next().unwrap_or_default();
    let target = request_parts.next().unwrap_or_default();
    let version = request_parts.next().unwrap_or_default();
    if request_parts.next().is_some()
        || method != "GET"
        || (version != "HTTP/1.1" && version != "HTTP/1.0")
        || !target.starts_with('/')
        || target.len() > MAX_REQUEST_TARGET_BYTES
    {
        return Err("Callback request line was invalid".to_string());
    }

    let mut host_header: Option<&str> = None;
    let mut content_length: Option<usize> = None;
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| "Callback header was malformed".to_string())?;
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            if host_header.replace(value).is_some() {
                return Err("Callback request contained multiple Host headers".to_string());
            }
        } else if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(
                    "Callback request contained multiple Content-Length headers".to_string()
                );
            }
            content_length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| "Callback Content-Length was invalid".to_string())?,
            );
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err("Callback request transfer encoding is not accepted".to_string());
        }
    }
    if content_length.unwrap_or(0) != 0 || header_end + 4 != request.len() {
        return Err("Callback GET request must not contain a body".to_string());
    }
    let host_header = host_header.ok_or_else(|| "Callback Host header was missing".to_string())?;
    validate_host_header(host_header, endpoint)?;

    let authority = if endpoint.address.is_ipv6() {
        format!("[{}]:{}", endpoint.address.ip(), endpoint.address.port())
    } else {
        endpoint.address.to_string()
    };
    let callback_url = Url::parse(&format!("http://{authority}{target}"))
        .map_err(|_| "Callback request target was invalid".to_string())?;
    if callback_url.path() != endpoint.path || callback_url.fragment().is_some() {
        return Err("Callback request path was invalid".to_string());
    }

    parse_callback_parameters(&callback_url)
}

fn validate_host_header(raw: &str, endpoint: &LoopbackEndpoint) -> Result<(), String> {
    let parsed = Url::parse(&format!("http://{raw}/"))
        .map_err(|_| "Callback Host header was invalid".to_string())?;
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("Callback Host header contained invalid components".to_string());
    }
    let address = parsed
        .host_str()
        .and_then(|host| {
            host.trim_matches(|character| character == '[' || character == ']')
                .parse::<IpAddr>()
                .ok()
        })
        .ok_or_else(|| "Callback Host header was not an IP address".to_string())?;
    if address != endpoint.address.ip() || parsed.port() != Some(endpoint.address.port()) {
        return Err("Callback Host header did not match the configured listener".to_string());
    }
    Ok(())
}

fn parse_callback_parameters(url: &Url) -> Result<CallbackPayload, String> {
    let mut state: Option<String> = None;
    let mut code: Option<String> = None;
    let mut provider_error: Option<String> = None;
    let mut count = 0usize;

    for (name, value) in url.query_pairs() {
        count = count.saturating_add(1);
        if count > MAX_CALLBACK_PARAMETERS
            || name.len() > MAX_PARAMETER_NAME_BYTES
            || name.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err("Callback query parameters exceeded their limits".to_string());
        }

        match name.as_ref() {
            "state" => set_unique_parameter(&mut state, value.into_owned(), MAX_STATE_BYTES)?,
            "code" => {
                set_unique_parameter(&mut code, value.into_owned(), MAX_AUTHORIZATION_CODE_BYTES)?
            }
            "error" => set_unique_parameter(
                &mut provider_error,
                value.into_owned(),
                MAX_PROVIDER_ERROR_BYTES,
            )?,
            _ => {
                if value.len() > MAX_AUTHORIZATION_CODE_BYTES
                    || value.bytes().any(|byte| byte.is_ascii_control())
                {
                    return Err("Callback query parameter was too large".to_string());
                }
            }
        }
    }

    let state = state.ok_or_else(|| "Callback state was missing".to_string())?;
    match (code, provider_error) {
        (Some(code), None) => Ok(CallbackPayload::AuthorizationCode { state, code }),
        (None, Some(_)) => Ok(CallbackPayload::ProviderError { state }),
        _ => Err("Callback must contain exactly one code or error".to_string()),
    }
}

fn set_unique_parameter(
    target: &mut Option<String>,
    value: String,
    maximum_bytes: usize,
) -> Result<(), String> {
    if target.is_some()
        || value.is_empty()
        || value.len() > maximum_bytes
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("Callback query parameter was invalid or duplicated".to_string());
    }
    *target = Some(value);
    Ok(())
}

fn write_browser_response(
    stream: &mut TcpStream,
    status: u16,
    title: &str,
    message: &str,
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        409 => "Conflict",
        _ => "Error",
    };
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Recorder sign-in</title></head><body><main><h1>{title}</h1><p>{message}</p></main></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nPragma: no-cache\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|_| "Unable to write the callback response".to_string())?;
    stream
        .flush()
        .map_err(|_| "Unable to flush the callback response".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn endpoint() -> LoopbackEndpoint {
        LoopbackEndpoint {
            address: "127.0.0.1:43829"
                .parse::<SocketAddr>()
                .expect("test address should parse"),
            path: "/oidc/callback".to_string(),
        }
    }

    #[test]
    fn callback_parser_accepts_one_code_and_state() {
        let request = b"GET /oidc/callback?code=abc123&state=state-value HTTP/1.1\r\nHost: 127.0.0.1:43829\r\nConnection: close\r\n\r\n";
        assert_eq!(
            parse_callback_request(request, &endpoint()).expect("callback should parse"),
            CallbackPayload::AuthorizationCode {
                state: "state-value".to_string(),
                code: "abc123".to_string(),
            }
        );
    }

    #[test]
    fn provider_error_description_is_not_retained() {
        let request = b"GET /oidc/callback?error=access_denied&error_description=user+cancelled&state=state-value HTTP/1.1\r\nHost: 127.0.0.1:43829\r\n\r\n";
        assert_eq!(
            parse_callback_request(request, &endpoint()).expect("provider error should parse"),
            CallbackPayload::ProviderError {
                state: "state-value".to_string(),
            }
        );
    }

    #[test]
    fn duplicate_state_and_mismatched_host_are_rejected() {
        let duplicate = b"GET /oidc/callback?code=abc&state=one&state=two HTTP/1.1\r\nHost: 127.0.0.1:43829\r\n\r\n";
        assert!(parse_callback_request(duplicate, &endpoint()).is_err());

        let wrong_host =
            b"GET /oidc/callback?code=abc&state=one HTTP/1.1\r\nHost: 127.0.0.1:43830\r\n\r\n";
        assert!(parse_callback_request(wrong_host, &endpoint()).is_err());
    }

    #[test]
    fn host_header_rejects_userinfo_and_accepts_ipv6_loopback() {
        assert!(validate_host_header("user@127.0.0.1:43829", &endpoint()).is_err());

        let ipv6_endpoint = LoopbackEndpoint {
            address: "[::1]:43829"
                .parse::<SocketAddr>()
                .expect("IPv6 test address should parse"),
            path: "/oidc/callback".to_string(),
        };
        assert!(validate_host_header("[::1]:43829", &ipv6_endpoint).is_ok());
    }
}
