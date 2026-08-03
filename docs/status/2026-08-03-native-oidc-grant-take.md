# One-time native OIDC grant take — 2026-08-03

## Scope

This batch advances the native OAuth authorization-code boundary without starting a
network exchange or creating a signed-in session.

The numeric-loopback callback runtime already validated the callback request, exact
Host/port/path, unique state, code/error exclusivity, callback deadline and invalid
attempt limit. It retained the successful authorization code only in native memory for
at most 60 seconds.

This batch adds the next internal lifecycle primitive: the retained callback grant can
be taken exactly once by the future native token-exchange coordinator.

## Implemented

- Added a distinct `grantTaken` callback stage.
- Added a native-only authorization-grant object containing:
  - the already validated callback state; and
  - the authorization code.
- Added an internal one-time take operation.
- The first take removes the grant from shared callback state before returning it.
- A second take fails with `missing` rather than replaying the code.
- An expired grant is cleared and fails with `expired`.
- The existing expiry worker cannot clear or replace a grant after it has been taken.
- State and authorization-code buffers continue to receive best-effort volatile
  zeroing when their native owner is dropped.
- Added a unit test covering successful take, byte preservation, replay rejection,
  `grantTaken` status and removal of the public `codeReceived` flag.
- Added path-free and secret-free `AuthHealth` diagnostics for future integration.

## Deliberately not implemented

- Taking the pending PKCE verifier and nonce from `SecureAuthStore` in the same
  coordinator operation.
- Sending an HTTPS token request.
- Reading a provider response.
- Verifying an ID-token signature, issuer, audience, expiry or nonce.
- Holding an access token in native session memory.
- Writing or rotating a refresh token in Windows Credential Manager.
- Calling `/api/v1/me/access` from the desktop.
- Unlocking any recorder route or native command.

The internal take function is not registered as a Tauri command. A compromised WebView
cannot request the authorization code, state, verifier or nonce through this batch.

## Status

```text
SAAS-01  🟡 One-time native callback grant take implemented; PKCE handoff,
             network exchange and ID-token validation remain.
SEC-12   🟡 Authorization code remains native-only, expires in 60 seconds and is now
             replay-safe at the callback-runtime handoff boundary.
HLT-29   🟡 Added grant-take success/missing/expired diagnostics; configured Windows
             runtime validation remains pending.
```

## Security impact

```text
Security impact:
Reduced authorization-code replay risk inside the desktop process by making the
callback grant removable exactly once and introducing an explicit post-consumption
state.

Data accessed:
The native callback runtime's already validated state and authorization code.

Data written:
In-memory callback lifecycle state and repository source/documentation only.

Network communication added:
None.

New permissions/capabilities:
None.

New dependencies:
None.

External processes:
None.

Untrusted inputs:
No new input surface. The grant contains callback values accepted by the existing
bounded loopback HTTP/state validator.

Validation added:
Rust unit coverage for first-take success, byte preservation, second-take rejection and
non-secret lifecycle status.

Secrets involved:
OIDC state and authorization code. Both remain native-only and are best-effort zeroed
on drop. No token, verifier, nonce or provider value is logged or returned to the
WebView.

Security tests completed:
Static API review confirmed that the take operation is internal to the private
`oidc_loopback::runtime` module and is not registered in the Tauri invoke handler.
Windows compilation and configured-provider runtime tests remain pending.

Remaining risks:
The callback code and the pending PKCE verifier/nonce are still held by separate native
state owners. The next batch must coordinate both one-time takes, build the bounded
public-client request, validate the complete token response and fail closed before any
session persistence. A same-user process compromise can still inspect Recorder memory.
```
