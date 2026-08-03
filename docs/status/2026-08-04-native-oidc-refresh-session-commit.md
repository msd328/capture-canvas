# Native OIDC refresh/session commit boundary

Date: 2026-08-04
Status: Implemented; Windows compilation and configured-provider validation pending
Roadmap: SAAS-01, SAAS-02, SEC-12, HLT-26, HLT-30, REL-14

## Delivered

The verified OIDC completion path now commits the durable refresh credential and the native in-memory access session as one lock-protected operation.

The order is:

1. consume the one-time callback code, PKCE verifier and nonce;
2. perform the bounded authorization-code exchange;
3. verify the ID-token signature and strict identity claims;
4. fully prepare and validate the native access-session candidate;
5. acquire the native session transaction lock and confirm its clear/logout generation is unchanged;
6. transactionally write and read back the refresh token in Windows Credential Manager;
7. assign the already-valid access-session candidate with `refresh_token_persisted=true`;
8. return only the non-secret session status.

No fallible session-validation or subject-matching step remains after the credential write succeeds.

## Failure behavior

- An invalid or expired session candidate fails before any credential write.
- A clear/logout request completed during token or JWKS network work changes the session generation; completion then fails before attempting the refresh write.
- A credential read, write or read-back failure uses the existing replacement primitive to restore the previous credential or remove the newly written credential.
- While the credential replacement and final memory assignment run, clear/status calls wait on the native session transaction lock.
- After completion releases the lock, a waiting clear operation removes both the in-memory session and the Windows credential.
- A failed credential operation never installs the candidate session.

## Residual limitation

Process termination between the successful Windows credential write and the immediately following in-memory assignment cannot be rolled back by an in-process lock. On restart this can leave a durable refresh credential without an in-memory access session. Startup refresh restoration and reconciliation remain required before durable sign-in is considered complete.

## WebView boundary

The completion command still accepts no token, subject, nonce, state, verifier, provider URL or user identifier from React.

The WebView receives only:

- whether a verified native session is active;
- its expiry timestamp;
- confirmation that the access token remains native-only;
- whether the refresh credential was persisted.

It does not receive access, refresh or ID-token bytes, the verified subject UUID, JWKS data or entitlement state.

## Paid-access boundary

This batch does not:

- call `/api/v1/me/access`;
- create or activate an entitlement;
- trust a browser redirect as payment evidence;
- unlock recording commands;
- implement the AuthGate or paywall.

A verified provider session and persisted refresh credential are authentication state only.

## Tests

The native session tests cover:

- candidate validation before commit;
- access-session expiry at the shorter OAuth/ID-token deadline;
- native-only token and subject handling;
- non-secret serialized status;
- refresh-persistence status on committed sessions;
- clear/logout generation invalidation;
- explicit clearing and missing-session behavior.

The existing refresh-store tests continue to cover successful replacement, read-back mismatch rollback, initial write failure and credential-size/text limits.

## Security impact

Data read:
- one verified native identity;
- one bounded access token;
- one bounded refresh token;
- the previous Windows refresh credential when present.

Data written:
- one Windows Credential Manager refresh credential;
- one short-lived native in-memory access session;
- repository source and documentation.

Network added:
- None beyond the existing bounded token and JWKS requests.

Persistent secrets added:
- one refresh token in the current user's Windows Credential Manager after full exchange and ID-token verification.

Tauri commands or capabilities added:
- None in this batch; the no-argument completion and non-secret status commands already existed.

Logs:
- typed stages, result booleans, replacement/read-back/rollback state and numeric Windows errors only;
- no token, subject, nonce, issuer, audience, key or credential value.

Dependencies:
- None.

Permissions:
- None.

## Validation required

```powershell
cd C:\Users\DINESH\Desktop\VidRec\capture-canvas

git pull --rebase --autostash origin feature/windows-native-capture

.\scripts\windows-local-check.ps1
```

Then validate a configured Supabase provider flow and confirm:

- the first completion succeeds once;
- a second completion cannot replay the consumed grant;
- `refreshTokenPersisted` is true only after credential read-back succeeds;
- Clear removes both native session status and the Credential Manager entry;
- cancellation during the network phase prevents a late session/credential commit;
- no paid access is granted without a successful `/api/v1/me/access` response.
