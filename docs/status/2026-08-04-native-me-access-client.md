# Native `/me/access` client boundary — 2026-08-04

## Scope

This batch connects the verified native OIDC access session to the authoritative
SaaS `GET /api/v1/me/access` endpoint without exposing the bearer token or Supabase
subject to React.

The endpoint is pinned at compile time through `RECORDER_SAAS_ACCESS_ENDPOINT`.
Production accepts HTTPS only. Debug builds may use an explicit numeric-loopback
HTTP URL with a fixed port for local development. The path must be exactly
`/api/v1/me/access`; user information, query strings and fragments are rejected.

## Native transport boundary

The dedicated Reqwest client uses Rustls and enforces:

- no redirects;
- no retries;
- no runtime proxy discovery;
- no automatic response decompression;
- five-second connect and read deadlines;
- a ten-second total deadline;
- an exact final response URL;
- HTTP 200 only;
- JSON with an optional UTF-8 charset only;
- `Cache-Control: no-store`;
- identity content encoding only;
- a 32 KiB declared and actual body ceiling.

The command accepts no token, user ID, endpoint or entitlement input. It snapshots
the access token only through the native session boundary, marks the Authorization
header as sensitive and zeroes the temporary token/header buffers on drop.

## Response validation

The Rust parser rejects unknown or duplicate response fields through a strict Serde
struct. It additionally requires:

- `authenticated=true`;
- the response `userId` to equal the verified native session subject;
- a supported account, provider and subscription status;
- no more than 64 valid feature keys;
- unique feature keys;
- `fullAccess` to equal `accountStatus=active` plus the
  `desktop_full_access` entitlement.

React receives only normalized account/subscription status, entitlement count,
`fullAccess`, check/expiry timestamps and configuration booleans. It never receives
the subject UUID, entitlement names or token bytes.

## Cache and race behavior

A successful result is cached in native memory for at most five minutes. The cache
is bound to both the verified subject and a SHA-256 fingerprint of the current native
access token. Status reads clear the cache when:

- there is no active native session;
- the native access token changes;
- the verified subject changes;
- the five-minute deadline expires.

Every refresh clears the prior decision before network I/O. The result is committed
only while the current native session still matches the original subject and token
fingerprint, so logout or refresh during the request cannot publish a late access
decision.

## Command surface

Two no-argument commands are registered:

- `get_account_access_status` — reads only the short-lived native status;
- `refresh_account_access` — calls the pinned endpoint with the native token.

This batch does not add a React route gate or recording-command entitlement guard.
A `fullAccess=true` response is status only until those independent enforcement
boundaries are implemented.

## Tests added

Unit tests cover:

- absent, HTTPS and debug-loopback endpoint configuration;
- production HTTP rejection;
- exact path/query/fragment enforcement;
- strict subject-bound response parsing;
- duplicate entitlement rejection;
- full-access consistency rejection;
- JSON, size and no-store header requirements;
- proof that the serialized WebView status contains no subject, token or feature key.

## Security impact

- Data accessed: current native OIDC access token and verified subject; normalized
  `/api/v1/me/access` response.
- Data written: five-minute native in-memory access status only.
- Network communication: bounded GET to the compile-time-pinned access endpoint.
- New dependencies: None.
- New Tauri commands: `get_account_access_status`, `refresh_account_access`.
- New Tauri permissions: None.
- Persistent secrets: None.
- Payment behavior: None.
- Recording access behavior: None.
- Remaining risk: Windows compilation/configured-provider validation, real
  Supabase token/runtime evidence, AuthGate, native command enforcement, payment
  provisioning and offline lease remain pending.
