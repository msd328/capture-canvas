# Pinned OIDC client configuration — 2026-08-02

## Scope

This batch adds the provider-neutral OIDC client layer immediately above the
existing native PKCE transaction state. It intentionally stops before browser
launch, callback interception, token exchange, token verification, or session
persistence.

## Implemented

- Added compile-time OIDC client inputs:
  - `RECORDER_OIDC_AUTHORIZATION_ENDPOINT`
  - `RECORDER_OIDC_CLIENT_ID`
  - `RECORDER_OIDC_REDIRECT_URI`
  - `RECORDER_OIDC_SCOPES`
- Added Cargo rebuild tracking for every OIDC build input.
- Added a direct `url` dependency for standards-compliant URL parsing and encoding.
- Added fail-closed client configuration status.
- Required an HTTPS authorization endpoint without userinfo, query, or fragment.
- Allowed only these native redirect forms:
  - `capture-canvas://auth/callback`
  - fixed-port HTTP loopback at `/oidc/callback` on an IPv4 or IPv6 loopback address.
- Required the `openid` scope and bounded scope count, token size, total scope size,
  client-ID size, and final authorization-URL size.
- Constructed an authorization-code request with:
  - `response_type=code`
  - `response_mode=query`
  - public client ID
  - exact redirect URI
  - scopes
  - one-time state
  - nonce
  - PKCE S256 challenge
- Kept the PKCE verifier native and omitted it from the authorization URL and Tauri
  response.
- Added path/token-free `AuthHealth` stages for client configuration and authorization
  request preparation.
- Added Settings UI status and a provider-configuration readiness check.

## Validation added

Rust unit tests cover:

- disabled-by-default configuration;
- partial configuration rejection;
- non-HTTPS authorization endpoint rejection;
- pre-seeded authorization query rejection;
- non-loopback HTTP redirect rejection;
- fixed loopback callback acceptance;
- mandatory `openid` scope;
- standard authorization parameters; and
- absence of `code_verifier` from the URL.

## Windows validation

Run:

```powershell
cd C:\Users\DINESH\Desktop\VidRec\capture-canvas
git pull --rebase --autostash origin feature/windows-native-capture
.\scripts\windows-local-check.ps1
bun run desktop:dev
```

With no OIDC build variables, **Settings → Cloud account** should show:

```text
OIDC provider not configured
```

The terminal should include:

```text
[Recorder][AuthHealth] stage=oidc_client_config ok=true configured=false
```

A configured-build validation should wait until the real provider, registered
redirect URI, and public client ID are selected. Do not use placeholder production
origins or client identifiers.

## Status

```text
SAAS-01  🟡 Pinned client config and authorization URL implemented; real provider,
             browser launch, callback, exchange and ID-token verification pending.
HLT-28   🟡 OIDC client configuration/preparation diagnostics implemented; Windows
             compile and configured-build evidence pending.
SEC-12   🟡 PKCE verifier remains native; real refresh-token storage/rotation pending.
SEC-13   ⚪ No signed-token, owner, tenancy or rate-limit enforcement yet.
```

## Security impact

```text
Security impact:
Added a fail-closed, compile-time-pinned OIDC public-client configuration and
standards-compliant authorization URL builder.

Data accessed:
Public build-time authorization endpoint, client ID, redirect URI and scopes;
existing native one-time state, nonce and PKCE challenge.

Data written:
No runtime file or credential write. Repository source, tests, roadmap and status
documentation were updated.

Network communication added:
None. The authorization URL is prepared but not opened or requested.

New permissions/capabilities:
None.

External processes:
None.

Untrusted inputs:
Compile-time OIDC configuration values and future frontend command invocation.
The command cannot override provider metadata at runtime.

Validation added:
HTTPS endpoint enforcement, userinfo/query/fragment rejection, native redirect
allowlist, loopback IP validation, scope grammar/count/size bounds, client-ID bound,
authorization-URL bound, standards-compliant percent encoding and negative tests.

Secrets involved:
No client secret. The public client ID and endpoints are not secrets. The PKCE
verifier remains native-only and is not included in the URL, Tauri response or logs.
State, nonce and challenge are one-time protocol values included in the provider URL.

Security tests completed:
Static trust-boundary review and Rust unit-test implementation. Windows compilation,
configured-provider URL inspection and negative build-configuration tests remain.

Remaining risks:
Browser launch and callback interception are not implemented. Authorization-code
exchange, issuer/audience/signature/expiry/nonce validation, refresh rotation,
revocation, API ownership, tenancy, quotas and rate limits remain unimplemented.
Custom-scheme registration and loopback-listener lifecycle require separate review.
```
