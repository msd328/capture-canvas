# Native paid recording-start gate

Date: 2026-08-04
Branch: `feature/windows-native-capture`
Status: 🟡 Implemented, Windows/configured-provider validation pending

## Scope

Recorder now enforces a backend-confirmed paid-access decision before starting a new recording.

The guard reads only the short-lived native `/api/v1/me/access` cache. It never accepts a browser redirect, local registration state, email address, payment-provider return value, persisted refresh-token presence, or React route state as proof of payment.

## Enforcement policy

Release builds require a fresh access result containing all of the following:

- configured pinned `/api/v1/me/access` endpoint;
- completed access check;
- authenticated native session;
- active account status;
- unexpired access-cache deadline;
- verified `desktop_full_access` entitlement expressed by `fullAccess=true`.

Missing, stale, disabled, unauthenticated, unpaid, or malformed access state rejects `start_recording` before the recording engine is invoked.

Debug builds remain usable for local recorder development when `RECORDER_PAID_ACCESS_REQUIRED` is absent or set to `0`/`false`. Setting it to `1`/`true` enables the same gate in debug builds. Release builds default to enforcement and reject any attempt to disable it through the build variable.

## Safe command boundary

The gate applies only to starting a new recording.

Pause, resume, stop, finalization, output publication, and cleanup remain available after a recording has started. This avoids trapping an active capture or losing output if the five-minute access cache expires, the network becomes unavailable, or the account changes while recording.

A future bounded renewal policy may decide whether resume should require a fresh entitlement, but stop/finalization must always remain available.

## Diagnostics

`AccessGateHealth` emits only:

- operation name;
- allow/deny result;
- whether enforcement is required;
- a typed denial code;
- non-secret booleans for checked/authenticated/full-access state.

It does not emit the user UUID, access token, refresh token, entitlement list, endpoint, subscription identifier, email, or payment-provider identifiers.

## Tests

Unit coverage includes:

- release enforcement enabled by default;
- release enforcement cannot be disabled by the build variable;
- debug opt-in and opt-out behavior;
- acceptance of fresh active full access;
- rejection of missing entitlement;
- rejection of expired access state;
- rejection of disabled accounts;
- proof that serialized access status contains neither identity nor entitlement names.

## Security impact

- Data accessed: non-secret cached account-access status derived from the verified native session.
- Data written: none.
- Network communication: none added by the gate; access refresh remains an explicit bounded native operation.
- New dependencies: none.
- New Tauri commands: none.
- New permissions: none.
- Secrets exposed to WebView: none.
- Paid access behavior: new recording starts are denied in enforced builds unless a fresh backend-confirmed `desktop_full_access` result exists.
- Recorder safety: pause/stop/finalization remain available regardless of entitlement-cache changes.

## Remaining work

- Run `scripts/windows-local-check.ps1` on Windows.
- Validate a configured Supabase access endpoint with active, unpaid, disabled, expired and cross-user accounts.
- Add the React AuthGate and payment-selection routes.
- Refresh access automatically after login, startup restoration, webhook-confirmed payment and bounded cache expiry.
- Extend native enforcement to other protected operations without blocking cleanup, logout, account management or safe recording finalization.
