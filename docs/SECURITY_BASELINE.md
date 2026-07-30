# Capture Canvas security baseline

This document defines the minimum security requirements for the desktop recorder and future SaaS. It is maintained alongside `docs/IMPLEMENTATION_ROADMAP.md`.

Security is evidence-based. No feature is considered production-ready merely because it compiles or works locally. Security-sensitive behaviour must have an identified trust boundary, least-privilege design, input validation, negative tests, and documented remaining risk.

No software can promise that attacks or data leakage are impossible. The project target is defence in depth, secure defaults, least privilege, auditable data flows, and prompt remediation.

## Current data inventory

### Sensitive user content

- Captured display and application-window frames.
- Microphone audio.
- System/desktop audio.
- Camera frames.
- Completed MP4 recordings and temporary recording segments.
- Hidden native-finalizer candidate MP4 files that may temporarily remain after an unsettled cancellation.
- Source preview images and generated video thumbnails.

### Local metadata

- `recordings.json`: recording IDs, titles, local file paths, timestamps, dimensions, duration, file size, and optional thumbnail data URLs.
- `settings.json`: selected device IDs, frame rate, output directory, startup preference, and camera-bubble preference.

### Current network behaviour

The inspected desktop command surface has no intentional upload, authentication, analytics, advertising, or crash-report transmission path. This statement must be reviewed whenever a networking dependency, remote URL, updater, authentication flow, telemetry system, or SaaS feature is introduced.

### Secrets

The current recorder has no application account token or API credential storage. Future secrets must never be stored in ordinary JSON, source control, logs, browser storage, or command-line arguments. Windows Credential Manager and macOS Keychain are the approved desktop secret stores.

## Trust boundaries

1. **Captured applications and desktop content** are untrusted data.
2. **The Tauri webview** is less trusted than the Rust backend.
3. **Every Tauri command argument** must be treated as attacker-controlled input.
4. **Local JSON metadata** may be edited or replaced outside the application.
5. **Filesystem paths** may contain traversal, links, reparse points, unexpected extensions, or targets outside approved folders.
6. **External executables** may be substituted through PATH, environment variables, or adjacent files.
7. **Device names and media metadata** may contain malformed or hostile strings.
8. **Future SaaS responses, share links, comments, and uploads** are untrusted network data.
9. **Dependencies and build actions** are supply-chain trust boundaries.
10. **Release signing and updater metadata** are security-critical distribution boundaries.

## Mandatory implementation rules

- Keep recording local by default. Network transfer requires an explicit user action and visible destination.
- Grant the webview and plugins only the permissions required for the active feature.
- Do not expose unrestricted filesystem, shell, process, or asset-protocol access.
- Canonicalise and allowlist every path before creating, opening, deleting, thumbnailing, uploading, or revealing it.
- Validate IDs, enums, dimensions, FPS, crop coordinates, text lengths, file extensions, and collection sizes in Rust.
- Never build shell command strings from user input. Use executable arguments and fixed program paths.
- Do not log captured content, authentication material, full tokens, or private URLs.
- Redact local usernames and directory paths from any diagnostic data that may leave the device.
- Keep temporary files in controlled directories, use unpredictable names, and clean them after success, failure, or crash recovery.
- Require TLS for every production network request and validate server identity normally; do not disable certificate checks.
- Enforce server-side authentication and authorisation for every SaaS object. Client-side checks are never sufficient.
- Store passwords only through a specialised password-hashing algorithm on the server; never store plaintext or reversible password encryption.
- Use short-lived access tokens, protected refresh-token rotation, revocation, rate limits, and session audit events for future authentication.
- Encrypt cloud-stored recordings and backups at rest, and encrypt traffic in transit.
- Sign production executables, installers, update bundles, and update metadata.
- Review licences and vulnerabilities for direct and transitive dependencies.

## Current implemented desktop controls

The following controls were implemented on 2026-07-29 and 2026-07-30 and remain validation-pending until the relevant Windows build and negative tests pass:

- Production CSP blocks unapproved script, connection, frame, form, object, image, and media origins.
- Development CSP allows only the local Tauri IPC endpoints and the localhost Vite/HMR server.
- Asset-protocol access is limited to direct `.mp4` children of `$HOME/Recordings` instead of a wildcard filesystem scope.
- The `main` window has an explicit core-only capability and no filesystem, shell, dialog, or opener plugin permissions.
- Unused filesystem, shell, dialog, and opener plugin initialisers and Cargo dependencies were removed.
- Custom recording output paths supplied by the WebView are rejected.
- Recording IDs must be canonical UUIDs and completed files must be named exactly `<UUID>.mp4`.
- Existing recording paths are canonicalised and must resolve to a non-empty regular file directly under the approved Recordings root before open, delete, rename validation, thumbnail generation, startup loading, or library persistence.
- FPS, device-ID length/content, title length/content, and output-directory settings receive Rust-side validation.
- Background thumbnail error logs no longer include the full recording path.
- Structured `StreamHealth`, `AvHealth`, `ControlHealth`, `ContinuityHealth`, `CameraHealth`, `AudioMixerHealth`, and capture-output warning lines omit local filesystem paths.
- `FinalizerHealth` reports only backend, stage, role, segment index, retry attempt, elapsed time, file size, HRESULT, timeout/cancellation state and render-reason identifiers.
- `ThumbnailHealth` reports only recording UUID, stage, retry attempt, elapsed time, generated/failed totals and HRESULT/status identifiers.
- Native MediaComposition renders into a hidden unique candidate rather than the final recording path, preventing a timed-out WinRT operation and FFmpeg fallback from writing the same destination concurrently.
- Native rendering uses a fixed 20-second deadline and two-second cancellation-settle grace; an unsettled candidate is not deleted while Windows may still own it.
- Startup cleanup scans only direct children of the approved Recordings root, recognises exact canonical UUID-based part/mixed/system/native-finalizer names, rejects links/non-regular files, applies a 24-hour age gate, limits each scan to 4,096 entries, and emits path-free aggregate `CleanupHealth` output.

