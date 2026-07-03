//! Shared types for lossless-scan analysis pipeline.

mod analyze;
mod calibration;
mod evidence;
mod fusion;
mod pcm;
mod scan_mode;
mod verdict;

pub use analyze::{AnalysisConfig, AnalysisError};
pub use calibration::Thresholds;
pub use evidence::Evidence;
pub use fusion::{fuse_evidence, fuse_hires_verdict, spectral_information_score};
pub use pcm::PcmBuffer;
pub use scan_mode::ScanMode;
pub use verdict::{HiresVerdict, TranscodeVerdict};

use serde::{Deserialize, Serialize};

/// Complete analysis output for one audio file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub path: String,
    pub transcode_verdict: TranscodeVerdict,
    pub hires_verdict: HiresVerdict,
    pub confidence: f64,
    pub evidence: Vec<Evidence>,
    pub codec_guess: Option<String>,
    pub est_source_bitrate_kbps: Option<u32>,
    pub spectral_info_score: f64,
    pub mode: ScanMode,
    pub duration_secs: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: Option<u16>,
}

impl AnalysisResult {
    pub fn is_borderline(&self) -> bool {
        matches!(self.transcode_verdict, TranscodeVerdict::Suspicious)
            || (self.confidence > 0.35 && self.confidence < 0.65)
    }
}
