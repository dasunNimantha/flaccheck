//! Tier 2: Quantization residual detector (Derrien JAES 2019).
//! AAC MDCT path + MP3 PQMF hybrid heuristic.

use crate::dsp::welch_psd;
use lossless_scan_core::{Evidence, PcmBuffer, Thresholds};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantSearchDepth {
    Skip,
    Coarse,
    Exhaustive,
}

#[derive(Debug, Clone)]
pub struct QuantResult {
    pub evidence: Vec<Evidence>,
    pub transcode_likelihood: f64,
}

pub fn analyze(pcm: &PcmBuffer, depth: QuantSearchDepth, thresholds: &Thresholds) -> QuantResult {
    if depth == QuantSearchDepth::Skip {
        return QuantResult {
            evidence: vec![],
            transcode_likelihood: 0.0,
        };
    }

    let mono = pcm.left();
    if mono.len() < 2048 {
        return QuantResult {
            evidence: vec![],
            transcode_likelihood: 0.0,
        };
    }

    let offsets: Vec<f64> = match depth {
        QuantSearchDepth::Coarse => (-4..=4).map(|x| x as f64 * 0.5).collect(),
        QuantSearchDepth::Exhaustive => (-40..=40).map(|x| x as f64 * 0.25).collect(),
        QuantSearchDepth::Skip => vec![],
    };

    let (best_residual, best_offset) = search_quant_residual(&mono, &offsets);
    let mdct_likelihood = score_residual(best_residual, thresholds);

    let pqmf_likelihood = detect_mp3_pqmf(&mono, pcm.sample_rate, thresholds);

    let likelihood = mdct_likelihood.max(pqmf_likelihood * 0.95);

    let mut evidence = Vec::new();
    if mdct_likelihood > 0.15 {
        evidence.push(Evidence::new(
            "quant",
            "aac_mdct_residual",
            mdct_likelihood,
            thresholds.weight_quant,
            format!(
                "quantization rounding residual energy {:.2e} at offset {:.2} (Derrien MDCT)",
                best_residual, best_offset
            ),
        ));
    }
    if pqmf_likelihood > 0.15 {
        evidence.push(Evidence::new(
            "quant",
            "mp3_pqmf",
            pqmf_likelihood,
            thresholds.weight_pqmf,
            "MP3 hybrid filterbank / granule structure detected",
        ));
    }

    QuantResult {
        evidence,
        transcode_likelihood: likelihood,
    }
}

/// Heuristic MP3 PQMF detector: 32-subband boundary discontinuities + granule periodicity.
fn detect_mp3_pqmf(samples: &[f32], sample_rate: u32, thresholds: &Thresholds) -> f64 {
    let boundary_score = pqmf_subband_boundaries(samples, sample_rate, thresholds);
    let granule_score = pqmf_granule_periodicity(samples, thresholds);
    // Require both subband structure and granule periodicity (reduces noise false positives).
    if boundary_score < 0.25 || granule_score < 0.25 {
        return 0.0;
    }
    let combined = boundary_score * 0.55 + granule_score * 0.45;
    if combined >= thresholds.pqmf_likelihood_high {
        thresholds.pqmf_likelihood_high
    } else if combined >= thresholds.pqmf_likelihood_mid {
        thresholds.pqmf_likelihood_mid
    } else if combined > 0.25 {
        combined * 0.6
    } else {
        0.0
    }
}

