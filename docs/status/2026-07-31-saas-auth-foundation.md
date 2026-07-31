# SaaS authentication and upload-contract foundation — 2026-07-31

## Evidence carried forward

The user confirmed that the automatic Library refresh is working on Windows after the previous batch. `LIB-08` can therefore move to validated. Exact `LibraryHealth` log evidence was not supplied, so `HLT-25` remains implemented pending diagnostic evidence.

## Scope

This batch establishes the first provider-neutral SaaS and native-auth boundary without handling a real account token or accepting a video upload.

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

### Canonical SaaS contract and server boundary

- Consolidated the shared contract under `src/saas/contracts.ts`.
- Added strict Zod contracts for OIDC authorization callbacks and PKCE transactions.
- Added authenticated-user, private/unlisted/public visibility, cloud-recording, upload-initiation, resumable upload-session, capability and structured-error contracts.
- Bounded UUIDs, titles, MP4 content type, file size, duration, lowercase SHA-256, URLs, headers and protocol values.
- Added same-origin `/api/v1/health` and `/api/v1/capabilities` responses.
- Reserved `/api/v1/upload-sessions` fails closed with `503 not_configured`, then `501 not_implemented` after configuration until authenticated storage signing exists.
- Provider-neutral environment detection uses `RECORDER_AUTH_ISSUER`, `RECORDER_AUTH_AUDIENCE`, `RECORDER_UPLOAD_ORIGIN`, `RECORDER_UPLOAD_BUCKET`, and optional `RECORDER_MAX_UPLOAD_BYTES`.
- Issuer and upload origins count as configured only when they are valid HTTPS URLs.
- Capability responses expose booleans and limits, not provider URLs, audience, bucket names or credentials.

### Settings readiness UI

- Added a Cloud account settings card.
- Added a secure-storage readiness button.
- Added local-session status and a clear-session action.
- The panel explicitly states that identity-provider and API endpoints are not configured for the desktop client.

## Network and CSP boundary

The server runtime now has same-origin, read-only health and capability endpoints. No outbound network request was added. The production Tauri CSP still permits only local IPC communication, so the packaged desktop cannot call a cloud origin until a real HTTPS API origin is selected and deliberately added to CSP.

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
SAAS-01  🟡  OIDC/PKCE and capability contracts implemented; provider and exchange runtime pending
SAAS-02  🟡  Windows secure-store status/probe/clear boundary implemented; Windows compile/probe and real login integration pending
SAAS-03  🟡  Bounded resumable upload-session contract and fail-closed endpoint reserved; authenticated storage adapter pending
SAAS-06  🟡  Private/unlisted/public contract implemented; server-side authorisation pending
SAAS-08  🟡  Cloud recording metadata contract implemented; persistence and cloud library pending
SEC-12   🟡  Refresh-token storage boundary targets Windows Credential Manager and exposes no secret-return command; runtime validation pending
HLT-26   🟡  Path-free AuthHealth status/probe/clear diagnostics implemented; Windows evidence pending
```

## Security impact

```text
Security impact:
Added a native secret-storage boundary for future authentication, strict shared SaaS contracts, and fail-closed same-origin capability endpoints. No real credential is accepted by the current Tauri command surface, and upload creation does not yet accept media or issue a signed URL.

Data accessed:
A fixed Recorder-owned generic credential target in the current Windows user's Credential Manager, random probe bytes, non-secret settings UI state, deployment configuration used only to compute capabilities, request method/path, and future upload metadata contract fields.

Data written:
A random temporary probe credential during an explicit readiness check. The probe is read back and deletion is attempted before the command returns. No real account credential or cloud data is written by this batch.

Network communication added:
Two same-origin read-only server endpoints are available when the server runtime is deployed. No outbound request or desktop cloud origin was added.

New permissions/capabilities:
Enabled the existing windows-rs `Win32_Security_Credentials` API projection. No new Tauri plugin or capability was added.

External processes:
None.

Untrusted inputs:
Future OIDC callback values, upload metadata, environment configuration, Credential Manager records, and `/api/v1` request method/path. Zod contracts bound string lengths, UUIDs, URLs, sizes, duration, hashes and header values. Current upload-session handling consumes no request body. Native status/clear/probe commands accept no user-provided secret or credential target.

Validation added:
A random secure-store round-trip probe; a Rust credential-size unit test; strict SaaS schemas; HTTPS-only configuration detection; fail-closed capability/upload behavior; no-store JSON; `nosniff`; and non-secret AuthHealth diagnostics.

Secrets involved:
Only random probe bytes in the native batch. Future refresh tokens are designated for the fixed Windows Credential Manager target and must never be returned to React, JSON, localStorage, logs or URLs. Deployment configuration values are never returned by capability responses.

Security tests completed:
Static review of fixed credential targets, secret-return prevention, probe cleanup ordering, Credential Manager buffer freeing, size limits, path-free logs, environment-value non-disclosure, SSR route isolation, fail-closed uploads and unchanged desktop CSP.

Remaining risks:
Windows compilation and Credential Manager runtime behaviour remain unvalidated. A failed credential deletion can leave random probe bytes under the dedicated probe target. OIDC state/nonce/PKCE generation, callback interception, token exchange, signature/issuer/audience verification, refresh rotation, logout revocation, API-origin allowlisting, database tenancy, upload signing, object-key ownership, request-body enforcement, quotas and rate limits are not implemented. Credential Manager protects data within the Windows account boundary but does not protect against a process already running as the same user. macOS secure storage remains unsupported.
```
