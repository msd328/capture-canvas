# Windows bounded-wait compile evidence — 2026-07-30

## Scope

This record captures the Windows build evidence supplied after bounded WinRT file-open, clip-decode and render waits were added.

Observed build result:

```text
recorder compiled successfully
recorder.exe started successfully
StorageFile open timeout macro expansion compiled
MediaClip decode timeout macro expansion compiled
MediaComposition render timeout macro expansion compiled
```

The build emitted three instances of the same `unused_assignments` warning from the macro-local `timeout_triggered = true` assignment. Every post-timeout outcome already carries `timeout_triggered: true` explicitly, so the assignment does not affect runtime state.

A Windows-only lint expectation is temporarily scoped to the `recording` module. It does not disable unrelated warnings elsewhere in the crate. The expectation must be removed when `wait_bounded_async!` is replaced with a typed helper or the redundant assignment is removed directly.

## Runtime evidence

Startup emitted:

```text
[Recorder][CleanupHealth] stage=complete ok=true scanned=2 matched=2 removed=0 recent=2 rejected=0 remove_failed=0 scan_limited=false min_age_hours=24 elapsed_ms=7
[Recorder][Health] native_media_warmup_ok=true warmup_ms=1663
```

This validates that:

- the application starts after the bounded-wait changes;
- startup orphan cleanup runs asynchronously;
- two matching files younger than 24 hours were preserved;
- no cleanup rejection or removal failure occurred;
- native media warm-up still completes.

It does not validate stale artifact deletion, an actual open/decode/render timeout, cancellation settling, candidate publication, or FFmpeg process termination.

## Local validation command

A repository script now mirrors the Windows CI commands:

```powershell
.\scripts\windows-local-check.ps1
```

It performs frozen Bun installation, frontend lint/build, Rust formatting, Rust tests and locked Windows cargo check.

## Status

```text
CTRL-13  🟡  Windows compilation validated; timeout runtime and FFmpeg bounds pending
HLT-21   🟡  Timeout/cancellation fields compile; forced-timeout evidence pending
HLT-23   🟡  Open/decode bounded-wait fields compile; runtime evidence pending
REL-10   🟡  Recent-file preservation observed; stale deletion matrix pending
REL-14   🟡  Local mirror script added; first green hosted CI run pending
```

## Security impact

```text
Security impact:
Added a local validation script and a narrowly scoped compiler lint expectation for a known macro-local redundant assignment. No capture or finalisation behaviour changed.

Data accessed:
Repository source, lockfiles, compiler/test output, and the already emitted aggregate CleanupHealth and warm-up timings.

Data written:
A PowerShell validation script, a module-level lint expectation, and repository status documentation.

Network communication added:
The local script may download locked Bun/Cargo dependencies when they are not already cached. No application runtime network path was added.

New permissions/capabilities:
None.

External processes:
The validation script invokes Bun, Vite, ESLint, Cargo, rustfmt, rustc and Rust tests. Recorder runtime process behaviour is unchanged.

Untrusted inputs:
Repository content, dependency metadata, compiler output, local directory metadata already inspected by startup cleanup, and test output.

Validation added:
One-command local parity with the Windows CI gate and explicit evidence tracking for bounded WinRT compilation and recent-artifact preservation.

Secrets involved:
None.

Security tests completed:
Static review of script commands, lint scope and supplied Windows startup output.

Remaining risks:
The lint expectation covers the recording module rather than only one macro expansion and must be removed after direct macro refactoring. Hosted CI has not reported green. Forced timeout/cancellation, stale-file deletion, reparse-point handling and FFmpeg process bounds remain unvalidated. Recordings remain unencrypted at rest.
```
