use crate::encoding;
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(120);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);
const PUBLISH_RETRY_DELAYS_MS: [u64; 4] = [0, 50, 150, 300];

enum ProcessOutcome {
    Exited(ExitStatus),
    TimedOut {
        kill_requested: bool,
        reaped: bool,
        exit_code: Option<i32>,
    },
    InspectionFailed {
        code: String,
        kill_requested: bool,
        reaped: bool,
        exit_code: Option<i32>,
    },
}

pub(super) fn concatenate_segments(segments: &[PathBuf], final_path: &Path) -> Result<u128> {
    let total_started = Instant::now();
    eprintln!(
        "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=begin ok=true segment_count={} timeout_ms={}",
        segments.len(),
        PROCESS_TIMEOUT.as_millis()
    );

    if segments.len() < 2 {
        eprintln!(
            "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=validate_segments ok=false segment_count={} elapsed_ms={}",
            segments.len(),
            total_started.elapsed().as_millis()
        );
        return Err(anyhow!(
            "FFmpeg concat fallback requires at least two recording segments"
        ));
    }

    for (index, segment) in segments.iter().enumerate() {
        let metadata = fs::metadata(segment).map_err(|error| {
            eprintln!(
                "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=segment_metadata ok=false segment_index={index} code={:?} elapsed_ms={}",
                error.kind(),
                total_started.elapsed().as_millis()
            );
            anyhow!("FFmpeg concat input metadata was unavailable segment_index={index}")
        })?;
        if !metadata.is_file() || metadata.len() == 0 {
            eprintln!(
                "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=segment_metadata ok=false segment_index={index} bytes={} elapsed_ms={}",
                metadata.len(),
                total_started.elapsed().as_millis()
            );
            return Err(anyhow!(
                "FFmpeg concat input was not a non-empty regular file segment_index={index}"
            ));
        }
    }

    let candidate_path = candidate_path(final_path);
    let manifest = concat_manifest(segments);
    let spawn_started = Instant::now();
    let mut command = encoding::ffmpeg_command();
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-protocol_whitelist",
            "file,pipe",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            "pipe:0",
            "-c",
            "copy",
            "-movflags",
            "+faststart",
        ])
        .arg(&candidate_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = command.spawn().map_err(|error| {
        eprintln!(
            "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=spawn ok=false code={:?} elapsed_ms={}",
            error.kind(),
            spawn_started.elapsed().as_millis()
        );
        anyhow!("Unable to start the FFmpeg concat fallback")
    })?;
    eprintln!(
        "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=spawn ok=true process_id={} elapsed_ms={}",
        child.id(),
        spawn_started.elapsed().as_millis()
    );

    let write_started = Instant::now();
    let write_result = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("FFmpeg concat fallback did not expose standard input"))
        .and_then(|mut stdin| {
            stdin
                .write_all(manifest.as_bytes())
                .context("Unable to submit the FFmpeg concat manifest")
        });
    if write_result.is_err() {
        let kill_requested = child.kill().is_ok();
        let reaped = if kill_requested {
            child.wait().is_ok()
        } else {
            child.try_wait().ok().flatten().is_some()
        };
        if reaped {
            let _ = fs::remove_file(&candidate_path);
        }
        eprintln!(
            "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=write_manifest ok=false manifest_bytes={} kill_requested={kill_requested} reaped={reaped} cleanup_deferred={} elapsed_ms={}",
            manifest.len(),
            !reaped,
            write_started.elapsed().as_millis()
        );
        return Err(anyhow!("Unable to submit the FFmpeg concat manifest"));
    }
    eprintln!(
        "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=write_manifest ok=true manifest_bytes={} elapsed_ms={}",
        manifest.len(),
        write_started.elapsed().as_millis()
    );

    match wait_bounded(child) {
        ProcessOutcome::Exited(status) if status.success() => {
            eprintln!(
                "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=process_exit ok=true exit_code={} elapsed_ms={}",
                status.code().unwrap_or(0),
                total_started.elapsed().as_millis()
            );
        }
        ProcessOutcome::Exited(status) => {
            let _ = fs::remove_file(&candidate_path);
            eprintln!(
                "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=process_exit ok=false exit_code={} elapsed_ms={}",
                status.code().unwrap_or(-1),
                total_started.elapsed().as_millis()
            );
            return Err(anyhow!(
                "FFmpeg could not concatenate paused recording segments"
            ));
        }
        ProcessOutcome::TimedOut {
            kill_requested,
            reaped,
            exit_code,
        } => {
            let cleanup_deferred = !reaped;
            if !cleanup_deferred {
                let _ = fs::remove_file(&candidate_path);
            }
            eprintln!(
                "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=complete ok=false timeout_triggered=true kill_requested={kill_requested} reaped={reaped} exit_code={} cleanup_deferred={cleanup_deferred} total_ms={}",
                optional_exit_code(exit_code),
                total_started.elapsed().as_millis()
            );
            return Err(anyhow!(
                "FFmpeg concat fallback exceeded its bounded deadline"
            ));
        }
        ProcessOutcome::InspectionFailed {
            code,
            kill_requested,
            reaped,
            exit_code,
        } => {
            let cleanup_deferred = !reaped;
            if !cleanup_deferred {
                let _ = fs::remove_file(&candidate_path);
            }
            eprintln!(
                "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=complete ok=false timeout_triggered=false inspection_failed=true code={code} kill_requested={kill_requested} reaped={reaped} exit_code={} cleanup_deferred={cleanup_deferred} total_ms={}",
                optional_exit_code(exit_code),
                total_started.elapsed().as_millis()
            );
            return Err(anyhow!(
                "Unable to inspect the FFmpeg concat fallback process code={code}"
            ));
        }
    }

    let candidate_metadata = fs::metadata(&candidate_path).map_err(|error| {
        eprintln!(
            "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=validate_candidate ok=false code={:?} elapsed_ms={}",
            error.kind(),
            total_started.elapsed().as_millis()
        );
        anyhow!("FFmpeg concat fallback did not produce an output candidate")
    })?;
    if !candidate_metadata.is_file() || candidate_metadata.len() == 0 {
        let _ = fs::remove_file(&candidate_path);
        eprintln!(
            "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=validate_candidate ok=false bytes={} elapsed_ms={}",
            candidate_metadata.len(),
            total_started.elapsed().as_millis()
        );
        return Err(anyhow!(
            "FFmpeg concat fallback produced an invalid output candidate"
        ));
    }
    eprintln!(
        "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=validate_candidate ok=true bytes={} elapsed_ms={}",
        candidate_metadata.len(),
        total_started.elapsed().as_millis()
    );

    if let Err(error) = publish_candidate(&candidate_path, final_path) {
        eprintln!(
            "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=complete ok=false timeout_triggered=false cleanup_deferred=true total_ms={}",
            total_started.elapsed().as_millis()
        );
        return Err(error);
    }

    for segment in segments {
        let _ = fs::remove_file(segment);
    }

    let total_ms = total_started.elapsed().as_millis();
    eprintln!(
        "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=complete ok=true timeout_triggered=false cleanup_deferred=false total_ms={total_ms}"
    );
    Ok(total_ms)
}

