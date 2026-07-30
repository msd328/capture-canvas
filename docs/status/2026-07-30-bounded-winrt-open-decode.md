# Bounded WinRT open and decode waits — 2026-07-30

## Scope

This batch extends the existing bounded MediaComposition render policy to the
WinRT operations that open destination/segment files and decode each segment into
a `MediaClip`.

Implemented limits:

```text
StorageFile open timeout:       3 seconds per started operation
MediaClip decode timeout:       8 seconds per segment
Open/decode cancellation grace: 1 second
Status polling interval:        25 milliseconds
MediaComposition render timeout: unchanged at 20 seconds
Render cancellation grace:      unchanged at 2 seconds
```

The existing bounded retry delays for starting/opening a `StorageFile` remain:

```text
0 ms, 100 ms, 250 ms, 500 ms
```

A started open operation that reaches its timeout is not retried. It is cancelled
and receives the one-second settle grace first. Immediate start/result failures
may continue through the existing bounded retry sequence.

## Runtime behavior

The shared async wait helper polls the WinRT operation through:

```text
Status
GetResults
ErrorCode
Cancel
Close
```

Terminal results are classified as:

```text
completed
canceled
error
result_error
unsettled timeout
```

When an operation reaches its deadline:

1. A path-free timeout line is emitted.
2. `Cancel()` is requested.
3. The operation is polled for one additional second.
4. A completed result is accepted even when completion occurs during cancellation settling.
5. A canceled/error result fails the native finalizer cleanly.
6. An operation that remains started is reported as unsettled and the native path returns to the existing FFmpeg fallback.

If the destination `StorageFile` open remains unsettled, candidate cleanup is
deferred because Windows may still own the candidate. Startup orphan cleanup will
remove the candidate after its 24-hour safety threshold. Unsettled segment-open or
decode operations read segment files only; the fallback may continue to read those
segments, and no native writer has started.

## New diagnostics

The `FinalizerHealth` begin line now includes:

```text
open_timeout_ms=3000
decode_timeout_ms=8000
render_timeout_ms=20000
operation_cancellation_grace_ms=1000
render_cancellation_grace_ms=2000
```

Open/decode timeout paths may emit:

```text
stage=open_file_timeout
stage=open_file_cancel
stage=open_file_cancel_settle
stage=decode_segment_timeout
stage=decode_segment_cancel
stage=decode_segment_cancel_settle
terminal_status=completed|canceled|error|started
operation_unsettled=true
```

Successful open/decode lines include `timeout_triggered=true|false`, allowing a
late completion during cancellation settling to be distinguished from a normal
completion.

No filesystem path, filename, device name or media content is added to these
structured lines.

## Validation matrix

### Build

```powershell
cd C:\Users\DINESH\Desktop\VidRec\capture-canvas
git pull origin feature/windows-native-capture
bun run desktop:dev
```

The build must complete without errors involving:

```text
AsyncStatus
Status
GetResults
ErrorCode
Cancel
Close
wait_bounded_async
AsyncOperationOutcome
```

### Normal Pause/Resume

```text
Record 10 seconds
Pause
Wait 3 seconds
Resume
Record 10 seconds
Stop
```

Expected normal-path values:

```text
open_timeout_ms=3000
decode_timeout_ms=8000
stage=open_file ok=true timeout_triggered=false
stage=decode_segment ok=true timeout_triggered=false
stage=render_result ok=true timeout_triggered=false
stage=complete ok=true cleanup_deferred=false
```

The final MP4 must play, exclude paused wall-clock time and leave no hidden native
candidate after successful publication.

### Failure-path interpretation

A real open/decode timeout is not expected during normal local recording. When it
occurs, collect every `FinalizerHealth` line plus the FFmpeg fallback outcome.
The Stop command must not wait indefinitely in the bounded WinRT stage.

## Roadmap status

```text
CTRL-13  🟡  Open, decode and render waits are bounded; FFmpeg execution remains unbounded.
HLT-23   🟡  Open/decode timeout, cancellation and settle diagnostics implemented.
CTRL-11  🟡  Native phase timing improved; explicit FFmpeg phase timing remains.
```

## Security impact

```text
Security impact:
Reduced indefinite native-finalizer waits by bounding WinRT file-open and segment-decode operations and requesting cancellation before fallback.

Data accessed:
WinRT operation status and HRESULT values, generated segment files, the isolated native candidate, retry attempts and monotonic timing.

Data written:
Existing candidate/final MP4 files plus path-free FinalizerHealth timeout, cancellation and settle diagnostics.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added. The existing FFmpeg emergency concat fallback remains available.

Untrusted inputs:
Generated segment containers, filesystem operation results, WinRT async status/HRESULT values and decoded media metadata.

Validation added:
Three-second StorageFile deadline, eight-second MediaClip deadline, one-second cancellation settle grace, terminal-state classification, no retry after a started operation times out, and deferred candidate cleanup when destination opening remains unsettled.

Secrets involved:
None.

Security tests completed:
Static async-control-flow, destination-ownership and structured-log review only. Windows compilation and runtime validation remain pending.

Remaining risks:
FFmpeg fallback execution and FFmpeg availability probing are not hard-bounded. An unsettled destination-open operation can retain a hidden candidate until startup cleanup reaches the 24-hour threshold. Recordings remain unencrypted at rest.
```
