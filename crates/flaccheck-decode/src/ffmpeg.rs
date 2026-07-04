use crate::DecodeError;
use flaccheck_core::PcmBuffer;
use std::path::Path;
use std::process::Command;

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn decode_via_ffmpeg(path: &Path) -> Result<PcmBuffer, DecodeError> {
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            path.to_str().unwrap_or(""),
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "-ac",
            "2",
            "-",
        ])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                let ext = path
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("?")
                    .to_string();
                DecodeError::FfmpegRequired { ext }
            } else {
                DecodeError::DecodeFailed(e.to_string())
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DecodeError::DecodeFailed(stderr.to_string()));
    }

    let bytes = output.stdout;
    let sample_count = bytes.len() / 4;
    let mut samples = Vec::with_capacity(sample_count);
    for chunk in bytes.chunks_exact(4) {
        samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }

    // Probe sample rate via ffprobe
    let sr = ffprobe_sample_rate(path).unwrap_or(44100);
    let channels = 2u16;

    Ok(PcmBuffer {
        samples,
        sample_rate: sr,
        channels,
        bits_per_sample: Some(32),
    })
}

fn ffprobe_sample_rate(path: &Path) -> Option<u32> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=sample_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            path.to_str()?,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}
