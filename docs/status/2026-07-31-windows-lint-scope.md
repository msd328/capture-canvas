# Windows lint-scope correction — 2026-07-31

## Evidence

The Windows local validation gate correctly stopped during `bun run lint`.

The first run reported 6,899 Prettier errors because tracked frontend files had Windows CRLF line endings. After frontend and Rust formatting, the second run narrowed the failure to:

- 40 CRLF errors in `eslint.config.js`, which was not included in the earlier narrow Prettier command;
- four generated `__global-api-script.js` files under `src-tauri/target`, which are Rust/Tauri build output and must not be linted;
- seven non-blocking React/Fast Refresh warnings.

Frontend production build, Rust tests and Windows `cargo check` had already succeeded in the earlier run. The lint failure therefore reflects repository formatting/scope drift rather than a Recorder runtime or compiler failure.

## Changes

- Added explicit ESLint ignores for generated frontend, deployment, dependency and Rust target output.
- Added matching Prettier ignores for `.wrangler` and Rust target directories.
- Normalised `eslint.config.js` to repository LF formatting.
- Updated the Windows recovery hint to include `eslint.config.js` while avoiding generated output.

## Validation status

```text
REL-14  🟡  Windows validation gate correctly rejects formatting drift; rerun pending after lint-scope correction
```

The seven warnings remain visible and do not currently fail the gate. The camera cleanup-ref warning should be handled separately because it may indicate a real lifecycle issue; UI-only Fast Refresh warnings can be cleaned without blocking the recorder reliability path.

## Security impact

```text
Security impact:
Reduced validation noise by preventing generated compiler output containing local build paths from entering frontend lint diagnostics. Recorder runtime behaviour did not change.

Data accessed:
Repository lint configuration, generated local build-output paths shown in user-supplied validation logs, and validation command exit status.

Data written:
ESLint/Prettier ignore policy, a corrected recovery hint, and repository tracking documentation.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
No new runtime process. Existing Bun/ESLint/Prettier validation processes remain.

Untrusted inputs:
Repository files and generated build output encountered by local lint discovery.

Validation added:
Generated Rust target output is excluded from ESLint and Prettier, root lint configuration is covered by the recovery command, and native-command failures continue to stop the PowerShell validation script.

Secrets involved:
None.

Security tests completed:
Static lint-scope and path-exposure review. Windows lint rerun remains pending.

Remaining risks:
Seven frontend warnings remain. The camera effect cleanup-ref warning may represent a stale-reference lifecycle issue and should be reviewed separately. Hosted Windows CI still lacks a successful run.
```
