use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::Path;
use windows::core::HSTRING;
use windows::Media::Editing::{MediaClip, MediaComposition};
use windows::Media::Transcoding::TranscodeFailureReason;
use windows::Storage::StorageFile;

fn storage_file(path: &Path) -> Result<StorageFile> {
    let absolute = fs::canonicalize(path)
        .with_context(|| format!("Unable to resolve media path {}", path.display()))?;
    let path = HSTRING::from(absolute.to_string_lossy().as_ref());
    StorageFile::GetFileFromPathAsync(&path)
        .context("Unable to open media file through Windows Storage")?
        .get()
        .context("Windows could not open the media file")
}

/// Concatenate already-finalized MP4 recording segments with Windows' native
/// media-editing pipeline. MediaComposition owns the timeline so paused wall-clock
/// time is not present in the resulting file.
pub fn concatenate_segments(segments: &[std::path::PathBuf], final_path: &Path) -> Result<()> {
    if segments.len() < 2 {
        return Err(anyhow!("Native media concatenation requires at least two segments"));
    }

    if final_path.exists() {
        fs::remove_file(final_path)
            .with_context(|| format!("Unable to replace {}", final_path.display()))?;
    }

    // StorageFile::GetFileFromPathAsync requires the destination to exist first.
    fs::File::create(final_path)
        .with_context(|| format!("Unable to create {}", final_path.display()))?;

    let result = (|| -> Result<()> {
        let destination = storage_file(final_path)?;
        let composition = MediaComposition::new()
            .context("Unable to create Windows media composition")?;
        let clips = composition
            .Clips()
            .context("Unable to access Windows media composition clips")?;

        for segment in segments {
            let file = storage_file(segment)?;
            let clip = MediaClip::CreateFromFileAsync(&file)
                .with_context(|| format!("Unable to load recording segment {}", segment.display()))?
                .get()
                .with_context(|| format!("Windows could not decode recording segment {}", segment.display()))?;
            clips
                .Append(&clip)
                .with_context(|| format!("Unable to append recording segment {}", segment.display()))?;
        }

        let reason = composition
            .RenderToFileAsync(&destination)
            .context("Unable to start native Windows recording finalization")?
            .get()
            .context("Windows media finalization failed")?;

        if reason != TranscodeFailureReason::None {
            return Err(anyhow!(
                "Windows media finalization rejected one or more recording segments"
            ));
        }

        Ok(())
    })();

    if let Err(error) = result {
        let _ = fs::remove_file(final_path);
        return Err(error);
    }

    for segment in segments {
        let _ = fs::remove_file(segment);
    }

    Ok(())
}
