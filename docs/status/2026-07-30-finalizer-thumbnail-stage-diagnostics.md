# Native finalizer and thumbnail stage diagnostics — 2026-07-30

## Runtime evidence that triggered this batch

The Windows development build completed successfully and emitted healthy high-motion
capture diagnostics for two pause/resume segments:

```text
segment 1 timeline_coverage_pct=99.8 sample_density_pct=98.7 processing_deficit=0
segment 2 timeline_coverage_pct=99.7 sample_density_pct=99.4 processing_deficit=0
pause total_ms=820
resume total_ms=1334
stop total_ms=3974
```

The same run exposed two unresolved native-media failures:

```text
Native Windows pause/resume finalization unavailable ... falling back to FFmpeg concat
thumbnail_ok=false
```

It also exposed two harmless dead-field warnings after structured path redaction. The
branch already contains the warning fix before this diagnostic batch; a fresh build must
confirm those warnings are gone.

## Roadmap movement

| ID | Previous | Current | Validation still required |
|---|---:|---:|---|
| CTRL-08 | 🟡 | 🟡 | Pause/Resume rerun; native success or exact failure stage/HRESULT |
| CTRL-11 | 🟡 | 🟡 | Native open/decode/append/render timing output |
| LIB-06 | 🟡 | 🟡 | Single-segment native thumbnail success or exact failure stage |
| LIB-07 | 🟡 | 🟡 | Backfill generated/failed totals and per-item stages |
| HLT-19 | New | 🟡 | `FinalizerHealth` compilation and runtime fields |
| HLT-20 | New | 🟡 | `ThumbnailHealth` compilation and runtime fields |
| SEC-08 | 🟡 | 🟡 | Confirm new structured lines remain path-free |

A code commit does not move these items to ✅.

## Implementation commits

| Commit | Change |
|---|---|
| `6090651` | Added path-free MediaComposition destination/segment/open/decode/append/render diagnostics |
| `161f62d` | Added typed thumbnail extraction failure stages and bounded retry propagation |
| `9520869` | Added asynchronous `ThumbnailHealth` output and backfill totals |
| `43c50d9` | Updated the implementation roadmap and introduced HLT-19/HLT-20 |
| `0326b93` | Updated the security baseline and diagnostic privacy record |

## Native finalizer diagnostics

Expected begin and metadata lines:

```text
[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=begin ok=true segment_count=2
[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=segment_metadata ok=true segment_index=0 bytes=... elapsed_ms=...
[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=segment_metadata ok=true segment_index=1 bytes=... elapsed_ms=...
```

Destination and segment open retries identify whether failure occurs while starting or
waiting for `StorageFile::GetFileFromPathAsync`:

```text
stage=normalize_path
stage=create_destination
stage=open_file
stage=open_file_start
stage=open_file_wait
```

Each segment then reports:

```text
stage=decode_segment
stage=decode_segment_start
stage=decode_segment_wait
stage=append_segment
```

Rendering reports:

```text
stage=render_start
stage=render_wait
stage=render_result reason=...
stage=complete total_ms=...
```

Structured failures include only role, segment index, retry attempt, elapsed time and
HRESULT identifier. They do not include the segment or destination path.

## Thumbnail diagnostics

The thumbnail worker now reports one of these stages instead of only
`thumbnail_ok=false`:

```text
normalize_path
open_file_start
open_file_wait
request_thumbnail_start
request_thumbnail_wait
thumbnail_size
thumbnail_size_invalid
create_buffer
read_thumbnail_start
read_thumbnail_wait
read_length
read_length_empty
create_reader
read_bytes
library_entry_missing
persist
spawn_worker
complete
already_present
```

Example failure:

```text
[Recorder][ThumbnailHealth] ok=false id=<uuid> stage=request_thumbnail_wait attempt=3 code=HRESULT(...) thumbnail_ms=...
```

Example success:

```text
[Recorder][ThumbnailHealth] ok=true id=<uuid> stage=complete attempt=0 code=none thumbnail_ms=...
```

Backfill emits per-recording failures and a final generated/failed total.

## Validation checklist

1. Stop the current development watcher.
2. Pull `feature/windows-native-capture`.
3. Run `bun run desktop:dev`.
4. Confirm the two previous `output_path is never read` warnings are gone.
5. Confirm the recorder window opens normally.
6. Create a 10–15 second recording without Pause.
7. Wait at least two seconds after Stop for the background thumbnail worker.
8. Capture the complete `ThumbnailHealth` line.
9. Confirm the recording remains playable even if thumbnail generation fails.
10. Create a second recording, Pause after about ten seconds, Resume after three seconds,
    record another ten seconds and Stop.
11. Capture every `FinalizerHealth` line plus the fallback/success message and
    `ControlHealth operation=stop`.
12. When native finalisation succeeds, confirm there is no FFmpeg concat warning and no
    `[mp4] track ... frame size` warning.
13. When native finalisation fails, provide the first `FinalizerHealth ok=false` line and
    the final `stage=complete ok=false` line.
14. Search the output for local path leakage:

```powershell
Select-String -Path .\recorder-dev.log -Pattern 'path=|C:\\Users\\|/Users/'
```

15. Confirm no `FinalizerHealth` or `ThumbnailHealth` line contains a local path.

## Acceptance criteria

`HLT-19` can become ✅ when the Windows build emits valid stage lines and either:

```text
native finalizer complete ok=true
```

or a failure is identified with a stable stage, segment index/role and HRESULT.

`HLT-20` can become ✅ when the Windows build emits a valid success/failure stage for a
single-segment recording and backfill does not block application startup or Stop.

`CTRL-08` and `LIB-06` remain 🟡 until the native operations themselves succeed.
Observability success is not feature success.

## Static review limits

- The Windows compiler is authoritative for generated WinRT enum/error formatting and
  async-operation APIs.
- HRESULT identifiers are logged through their Debug representation and require runtime
  inspection.
- MediaComposition retries can add up to 850 ms only when a StorageFile open attempt is
  failing; successful first attempts add no retry sleep.
- Thumbnail retries remain asynchronous and can add up to 550 ms outside Stop.
- FFmpeg fallback remains active and still determines final Stop time after a native
  concatenation failure.
- This batch does not add the finalisation timeout or repair the failure before its stage
  is known.

## Security impact

```text
Security impact:
Added path-free failure-stage observability for native segment finalisation and native
thumbnail extraction.

Data accessed:
Segment file metadata and sizes, WinRT operation results, retry attempts, elapsed timing,
thumbnail byte length/content type, recording UUID and MediaComposition result.

Data written:
Existing MP4/thumbnail metadata plus local FinalizerHealth/ThumbnailHealth diagnostics
and repository tracking documents.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added. Existing FFmpeg concat fallback remains available after native failure.

Untrusted inputs:
Generated segment files, WinRT HRESULTs/results, thumbnail streams, media metadata,
recording UUIDs and worker outcomes.

Validation added:
Non-empty regular-segment checks, bounded StorageFile retries, stage/HRESULT capture,
render-reason reporting, thumbnail size limits, empty-read rejection, typed errors and
path-free worker logs.

Secrets involved:
None.

Security tests completed:
Static stage/data-flow and structured-log review only.

Remaining risks:
Windows compilation/runtime evidence is pending. The native failure causes remain unknown,
FFmpeg fallback still trusts its current discovery path, recordings remain unencrypted,
and native finalisation still lacks a hard timeout.
```
