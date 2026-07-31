# Windows formatting gate recovery — 2026-07-31

## Evidence

Running `scripts/windows-local-check.ps1` reached the frontend lint stage and reported 6,899 Prettier errors. Most failures were `Delete CR` diagnostics caused by a Windows CRLF checkout while the repository formatter expects LF. The same run also exposed ordinary Prettier wrapping changes in the affected TypeScript/TSX files.

## Implementation

- Added `.gitattributes` with explicit LF rules for source, configuration, documentation and Rust files.
- Kept PowerShell scripts as CRLF for normal Windows editing and execution.
- Marked common media and image formats as binary.
- Added `endOfLine: "lf"` to `.prettierrc`.
- Updated `scripts/windows-local-check.ps1` to inspect every Bun/Cargo exit code explicitly.
- Added short recovery guidance when frontend or Rust formatting fails.
- Preserved validation behaviour: the check does not silently auto-format source.

## Roadmap impact

```text
REL-14  🟡  Local Windows validation gate now reports native-command failures reliably; frontend normalization and a full green run remain pending.
SEC-09  🟡  Cross-platform source normalization added; first green Windows CI/local cycle remains pending.
```

## User recovery

After pulling this batch:

```powershell
bun run format
git diff --check
.\scripts\windows-local-check.ps1
```

Review the formatting-only diff before committing or discarding it. A later repository normalization commit should contain the formatted frontend files so clean Windows checkouts pass without local modifications.

## Security impact

```text
Security impact:
Reduced ambiguity in validation results and prevented platform-dependent text conversion from bypassing or flooding the formatting gate.

Data accessed:
Repository text files, Bun/Cargo process exit codes and formatter/linter output.

Data written:
Git attributes, Prettier configuration, local validation-script output and repository tracking documentation.

Network communication added:
None at Recorder runtime. The existing validation dependency-install step remains unchanged.

New permissions/capabilities:
None.

External processes:
No new Recorder runtime process. The validation script continues to run Bun, ESLint, Vite, Cargo, rustfmt and rustc/test processes.

Untrusted inputs:
Repository file contents, platform line endings and native tool exit codes/output.

Validation added:
Explicit LF policy, binary-file exclusions, explicit Prettier end-of-line policy, per-command exit-code enforcement and actionable failure guidance.

Secrets involved:
None.

Security tests completed:
Static review of Git attribute matching, formatter policy and PowerShell native-command failure propagation.

Remaining risks:
The current frontend files still require one Prettier normalization pass and a green local/hosted run. The validation script intentionally does not mutate source automatically.
```
