use crate::encoding;
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::Foundation::AsyncStatus;
use windows::Media::Editing::{MediaClip, MediaComposition};
use windows::Media::Transcoding::TranscodeFailureReason;
use windows::Storage::StorageFile;

const STORAGE_RETRY_DELAYS_MS: [u64; 4] = [0, 100, 250, 500];
const MOVE_RETRY_DELAYS_MS: [u64; 4] = [0, 50, 150, 300];
const RENDER_TIMEOUT: Duration = Duration::from_secs(20);
const CANCELLATION_GRACE: Duration = Duration::from_secs(2);
const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(25);

fn error_code(error: &windows::core::Error) -> String {
    format!("{:?}", error.code())
}

fn optional_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn log_stage(
    stage: &str,
    ok: bool,
    role: &str,
    segment_index: Option<usize>,
    attempt: Option<usize>,
    elapsed_ms: u128,
    code: Option<&str>,
) {
    eprintln!(
        "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage} ok={ok} role={role} segment_index={} attempt={} elapsed_ms={elapsed_ms} hresult={}",
        optional_usize(segment_index),
        optional_usize(attempt),
        code.unwrap_or("none"),
    );
}

fn storage_file(
    path: &Path,
    role: &'static str,
    segment_index: Option<usize>,
) -> Result<StorageFile> {
    let normalize_started = Instant::now();
    let path = match encoding::windows_storage_path(path) {
        Ok(path) => {
            log_stage(
                "normalize_path",
                true,
                role,
                segment_index,
                None,
                normalize_started.elapsed().as_millis(),
                None,
            );
            path
        }
        Err(_) => {
            log_stage(
                "normalize_path",
                false,
                role,
                segment_index,
                None,
                normalize_started.elapsed().as_millis(),
                None,
            );
            return Err(anyhow!(
                "Windows media path normalization failed role={role} segment_index={}",
                optional_usize(segment_index)
            ));
        }
    };

    let mut last_stage = "open_file_start";
    let mut last_code = "none".to_string();
    for (attempt_index, delay_ms) in STORAGE_RETRY_DELAYS_MS.into_iter().enumerate() {
        let attempt = attempt_index + 1;
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }

        let started = Instant::now();
        match StorageFile::GetFileFromPathAsync(&path) {
            Ok(operation) => match operation.get() {
                Ok(file) => {
                    log_stage(
                        "open_file",
                        true,
                        role,
                        segment_index,
                        Some(attempt),
                        started.elapsed().as_millis(),
                        None,
                    );
                    return Ok(file);
                }
                Err(error) => {
                    last_stage = "open_file_wait";
                    last_code = error_code(&error);
                    log_stage(
                        last_stage,
                        false,
                        role,
                        segment_index,
                        Some(attempt),
                        started.elapsed().as_millis(),
                        Some(&last_code),
                    );
                }
            },
            Err(error) => {
                last_stage = "open_file_start";
                last_code = error_code(&error);
                log_stage(
                    last_stage,
                    false,
                    role,
                    segment_index,
                    Some(attempt),
                    started.elapsed().as_millis(),
                    Some(&last_code),
                );
            }
        }
    }

    Err(anyhow!(
        "Windows media file open failed stage={last_stage} role={role} segment_index={} hresult={last_code}",
        optional_usize(segment_index)
    ))
}

fn native_candidate_path(final_path: &Path) -> PathBuf {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = final_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("recording");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    parent.join(format!(
        ".{stem}.native-finalizing-{}-{nonce}.mp4",
        std::process::id()
    ))
}

fn move_candidate_to_final(candidate: &Path, final_path: &Path) -> Result<()> {
    let started = Instant::now();
    let mut last_error = None;

    for (attempt_index, delay_ms) in MOVE_RETRY_DELAYS_MS.into_iter().enumerate() {
        let attempt = attempt_index + 1;
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }

        match fs::rename(candidate, final_path) {
            Ok(()) => {
                log_stage(
                    "publish_candidate",
                    true,
                    "destination",
                    None,
                    Some(attempt),
                    started.elapsed().as_millis(),
                    None,
                );
                return Ok(());
            }
            Err(error) => {
                last_error = Some(error);
                log_stage(
                    "publish_candidate",
                    false,
                    "destination",
                    None,
                    Some(attempt),
                    started.elapsed().as_millis(),
                    None,
                );
            }
        }
    }

    Err(anyhow!(
        "Windows native finalizer could not publish its completed candidate: {}",
        last_error
            .map(|error| error.kind().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ))
}

