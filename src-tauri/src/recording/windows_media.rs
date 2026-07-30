use crate::encoding;
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::Media::Editing::{MediaClip, MediaComposition};
use windows::Media::Transcoding::TranscodeFailureReason;
use windows::Storage::StorageFile;
use windows_future::AsyncStatus;

const STORAGE_RETRY_DELAYS_MS: [u64; 4] = [0, 100, 250, 500];
const MOVE_RETRY_DELAYS_MS: [u64; 4] = [0, 50, 150, 300];
const STORAGE_OPEN_TIMEOUT: Duration = Duration::from_secs(3);
const CLIP_DECODE_TIMEOUT: Duration = Duration::from_secs(8);
const RENDER_TIMEOUT: Duration = Duration::from_secs(20);
const OPERATION_CANCELLATION_GRACE: Duration = Duration::from_secs(1);
const RENDER_CANCELLATION_GRACE: Duration = Duration::from_secs(2);
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

enum AsyncOperationOutcome<T> {
    Completed {
        value: T,
        timeout_triggered: bool,
    },
    Failed {
        terminal_status: &'static str,
        code: String,
        timeout_triggered: bool,
    },
    UnsettledTimeout {
        cancel_requested: bool,
        last_status: String,
    },
}

macro_rules! wait_bounded_async {
    (
        $operation:expr,
        timeout = $timeout:expr,
        grace = $grace:expr,
        stage = $stage:expr,
        role = $role:expr,
        segment_index = $segment_index:expr,
        attempt = $attempt:expr
    ) => {{
        let operation = &$operation;
        let timeout = $timeout;
        let grace = $grace;
        let stage: &str = $stage;
        let role: &str = $role;
        let segment_index: Option<usize> = $segment_index;
        let attempt: Option<usize> = $attempt;
        let wait_started = Instant::now();
        let mut timeout_triggered = false;

        'operation_wait: loop {
            let status = match operation.Status() {
                Ok(status) => status,
                Err(error) => {
                    let code = error_code(&error);
                    log_stage(
                        &format!("{stage}_status"),
                        false,
                        role,
                        segment_index,
                        attempt,
                        wait_started.elapsed().as_millis(),
                        Some(&code),
                    );
                    break 'operation_wait AsyncOperationOutcome::UnsettledTimeout {
                        cancel_requested: false,
                        last_status: format!("status_error:{code}"),
                    };
                }
            };

            if status == AsyncStatus::Completed {
                match operation.GetResults() {
                    Ok(value) => {
                        break 'operation_wait AsyncOperationOutcome::Completed {
                            value,
                            timeout_triggered,
                        };
                    }
                    Err(error) => {
                        break 'operation_wait AsyncOperationOutcome::Failed {
                            terminal_status: "result_error",
                            code: error_code(&error),
                            timeout_triggered,
                        };
                    }
                }
            }
            if status == AsyncStatus::Canceled {
                break 'operation_wait AsyncOperationOutcome::Failed {
                    terminal_status: "canceled",
                    code: "none".to_string(),
                    timeout_triggered,
                };
            }
            if status == AsyncStatus::Error {
                let code = operation
                    .ErrorCode()
                    .map(|value| format!("{value:?}"))
                    .unwrap_or_else(|_| "unknown".to_string());
                break 'operation_wait AsyncOperationOutcome::Failed {
                    terminal_status: "error",
                    code,
                    timeout_triggered,
                };
            }

            if wait_started.elapsed() >= timeout {
                timeout_triggered = true;
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage}_timeout ok=false role={role} segment_index={} attempt={} elapsed_ms={} timeout_ms={}",
                    optional_usize(segment_index),
                    optional_usize(attempt),
                    wait_started.elapsed().as_millis(),
                    timeout.as_millis(),
                );

                let cancel_started = Instant::now();
                let cancel_result = operation.Cancel();
                let cancel_requested = cancel_result.is_ok();
                let cancel_code = cancel_result.err().map(|error| error_code(&error));
                log_stage(
                    &format!("{stage}_cancel"),
                    cancel_requested,
                    role,
                    segment_index,
                    attempt,
                    cancel_started.elapsed().as_millis(),
                    cancel_code.as_deref(),
                );

                loop {
                    let status = match operation.Status() {
                        Ok(status) => status,
                        Err(error) => {
                            let code = error_code(&error);
                            break 'operation_wait AsyncOperationOutcome::UnsettledTimeout {
                                cancel_requested,
                                last_status: format!("status_error:{code}"),
                            };
                        }
                    };

                    if status == AsyncStatus::Completed {
                        let result = operation.GetResults();
                        eprintln!(
                            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage}_cancel_settle ok={} role={role} segment_index={} attempt={} terminal_status=completed elapsed_ms={}",
                            result.is_ok(),
                            optional_usize(segment_index),
                            optional_usize(attempt),
                            cancel_started.elapsed().as_millis(),
                        );
                        match result {
                            Ok(value) => {
                                break 'operation_wait AsyncOperationOutcome::Completed {
                                    value,
                                    timeout_triggered: true,
                                };
                            }
                            Err(error) => {
                                break 'operation_wait AsyncOperationOutcome::Failed {
                                    terminal_status: "result_error",
                                    code: error_code(&error),
                                    timeout_triggered: true,
                                };
                            }
                        }
                    }
                    if status == AsyncStatus::Canceled {
                        eprintln!(
                            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage}_cancel_settle ok=true role={role} segment_index={} attempt={} terminal_status=canceled elapsed_ms={}",
                            optional_usize(segment_index),
                            optional_usize(attempt),
                            cancel_started.elapsed().as_millis(),
                        );
                        break 'operation_wait AsyncOperationOutcome::Failed {
                            terminal_status: "canceled",
                            code: "none".to_string(),
                            timeout_triggered: true,
                        };
                    }
                    if status == AsyncStatus::Error {
                        let code = operation
                            .ErrorCode()
                            .map(|value| format!("{value:?}"))
                            .unwrap_or_else(|_| "unknown".to_string());
                        eprintln!(
                            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage}_cancel_settle ok=true role={role} segment_index={} attempt={} terminal_status=error elapsed_ms={} hresult={code}",
                            optional_usize(segment_index),
                            optional_usize(attempt),
                            cancel_started.elapsed().as_millis(),
                        );
                        break 'operation_wait AsyncOperationOutcome::Failed {
                            terminal_status: "error",
                            code,
                            timeout_triggered: true,
                        };
                    }
                    if cancel_started.elapsed() >= grace {
                        eprintln!(
                            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage}_cancel_settle ok=false role={role} segment_index={} attempt={} terminal_status=started elapsed_ms={} grace_ms={} operation_unsettled=true",
                            optional_usize(segment_index),
                            optional_usize(attempt),
                            cancel_started.elapsed().as_millis(),
                            grace.as_millis(),
                        );
                        break 'operation_wait AsyncOperationOutcome::UnsettledTimeout {
                            cancel_requested,
                            last_status: "started".to_string(),
                        };
                    }

                    std::thread::sleep(STATUS_POLL_INTERVAL);
                }
            }

            std::thread::sleep(STATUS_POLL_INTERVAL);
        }
    }};
}

