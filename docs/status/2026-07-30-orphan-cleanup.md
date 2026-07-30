# 2026-07-30 — Stale recorder artifact cleanup

## Scope

This batch implements the first `REL-10` startup cleanup layer for recorder-owned temporary media.

## Roadmap movement

| ID | Previous | New | Evidence required |
|---|---:|---:|---|
| REL-10 | 🔵 | 🟡 | Windows compile plus stale/recent/final-file negative test |
| HLT-22 | New | 🟡 | `CleanupHealth` startup output on Windows |

The status remains 🟡 because repository changes alone do not prove filesystem behaviour on Windows.

## Implementation

Startup launches a named background worker that scans only direct children of the canonical approved Recordings directory. The scanner recognises only these exact recorder-owned temporary forms:

```text
<uuid>.partNNN.mp4
<uuid>.partNNN.mixed.mp4
<uuid>.systemNNN.wav
.<uuid>.native-finalizing-<process-id>-<nonce>.mp4
```

A candidate is removable only when all of the following hold:

1. The UUID is canonical lowercase text.
2. The temporary suffix exactly matches one of the supported forms.
3. The entry is a regular file and not a symbolic link.
4. Its modification time is at least 24 hours old.
5. It is a direct directory entry returned from the approved Recordings root.

The final `<uuid>.mp4` form never matches. The scanner processes at most 4,096 entries per startup and reports whether the scan was truncated.

## Health output

Expected startup output:

```text
[Recorder][CleanupHealth] stage=complete ok=true scanned=... matched=... removed=... recent=... rejected=... remove_failed=... scan_limited=false min_age_hours=24 elapsed_ms=...
```

Failure output contains only stage and error-kind identifiers, never filenames or paths.

## Windows validation

### Build

```powershell
cd C:\Users\DINESH\Desktop\VidRec\capture-canvas
git pull origin feature/windows-native-capture
bun run desktop:dev
```

Confirm that the application opens and emits one `CleanupHealth` line.

### Safe cleanup matrix

Stop Recorder before creating fixtures:

```powershell
$root = Join-Path $env:USERPROFILE "Recordings"
$id = "123e4567-e89b-12d3-a456-426614174000"
$old = (Get-Date).AddHours(-25)

$stale = @(
  "$id.part000.mp4",
  "$id.part001.mixed.mp4",
  "$id.system000.wav",
  ".$id.native-finalizing-1234-987654321.mp4"
)

$recent = "$id.part999.mp4"
$final = "$id.mp4"
$unrelated = "notes.tmp"

foreach ($name in $stale + @($recent, $final, $unrelated)) {
  Set-Content -Path (Join-Path $root $name) -Value "cleanup-test"
}
foreach ($name in $stale) {
  (Get-Item (Join-Path $root $name)).LastWriteTime = $old
}
```

Start Recorder again. Expected:

- all four `$stale` files are removed;
- `$recent` remains because it is less than 24 hours old;
- `$final` remains because final recordings are never candidates;
- `$unrelated` remains because its name is not recorder-owned;
- `removed=4`, `recent=1`, and `remove_failed=0` are reported.

Remove the remaining fixtures afterward:

```powershell
Remove-Item (Join-Path $root $recent), (Join-Path $root $final), (Join-Path $root $unrelated) -ErrorAction SilentlyContinue
```

## Security impact

```text
Security impact:
Added conservative deletion of stale recorder-owned temporary media under the approved Recordings root.

Data accessed:
Direct-child filenames, file type, modification time and delete results for entries in the approved Recordings directory.

Data written:
No new persistent application data. Matching stale temporary files may be deleted. Path-free CleanupHealth diagnostics and repository documentation are written.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None.

Untrusted inputs:
Local directory entries, filenames, file metadata and timestamps.

Validation added:
Canonical lowercase UUID parsing, exact suffix parsing, regular-file/link rejection, 24-hour minimum age, 4,096-entry scan bound, final-recording exclusion, path-free aggregate diagnostics and filename-parser unit tests.

Secrets involved:
None.

Security tests completed:
Static name-pattern, path-boundary, age-gate, link-rejection and logging review. Windows filesystem negative testing remains pending.

Remaining risks:
A same-user process can race a directory entry between validation and deletion. Cleanup does not yet recover playable segments, identify a live process across machines/sessions, or remove temporary files younger than 24 hours. Recordings remain unencrypted at rest.
```
