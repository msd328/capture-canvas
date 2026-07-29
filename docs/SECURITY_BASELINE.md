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

- Tauri Content Security Policy is currently disabled.
- The asset protocol scope currently uses a wildcard.
- Recording output paths are not yet constrained to canonical approved roots.
- Library delete/open/thumbnail operations trust persisted metadata paths more than they should.
- Metadata and recordings are not encrypted at rest.
- Filesystem, shell, opener, and dialog plugins require a least-privilege capability audit.
- FFmpeg compatibility discovery can use an environment override or PATH lookup.
- Health logs can contain full local paths.
- Dependency vulnerability scanning, secret scanning, signing, updater verification, and penetration testing are not yet complete.

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

## Vulnerability handling

Do not publish exploitable vulnerability details before a fix is available. Security reports should include affected version, reproducible behaviour, impact, and suggested mitigation. A public security contact and disclosure policy must be added before the first public release.
