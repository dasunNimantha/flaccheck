use flaccheck_core::PcmBuffer;
use std::path::{Path, PathBuf};
use thiserror::Error;

mod ffmpeg;
mod symphonia_decode;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("decode failed: {0}")]
    DecodeFailed(String),
    #[error("ffmpeg required for {ext} but not found in PATH")]
    FfmpegRequired { ext: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

const LOSSLESS_EXTENSIONS: &[&str] = &[
    "flac", "wav", "wave", "aiff", "aif", "m4a", "alac", "ape", "wv", "opus",
];

pub fn is_supported_lossless(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| LOSSLESS_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn needs_ffmpeg(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .as_deref(),
        Some("ape" | "wv" | "opus")
    )
}

pub fn decode_file(path: &Path) -> Result<PcmBuffer, DecodeError> {
    if needs_ffmpeg(path) {
        if ffmpeg::ffmpeg_available() {
            return ffmpeg::decode_via_ffmpeg(path);
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("?")
            .to_string();
        return Err(DecodeError::FfmpegRequired { ext });
    }
    symphonia_decode::decode(path)
}

pub fn collect_audio_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_recursive(root, &mut files);
    files.sort();
    files
}

fn collect_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recursive(&path, out);
        } else if is_supported_lossless(&path) {
            out.push(path);
        }
    }
}
