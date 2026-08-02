# Native OIDC browser and loopback callback boundary — 2026-08-02

## Scope

This batch implements the next provider-neutral native sign-in boundary above the
existing compile-time-pinned OIDC client and native PKCE transaction. It can open a
configured authorization page in the Windows default browser and receive one bounded
loopback callback. It intentionally stops before authorization-code exchange, token
verification, account persistence, upload authorization, or SaaS ownership decisions.

Custom-scheme callbacks remain disabled until the installer can register the protocol
and the desktop application has reviewed single-instance callback dispatch.

## Implemented

- Added native Windows browser launch through `ShellExecuteW` with direct wide-string
  arguments and no constructed shell command.
- Enabled only the narrow `Win32_UI_Shell` Windows API projection; no Tauri plugin or
  WebView capability was added.
- Required a compile-time-pinned numeric loopback callback:
  - `http://127.0.0.1:<fixed-port>/oidc/callback`, or
  - `http://[::1]:<fixed-port>/oidc/callback`.
- Rejected `localhost`, LAN addresses, missing ports, different paths, userinfo,
  queries, and fragments in the configured callback.
- Bound the loopback listener before opening the browser so port conflicts fail closed.
- Added a ten-minute overall flow deadline and two-second connection read/write
  deadlines.
- Limited callback headers to 16 KiB and the request target to 8 KiB.
- Accepted only bodyless `GET` requests using HTTP/1.0 or HTTP/1.1.
- Required an exact numeric `Host` address and port matching the listener.
- Rejected transfer encoding, duplicate `Host`, duplicate `Content-Length`, bodies,
  malformed UTF-8, and malformed request lines.
- Limited callbacks to sixteen bounded query parameters.
- Required exactly one unique `state` and exactly one of `code` or `error`.
- Compared callback state in constant time.
- Retained neither provider error descriptions nor unknown provider text.
- Allowed at most eight invalid local callback attempts before stopping the listener.
- Returned a static no-store browser response with `default-src 'none'`,
  `nosniff`, and `no-referrer` headers.
- Stored the received authorization code and callback state only in native memory.
- Added best-effort volatile clearing for native callback secret buffers.
- Limited a received authorization code to sixty seconds. If the expiry worker cannot
  start, the code is cleared immediately instead of remaining resident.
- Added Settings controls for starting, cancelling, and observing the native callback
  flow. The button is enabled only for a configured loopback build with supported
  secure storage.
- Kept authorization URL and authorization code values out of the WebView-facing
  sign-in command and out of structured diagnostics.

## Validation added

Rust unit tests cover:

- IPv4 and IPv6 numeric loopback configuration;
- rejection of `localhost`, LAN callbacks, and wrong callback paths;
- a valid code-and-state callback;
- provider-error parsing without retaining `error_description`;
- duplicate-state rejection;
- mismatched Host-port rejection;
- Host userinfo rejection; and
- IPv6 Host acceptance.

The native runtime also contains a one-time state acceptance/replay test.

## Expected diagnostics

A configured loopback build can emit:

```text
[Recorder][AuthHealth] stage=oidc_callback_listen ok=true callback_mode=loopback
[Recorder][AuthHealth] stage=oidc_browser_launch ok=true callback_mode=loopback
[Recorder][AuthHealth] stage=oidc_callback_code ok=true code_received=true verifier_kept_native=true
[Recorder][AuthHealth] stage=oidc_callback_code_expiry ok=true code_cleared=true
```

Provider cancellation can emit:

```text
[Recorder][AuthHealth] stage=oidc_callback_provider_error ok=true
```

No diagnostic includes the provider URL, client ID, state, nonce, PKCE challenge,
authorization code, provider error description, or local callback request.

## Windows validation

Run the normal unconfigured development build first:

```powershell
cd C:\Users\DINESH\Desktop\VidRec\capture-canvas
git pull --rebase --autostash origin feature/windows-native-capture
.\scripts\windows-local-check.ps1
bun run desktop:dev
```

The normal build should continue to show:

```text
OIDC provider not configured
```

It must not show an enabled **Open provider sign-in** action and must not bind a
callback port or open a browser.

Configured runtime validation must wait for the real provider, public client ID, and
registered loopback redirect URI. Do not test with a production client whose redirect
URI has not been registered exactly.

## Status

```text
SAAS-01  🟡 Native Windows browser launch and bounded loopback callback capture are
             implemented; real-provider runtime, code exchange and ID-token validation
             remain pending.
HLT-29   🟡 Browser/listener/callback/state/code-expiry diagnostics are implemented;
             Windows runtime evidence remains pending.
SEC-12   🟡 PKCE verifier and temporary authorization code remain native-only; real
             refresh-token write/rotation remains pending.
SEC-13   ⚪ No token signature validation, owner derivation, tenancy enforcement,
             object authorization, quota, or rate limiting exists yet.
```

## Security impact

```text
Security impact:
Added a fail-closed native browser and loopback callback boundary for a future OIDC
public-client flow.

Data accessed:
Compile-time-pinned authorization URL and loopback redirect configuration; one-time
native state and PKCE transaction; bounded local HTTP callback metadata.

Data written:
No runtime file, database, recording metadata, browser storage, or credential write.
A received authorization code exists only in native memory for at most sixty seconds.

Network communication added:
Unconfigured builds add none. A configured build may open the pinned HTTPS provider
URL in the system browser and listen on the exact configured numeric loopback address
and port. Recorder itself does not call the provider, token endpoint, SaaS API, object
storage, analytics, crash reporting, or telemetry service in this batch.

New permissions/capabilities:
Added the Windows `Win32_UI_Shell` API projection. No Tauri plugin permission, WebView
capability, filesystem permission, or unrestricted network capability was added.

External processes:
The Windows default browser may be invoked through `ShellExecuteW`. No command shell,
argument interpolation, PATH lookup, or arbitrary executable selection was added.

Untrusted inputs:
Local callback TCP connections, HTTP request line and headers, Host value, query
parameters, provider state/code/error values, and compile-time callback configuration.

Validation added:
Numeric-loopback allowlist, fixed address/port/path checks, listener-before-browser
ordering, request and time bounds, exact Host validation, GET/bodyless enforcement,
query count/value bounds, duplicate rejection, constant-time state comparison, invalid
attempt ceiling, no-store response headers, and automatic native code expiry.

Secrets involved:
No client secret. PKCE verifier, callback state, and authorization code stay native and
are not returned through the sign-in command, persisted, or logged. Native secret
buffers receive best-effort volatile clearing on drop.

Security tests completed:
Static trust-boundary review and Rust negative-test implementation. Windows compile,
real-provider callback, port-collision, timeout, cancellation, malformed-request, and
browser-launch runtime evidence remain pending.

Remaining risks:
Authorization-code exchange is not implemented. ID-token signature, issuer, audience,
expiry and nonce validation are absent. Refresh-token storage/rotation/revocation,
server ownership, tenancy, object authorization, quotas, rate limiting and audit logs
remain absent. A fixed loopback port can be unavailable; the flow fails closed but
needs runtime validation. Same-user local processes can connect to the loopback port,
so one-time state validation remains mandatory. Custom-scheme registration and
single-instance dispatch remain unimplemented.
```
