//! Analysis pipeline orchestrating all detector tiers.

use crate::artifacts;
use crate::hires;
use crate::quant::{self, QuantSearchDepth};
use crate::spectral;
use lossless_scan_core::{fuse_evidence, fuse_hires_verdict, AnalysisConfig, AnalysisError, AnalysisResult, Evidence, PcmBuffer, ScanMode, TranscodeVerdict};

pub fn analyze_pcm(path: &str, pcm: &PcmBuffer, config: &AnalysisConfig) -> Result<AnalysisResult, AnalysisError> {
  if pcm.samples.is_empty() {
    return Err(AnalysisError::EmptyBuffer);
  }

  let window_count = if config.full_file {
    config.window_count.max(5)
  } else {
    config.window_count
  };

  // Tier 1
  let spectral = spectral::analyze(pcm, config.window_secs, window_count);

  // Tier 4
  let hires_r = hires::analyze(pcm);

  // Tier 3 light in fast mode
  let light_artifacts = matches!(config.mode, ScanMode::Fast);
  let artifacts_r = artifacts::analyze(pcm, light_artifacts);

  let mut all_evidence: Vec<Evidence> = Vec::new();
  all_evidence.extend(spectral.evidence.clone());
  all_evidence.extend(hires_r.evidence.clone());
  all_evidence.extend(artifacts_r.evidence.clone());

  // Abstention (Tier 5)
  let abstain = check_abstention(spectral.rolloff_hz, spectral.spectral_info_score);
  all_evidence.extend(abstain);

  let suspicion_gate = spectral.suspicion > 0.25
    || artifacts_r.suspicion > 0.35
    || matches!(config.mode, ScanMode::Max);

  // Tier 2
  let quant_depth = match config.mode {
    ScanMode::Fast => QuantSearchDepth::Skip,
    ScanMode::Balanced => {
      if suspicion_gate {
        QuantSearchDepth::Coarse
      } else {
        QuantSearchDepth::Skip
      }
    }
    ScanMode::Max => QuantSearchDepth::Exhaustive,
  };

  let quant_r = quant::analyze(pcm, quant_depth);
  all_evidence.extend(quant_r.evidence);

  // Re-run full artifacts in balanced/max when suspicious
  if !light_artifacts && suspicion_gate && !matches!(config.mode, ScanMode::Fast) {
    // already ran full in balanced/max
  } else if suspicion_gate && matches!(config.mode, ScanMode::Balanced | ScanMode::Max) {
    let full_art = artifacts::analyze(pcm, false);
    for e in full_art.evidence {
      if !all_evidence.iter().any(|x| x.signal == e.signal) {
        all_evidence.push(e);
      }
    }
  }

  let (transcode_verdict, mut confidence, codec_guess, est_bitrate) = fuse_evidence(&all_evidence);

  // Boost suspicion from quant
  let transcode_verdict = if quant_r.transcode_likelihood > 0.7 && transcode_verdict == TranscodeVerdict::Genuine {
    TranscodeVerdict::Suspicious
  } else if quant_r.transcode_likelihood > 0.85 {
    TranscodeVerdict::Transcoded
  } else {
    transcode_verdict
  };

  if quant_r.transcode_likelihood > 0.5 {
    confidence = confidence.max(quant_r.transcode_likelihood);
  }

  let hires_verdict = fuse_hires_verdict(&all_evidence);

  Ok(AnalysisResult {
    path: path.to_string(),
    transcode_verdict,
    hires_verdict,
    confidence,
    evidence: all_evidence,
    codec_guess: codec_guess.or(spectral.codec_guess),
    est_source_bitrate_kbps: est_bitrate.or(spectral.est_bitrate_kbps),
    spectral_info_score: spectral.spectral_info_score,
    mode: config.mode,
    duration_secs: pcm.duration_secs(),
    sample_rate: pcm.sample_rate,
    channels: pcm.channels,
    bits_per_sample: pcm.bits_per_sample,
  })
}

fn check_abstention(rolloff_hz: f64, info_score: f64) -> Vec<Evidence> {
  let mut out = Vec::new();
  if rolloff_hz > 0.0 && rolloff_hz < 7000.0 {
    out.push(Evidence::new(
      "abstention",
      "abstain_band_limited",
      1.0,
      1.0,
      format!(
        "naturally band-limited content (rolloff {:.0} Hz < 7 kHz); cannot verify lossless authenticity",
        rolloff_hz
      ),
    ));
  } else if info_score < 0.05 && rolloff_hz < 12000.0 {
    out.push(Evidence::new(
      "abstention",
      "abstain_band_limited",
      0.8,
      0.8,
      "low high-frequency information; abstaining from genuine verdict",
    ));
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;
  use lossless_scan_core::ScanMode;

  fn tone_pcm() -> PcmBuffer {
    let n = 44100 * 2;
    let samples: Vec<f32> = (0..n)
      .map(|i| {
        (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin() * 0.5
          + (2.0 * std::f32::consts::PI * 12000.0 * i as f32 / 44100.0).sin() * 0.2
      })
      .collect();
    PcmBuffer {
      samples,
      sample_rate: 44100,
      channels: 1,
      bits_per_sample: Some(16),
    }
  }

  #[test]
  fn pipeline_fast_mode() {
    let pcm = tone_pcm();
    let cfg = AnalysisConfig::for_mode(ScanMode::Fast);
    let r = analyze_pcm("test.flac", &pcm, &cfg).unwrap();
    assert!(!r.evidence.is_empty());
    assert!(r.confidence >= 0.0);
  }
}
