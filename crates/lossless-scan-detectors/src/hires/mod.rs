//! Tier 4: Fake hi-res detection (Lacroix et al. AES 2015).

use crate::dsp::{db_from_power, welch_psd};
use lossless_scan_core::{Evidence, PcmBuffer, Thresholds};

#[derive(Debug, Clone)]
pub struct HiresResult {
    pub evidence: Vec<Evidence>,
}

pub fn analyze(pcm: &PcmBuffer, thresholds: &Thresholds) -> HiresResult {
    let mut evidence = Vec::new();
    let nyquist = pcm.sample_rate as f64 / 2.0;

    let upsample_score = detect_upsampling(pcm, nyquist, thresholds);
    let resampler_score = detect_resampler_imaging(pcm, nyquist, thresholds);

    let combined_upsample = upsample_score.max(resampler_score * 0.85);

    if combined_upsample > 0.0 {
        evidence.push(Evidence::new(
            "hires",
            "upsampled",
            combined_upsample,
            1.0,
            if combined_upsample > 0.7 {
                format!(
                    "hard cliff near {:.0} Hz with silent floor above (likely upsampled)",
                    detect_cliff_hz(pcm, nyquist)
                )
            } else {
                "mild upsampling / resampler imaging indicators".to_string()
            },
        ));
    }

    if resampler_score > 0.3 {
        evidence.push(Evidence::new(
            "hires",
            "resampler_imaging",
            resampler_score,
            0.8,
            "resampler anti-alias / imaging signature detected",
        ));
    }

    let padded_score = detect_padded_depth(pcm, thresholds);
    if padded_score > 0.0 {
        evidence.push(Evidence::new(
            "hires",
            "padded_depth",
            padded_score,
            1.0,
            if padded_score > 0.7 {
                "16-bit audio padded into 24-bit container".to_string()
            } else {
                "possible bit-depth padding".to_string()
            },
        ));
    }

    if evidence.is_empty() {
        evidence.push(Evidence::new(
            "hires",
            "genuine_hires",
            1.0,
            0.0,
            "no fake hi-res indicators",
        ));
    }

    HiresResult { evidence }
}

fn detect_upsampling(pcm: &PcmBuffer, nyquist: f64, thresholds: &Thresholds) -> f64 {
    let candidates = [22050.0, 24000.0, 44100.0 / 2.0, 48000.0 / 2.0];
    let mono = pcm.left();
    let (freqs, psd) = welch_psd(&mono, pcm.sample_rate, 8192);
    if freqs.is_empty() {
        return 0.0;
    }

    for cliff in candidates {
        if cliff >= nyquist * 0.9 {
            continue;
        }
        let below: f64 = band_energy(&freqs, &psd, cliff - 2000.0, cliff);
        let above: f64 = band_energy(&freqs, &psd, cliff + 500.0, nyquist * 0.95);
        if below < 1e-20 {
            continue;
        }
        let ratio = above / below;
        let floor_db = db_from_power(above.max(1e-30));
        if ratio < thresholds.upsample_energy_ratio_high && floor_db < -55.0 {
            return 0.85;
        }
        if ratio < thresholds.upsample_energy_ratio_mid && floor_db < -45.0 {
            return 0.55;
        }
    }
    0.0
}

/// Detect resampler imaging: mirrored energy lobes above source Nyquist.
fn detect_resampler_imaging(pcm: &PcmBuffer, nyquist: f64, thresholds: &Thresholds) -> f64 {
    if pcm.sample_rate <= 48000 {
        return 0.0;
    }
    let mono = pcm.left();
    let (freqs, psd) = welch_psd(&mono, pcm.sample_rate, 8192);
    if freqs.is_empty() {
        return 0.0;
    }
    // Source band likely 44.1/48 kHz — look for imaging between 22-24 kHz and Nyquist
    let source_nyquist = 22050.0f64.max(24000.0);
    if source_nyquist >= nyquist * 0.9 {
        return 0.0;
    }
    let source_band: f64 = band_energy(&freqs, &psd, source_nyquist - 2000.0, source_nyquist);
    let image_band: f64 = band_energy(
        &freqs,
        &psd,
        source_nyquist + 500.0,
        (source_nyquist * 1.15).min(nyquist * 0.95),
    );
    if source_band < 1e-20 {
        return 0.0;
    }
    let ratio = image_band / source_band;
    if ratio > thresholds.resampler_imaging_ratio {
        ((ratio - thresholds.resampler_imaging_ratio) / thresholds.resampler_imaging_ratio)
            .clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn detect_cliff_hz(pcm: &PcmBuffer, nyquist: f64) -> f64 {
    let mono = pcm.left();
    let (freqs, psd) = welch_psd(&mono, pcm.sample_rate, 8192);
    let mut best_f = nyquist;
    let mut best_drop = 0.0;
    for i in 1..freqs.len().saturating_sub(1) {
        if freqs[i] < 10000.0 || freqs[i] > nyquist * 0.95 {
            continue;
        }
        let drop = psd[i - 1] / psd[i].max(1e-30);
        if drop > best_drop {
            best_drop = drop;
            best_f = freqs[i];
        }
    }
    best_f
}

fn band_energy(freqs: &[f64], psd: &[f64], lo: f64, hi: f64) -> f64 {
    freqs
        .iter()
        .zip(psd.iter())
        .filter(|(f, _)| **f >= lo && **f < hi)
        .map(|(_, p)| p)
        .sum()
}

fn detect_padded_depth(pcm: &PcmBuffer, thresholds: &Thresholds) -> f64 {
    let declared = pcm.bits_per_sample.unwrap_or(16);
    if declared < 24 {
        return 0.0;
    }
    let mono = pcm.left();
    if mono.is_empty() {
        return 0.0;
    }
    let scale = 32768.0f32;
    let step = (mono.len() / 5000).max(1);
    let mut on_grid = 0usize;
    for s in mono.iter().step_by(step) {
        let q = (*s * scale).round() / scale;
        if (q - *s).abs() < 1.0 / (scale * 4.0) {
            on_grid += 1;
        }
    }
    let total = mono.len() / step;
    let grid_ratio = on_grid as f64 / total.max(1) as f64;
    let mut lsb_zeros = 0usize;
    for s in mono.iter().step_by(step) {
        let v = ((*s * 8388608.0).round() as i32) & 0xFF;
        if v == 0 {
            lsb_zeros += 1;
        }
    }
    let lsb_zero_ratio = lsb_zeros as f64 / total.max(1) as f64;

    if grid_ratio > thresholds.padded_grid_ratio_high
        && lsb_zero_ratio > thresholds.padded_lsb_zero_ratio_high
    {
        0.9
    } else if grid_ratio > 0.85 && lsb_zero_ratio > 0.7 {
        0.5
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lossless_scan_core::PcmBuffer;

    #[test]
    fn detects_padded_24bit() {
        let n = 44100;
        let samples: Vec<f32> = (0..n).map(|i| (i % 1000) as f32 / 32768.0).collect();
        let pcm = PcmBuffer {
            samples,
            sample_rate: 44100,
            channels: 1,
            bits_per_sample: Some(24),
        };
        let r = analyze(&pcm, &Thresholds::default());
        assert!(r
            .evidence
            .iter()
            .any(|e| e.signal == "padded_depth" && e.value > 0.5));
    }
}
