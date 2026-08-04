# Native refresh startup reconciliation

Date: 2026-08-04
Status: implemented, Windows validation pending

## Scope

Recorder now inspects the dedicated Windows Credential Manager refresh-token target during startup and whenever native session status is requested. The inspection publishes only non-secret booleans. It never returns, logs, serializes, or automatically exchanges the stored refresh token.

## Session status contract

The native status now distinguishes:

- `active`: a verified in-memory access session exists;
- `refreshCredentialPresent`: Recorder's dedicated durable refresh credential exists;
- `restorationRequired`: a usable durable credential exists but no verified in-memory session exists;
- `reconciliationComplete`: the dedicated credential target was inspected successfully.

A persisted credential alone never sets `active=true`, never grants paid access, and never unlocks recording or cloud features.

## Startup behavior

A background startup task inspects only Recorder's dedicated credential target. It performs no provider, token-endpoint, JWKS, SaaS API, upload, analytics, or payment request.

Automatic refresh remains deliberately disabled until refresh-token response verification, rotation, cancellation, and rollback are implemented together.

## Local clear/logout behavior

`clear_secure_auth_session` now attempts all of the following:

1. cancel the loopback callback;
2. clear the in-memory access session and increment its cancellation generation;
3. remove the dedicated persisted refresh credential;
4. clear the legacy secure-auth credential target.

The command reports success only when both durable credential clears succeed. A partial failure is returned to the UI instead of claiming that the local session was cleared.

## Security impact

- Data read: presence and bounded bytes of Recorder's dedicated refresh credential.
- Data returned to the WebView: booleans and access-session expiry only.
- Data written: no new persistent data.
- Data deleted: dedicated refresh credential and legacy secure-auth record during explicit local clear.
- Network communication added: none.
- New dependency: none.
- New Tauri permission: none.
- Token exposure: none.
- Paid access change: none.

## Remaining work

- bounded refresh-token exchange;
- signature and claim validation for refresh results;
- atomic refresh-token rotation and in-memory session replacement;
- startup restoration after successful refresh;
- provider revocation/logout;
- `/api/v1/me/access` and paid-entitlement enforcement;
- Windows compilation and Credential Manager runtime evidence.
