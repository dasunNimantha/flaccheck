//! Analysis pipeline orchestrating all detector tiers.

use crate::artifacts;
use crate::hires;
use crate::quant::{self, QuantSearchDepth};
use crate::spectral;
use lossless_scan_core::{
    fuse_evidence, fuse_hires_verdict, AnalysisConfig, AnalysisError, AnalysisResult, Evidence,
    PcmBuffer, ScanMode, Thresholds, TranscodeVerdict,
};

pub fn analyze_pcm(
    path: &str,
    pcm: &PcmBuffer,
    config: &AnalysisConfig,
) -> Result<AnalysisResult, AnalysisError> {
    if pcm.samples.is_empty() {
        return Err(AnalysisError::EmptyBuffer);
    }

    let thresholds = &config.thresholds;
    let window_count = if config.full_file {
        config.window_count.max(5)
    } else {
        config.window_count
    };

    let spectral = spectral::analyze(pcm, config.window_secs, window_count, thresholds);
    let hires_r = hires::analyze(pcm, thresholds);

    let light_artifacts = matches!(config.mode, ScanMode::Fast);
    let artifacts_r = artifacts::analyze(pcm, light_artifacts, thresholds);

    let mut all_evidence: Vec<Evidence> = Vec::new();
    all_evidence.extend(spectral.evidence.clone());
    all_evidence.extend(hires_r.evidence.clone());
    all_evidence.extend(artifacts_r.evidence.clone());

    let nyquist = pcm.sample_rate as f64 / 2.0;
    let abstain = check_abstention(
        spectral.edge_hz,
        spectral.full_band,
        spectral.suspicion,
        nyquist,
        thresholds,
    );
    all_evidence.extend(abstain);

    let suspicion_gate = spectral.suspicion > 0.25
        || artifacts_r.suspicion > 0.35
        || matches!(config.mode, ScanMode::Max);

    let quant_depth = match config.mode {
        ScanMode::Fast => QuantSearchDepth::Skip,
        ScanMode::Balanced => {
            if spectral.full_band && spectral.suspicion < 0.3 {
                QuantSearchDepth::Skip
            } else if suspicion_gate {
                QuantSearchDepth::Coarse
            } else {
                QuantSearchDepth::Skip
            }
        }
        ScanMode::Max => {
            if spectral.full_band && spectral.suspicion < 0.35 {
                QuantSearchDepth::Skip
            } else {
                QuantSearchDepth::Exhaustive
            }
        }
    };

    let quant_r = quant::analyze(pcm, quant_depth, thresholds);
    all_evidence.extend(quant_r.evidence);

    if suspicion_gate
        && matches!(config.mode, ScanMode::Balanced | ScanMode::Max)
        && light_artifacts
    {
        let full_art = artifacts::analyze(pcm, false, thresholds);
        for e in full_art.evidence {
            if !all_evidence.iter().any(|x| x.signal == e.signal) {
                all_evidence.push(e);
            }
        }
    }

    let (transcode_verdict, confidence, codec_guess, est_bitrate) =
        fuse_evidence(&all_evidence, thresholds);

    // The quant-tier heuristics (MP3 PQMF, MDCT residual) are unreliable on their own and
    // misfire on genuine acoustic/live material. A real lossy transcode always leaves a
    // spectral low-pass cutoff below Nyquist, so quant evidence may only *corroborate* a
    // detected cutoff — it must never independently promote full-bandwidth audio.
    let spectral_cutoff_present = spectral.cutoff_hz < nyquist * 0.97;
    let transcode_verdict = if spectral.full_band && artifacts_r.suspicion < 0.45 {
        transcode_verdict
    } else if spectral_cutoff_present
        && quant_r.transcode_likelihood > thresholds.quant_promote_transcoded
    {
        TranscodeVerdict::Transcoded
    } else if spectral_cutoff_present
        && quant_r.transcode_likelihood > thresholds.quant_promote_suspicious
        && transcode_verdict == TranscodeVerdict::Genuine
    {
        TranscodeVerdict::Suspicious
    } else {
        transcode_verdict
    };

    let confidence = match transcode_verdict {
        TranscodeVerdict::Suspicious | TranscodeVerdict::Transcoded => confidence
            .max(quant_r.transcode_likelihood.min(0.95))
            .max(spectral.codec_certainty * 0.5),
        _ => confidence,
    };

    let hires_verdict = fuse_hires_verdict(&all_evidence);

    let transcode_verdict = if matches!(
        hires_verdict,
        lossless_scan_core::HiresVerdict::Upsampled | lossless_scan_core::HiresVerdict::PaddedDepth
    ) && transcode_verdict == TranscodeVerdict::Genuine
    {
        TranscodeVerdict::Suspicious
    } else {
        transcode_verdict
    };

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

fn check_abstention(
    edge_hz: f64,
    full_band: bool,
    brick_wall_strength: f64,
    nyquist: f64,
    thresholds: &Thresholds,
) -> Vec<Evidence> {
    let mut out = Vec::new();
    if full_band || brick_wall_strength >= 0.35 {
        return out;
    }
    let ambiguous_ceiling = (nyquist * thresholds.abstain_edge_ceiling_ratio)
        .min(thresholds.abstain_edge_ceiling_max_hz);
    if edge_hz > 0.0 && edge_hz < ambiguous_ceiling {
        out.push(Evidence::new(
            "abstention",
            "abstain_band_limited",
            1.0,
            1.0,
            format!(
                "content ends at {:.0} Hz with no cliff; too band-limited to verify authenticity",
                edge_hz
            ),
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

    #[test]
    fn joint_stereo_transcode_reads_suspicious() {
        use lossless_scan_testkit::synth;
        let pcm = synth::joint_stereo_transcode(44100, 3.0, 10, 16000.0);
        let cfg = AnalysisConfig::for_mode(ScanMode::Balanced);
        let r = analyze_pcm("joint.flac", &pcm, &cfg).unwrap();
        assert!(
            !matches!(r.transcode_verdict, TranscodeVerdict::Genuine),
            "expected not genuine, got {:?} conf {:.2}",
            r.transcode_verdict,
            r.confidence
        );
    }
}
