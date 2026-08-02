# Pinned OIDC token trust contract — 2026-08-02

## Scope

This batch adds the provider-neutral configuration boundary required before Recorder
can safely exchange an authorization code or trust a returned identity. It does not
perform token endpoint network requests, parse tokens, validate signatures, persist a
session, or derive a SaaS owner.

A real native browser flow now requires both the existing authorization configuration
and a complete compile-time token-validation contract.

## Build-time configuration

The following values are accepted only while compiling Recorder:

```text
RECORDER_OIDC_TOKEN_ENDPOINT
RECORDER_OIDC_ISSUER
RECORDER_OIDC_AUDIENCE
RECORDER_OIDC_JWKS_URI
```

They are not read from runtime environment variables, JSON, frontend settings,
localStorage, browser storage, command arguments, or server responses.

## Implemented validation

- Treats the four token-validation values as an all-or-none set.
- Leaves token exchange disabled when all four values are absent.
- Rejects partially configured builds.
- Requires HTTPS for the token endpoint, issuer, and JWKS URI.
- Requires a host and rejects username/password authority components.
- Rejects pre-seeded query strings and fragments.
- Limits each endpoint to 8 KiB.
- Requires a non-empty audience of at most 512 characters.
- Rejects audience whitespace and control characters.
- Returns only readiness booleans to the WebView.
- Emits only boolean readiness and typed failure codes in structured diagnostics.
- Requires the complete token contract before the native sign-in command can create a
  PKCE transaction, bind a loopback listener, or open the browser.

## WebView readiness contract

`get_oidc_client_status` now reports the combined authorization and token-validation
readiness:

```text
configured
authorizationEndpointHttps
callbackMode
scopeCount
tokenExchangeConfigured
tokenEndpointHttps
issuerHttps
audienceConfigured
jwksUriHttps
```

`configured=true` means both halves are present and valid. The endpoint, issuer,
audience, JWKS URI, client ID, scopes, state, nonce, challenge, verifier and code are
not returned.

## Diagnostics

Unconfigured builds can emit:

```text
[Recorder][AuthHealth] stage=oidc_token_config ok=true configured=false
```

Fully configured builds can emit:

```text
[Recorder][AuthHealth] stage=oidc_token_config ok=true configured=true token_endpoint_https=true issuer_https=true audience_configured=true jwks_uri_https=true
```

Invalid builds emit only a typed code, for example:

```text
[Recorder][AuthHealth] stage=oidc_token_config ok=false configured=false code=incomplete
```

No configured value is logged.

## Tests added

Rust tests cover:

- disabled-by-default behavior;
- partial configuration rejection;
- insecure token endpoint rejection;
- pre-seeded token query rejection;
- invalid audience rejection; and
- acceptance of a complete HTTPS token contract.

## Status

```text
SAAS-01  🟡 Authorization and token trust metadata can be pinned together; actual
             code exchange and ID-token verification remain pending.
HLT-30   🟡 Token-contract readiness and typed failure diagnostics are implemented;
             Windows build/runtime evidence remains pending.
SEC-12   🟡 Native sign-in is now gated on the future token trust contract; refresh
             token persistence/rotation is still absent.
SEC-13   ⚪ No token signature validation, verified subject, owner derivation,
             tenancy enforcement, object authorization, quota or rate limiting exists.
```

## Security impact

```text
Security impact:
Added a fail-closed compile-time trust contract for the future OIDC token exchange and
identity-validation stages.

Data accessed:
Compile-time token endpoint, issuer, API audience, and JWKS URI.

Data written:
Repository source and documentation only. No runtime file, database, credential,
recording metadata, WebView storage, or browser storage write.

Network communication added:
None. The token endpoint and JWKS URI are validated but never contacted by this batch.

New permissions/capabilities:
None.

New dependencies:
None.

External processes:
None beyond the browser behavior from the preceding native-loopback batch; that flow
is now more strictly gated.

Untrusted inputs:
Tauri sign-in command invocation from the WebView. Provider trust metadata itself is
compile-time pinned and is not accepted from the WebView.

Validation added:
All-or-none configuration, HTTPS and authority checks, query/fragment rejection,
endpoint and audience bounds, whitespace/control rejection, backend sign-in gate, and
unit tests.

Secrets involved:
None. These four values are public provider/client trust metadata. Existing PKCE
verifier, state, nonce and temporary authorization code remain native-only.

Security tests completed:
Static trust-boundary review and Rust unit-test implementation. Windows compilation
and configured-build runtime evidence remain pending.

Remaining risks:
Authorization-code exchange, response-size/content-type/time bounds, ID-token parsing,
JWKS retrieval/cache policy, algorithm allowlist, signature verification, exact issuer
and audience comparison, expiry/not-before/nonce validation, refresh-token rotation,
revocation, server-side owner authorization, tenancy, quota, rate limiting and audit
logging remain unimplemented.
```
