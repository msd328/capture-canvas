# SaaS authentication and upload-contract foundation — 2026-07-31

## Evidence carried forward

The user confirmed that the automatic Library refresh is working on Windows after the previous batch. `LIB-08` can therefore move to validated. Exact `LibraryHealth` log evidence was not supplied, so `HLT-25` remains implemented pending diagnostic evidence.

## Scope

This batch establishes the first provider-neutral SaaS boundary without enabling cloud traffic or handling a real account token.

### Native desktop boundary

- Added a Windows Credential Manager-backed secure-auth store.
- Added a non-secret status command that reports only support and whether a session credential exists.
- Added a readiness probe that writes random temporary bytes to a dedicated probe credential, reads them back, verifies equality, and attempts deletion before returning.
- Added a clear-session command that deletes only Recorder's fixed refresh-token credential target.
- Kept Credential Manager operations off the Tauri command thread through `spawn_blocking`.
- Serialized secure-store operations through one in-process mutex.
- Added a 2,560-byte maximum credential blob size.
- Added path-free and token-free `AuthHealth` diagnostics.
- Added no command that returns secret bytes to React.
- Added no command that accepts or persists a real refresh token yet.

### Shared SaaS contract

- Added strict Zod contracts for OIDC authorization callbacks and PKCE transactions.
- Added private, unlisted, and public visibility values.
- Added bounded MP4 upload-initiation metadata including recording UUID, size, duration, SHA-256, title, and visibility.
- Added multipart upload-session and ordered completion-part contracts.
- Added cloud-recording metadata states.
- Added a server-only configuration reader for API origin, OIDC issuer, client ID, and redirect URI.
- Missing configuration keeps SaaS disabled instead of guessing an endpoint.
- Non-local API and issuer URLs must use HTTPS.

### Settings readiness UI

- Added a Cloud account settings card.
- Added a secure-storage readiness button.
- Added local-session status and a clear-session action.
- The panel explicitly states that identity-provider and API endpoints are not configured.

## Network and CSP boundary

The production Tauri CSP still permits only local IPC communication. No external HTTPS origin was added. The desktop cannot make a cloud request until a real API origin is selected and deliberately added to the CSP.

## Expected Windows evidence

Run the local validation gate, start Recorder, open Settings, and select **Check secure storage**.

Expected startup/status output:

```text
[Recorder][AuthHealth] stage=status ok=true supported=true signed_in=false
```

Expected readiness output:

```text
[Recorder][AuthHealth] stage=probe ok=true supported=true round_trip=true
```

The Settings page should show `Windows Credential Manager ready` and `No cloud session is stored on this device.`

## Validation status

```text
LIB-08   ✅  Automatic Library refresh confirmed working by the user on Windows
SAAS-01  🟡  OIDC/PKCE, visibility and upload-session contracts implemented; provider and exchange runtime pending
SAAS-02  🟡  Windows secure-store status/probe/clear boundary implemented; Windows compile/probe and real login integration pending
SEC-12   🟡  Refresh-token storage boundary targets Windows Credential Manager and exposes no secret-return command; runtime validation pending
HLT-26   🟡  Path-free AuthHealth status/probe/clear diagnostics implemented; Windows evidence pending
```

## Security impact

```text
Security impact:
Added a native secret-storage boundary for future authentication and strict data contracts for future upload requests. Cloud access remains disabled and no real credential is accepted by the current command surface.

Data accessed:
A fixed Recorder-owned generic credential target in the current Windows user's Credential Manager, random probe bytes, existing non-secret settings UI state, and future upload metadata contract fields.

Data written:
A random temporary probe credential during an explicit readiness check. The probe is read back and deletion is attempted before the command returns. No real account credential is written by this batch.

Network communication added:
None. Production and development CSP rules were not expanded for a SaaS origin.

New permissions/capabilities:
Enabled the existing windows-rs `Win32_Security_Credentials` API projection. No new Tauri plugin or capability was added.

External processes:
None.

Untrusted inputs:
Future OIDC callback values, upload metadata, multipart part lists, environment configuration, and Credential Manager records. Zod contracts bound string lengths, UUIDs, URLs, sizes, duration, hashes, part counts and ordering. The native status/clear/probe commands accept no user-provided secret or credential target.

Validation added:
A random secure-store round-trip probe; a Rust credential-size unit test; strict SaaS schemas; HTTPS enforcement for non-local configured endpoints; disabled-by-default server configuration; and non-secret AuthHealth diagnostics.

Secrets involved:
Only random probe bytes in this batch. Future refresh tokens are designated for the fixed Windows Credential Manager target and must never be returned to React, JSON, localStorage, logs, or URLs.

Security tests completed:
Static review of fixed credential targets, secret-return prevention, probe cleanup ordering, Credential Manager buffer freeing, size limits, path-free logs, disabled network configuration and unchanged CSP.

Remaining risks:
Windows compilation and Credential Manager runtime behaviour remain unvalidated. A failed credential deletion can leave random probe bytes under the dedicated probe target. OIDC state/nonce/PKCE generation, callback interception, token exchange, refresh rotation, logout revocation, API-origin allowlisting, server-side authorisation, upload ownership checks, quotas and rate limits are not implemented. Credential Manager protects data within the Windows account boundary but does not protect against a process already running as the same user. macOS secure storage remains unsupported.
```
