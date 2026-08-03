# Strict native OIDC ID-token claims — 2026-08-03

## Scope

This batch adds a dependency-neutral native parser and validator for the
standards-critical OpenID Connect ID-token payload claims required before Recorder can
create a verified local identity.

The implementation validates the payload against compile-time-pinned configuration and
the one-time authorization-request nonce. It does not verify the JWS signature. Its
result is therefore explicitly named `UnverifiedIdTokenClaims` and cannot establish a
session, persist a token, call `/api/v1/me/access`, or unlock Recorder.

## Implemented

- Added bounded compact-JWT payload parsing:
  - maximum 16 KiB token;
  - exactly three non-empty compact segments;
  - maximum 12 KiB encoded payload segment;
  - maximum 8 KiB decoded JSON payload;
  - unpadded Base64URL only; and
  - no whitespace or control characters in the compact token.
- Added typed Serde parsing for the critical claims:
  - `iss`;
  - `sub`;
  - `aud`;
  - `exp`;
  - `iat`;
  - `auth_time`;
  - `nonce`;
  - optional `nbf`; and
  - optional `azp`.
- Duplicate critical claims are rejected by Serde while unknown non-critical provider
  profile claims remain allowed for compatibility.
- Requires the exact compile-time-pinned issuer.
- Requires the public OAuth client ID in the ID-token audience.
- Supports a single audience or a bounded array of at most four unique audiences.
- Requires an exact matching `azp` when more than one audience is present.
- Requires a canonical lowercase hyphenated UUID subject.
- Compares the ID-token nonce with the native authorization nonce using a
  length-independent constant-time-style comparison loop.
- Applies a sixty-second clock-skew allowance.
- Requires positive `exp`, `iat`, and `auth_time` values.
- Requires expiration after issue time and limits the ID-token lifetime to one hour.
- Rejects expired tokens, future issue times, invalid authentication times, invalid
  `nbf`, and tokens not yet valid.
- Decoded payload bytes receive best-effort volatile clearing on drop.
- Adds path-, subject-, issuer-, client-, and nonce-free `AuthHealth` diagnostics.
- Adds unit coverage for:
  - a valid Supabase-shaped ID-token payload;
  - issuer, audience, subject, and nonce mismatch;
  - expiration, future issue time, and excessive lifetime;
  - multi-audience `azp` requirements;
  - duplicate critical claims;
  - malformed compact tokens; and
  - nonce value and length changes.

## Deliberately not implemented

- RS256 or ES256 signature verification.
- Promotion from `UnverifiedIdTokenClaims` to a verified identity.
- Connecting the private token transport, JWKS resolver, and claims validator into a
  live callback/session flow.
- Persisting or rotating the refresh token.
- Holding an authenticated access token session.
- Calling `/api/v1/me/access` from the desktop.
- React AuthGate, paid routes, native entitlement enforcement, or application unlock.

## Dependency and lockfile note

No new Rust crate was added. The implementation reuses existing Base64, Serde, Chrono,
and UUID dependencies.

The repository still lacks a committed `src-tauri/Cargo.lock` while the Windows local
validation script invokes Cargo with `--locked`. A clean-checkout validation cannot be
considered reproducible until Cargo generates the lockfile, the resolved dependencies
are reviewed, and the file is committed.

## Status

```text
SAAS-01  🟡 Strict issuer/client audience/subject/time/nonce claim validation is
             implemented, but the result remains unverified without a signature.
SEC-12   🟡 Parsed identity remains native-only, non-serializable and unpersisted.
HLT-30   🟡 Added typed ID-token claim diagnostics and negative unit coverage.
REL-14   🟡 The missing committed Cargo.lock remains a clean-checkout blocker.
```

## Security impact

```text
Security impact:
Added strict fail-closed validation for the critical ID-token payload fields while
preserving the rule that payload contents are untrusted until the JWS signature is
verified.

Data accessed:
The native ID token, expected authorization nonce, compile-time-pinned issuer, public
OAuth client ID, and current UTC time when the private function is called in a future
batch.

Data written:
Transient decoded native payload bytes and repository source/documentation only.

Network communication added:
None.

New permissions/capabilities:
None.

New dependencies:
None.

External processes:
None.

Untrusted inputs:
The compact ID token and every decoded payload claim. Token shape, encoding, sizes,
critical claim types, values, audience cardinality, time bounds, UUID form, and nonce
are validated before an unverified claims object is returned.

Validation added:
Unit tests for accepted claims and issuer, audience, authorized-party, subject, nonce,
expiration, issue-time, lifetime, duplicate-field and compact-token rejection.

Secrets involved:
The ID token and authorization nonce remain native-only. Neither value nor the subject,
issuer, audience, client ID, or decoded payload is logged or serialized. Decoded payload
bytes receive best-effort volatile clearing on drop.

Security tests completed:
Static visibility review confirms the claims validator and result are crate-private and
absent from the Tauri invoke handler. Static naming and call-path review confirms the
result is explicitly unverified and has no session, persistence, access-check, billing,
or entitlement caller. Compilation, rustfmt and unit execution remain pending.

Remaining risks:
Claim validation without signature verification does not authenticate the token. An
attacker can construct arbitrary claims, so the result must never be used as identity or
session state. The next crypto batch must verify the original compact JWS with the exact
JWKS-selected RS256/ES256 key before claims can be promoted. Provider compatibility,
clock behavior, UUID representation, and the clean-checkout lockfile path require
Windows and configured-provider validation.
```