fn wait_bounded(mut child: Child) -> ProcessOutcome {
    let started = Instant::now();
    loop {
        let current_status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                let code = format!("{:?}", error.kind());
                let terminate_started = Instant::now();
                let kill_requested = child.kill().is_ok();
                let reaped_status = if kill_requested {
                    child.wait().ok()
                } else {
                    child.try_wait().ok().flatten()
                };
                let reaped = reaped_status.is_some();
                let exit_code = reaped_status.and_then(|status| status.code());
                eprintln!(
                    "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=inspect_process ok=false code={code} kill_requested={kill_requested} reaped={reaped} exit_code={} elapsed_ms={}",
                    optional_exit_code(exit_code),
                    terminate_started.elapsed().as_millis()
                );
                return ProcessOutcome::InspectionFailed {
                    code,
                    kill_requested,
                    reaped,
                    exit_code,
                };
            }
        };

        if let Some(status) = current_status {
            return ProcessOutcome::Exited(status);
        }

        if started.elapsed() >= PROCESS_TIMEOUT {
            eprintln!(
                "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=timeout ok=false timeout_ms={} elapsed_ms={}",
                PROCESS_TIMEOUT.as_millis(),
                started.elapsed().as_millis()
            );
            let terminate_started = Instant::now();
            let kill_requested = child.kill().is_ok();
            let reaped_status = if kill_requested {
                child.wait().ok()
            } else {
                child.try_wait().ok().flatten()
            };
            let reaped = reaped_status.is_some();
            let exit_code = reaped_status.and_then(|status| status.code());
            eprintln!(
                "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=terminate ok={} kill_requested={kill_requested} reaped={reaped} exit_code={} elapsed_ms={}",
                kill_requested && reaped,
                optional_exit_code(exit_code),
                terminate_started.elapsed().as_millis()
            );
            return ProcessOutcome::TimedOut {
                kill_requested,
                reaped,
                exit_code,
            };
        }

        std::thread::sleep(PROCESS_POLL_INTERVAL);
    }
}

fn optional_exit_code(exit_code: Option<i32>) -> String {
    exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn candidate_path(final_path: &Path) -> PathBuf {
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
        ".{stem}.ffmpeg-finalizing-{}-{nonce}.mp4",
        std::process::id()
    ))
}

fn concat_manifest(segments: &[PathBuf]) -> String {
    let mut body = String::new();
    for segment in segments {
        let normalized = segment
            .to_string_lossy()
            .replace('\\', "/")
            .replace('\'', "'\\''");
        body.push_str(&format!("file '{normalized}'\n"));
    }
    body
}

fn publish_candidate(candidate: &Path, final_path: &Path) -> Result<()> {
    let started = Instant::now();
    let mut last_error = None;

    for (attempt_index, delay_ms) in PUBLISH_RETRY_DELAYS_MS.into_iter().enumerate() {
        let attempt = attempt_index + 1;
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
        match fs::rename(candidate, final_path) {
            Ok(()) => {
                eprintln!(
                    "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=publish_candidate ok=true attempt={attempt} elapsed_ms={}",
                    started.elapsed().as_millis()
                );
                return Ok(());
            }
            Err(error) => {
                last_error = Some(error.kind());
                eprintln!(
                    "[Recorder][FallbackHealth] backend=ffmpeg-concat stage=publish_candidate ok=false attempt={attempt} code={:?} elapsed_ms={}",
                    error.kind(),
                    started.elapsed().as_millis()
                );
            }
        }
    }

    Err(anyhow!(
        "FFmpeg concat fallback could not publish its completed candidate code={}",
        last_error
            .map(|kind| format!("{kind:?}"))
            .unwrap_or_else(|| "unknown".to_string())
    ))
}
