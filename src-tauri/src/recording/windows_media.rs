use crate::encoding;
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use windows::Media::Editing::{MediaClip, MediaComposition};
use windows::Media::Transcoding::TranscodeFailureReason;
use windows::Storage::StorageFile;

const STORAGE_RETRY_DELAYS_MS: [u64; 4] = [0, 100, 250, 500];

fn error_code(error: &windows::core::Error) -> String {
    format!("{:?}", error.code())
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
    match (segment_index, attempt, code) {
        (Some(index), Some(attempt), Some(code)) => eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage} ok={ok} role={role} segment_index={index} attempt={attempt} elapsed_ms={elapsed_ms} hresult={code}"
        ),
        (Some(index), Some(attempt), None) => eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage} ok={ok} role={role} segment_index={index} attempt={attempt} elapsed_ms={elapsed_ms}"
        ),
        (Some(index), None, Some(code)) => eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage} ok={ok} role={role} segment_index={index} elapsed_ms={elapsed_ms} hresult={code}"
        ),
        (Some(index), None, None) => eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage} ok={ok} role={role} segment_index={index} elapsed_ms={elapsed_ms}"
        ),
        (None, Some(attempt), Some(code)) => eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage} ok={ok} role={role} attempt={attempt} elapsed_ms={elapsed_ms} hresult={code}"
        ),
        (None, Some(attempt), None) => eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage} ok={ok} role={role} attempt={attempt} elapsed_ms={elapsed_ms}"
        ),
        (None, None, Some(code)) => eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage} ok={ok} role={role} elapsed_ms={elapsed_ms} hresult={code}"
        ),
        (None, None, None) => eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage={stage} ok={ok} role={role} elapsed_ms={elapsed_ms}"
        ),
    }
}

fn storage_file(path: &Path, role: &'static str, segment_index: Option<usize>) -> Result<StorageFile> {
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
                "Windows media path normalization failed for role={role} segment_index={}",
                segment_index
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string())
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
        "Windows media file open failed at stage={last_stage} role={role} segment_index={} hresult={last_code}",
        segment_index
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    ))
}

/// Concatenate already-finalized MP4 recording segments with Windows' native
/// media-editing pipeline. MediaComposition owns the timeline so paused wall-clock
/// time is not present in the resulting file.
pub fn concatenate_segments(segments: &[std::path::PathBuf], final_path: &Path) -> Result<()> {
    let total_started = Instant::now();
    eprintln!(
        "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=begin ok=true segment_count={}",
        segments.len()
    );

    if segments.len() < 2 {
        eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=validate_segments ok=false segment_count={}",
            segments.len()
        );
        return Err(anyhow!("Native media concatenation requires at least two segments"));
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
                    "Recording segment metadata was invalid for segment_index={index}"
                ));
            }
            Err(_) => {
                eprintln!(
                    "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=segment_metadata ok=false segment_index={index} elapsed_ms={}",
                    started.elapsed().as_millis()
                );
                return Err(anyhow!(
                    "Recording segment metadata was unavailable for segment_index={index}"
                ));
            }
        }
    }

    if final_path.exists() {
        fs::remove_file(final_path).context("Unable to replace native finalizer destination")?;
    }

    let create_started = Instant::now();
    fs::File::create(final_path).context("Unable to create native finalizer destination")?;
    log_stage(
        "create_destination",
        true,
        "destination",
        None,
        None,
        create_started.elapsed().as_millis(),
        None,
    );

    let result = (|| -> Result<()> {
        let destination = storage_file(final_path, "destination", None)?;

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
        let reason = render.get().map_err(|error| {
            let code = error_code(&error);
            log_stage(
                "render_wait",
                false,
                "destination",
                None,
                None,
                render_started.elapsed().as_millis(),
                Some(&code),
            );
            anyhow!("Windows media finalization failed hresult={code}")
        })?;

        if reason != TranscodeFailureReason::None {
            eprintln!(
                "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=render_result ok=false role=destination elapsed_ms={} reason={reason:?}",
                render_started.elapsed().as_millis()
            );
            return Err(anyhow!(
                "Windows media finalization rejected recording segments reason={reason:?}"
            ));
        }
        eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=render_result ok=true role=destination elapsed_ms={} reason={reason:?}",
            render_started.elapsed().as_millis()
        );

        Ok(())
    })();

    if let Err(error) = result {
        let _ = fs::remove_file(final_path);
        eprintln!(
            "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=complete ok=false segment_count={} total_ms={}",
            segments.len(),
            total_started.elapsed().as_millis()
        );
        return Err(error);
    }

    for segment in segments {
        let _ = fs::remove_file(segment);
    }

    eprintln!(
        "[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=complete ok=true segment_count={} total_ms={}",
        segments.len(),
        total_started.elapsed().as_millis()
    );
    Ok(())
}
