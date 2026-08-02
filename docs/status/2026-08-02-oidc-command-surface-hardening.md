# OIDC command-surface hardening — 2026-08-02

## Scope

This follow-up closes legacy WebView command surfaces that were useful while the OIDC
client was only a readiness prototype but are no longer required after native browser
launch and loopback callback interception were implemented.

## Removed from the Tauri/WebView contract

- `prepare_oidc_authorization`
- `prepare_oidc_transaction`
- `get_oidc_transaction_status`
- `prepareOidcAuthorization`
- `prepareOidcTransaction`
- `getOidcTransactionStatus`
- Frontend types that carried authorization URLs, state, nonce, challenge, or raw
  transaction status.

## Result

Only native Rust code can now construct a real authorization URL or start a real PKCE
transaction. The WebView retains only:

- non-secret secure-storage status and probe;
- non-secret provider-configuration status;
- native sign-in start;
- non-secret callback lifecycle status;
- cancellation/clear operations; and
- a self-contained PKCE security probe that does not mutate the real pending flow.

The native sign-in command returns only launch state, callback mode, and expiry. It
does not return the provider URL, state, nonce, PKCE challenge, verifier, authorization
code, provider error text, access token, or refresh token.

## Status

```text
SAAS-01  🟡 Native sign-in boundary hardened; provider exchange/validation pending.
SEC-12   🟡 Native-only PKCE and temporary-code boundary strengthened.
SEC-13   ⚪ Server token verification, ownership and tenancy remain unimplemented.
```

## Security impact

```text
Security impact:
Reduced the WebView authentication command surface and prevented a compromised
frontend from replacing a real native PKCE transaction through raw preparation
commands.

Data accessed:
None beyond existing non-secret readiness and callback-status data.

Data written:
Repository source and status documentation only.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None.

Untrusted inputs:
Tauri command invocation from the WebView.

Validation added:
Repository search confirmed no remaining frontend or registered-command reference to
the removed raw preparation/status operations.

Secrets involved:
No new secret. Authorization URL protocol values and authorization codes remain native.

Security tests completed:
Static command-registration, TypeScript service/type, and repository-reference audit.
Windows compilation remains pending.

Remaining risks:
The WebView can still request native sign-in start and cancellation as intended. Token
exchange, token validation, refresh rotation, owner authorization and rate limiting
remain unimplemented.
```
