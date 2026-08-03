# Native OIDC exchange contract

Date: 2026-08-03  
Branch: `feature/windows-native-capture`

## Scope

This batch adds the provider-aligned native request and response contract required
before Recorder can exchange a Supabase OAuth authorization code. It does not make a
network request, consume the real callback grant, persist a token, create an account,
or unlock any paid feature.

## Implemented

- Supabase public-client authorization-code form fields:
  - `grant_type=authorization_code`
  - `code`
  - `client_id`
  - exact `redirect_uri`
  - `code_verifier`
- Supabase public-client refresh form fields:
  - `grant_type=refresh_token`
  - `refresh_token`
  - `client_id`
- `application/x-www-form-urlencoded` percent encoding without a client secret.
- 16 KiB maximum request body.
- 64 KiB maximum success response.
- Strict success response fields:
  - `access_token`
  - `token_type`
  - `expires_in`
  - `refresh_token`
  - `scope`
  - `id_token`
- Duplicate, unknown, missing and trailing response data rejection.
- `bearer` token type enforcement.
- Token and scope length limits.
- Maximum 24-hour response lifetime.
- JWT-shaped access and ID token checks before later cryptographic validation.
- Approved Supabase scopes only: `openid`, `email`, `profile`, and `phone`.
- `openid` is mandatory because Recorder requires an ID token and nonce validation.
- Native secret buffers use best-effort volatile clearing when dropped.
- Tauri readiness and self-contained probe commands expose booleans and limits only.
- Settings shows the exchange contract separately from secure storage and provider
  configuration.

## Supabase alignment

The default desktop scope set is now:

```text
openid email profile
```

`offline_access` was removed. Supabase OAuth currently documents `openid`, `email`,
`profile`, and `phone` as supported scopes, and its authorization-code response already
contains a refresh token for a public client.

## Security impact

| Field | Impact |
|---|---|
| New data handled | Synthetic probe tokens only; no real token is exchanged in this batch |
| New network destinations | None |
| New secrets | None |
| Secret persistence | None |
| WebView exposure | Readiness booleans, size ceilings and probe booleans only |
| Logs | Boolean stages/results only; no endpoint, code, verifier, token, nonce or user ID |
| Tauri permissions | None |
| CSP changes | None |
| Dependencies | None |
| Database changes | None |
| Payment changes | None |
| Paid-access changes | None |

## Validation included in code

Rust tests cover:

- bounded and percent-encoded authorization-code form generation;
- invalid PKCE verifier rejection;
- refresh form generation with no client secret;
- the documented Supabase token response shape;
- duplicate and unknown field rejection;
- unsupported scope rejection;
- oversized response rejection; and
- unsupported Supabase authorization scopes.

## Validation still required

Run:

```powershell
.\scripts\windows-local-check.ps1
```

A configured Supabase project is not required for the self-contained probe. A real
provider configuration is required later for transport and identity-validation tests.

## Deliberately absent

- Atomic callback code, state, nonce and PKCE verifier consumption.
- HTTPS token endpoint request.
- HTTP status, content-type, timeout and redirect enforcement.
- JWKS retrieval from the desktop.
- ID-token signature, issuer, audience, expiry and nonce validation.
- Access-token validation in the desktop.
- Refresh-token Credential Manager write or rotation.
- Logout, revocation or session refresh.
- `/api/v1/me/access` invocation from Recorder.
- AuthGate, paywall or native recorder entitlement enforcement.
