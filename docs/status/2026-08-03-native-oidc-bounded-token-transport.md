# Bounded native OIDC token transport — 2026-08-03

## Scope

This batch adds the private native HTTPS transport that will eventually submit the
coordinated authorization-code, PKCE-verifier and nonce handoff to the compile-time
pinned token endpoint.

The transport can construct and parse a provider token exchange, but it is deliberately
not registered as a Tauri command and is not called by the current sign-in flow. A token
response remains unverified native state until a later batch validates the ID-token
signature and claims.

## Implemented

- Added a dedicated Reqwest 0.13.4 client using the Rustls TLS backend.
- Disabled Reqwest default features and system/runtime proxy discovery.
- Disabled automatic redirects and request retries.
- Disabled gzip, Brotli, deflate and Zstandard response decompression.
- Sends only the existing bounded public-client authorization-code form:
  - `grant_type=authorization_code`;
  - authorization code;
  - compile-time client ID;
  - compile-time redirect URI; and
  - PKCE verifier.
- Uses the compile-time-pinned HTTPS token endpoint from the existing trust contract.
- Applies bounded transport deadlines:
  - five-second connection timeout;
  - five-second per-read timeout; and
  - twelve-second total request deadline.
- Requires the response URL to remain the exact pinned token endpoint.
- Accepts only HTTP 200.
- Requires one `application/json` content type with no parameter other than optional
  UTF-8 charset.
- Rejects compressed response content encodings.
- Rejects duplicate, malformed or declared-oversized content lengths.
- Streams the body through an enforced 64 KiB actual-byte ceiling.
- Reuses the existing strict token-response parser, including duplicate-field,
  unknown-field, token-shape, lifetime and scope checks.
- Produces one private, non-serializable `UnverifiedOidcExchange` containing:
  - access token;
  - refresh token;
  - ID token;
  - approved scopes;
  - token lifetime; and
  - the expected authorization-request nonce.
- All token and nonce buffers receive the existing best-effort volatile clearing on
  drop.
- Added secret-free readiness fields for transport availability, disabled redirects,
  disabled runtime proxy use and configured deadlines.
- Added non-networked probes and unit tests for client construction, strict JSON
  headers, redirect/status rejection and declared response-size rejection.
- Updated Settings so its sign-in security check includes the new transport probes and
  continues to describe live exchange as disabled.

## Deliberately not implemented

- Calling the private exchange function from a Tauri command or callback worker.
- Fetching or caching the pinned JWKS document.
- Selecting a key by a validated `kid`.
- Restricting the ID-token algorithm to the approved asymmetric set.
- Verifying the ID-token signature.
- Verifying issuer, audience/client ID, subject, expiry, not-before, issued-at or nonce.
- Treating the access token as an authenticated API credential.
- Persisting or rotating the refresh token in Windows Credential Manager.
- Creating a native signed-in session.
- Refresh, logout or provider revocation requests.
- Calling `/api/v1/me/access` from the desktop.
- React AuthGate, payment routes or native entitlement enforcement.

No access token, refresh token, ID token, authorization code, verifier, nonce, endpoint,
client ID or redirect URI is returned to React or written to diagnostics.

## Status

```text
SAAS-01  🟡 Bounded private HTTPS token transport and strict response framing are
             implemented. Live exchange and ID-token validation remain disabled.
SEC-12   🟡 Unverified token material remains native-only and is never persisted or
             accepted as a session. Windows compile/runtime validation remains.
HLT-30   🟡 Added transport/deadline/redirect/proxy/header/size readiness diagnostics;
             configured-provider and Windows evidence remain pending.
```

## Security impact

```text
Security impact:
Added a tightly bounded native network boundary for the future OAuth public-client
exchange while preserving the rule that unverified token material cannot create a
session or cross into the WebView.

Data accessed:
The coordinated native authorization code, PKCE verifier and nonce; compile-time client
ID, redirect URI and token endpoint; and, only when the private function is called in a
future batch, the provider's token response.

Data written:
Transient native request/response buffers and repository source/documentation only. No
token or session is written to persistent storage.

Network communication added:
A crate-private HTTPS POST capability to the compile-time-pinned token endpoint. No
current Tauri command, callback worker or frontend path invokes it.

New permissions/capabilities:
None.

New dependencies:
Reqwest 0.13.4 with Rustls and explicitly disabled compression handling. Its transitive
TLS, HTTP and decompression dependency resolution requires the Windows local gate and
licence audit.

External processes:
None.

Untrusted inputs:
Provider HTTP status, response headers, declared length, streamed response bytes and
strict token JSON fields. All are rejected unless they satisfy the bounded response
contract.

Validation added:
Non-networked Rust probes/tests for client construction, strict JSON media type,
redirect/status rejection, declared size rejection and the existing actual-body/parser
limits. Settings now requires these probe results before reporting success.

Secrets involved:
Authorization code, PKCE verifier, nonce, request form, access token, refresh token and
ID token. They remain native-only, are excluded from logs and serialization, and receive
best-effort volatile clearing where owned by Recorder secret buffers.

Security tests completed:
Static command-registration review confirms the exchange function and unverified token
bundle are crate-private and absent from the Tauri invoke handler. Static transport
review confirms pinned HTTPS destination, no redirect, no retry, no runtime proxy,
strict response media type/content encoding and bounded body handling. Compilation,
unit execution and configured-provider runtime tests remain pending.

Remaining risks:
The ID token is not yet cryptographically verified, so the private result must never be
used as a session. Reqwest/Rustls and operating-system networking may retain temporary
copies of request or TLS plaintext outside Recorder's explicit secret buffers. A
same-user process compromise can inspect process memory. Dependency MSRV, Windows trust
store behavior, provider response compatibility and timeout behavior require local and
configured-provider validation before the transport is enabled.
```
