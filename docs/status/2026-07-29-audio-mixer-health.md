# Audio mixer health diagnostics — 2026-07-29

## Roadmap movement

| ID | Previous | Current | Validation still required |
|---|---:|---:|---|
| AUD-08 | 🔵 | 🟡 | Windows build and mixed microphone/system-audio recordings showing separate underrun counts |
| AUD-09 | 🔵 | 🟡 | Confirm bounded-queue drop counts and peak depths; an overflow may require a stress condition |
| HLT-12 | 🔵 | 🟡 | Confirm one `AudioMixerHealth` line per mixed-audio segment and warning behaviour |

A code commit does not move these items to ✅.

## Implementation commits

| Commit | Change |
|---|---|
| `8a8990e` | Added the shared mixer-health queue module |
| `16fb715` | Converted the queue into a drop-in `VecDeque`-shaped wrapper |
| `b482644` | Exposed mixer diagnostics from the local `windows_capture` facade |
| `6da841e` | Added backend-specific queue pairing, underrun/drop counters and health output |
| `9ccae7f` | Wired both recorder backends through module wrappers without rewriting their source files |
| `3ddce41` | Exposed the public marker bound used by the facade queue aliases |
| `d8496d8` | Suppressed wrapper-only unused-reexport lints |
| `f5419d9` | Updated the master implementation roadmap |

## Implemented measurements

For recordings that mix microphone and system audio, each source queue records:

- mixer ticks where fewer than 480 source frames were available and silence was substituted;
- frames evicted from the front after the existing 96,000-frame queue bound was exceeded;
- highest observed queue depth before overflow trimming;
- backend label and segment wall time.

The existing 48 kHz stereo resampling, 480-frame mixer chunk, 2,400-frame startup
prebuffer, 96,000-frame bound, microphone gain, system gain and ducking values are
unchanged.

The implementation compiles the existing full-source and selected-area backend files
through small wrapper modules. Only their local `VecDeque` name is replaced by the
instrumented queue; their mixer, capture, camera and encoder source files are unchanged.

## Expected output

```text
[Recorder][AudioMixerHealth] backend=native-wgc-d3d11 wall_ms=... microphone_underruns=... system_underruns=... microphone_dropped_frames=... system_dropped_frames=... microphone_peak_queue_frames=... system_peak_queue_frames=...
```

Selected-area or native-buffer fallback recordings use:

```text
backend=native-wgc-buffer
```

Possible warnings:

```text
[Recorder][AudioMixerHealth] warning=mixer_underruns microphone=... system=...
[Recorder][AudioMixerHealth] warning=queue_overflow_drops microphone_frames=... system_frames=...
```

## Validation checklist

1. `bun run desktop:dev` compiles the facade and recorder without new module-resolution, visibility or queue-method errors.
2. A recording with no audio produces no `AudioMixerHealth` line.
3. A microphone-only recording produces no `AudioMixerHealth` line.
4. A system-audio-only recording produces no `AudioMixerHealth` line.
5. A 30-second microphone + system-audio full-display recording produces one `AudioMixerHealth` line.
6. A 30-second microphone + system-audio selected-area recording produces one line with `backend=native-wgc-buffer`.
7. Pause/Resume produces one line for each mixed-audio segment.
8. Normal playback is checked for audible gaps and compared with the underrun counts.
9. Peak queue depths remain bounded and dropped-frame counts are normally zero.
10. A stress test or device/scheduler stall is used later to verify nonzero overflow-drop accounting.

## Interpretation limits

- An underrun means the mixer's 10 ms drain requested a complete chunk after mixing had already begun, but that source queue had fewer than 480 frames. It does not identify the driver or scheduler as the root cause.
- The intentional startup prebuffer is excluded from underrun counting.
- Queue-drop counts represent old frames discarded to preserve the existing newest-data policy at the two-second bound.
- The line is emitted only for the microphone + system-audio mixer. Single-source audio bypasses these queues.
- Zero counters do not prove decoded playback has no device latency or A/V synchronisation issue; `AvHealth` and playback tests remain separate evidence.

## Security impact

```text
Security impact:
Added passive local queue-depth and event counters. No captured audio samples are
logged, copied into diagnostics, retained, transmitted or inspected for content.

Data accessed:
Per-source queue lengths, source category (microphone/system), dropped-frame counts,
and monotonic segment time.

Data written:
Local stderr health lines and repository tracking documents.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added.

Untrusted inputs:
Queue sizes resulting from native audio callbacks and scheduler timing.

Validation added:
Saturating counters, retained queue bound, startup-prebuffer exclusion, separate
per-source accounting and warning output for nonzero underruns/drops.

Secrets involved:
None.

Security tests completed:
Static source/diff review only. Windows compilation and runtime validation remain pending.

Remaining risks:
The module-wrapper integration is not yet Windows-compiled. Diagnostic counters do
not include audio content, but other health logs still contain local output paths
under SEC-08. Recordings remain unencrypted at rest, and device-driver latency is
not measured by this batch.
```
