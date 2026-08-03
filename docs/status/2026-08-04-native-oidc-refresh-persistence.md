# Native OIDC refresh-token persistence

Date: 2026-08-04
Branch: `feature/windows-native-capture`
Status: 🟡 Implemented, Windows validation pending

## Roadmap delta

- `SAAS-01` remains 🟡: the verified authorization-code completion path now persists the validated refresh token transactionally after signature and claim verification.
- `SEC-12` remains 🟡: access tokens stay native-memory-only; refresh tokens use Windows Credential Manager and are never returned to the WebView.
- `HLT-26` remains 🟡: refresh replacement, read-back, rollback and session-commit diagnostics are now on the live completion path.
- `HLT-30` remains 🟡: successful completion reports `refresh_token_persisted=true`; paid access remains false.
- `REL-14` remains 🟡 until the complete locked Windows validation gate passes on the new code.

## Implemented flow

```text
one-time callback/PKCE/nonce grant
→ bounded token exchange
→ JWKS signature verification
→ strict ID-token claim validation
→ native in-memory access session
→ transactional refresh-token replacement
→ read-back verification
→ durable-session status
```

The access session is installed before persistence. If credential replacement or read-back fails, the newly installed in-memory session is cleared. The refresh writer restores the previous credential, or removes the new credential when no prior value existed. A generation snapshot prevents a concurrent clear/logout operation from being undone by a later network completion.

## Security properties

- Refresh token is persisted only after ID-token signature and claims succeed.
- Refresh token is never serialized, logged, returned to React, or stored in Recorder JSON/localStorage.
- Access token remains native-memory-only.
- Session status exposes only active/expiry/native-only/persisted booleans.
- Subject and generation must still match when durable commit is marked complete.
- A failed durable commit clears the in-memory session.
- Paid access is not granted locally.

## Not implemented

- Refresh-token restoration after restart.
- Refresh-token rotation.
- Provider revocation/logout request.
- Native `/api/v1/me/access` call.
- Desktop AuthGate or entitlement enforcement.
- Offline entitlement lease.

## Security impact

Data accessed:
- Verified native identity, access token, refresh token and session generation.

Data written:
- Refresh token to the current Windows user's Credential Manager generic credential target.
- Non-secret repository documentation.

Network communication added:
- None beyond the already implemented bounded token/JWKS requests.

New permissions/capabilities:
- None.

External processes:
- None.

Secrets involved:
- Access token remains native-memory-only.
- Refresh token is persisted only through the transactional Windows credential writer.

Remaining risks:
- Durable credentials cannot yet restore or rotate a session.
- Logout does not yet revoke the provider token.
- Paid access and entitlement checks remain unimplemented in the desktop client.
