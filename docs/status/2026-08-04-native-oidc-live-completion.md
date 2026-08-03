# Native OIDC live completion flow

Date: 2026-08-04
Status: Implemented; Windows compilation and configured-provider validation pending
Roadmap: SAAS-01, SAAS-02, SEC-12, HLT-30, REL-14

## Delivered

Recorder now exposes a no-argument native completion path for the browser/loopback OIDC flow.

After the loopback listener reports `codeReceived`, Settings requests one native completion attempt. The command accepts no authorization code, verifier, nonce, token, subject or provider metadata from the WebView. Native code then:

1. consumes the callback authorization code, PKCE verifier and nonce exactly once;
2. performs the bounded pinned token-endpoint request;
3. parses the strict bounded token response;
4. resolves the exact JWKS key with the existing bounded cache/rotation behavior;
5. verifies the original RS256/ES256 compact-JWS signing input;
6. validates issuer, client audience/authorized party, canonical UUID subject, timestamps and nonce;
7. installs the access token and verified subject in the native in-memory session only.

A separate no-argument status command returns only:

- whether the native session is active;
- its expiry timestamp;
- that the access token is native-only;
- that the refresh token is not persisted.

Settings now distinguishes:

- Windows Credential Manager readiness and durable-credential presence;
- a transient verified native identity session;
- paid access, which is explicitly not checked or granted by this flow.

The clear-session command cancels pending callback state, clears the in-memory access session and then clears the secure-store credential target.

## Replay and failure behavior

- Settings requests completion once per browser-flow generation.
- The native callback grant remains single-use and has a 60-second lifetime.
- Concurrent or repeated completion calls cannot obtain a second grant.
- A failed exchange or verification never creates a new native session.
- A successful identity session does not activate `desktop_full_access`.
- Browser redirects and WebView state cannot grant paid access.

## Deliberately still blocked

- refresh-token persistence in Windows Credential Manager;
- refresh-token rotation and replacement rollback;
- startup restoration of a native session;
- provider revocation and logout network calls;
- authenticated `GET /api/v1/me/access` from the desktop;
- React AuthGate/paywall routing;
- native recorder-command entitlement enforcement;
- any payment-provider checkout or webhook activation.

## Tests and validation

Native unit coverage now also confirms that serialized session status contains neither the verified subject UUID nor access-token bytes.

The pushed lockfile commit `ad208e5` adds the direct root `ring` dependency edge without changing its resolved package version, source or checksum.

Run on Windows:

```powershell
cd C:\Users\DINESH\Desktop\VidRec\capture-canvas

git pull --rebase --autostash origin feature/windows-native-capture
.\scripts\windows-local-check.ps1
```

Configured-provider validation additionally requires a build with the pinned Supabase OIDC values and evidence for successful login, key resolution, signature verification, nonce validation, in-memory expiry and one-time replay rejection.

## Security impact

Data read:
- one native-only authorization code, PKCE verifier and nonce;
- one bounded provider token response;
- the pinned issuer, audience and JWKS configuration;
- one validated JWKS public key.

Data written:
- one short-lived native in-memory access-token session;
- non-secret React status state;
- repository source and documentation.

Network added:
- the previously implemented bounded token-endpoint and JWKS requests are now reachable through the no-argument completion command after a valid callback.

Persistent secrets added:
- None. The refresh token remains unpersisted and is zeroed when the exchange object is dropped.

Tauri commands or capabilities added:
- `complete_oidc_sign_in`, with no arguments and a non-secret status response;
- `get_oidc_session_status`, with no arguments and a non-secret status response;
- no new Tauri capability or plugin permission.

Logs:
- typed stages, booleans and failure codes only;
- no authorization code, verifier, nonce, access token, refresh token, ID token, subject, issuer, audience, key ID or key material.

Remaining security risk:
- the session is not durable and cannot refresh after expiry;
- real-provider behavior and Windows compilation are not yet evidenced;
- paid-access authorization remains unimplemented on the desktop and therefore no protected feature should be unlocked from this identity session alone.