fn storage_file(
    path: &Path,
    role: &'static str,
    segment_index: Option<usize>,
    cleanup_deferred: &mut bool,
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
        let operation = match StorageFile::GetFileFromPathAsync(&path) {
            Ok(operation) => operation,
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
                continue;
            }
        };

        let outcome = wait_bounded_async!(
            operation,
            timeout = STORAGE_OPEN_TIMEOUT,
            grace = OPERATION_CANCELLATION_GRACE,
            stage = "open_file",
            role = role,
            segment_index = segment_index,
            attempt = Some(attempt)
        );
        let _ = operation.Close();

        match outcome {
            AsyncOperationOutcome::Completed {
                value,
                timeout_triggered,
            } => {
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=open_file ok=true role={role} segment_index={} attempt={attempt} elapsed_ms={} timeout_triggered={timeout_triggered}",
                    optional_usize(segment_index),
                    started.elapsed().as_millis(),
                );
                return Ok(value);
            }
            AsyncOperationOutcome::Failed {
                terminal_status,
                code,
                timeout_triggered,
            } => {
                last_stage = "open_file_wait";
                last_code = code;
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={last_stage} ok=false role={role} segment_index={} attempt={attempt} elapsed_ms={} terminal_status={terminal_status} timeout_triggered={timeout_triggered} hresult={last_code}",
                    optional_usize(segment_index),
                    started.elapsed().as_millis(),
                );
                if timeout_triggered {
                    break;
                }
            }
            AsyncOperationOutcome::UnsettledTimeout {
                cancel_requested,
                last_status,
            } => {
                if role == "destination" {
                    *cleanup_deferred = true;
                }
                return Err(anyhow!(
                    "Windows media file open exceeded its bounded deadline role={role} segment_index={} cancel_requested={cancel_requested} last_status={last_status}",
                    optional_usize(segment_index)
                ));
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

/// Concatenate already-finalized MP4 recording segments with Windows' native
/// media-editing pipeline. MediaComposition owns the timeline so paused wall-clock
/// time is not present in the resulting file.
pub fn concatenate_segments(segments: &[PathBuf], final_path: &Path) -> Result<()> {
    let total_started = Instant::now();
    eprintln!(
        "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=begin ok=true segment_count={} open_timeout_ms={} decode_timeout_ms={} render_timeout_ms={} operation_cancellation_grace_ms={} render_cancellation_grace_ms={}",
        segments.len(),
        STORAGE_OPEN_TIMEOUT.as_millis(),
        CLIP_DECODE_TIMEOUT.as_millis(),
        RENDER_TIMEOUT.as_millis(),
        OPERATION_CANCELLATION_GRACE.as_millis(),
        RENDER_CANCELLATION_GRACE.as_millis(),
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
        let destination = storage_file(
            &candidate_path,
            "destination",
            None,
            &mut cleanup_deferred,
        )?;

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
            let file = storage_file(segment, "segment", Some(index), &mut cleanup_deferred)?;

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
            let outcome = wait_bounded_async!(
                operation,
                timeout = CLIP_DECODE_TIMEOUT,
                grace = OPERATION_CANCELLATION_GRACE,
                stage = "decode_segment",
                role = "segment",
                segment_index = Some(index),
                attempt = None
            );
            let _ = operation.Close();

            let clip = match outcome {
                AsyncOperationOutcome::Completed {
                    value,
                    timeout_triggered,
                } => {
                    eprintln!(
                        "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=decode_segment ok=true role=segment segment_index={index} attempt=none elapsed_ms={} timeout_triggered={timeout_triggered}",
                        decode_started.elapsed().as_millis(),
                    );
                    value
                }
                AsyncOperationOutcome::Failed {
                    terminal_status,
                    code,
                    timeout_triggered,
                } => {
                    eprintln!(
                        "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=decode_segment_wait ok=false role=segment segment_index={index} attempt=none elapsed_ms={} terminal_status={terminal_status} timeout_triggered={timeout_triggered} hresult={code}",
                        decode_started.elapsed().as_millis(),
                    );
                    return Err(anyhow!(
                        "Windows could not decode recording segment_index={index} terminal_status={terminal_status} hresult={code}"
                    ));
                }
                AsyncOperationOutcome::UnsettledTimeout {
                    cancel_requested,
                    last_status,
                } => {
                    return Err(anyhow!(
                        "Windows segment decoding exceeded its bounded deadline segment_index={index} cancel_requested={cancel_requested} last_status={last_status}"
                    ));
                }
            };

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

        let outcome = wait_bounded_async!(
            render,
            timeout = RENDER_TIMEOUT,
            grace = RENDER_CANCELLATION_GRACE,
            stage = "render",
            role = "destination",
            segment_index = None,
            attempt = None
        );
        let _ = render.Close();
        drop(render);
        drop(destination);

        match outcome {
            AsyncOperationOutcome::Completed {
                value: reason,
                timeout_triggered,
            } if reason == TranscodeFailureReason::None => {
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=render_result ok=true role=destination elapsed_ms={} reason={reason:?} timeout_triggered={timeout_triggered}",
                    render_started.elapsed().as_millis()
                );
                move_candidate_to_final(&candidate_path, final_path)?;
                Ok(())
            }
            AsyncOperationOutcome::Completed {
                value: reason,
                timeout_triggered,
            } => {
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=render_result ok=false role=destination elapsed_ms={} reason={reason:?} timeout_triggered={timeout_triggered}",
                    render_started.elapsed().as_millis()
                );
                Err(anyhow!(
                    "Windows media finalization rejected recording segments reason={reason:?}"
                ))
            }
            AsyncOperationOutcome::Failed {
                terminal_status,
                code,
                timeout_triggered,
            } => Err(anyhow!(
                "Windows media finalization failed terminal_status={terminal_status} timeout_triggered={timeout_triggered} hresult={code}"
            )),
            AsyncOperationOutcome::UnsettledTimeout {
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