fn pqmf_subband_boundaries(samples: &[f32], sample_rate: u32, thresholds: &Thresholds) -> f64 {
    let (_, psd) = welch_psd(samples, sample_rate, 4096);
    if psd.len() < 64 {
        return 0.0;
    }
    // Group into 32 pseudo-subbands
    let bands = 32usize;
    let band_size = psd.len() / bands;
    if band_size < 2 {
        return 0.0;
    }
    let mut band_energy = vec![0.0f64; bands];
    for (b, slot) in band_energy.iter_mut().enumerate().take(bands) {
        let lo = b * band_size;
        let hi = ((b + 1) * band_size).min(psd.len());
        *slot = psd[lo..hi].iter().sum();
    }
    let mut max_jump = 0.0f64;
    for w in band_energy.windows(2) {
        let ratio = if w[1] > 1e-30 {
            w[0] / w[1]
        } else if w[0] > 1e-30 {
            thresholds.pqmf_subband_boundary_ratio * 2.0
        } else {
            1.0
        };
        let jump = ratio.max(1.0 / ratio.max(1e-30));
        max_jump = max_jump.max(jump);
    }
    if max_jump >= thresholds.pqmf_subband_boundary_ratio {
        ((max_jump - thresholds.pqmf_subband_boundary_ratio)
            / thresholds.pqmf_subband_boundary_ratio)
            .clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn pqmf_granule_periodicity(samples: &[f32], thresholds: &Thresholds) -> f64 {
    const GRANULE: usize = 1152;
    if samples.len() < GRANULE * 4 {
        return 0.0;
    }
    let mut granule_energies = Vec::new();
    let mut i = GRANULE;
    while i + GRANULE <= samples.len().min(GRANULE * 20) {
        let e: f64 = samples[i..i + GRANULE]
            .iter()
            .map(|&s| (s as f64) * (s as f64))
            .sum::<f64>()
            / GRANULE as f64;
        granule_energies.push(e.sqrt());
        i += GRANULE;
    }
    if granule_energies.len() < 3 {
        return 0.0;
    }
    let mean: f64 = granule_energies.iter().sum::<f64>() / granule_energies.len() as f64;
    if mean < 1e-8 {
        return 0.0;
    }
    let var: f64 = granule_energies
        .iter()
        .map(|e| (e - mean).powi(2))
        .sum::<f64>()
        / granule_energies.len() as f64;
    let cv = var.sqrt() / mean;
    // MP3 granules show moderate energy variation at frame boundaries
    if cv > 0.12 && cv < thresholds.pqmf_granule_energy_cv + 0.15 {
        ((cv - 0.12) / thresholds.pqmf_granule_energy_cv).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn search_quant_residual(samples: &[f32], offsets: &[f64]) -> (f64, f64) {
    let mut best_r = f64::MAX;
    let mut best_off = 0.0;
    for &off in offsets {
        let r = mdct_quant_residual_energy(samples, off);
        if r < best_r {
            best_r = r;
            best_off = off;
        }
    }
    (best_r, best_off)
}

fn mdct_quant_residual_energy(samples: &[f32], scalefactor_offset: f64) -> f64 {
    const N: usize = 1024;
    let start = samples.len() / 4;
    if start + N > samples.len() {
        return f64::MAX;
    }
    let block = &samples[start..start + N];
    let coeffs = type_iv_mdct(block);
    let mut residual_sum = 0.0f64;
    for &c in &coeffs {
        let scaled = c.abs().powf(0.75) * (2.0_f64.powf(scalefactor_offset / 4.0));
        let rounded = scaled.round();
        let err = scaled - rounded;
        residual_sum += err * err;
    }
    residual_sum / coeffs.len() as f64
}

fn type_iv_mdct(x: &[f32]) -> Vec<f64> {
    let n = x.len();
    let mut out = vec![0.0f64; n];
    let pi_n = std::f64::consts::PI / n as f64;
    for (k, out_k) in out.iter_mut().enumerate().take(n) {
        let mut sum = 0.0f64;
        for (i, &xi) in x.iter().enumerate() {
            let phase = pi_n * (i as f64 + 0.5 + n as f64 / 2.0) * (k as f64 + 0.5);
            sum += xi as f64 * phase.cos();
        }
        *out_k = sum * 2.0 / n as f64;
    }
    out
}

fn score_residual(residual: f64, thresholds: &Thresholds) -> f64 {
    if residual < thresholds.quant_residual_high {
        thresholds.quant_likelihood_high
    } else if residual < thresholds.quant_residual_mid {
        thresholds.quant_likelihood_mid
    } else if residual < thresholds.quant_residual_low {
        thresholds.quant_likelihood_low
    } else {
        0.05
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lossless_scan_core::PcmBuffer;

    #[test]
    fn quant_residual_runs() {
        let n = 44100;
        let samples: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        let pcm = PcmBuffer {
            samples,
            sample_rate: 44100,
            channels: 1,
            bits_per_sample: Some(16),
        };
        let r = analyze(&pcm, QuantSearchDepth::Coarse, &Thresholds::default());
        assert!(r.evidence.is_empty() || r.transcode_likelihood >= 0.0);
    }
}