## Required security impact block for every implementation batch

Every status update and roadmap commit must include:

```text
Security impact:
Data accessed:
Data written:
Network communication added:
New permissions/capabilities:
External processes:
Untrusted inputs:
Validation added:
Secrets involved:
Security tests completed:
Remaining risks:
```

Use `None` explicitly rather than omitting a field.

## Current known risks

- Development CSP and capabilities compile and run on Windows, but packaged production validation is still pending.
- Restricted asset playback has not yet been verified against WebView2 and existing local recordings.
- The canonical path checks narrow access to one app-approved root, but operating-system filesystem races between validation and use require further hardening for hostile same-user local processes.
- The approved root is currently derived from the process home environment and should later use the operating system known-folder API.
- Metadata and recordings are not encrypted at rest.
- Full command validation is not complete for every dimension, crop, source identifier, and collection size.
- FFmpeg compatibility discovery can use an environment override or PATH lookup.
- Structured recorder health lines are path-redacted, but free-form backend errors, future crash reports, support bundles, and exported diagnostics still require a complete privacy review.
- Finalizer and thumbnail HRESULT/stage logs require Windows runtime review to ensure platform-provided identifiers never contain unexpected user-controlled text.
- Startup orphan cleanup is implemented but still requires a Windows stale/recent/final-file negative test. It intentionally retains matching files younger than 24 hours and remains subject to same-user filesystem races between metadata inspection and deletion.
- The MediaComposition render phase is bounded, but WinRT file-open/clip-decode waits and FFmpeg fallback execution are not yet hard-bounded.
- Dependency vulnerability alerts, secret scanning, signing, updater verification, and penetration testing are not yet complete.

These risks are tracked by SEC IDs in the implementation roadmap and block public production release where applicable.

## Security verification layers

### Per commit

- Compiler and lint checks.
- Input-validation and negative-path tests for changed commands.
- Review of permissions, filesystem access, process spawning, logs, and network changes.
- Dependency and lockfile diff review.
- Roadmap and security-impact update.

### Before beta distribution

- Strict production CSP.
- Minimal Tauri capability files.
- Canonical filesystem allowlists.
- No unrestricted asset scope.
- No PATH-selected production executable.
- Dependency and licence reports.
- Signed installer and update path.
- Privacy review and data-retention policy.
- Static analysis and secret scanning.

### Before public SaaS release

- Threat-model review covering authentication, upload, sharing, billing, deletion, tenancy, and abuse.
- Server-side authorisation tests for every object type.
- Upload validation, size limits, malware/content processing isolation, and resumable-upload integrity checks.
- Encryption and key-management review.
- Rate limiting, audit logs, alerting, backup/restore testing, and incident-response runbook.
- Independent penetration test and remediation verification.

## Batch security record — 2026-07-29 security hardening

```text
Security impact:
Reduced WebView, local-file, plugin, and command-input attack surface.

Data accessed:
Local recording metadata, recording files under the approved Recordings directory,
and recorder settings.

Data written:
Tauri security configuration, capability policy, validated settings, and filtered
recording metadata.

Network communication added:
None. Development CSP permits only localhost Vite/HMR and Tauri IPC endpoints.

New permissions/capabilities:
Added a core-only capability for the main window. No plugin permission was added.

External processes:
No new process. Existing fixed explorer.exe launch remains for opening an approved
recording location. Existing FFmpeg compatibility paths remain.

Untrusted inputs:
Tauri command arguments, persisted recordings.json/settings.json values, UUIDs,
titles, device IDs, FPS values, and local filesystem paths.

Validation added:
Canonical UUID and filename binding, direct-child canonical path checks, regular-file
and non-empty checks, custom-output rejection, title/device/FPS/settings limits, and
validation before thumbnail/open/delete/startup-library operations.

Secrets involved:
None.

Security tests completed:
Static code/configuration review only. Windows compilation, CSP runtime, local video
playback, and tampered-metadata negative tests remain pending.

Remaining risks:
Filesystem race hardening, OS known-folder resolution, encryption at rest, FFmpeg
executable trust, complete log redaction, secret scanning, signing, and penetration
testing remain open.
```

## Batch security record — 2026-07-30 timeline and log privacy

