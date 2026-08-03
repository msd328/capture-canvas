# Coordinated native OIDC exchange handoff — 2026-08-03

## Scope

This batch connects the two native-only owners created by the browser authorization
flow:

- the loopback callback runtime, which retains the validated authorization code for at
  most 60 seconds; and
- `SecureAuthStore`, which retains the matching state, nonce and PKCE verifier for at
  most ten minutes.

The new handoff produces one private native exchange grant only when both owners are
present, unexpired and bound to the same state. It does not contact the token endpoint
or establish a signed-in session.

## Implemented

- Added a native-only `OidcExchangeMaterial` owner for the nonce and PKCE verifier.
- The nonce now uses the same best-effort volatile memory clearing as the verifier.
- Added a one-time, constant-time state-matched PKCE/nonce take operation.
- Added a coordinated exchange-grant take under the callback runtime lock.
- The callback authorization code remains retained until the matching PKCE/nonce take
  succeeds.
- After a successful match, the callback code and PKCE/nonce are removed from their
  shared stores before the combined grant is returned.
- The resulting native grant contains only:
  - the authorization code;
  - the PKCE verifier; and
  - the authorization-request nonce.
- A second coordinated take is rejected.
- Callback-code expiry clears the still-pending PKCE transaction.
- Missing, expired or mismatched PKCE state clears the callback grant and any remaining
  PKCE transaction.
- Added typed, secret-free failure codes:
  - `missing`;
  - `expired`;
  - `pkce_missing`;
  - `pkce_expired`; and
  - `state_mismatch`.
- Added Rust unit coverage for successful coordinated consumption, replay rejection,
  nonce/verifier retention and mismatched-state fail-closed cleanup.

## Deliberately not implemented

- HTTPS communication with the pinned token endpoint.
- Submission of the existing authorization-code form body.
- Token response reading or parsing during a live flow.
- ID-token signature, algorithm, issuer, audience, time or nonce verification.
- Native access-token session memory.
- Refresh-token write or rotation in Windows Credential Manager.
- Refresh, logout or provider revocation requests.
- Calling `/api/v1/me/access` from the desktop.
- React AuthGate, paywall routes or native recording-command entitlement enforcement.

The coordinated take is not registered as a Tauri command and none of its secret
fields implement `Serialize`. React and the WebView cannot request the code, verifier,
nonce or combined exchange grant.

## Status

```text
SAAS-01  🟡 Callback code, PKCE verifier and nonce now have a coordinated one-time
             native handoff. HTTPS exchange and ID-token validation remain.
SEC-12   🟡 Native authorization material is state-bound, replay-safe, expiry-bounded
             and best-effort zeroed. Refresh-token persistence remains absent.
HLT-27   🟡 Added typed PKCE-take success/failure diagnostics and coordinated tests.
HLT-29   🟡 Added combined callback/PKCE take diagnostics and clear-both failure paths.
```

## Security impact

```text
Security impact:
Reduced split-owner replay and mix-up risk by requiring the callback authorization code
and the pending PKCE/nonce transaction to match in constant time and be consumed as one
native exchange input.

Data accessed:
The validated native callback state and authorization code, plus the pending OIDC
state, nonce and PKCE verifier held by SecureAuthStore.

Data written:
In-memory native authentication lifecycle state and repository source/documentation
only.

Network communication added:
None.

New permissions/capabilities:
None.

New dependencies:
None.

External processes:
None.

Untrusted inputs:
No new input surface. The callback values have already passed the bounded loopback
HTTP, Host, path, parameter and state validation boundary.

Validation added:
Rust unit coverage for successful combined take, authorization-code/nonce/verifier
preservation, replay rejection and mismatched-state cleanup of both native owners.

Secrets involved:
Authorization code, state, nonce and PKCE verifier. They remain native-only, are never
serialized or logged, and receive best-effort volatile clearing when dropped.

Security tests completed:
Static visibility review confirmed that the combined exchange grant is internal Rust
state and is not registered in the Tauri invoke handler. The lock/order review confirms
that the callback grant remains retained until PKCE state validation succeeds and that
all typed failure paths clear both owners. Rust tests and Windows compilation remain
pending.

Remaining risks:
The combined grant will contain usable authorization material while the future HTTPS
exchange runs. The next batch must apply strict request deadlines, response-size and
content-type limits, reject redirects, validate the complete ID token including nonce,
and persist no refresh token until validation succeeds. A same-user process compromise
can still inspect Recorder memory.
```