enum RenderOutcome {
    Completed(TranscodeFailureReason),
    Failed(anyhow::Error),
    UnsettledTimeout {
        cancel_requested: bool,
        last_status: String,
    },
}

/// Concatenate already-finalized MP4 recording segments with Windows' native
/// media-editing pipeline. MediaComposition owns the timeline so paused wall-clock
/// time is not present in the resulting file.
pub fn concatenate_segments(segments: &[PathBuf], final_path: &Path) -> Result<()> {
    let total_started = Instant::now();
    eprintln!(
        "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=begin ok=true segment_count={} render_timeout_ms={} cancellation_grace_ms={}",
        segments.len(),
        RENDER_TIMEOUT.as_millis(),
        CANCELLATION_GRACE.as_millis(),
    );

    if segments.len() < 2 {
        eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=validate_segments ok=false segment_count={}",
            segments.len()
        );
        return Err(anyhow!(
            "Native media concatenation requires at least two segments"
        ));
    }

    for (index, segment) in segments.iter().enumerate() {
        let started = Instant::now();
        match fs::metadata(segment) {
            Ok(metadata) if metadata.is_file() && metadata.len() > 0 => eprintln!(
                "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=segment_metadata ok=true segment_index={index} bytes={} elapsed_ms={}",
                metadata.len(),
                started.elapsed().as_millis()
            ),
            Ok(metadata) => {
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=segment_metadata ok=false segment_index={index} bytes={} elapsed_ms={}",
                    metadata.len(),
                    started.elapsed().as_millis()
                );
                return Err(anyhow!(
                    "Recording segment metadata was invalid segment_index={index}"
                ));
            }
            Err(_) => {
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=segment_metadata ok=false segment_index={index} elapsed_ms={}",
                    started.elapsed().as_millis()
                );
                return Err(anyhow!(
                    "Recording segment metadata was unavailable segment_index={index}"
                ));
            }
        }
    }

    if final_path.exists() {
        fs::remove_file(final_path).context("Unable to replace native finalizer destination")?;
    }

    let candidate_path = native_candidate_path(final_path);
    let create_started = Instant::now();
    fs::File::create(&candidate_path)
        .context("Unable to create isolated native finalizer candidate")?;
    log_stage(
        "create_candidate",
        true,
        "destination",
        None,
        None,
        create_started.elapsed().as_millis(),
        None,
    );

    let mut cleanup_deferred = false;
    let result = (|| -> Result<()> {
        let destination = storage_file(&candidate_path, "destination", None)?;

        let composition_started = Instant::now();
        let composition = MediaComposition::new().map_err(|error| {
            let code = error_code(&error);
            log_stage(
                "create_composition",
                false,
                "composition",
                None,
                None,
                composition_started.elapsed().as_millis(),
                Some(&code),
            );
            anyhow!("Unable to create Windows media composition hresult={code}")
        })?;
        log_stage(
            "create_composition",
            true,
            "composition",
            None,
            None,
            composition_started.elapsed().as_millis(),
            None,
        );

        let clips_started = Instant::now();
        let clips = composition.Clips().map_err(|error| {
            let code = error_code(&error);
            log_stage(
                "access_clips",
                false,
                "composition",
                None,
                None,
                clips_started.elapsed().as_millis(),
                Some(&code),
            );
            anyhow!("Unable to access Windows media composition clips hresult={code}")
        })?;
        log_stage(
            "access_clips",
            true,
            "composition",
            None,
            None,
            clips_started.elapsed().as_millis(),
            None,
        );

        for (index, segment) in segments.iter().enumerate() {
            let file = storage_file(segment, "segment", Some(index))?;

            let decode_started = Instant::now();
            let operation = MediaClip::CreateFromFileAsync(&file).map_err(|error| {
                let code = error_code(&error);
                log_stage(
                    "decode_segment_start",
                    false,
                    "segment",
                    Some(index),
                    None,
                    decode_started.elapsed().as_millis(),
                    Some(&code),
                );
                anyhow!(
                    "Unable to start decoding recording segment_index={index} hresult={code}"
                )
            })?;
            let clip = operation.get().map_err(|error| {
                let code = error_code(&error);
                log_stage(
                    "decode_segment_wait",
                    false,
                    "segment",
                    Some(index),
                    None,
                    decode_started.elapsed().as_millis(),
                    Some(&code),
                );
                anyhow!(
                    "Windows could not decode recording segment_index={index} hresult={code}"
                )
            })?;
            log_stage(
                "decode_segment",
                true,
                "segment",
                Some(index),
                None,
                decode_started.elapsed().as_millis(),
                None,
            );

            let append_started = Instant::now();
            clips.Append(&clip).map_err(|error| {
                let code = error_code(&error);
                log_stage(
                    "append_segment",
                    false,
                    "segment",
                    Some(index),
                    None,
                    append_started.elapsed().as_millis(),
                    Some(&code),
                );
                anyhow!(
                    "Unable to append recording segment_index={index} hresult={code}"
                )
            })?;
            log_stage(
                "append_segment",
                true,
                "segment",
                Some(index),
                None,
                append_started.elapsed().as_millis(),
                None,
            );
        }

        let render_started = Instant::now();
        let render = composition.RenderToFileAsync(&destination).map_err(|error| {
            let code = error_code(&error);
            log_stage(
                "render_start",
                false,
                "destination",
                None,
                None,
                render_started.elapsed().as_millis(),
                Some(&code),
            );
            anyhow!("Unable to start native Windows recording finalization hresult={code}")
        })?;
        log_stage(
            "render_start",
            true,
            "destination",
            None,
            None,
            render_started.elapsed().as_millis(),
            None,
        );

        let mut timeout_triggered = false;
        let outcome = 'render_wait: loop {
            let status = match render.Status() {
                Ok(status) => status,
                Err(error) => {
                    let code = error_code(&error);
                    log_stage(
                        "render_status",
                        false,
                        "destination",
                        None,
                        None,
                        render_started.elapsed().as_millis(),
                        Some(&code),
                    );
                    break 'render_wait RenderOutcome::UnsettledTimeout {
                        cancel_requested: false,
                        last_status: format!("status_error:{code}"),
                    };
                }
            };

            if status == AsyncStatus::Completed {
                let reason = match render.GetResults() {
                    Ok(reason) => reason,
                    Err(error) => {
                        let code = error_code(&error);
                        break 'render_wait RenderOutcome::Failed(anyhow!(
                            "Windows media finalization result failed hresult={code}"
                        ));
                    }
                };
                break 'render_wait RenderOutcome::Completed(reason);
            }
            if status == AsyncStatus::Canceled {
                break 'render_wait RenderOutcome::Failed(anyhow!(
                    "Windows media finalization was canceled"
                ));
            }
            if status == AsyncStatus::Error {
                let code = render
                    .ErrorCode()
                    .map(|value| format!("{value:?}"))
                    .unwrap_or_else(|_| "unknown".to_string());
                break 'render_wait RenderOutcome::Failed(anyhow!(
                    "Windows media finalization entered error state hresult={code}"
                ));
            }

            if render_started.elapsed() >= RENDER_TIMEOUT {
                timeout_triggered = true;
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=render_timeout ok=false role=destination elapsed_ms={} timeout_ms={}",
                    render_started.elapsed().as_millis(),
                    RENDER_TIMEOUT.as_millis()
                );

                let cancel_started = Instant::now();
                let cancel_result = render.Cancel();
                let cancel_requested = cancel_result.is_ok();
                let cancel_code = cancel_result.err().map(|error| error_code(&error));
                log_stage(
                    "cancel_render",
                    cancel_requested,
                    "destination",
                    None,
                    None,
                    cancel_started.elapsed().as_millis(),
                    cancel_code.as_deref(),
                );

                loop {
                    let status = match render.Status() {
                        Ok(status) => status,
                        Err(error) => {
                            let code = error_code(&error);
                            break 'render_wait RenderOutcome::UnsettledTimeout {
                                cancel_requested,
                                last_status: format!("status_error:{code}"),
                            };
                        }
                    };

                    if status == AsyncStatus::Completed {
                        let reason = match render.GetResults() {
                            Ok(reason) => reason,
                            Err(error) => {
                                let code = error_code(&error);
                                break 'render_wait RenderOutcome::Failed(anyhow!(
                                    "Windows media finalization result failed after timeout hresult={code}"
                                ));
                            }
                        };
                        eprintln!(
                            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=cancel_settle ok=true role=destination terminal_status=completed elapsed_ms={}",
                            cancel_started.elapsed().as_millis()
                        );
                        break 'render_wait RenderOutcome::Completed(reason);
                    }
                    if status == AsyncStatus::Canceled {
                        eprintln!(
                            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=cancel_settle ok=true role=destination terminal_status=canceled elapsed_ms={}",
                            cancel_started.elapsed().as_millis()
                        );
                        break 'render_wait RenderOutcome::Failed(anyhow!(
                            "Windows media finalization timed out and was canceled"
                        ));
                    }
                    if status == AsyncStatus::Error {
                        let code = render
                            .ErrorCode()
                            .map(|value| format!("{value:?}"))
                            .unwrap_or_else(|_| "unknown".to_string());
                        eprintln!(
                            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=cancel_settle ok=true role=destination terminal_status=error elapsed_ms={} hresult={code}",
                            cancel_started.elapsed().as_millis()
                        );
                        break 'render_wait RenderOutcome::Failed(anyhow!(
                            "Windows media finalization timed out then entered error state hresult={code}"
                        ));
                    }
                    if cancel_started.elapsed() >= CANCELLATION_GRACE {
                        eprintln!(
                            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=cancel_settle ok=false role=destination terminal_status=started elapsed_ms={} grace_ms={} cleanup_deferred=true",
                            cancel_started.elapsed().as_millis(),
                            CANCELLATION_GRACE.as_millis()
                        );
                        break 'render_wait RenderOutcome::UnsettledTimeout {
                            cancel_requested,
                            last_status: "started".to_string(),
                        };
                    }

                    std::thread::sleep(STATUS_POLL_INTERVAL);
                }
            }

            std::thread::sleep(STATUS_POLL_INTERVAL);
        };

        let _ = render.Close();
        drop(render);
        drop(destination);

        match outcome {
            RenderOutcome::Completed(reason) if reason == TranscodeFailureReason::None => {
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=render_result ok=true role=destination elapsed_ms={} reason={reason:?} timeout_triggered={timeout_triggered}",
                    render_started.elapsed().as_millis()
                );
                move_candidate_to_final(&candidate_path, final_path)?;
                Ok(())
            }
            RenderOutcome::Completed(reason) => {
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=render_result ok=false role=destination elapsed_ms={} reason={reason:?} timeout_triggered={timeout_triggered}",
                    render_started.elapsed().as_millis()
                );
                Err(anyhow!(
                    "Windows media finalization rejected recording segments reason={reason:?}"
                ))
            }
            RenderOutcome::Failed(error) => Err(error),
            RenderOutcome::UnsettledTimeout {
                cancel_requested,
                last_status,
            } => {
                cleanup_deferred = true;
                Err(anyhow!(
                    "Windows media finalization exceeded the bounded deadline cancel_requested={cancel_requested} last_status={last_status} candidate_cleanup_deferred=true"
                ))
            }
        }
    })();

    if let Err(error) = result {
        if !cleanup_deferred {
            let _ = fs::remove_file(&candidate_path);
        }
        eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=complete ok=false segment_count={} total_ms={} cleanup_deferred={cleanup_deferred}",
            segments.len(),
            total_started.elapsed().as_millis()
        );
        return Err(error);
    }

    for segment in segments {
        let _ = fs::remove_file(segment);
    }

    eprintln!(
        "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=complete ok=true segment_count={} total_ms={} cleanup_deferred=false",
        segments.len(),
        total_started.elapsed().as_millis()
    );
    Ok(())
}
