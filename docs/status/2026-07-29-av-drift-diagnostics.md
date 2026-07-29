# Submitted A/V drift diagnostics — 2026-07-29

## Roadmap movement

| ID | Previous | Current | Validation still required |
|---|---:|---:|---|
| AUD-10 | 🔵 | 🟡 | Windows build, audio-enabled recordings, `AvHealth` logs, and playback comparison |
| HLT-13 | 🔵 | 🟡 | Confirm per-segment signed drift/startup-offset fields and warning thresholds |

`AUD-08`, `AUD-09`, and `HLT-12` remain 🔵. This batch does not claim to count
actual microphone/system mixer queue underruns or queue evictions.

A code commit does not move these items to ✅.

## Implementation commits

| Commit | Change |
|---|---|
| `9a95245` | Added the audio-settings and encoder-submission A/V diagnostic wrapper draft |
| `9adc100` | Separated health ownership from the consumed native encoder and removed the draft panic path |
| `e5e839f` | Updated the master implementation roadmap |

## Implemented measurements

For each audio-enabled recording segment, the facade records:

- accepted audio buffer count and PCM byte count;
- submitted PCM duration based on sample rate, channels, and bits per sample;
- first and last accepted video frame timestamps;
- submitted video timestamp span;
- signed `media_drift_ms = audio_duration_ms - video_duration_ms`;
- wall-clock `startup_offset_ms` between first accepted audio and video submissions;
- largest wall-clock gap between accepted audio buffers;
- encoder finalisation success.

The native encoder, audio samples, frame timestamps, capture cadence, and muxing
behaviour are delegated unchanged.

## Expected output

```text
[Recorder][AvHealth] finalize_ok=true audio_buffers=... audio_bytes=... audio_duration_ms=... video_duration_ms=... media_drift_ms=... startup_offset_ms=... max_audio_submit_gap_ms=... path=...
```

Possible warnings:

```text
[Recorder][AvHealth] warning=large_media_drift ...
[Recorder][AvHealth] warning=large_audio_submission_gap ...
```

The large-media-drift warning requires at least two seconds of submitted video and
an absolute submitted-duration difference greater than 250 ms. The audio-gap warning
uses a 250 ms threshold.

## Validation checklist

1. `bun run desktop:dev` compiles the local facade and recorder without new errors.
2. A no-audio recording produces no `AvHealth` line.
3. A microphone-only recording produces one `AvHealth` line per segment.
4. A system-audio-only recording produces one `AvHealth` line per segment.
5. A microphone + system-audio recording produces one `AvHealth` line per segment.
6. Pause/Resume produces separate segment health lines without a crash or duplicate finalisation.
7. A 20-second moving-content recording has a small signed drift and audible, synchronized playback.
8. A 60-second static-screen recording is used to compare `media_drift_ms` with timeline coverage.
9. `max_audio_submit_gap_ms` is reviewed against audible gaps in playback.
10. Warm-up clips do not emit `AvHealth` output.

## Interpretation limits

- `media_drift_ms` compares submitted PCM duration with the span between accepted
  video timestamps. It is an encoder-input diagnostic, not a complete measurement
  of decoded playback lip-sync.
- The video span excludes one final frame interval, so a healthy short recording can
  show a small positive audio lead of approximately one frame.
- `startup_offset_ms` is based on process monotonic time, not media timestamps.
- This batch does not measure actual mixer queue underruns, queue overflow evictions,
  device-driver latency, final mux edit lists, or player decoding latency.

## Security impact

```text
Security impact:
Added local timing and byte-count diagnostics. No captured media content is inspected
or retained by the diagnostic layer.

Data accessed:
Accepted video timestamps, PCM buffer lengths, audio format values, monotonic process
time, encoder result, and local output path for existing diagnostics.

Data written:
Local stderr health lines and repository tracking documents.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added.

Untrusted inputs:
Frame timestamps, PCM buffer lengths, and audio format values supplied through the
existing native capture pipeline.

Validation added:
Checked/saturating duration arithmetic, disabled-audio suppression, warm-up log
suppression, and warning thresholds for submitted drift and audio submission gaps.

Secrets involved:
None.

Security tests completed:
Static ownership and API review only. Windows compilation/runtime validation remains pending.

Remaining risks:
Health logs still contain full local output paths under SEC-08. Recordings remain
unencrypted at rest. This metric does not replace actual playback synchronization,
long-duration drift, mixer underrun, queue overflow, or device-disconnect tests.
```
