# Security hardening batch — 2026-07-29

## Roadmap movement

| ID | Previous | Current | Validation still required |
|---|---:|---:|---|
| SEC-02 | 🔵 | 🟡 | Windows dev build, packaged build, CSP violation checks |
| SEC-03 | 🔵 | 🟡 | Local MP4 playback and arbitrary local-file rejection |
| SEC-04 | 🔵 | 🟡 | Tampered metadata, outside-root, link/reparse, and race tests |
| SEC-05 | 🔵 | 🟡 | Tauri schema/runtime validation and command regression test |
| SEC-06 | 🔵 | 🟡 | Remaining crop/dimension/source validation and negative tests |

A code commit does not move these items to ✅. The evidence above is still required.

## Implementation commits

| Commit | Change |
|---|---|
| `a96773e` | Added canonical recording path and input security module |
| `b784f3a` | Removed unused Tauri runtime plugin initialisers |
| `64f230e` | Removed unused Tauri plugin dependencies |
| `fba3616` | Added recording-command boundary validation |
| `4c775ff` | Protected delete/open/rename against tampered metadata paths |
| `9a6e1ac` | Added settings validation before persistence |
| `613afc6` | Added startup and thumbnail-worker path validation |
| `86c3a8b` | Added explicit main-window capability |
| `15093a5` | Added production/dev CSP and recording-only asset scope |
| `16a6595` | Corrected mutable startup-library filtering |
| `eaa5edc` | Updated the security baseline |
| `aaf394b` | Updated the master implementation roadmap |
| `4442b76` | Kept security headers compatible with asset playback |
| `ee08d28` | Kept Windows canonical paths internal to Rust checks |
| `cbf6760` | Preserved normal player-compatible recording paths |
| `11e1a13` | Preserved normal paths in persisted metadata |
| `960e667` | Required canonical lowercase UUIDs |
| `9af0ad7` | Persisted trimmed, validated recording titles |

## Implemented controls

- Production and development CSP configurations.
- Asset protocol limited to direct MP4 files under `$HOME/Recordings`.
- Explicit main-window core capability.
- Removed filesystem, shell, dialog, and opener plugin initialisers and dependencies.
- Rejected WebView-supplied custom output paths.
- Required canonical lowercase UUID recording IDs.
- Bound completed filenames to `<UUID>.mp4`.
- Canonicalised existing recording paths for security comparison.
- Required files to be non-empty regular direct children of the approved root.
- Applied validation before startup loading, thumbnailing, opening, deleting, renaming, and library insertion.
- Added FPS, title, device-ID, and output-directory validation.
- Kept Windows extended canonical paths out of UI metadata and asset URLs.

## Validation checklist

1. `bun run desktop:dev` compiles without Rust or Tauri configuration errors.
2. Existing recordings remain visible and playable.
3. A new recording starts, stops, appears in the library, and plays.
4. Rename, Open file location, and Delete still work.
5. Browser developer tools show no unexpected CSP violation messages.
6. A manually tampered `recordings.json` path outside the Recordings root is rejected and removed from the library.
7. A copied unrelated MP4 inside the root cannot be accessed by changing only a recording's metadata path because the UUID filename binding fails.
8. A custom `outputPath` sent through the command boundary is rejected.
9. WGC `StreamHealth` diagnostics still appear after Stop.
10. A production `bun run desktop:build` is completed before SEC-02/03/05 can become ✅.

## Security impact

```text
Security impact:
Reduced WebView, local-file, command-input, plugin, and metadata-tampering attack surface.

Data accessed:
Local recorder settings, recordings.json, and UUID-named MP4 files in the approved
Recordings directory.

Data written:
Validated settings, filtered recording metadata, Tauri CSP/capability configuration,
and security tracking documents.

Network communication added:
None. Development CSP permits localhost Vite/HMR and Tauri IPC only.

New permissions/capabilities:
One main-window capability containing selected Tauri core defaults. No filesystem,
shell, dialog, or opener plugin permission.

External processes:
No new external process. Existing fixed explorer.exe and FFmpeg compatibility paths
remain.

Untrusted inputs:
Tauri command payloads, local JSON metadata, UUIDs, titles, device IDs, FPS values,
settings paths, and filesystem paths.

Validation added:
Canonical UUID and filename binding, canonical root comparison, regular/non-empty
file checks, custom-output rejection, title/device/FPS/settings constraints, and
validation before all current library file operations.

Secrets involved:
None.

Security tests completed:
Static review only. Windows compiler, runtime, CSP, playback, and negative-path tests
remain pending.

Remaining risks:
Filesystem race hardening, OS known-folder APIs, encryption at rest, FFmpeg executable
trust, complete log redaction, secret scanning, signing, updater verification, and
penetration testing.
```