```text
Security impact:
Removed local filesystem paths from current structured recorder health output and
separated variable-frame timeline continuity from constant-FPS sample density.

Data accessed:
Existing frame counts, frame timestamps, monotonic wall time, encoder results, audio
byte counts, and output-path values used only for warm-up suppression/internal file checks.

Data written:
Path-free local health diagnostics and repository tracking documentation.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added.

Untrusted inputs:
WGC timestamps, encoder submission counters, audio buffer sizes, and filesystem paths
already used by the recording pipeline.

Validation added:
Timestamp-span continuity percentage, separate sample-density percentage, path-free
warning formats, and suppression of the obsolete density-based continuity warning.

Secrets involved:
None.

Security tests completed:
Windows compilation and path-free structured StreamHealth/ContinuityHealth/ControlHealth
runtime output were observed. Free-form error and exported-report review remains pending.

Remaining risks:
Free-form backend errors may still contain local context. Future crash reporting,
support bundles, and exported diagnostic reports need explicit redaction. Recordings
remain unencrypted at rest.
```

## Batch security record — 2026-07-30 finalizer and thumbnail diagnostics

```text
Security impact:
Added path-free failure-stage observability for native segment finalisation and native
thumbnail extraction. No media data or filesystem path is added to diagnostics.

Data accessed:
Recording-segment metadata and sizes, WinRT operation results, retry attempts, elapsed
timing, thumbnail byte length/content type, recording UUID, and MediaComposition result.

Data written:
Existing final MP4/thumbnail metadata plus local FinalizerHealth and ThumbnailHealth
lines and repository tracking documents.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added. Existing FFmpeg concat fallback remains available after native failure.

Untrusted inputs:
Generated segment files, WinRT HRESULTs/results, thumbnail streams, media metadata,
recording UUIDs, and asynchronous worker outcomes.

Validation added:
Non-empty regular-segment checks, bounded StorageFile retries, per-stage HRESULT capture,
MediaComposition render-reason reporting, bounded thumbnail retries, thumbnail size limits,
empty-read rejection, typed failure propagation, and path-free worker logs.

Secrets involved:
None.

Security tests completed:
Static stage/data-flow and log-field review only. Windows compilation and runtime output
remain pending.

Remaining risks:
The exact native MediaComposition and thumbnail failure causes are not yet known. HRESULT
identifiers require runtime review. FFmpeg fallback still trusts current discovery rules.
Recordings remain unencrypted at rest.
```

## Batch security record — 2026-07-30 bounded native render

```text
Security impact:
Isolated native MediaComposition output from the final recording path and bounded the
render wait before requesting cancellation.

Data accessed:
Recording segment files, WinRT async status/error state, monotonic timing and the final
recording filename used to derive a hidden unique candidate name.

Data written:
A hidden native-finalizer candidate MP4, the final MP4 after successful publication, and
path-free FinalizerHealth timeout/cancellation diagnostics.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added. Existing FFmpeg concat fallback remains available after native failure or
cancellation.

Untrusted inputs:
Generated MP4 segments, WinRT status/HRESULT values, filesystem operation results and
MediaComposition render output.

Validation added:
A fixed 20-second render deadline, two-second cancellation-settle grace, isolated unique
candidate destination, terminal-state polling, bounded publication retries, deferred
cleanup reporting and no concurrent native/fallback writes to the final MP4.

Secrets involved:
None.

Security tests completed:
Static control-flow, destination-isolation, cleanup and structured-log review only.

Remaining risks:
Windows compilation/runtime validation remains required. An unsettled cancellation can leave
a hidden candidate containing captured content until the background startup cleanup reaches
the 24-hour age threshold. WinRT open/decode waits and FFmpeg fallback are not yet hard-bounded.
Recordings remain unencrypted at rest.
```

## Batch security record — 2026-07-30 stale artifact cleanup

```text
Security impact:
Added conservative deletion of stale recorder-owned temporary media under the approved
Recordings root.

Data accessed:
Direct-child filenames, file type, modification time and deletion results for entries in
the approved Recordings directory.

Data written:
No new persistent application data. Exact matching temporary files older than 24 hours
may be deleted. Path-free CleanupHealth diagnostics and repository documentation are written.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None.

Untrusted inputs:
Local directory entries, filenames, file metadata and timestamps.

Validation added:
Canonical lowercase UUID parsing, exact suffix parsing, regular-file/link rejection,
24-hour minimum age, 4,096-entry scan bound, final-recording exclusion, path-free aggregate
diagnostics and filename-parser unit tests.

Secrets involved:
None.

Security tests completed:
Static name-pattern, path-boundary, age-gate, link-rejection and logging review. Windows
filesystem negative testing remains pending.

Remaining risks:
A same-user process can race an entry between metadata inspection and deletion. Cleanup
does not recover playable segments or remove matching files younger than 24 hours.
Recordings remain unencrypted at rest.
```

## Vulnerability handling

Do not publish exploitable vulnerability details before a fix is available. Security reports should include affected version, reproducible behaviour, impact, and suggested mitigation. A public security contact and disclosure policy must be added before the first public release.
