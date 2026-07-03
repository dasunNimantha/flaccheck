//! Tier 2: Quantization residual detector (Derrien JAES 2019).
//! AAC MDCT path; MP3 PQMF variant stubbed.

use lossless_scan_core::{Evidence, PcmBuffer};

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

pub fn analyze(pcm: &PcmBuffer, depth: QuantSearchDepth) -> QuantResult {
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
    let likelihood = score_residual(best_residual);

    let mut evidence = Vec::new();
    if likelihood > 0.15 {
        evidence.push(Evidence::new(
            "quant",
            "aac_mdct_residual",
            likelihood,
            1.2,
            format!(
                "quantization rounding residual energy {:.2e} at offset {:.2} (Derrien MDCT)",
                best_residual, best_offset
            ),
        ));
    }

    // MP3 PQMF stub
    evidence.push(Evidence::new(
        "quant",
        "mp3_pqmf",
        0.0,
        0.0,
        "TODO: MP3 PQMF/MDCT hybrid detector not yet calibrated",
    ));

    QuantResult {
        evidence,
        transcode_likelihood: likelihood,
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

/// Simplified AAC-like MDCT block + power-law quantizer residual.
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

/// Pruned Type-IV MDCT (L=1024).
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

/// Lower residual -> more likely transcoded (rounding idempotence property).
fn score_residual(residual: f64) -> f64 {
    // TODO: calibrate against labeled DB (Derrien paper used null FPR at high precision)
    if residual < 1e-4 {
        0.85
    } else if residual < 5e-4 {
        0.6
    } else if residual < 2e-3 {
        0.35
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
        let r = analyze(&pcm, QuantSearchDepth::Coarse);
        assert!(r.evidence.iter().any(|e| e.signal == "mp3_pqmf"));
    }
}
