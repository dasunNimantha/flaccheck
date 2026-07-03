//! Tier 1: Spectral / bitrate fingerprint (D'Alessandro & Shi 2009).

use crate::dsp::{average_psd, db_from_power};
use lossless_scan_core::{Evidence, PcmBuffer, Thresholds};

const MP3_CUTOFFS_KHZ: &[(u32, f64)] = &[
    (128, 16.0),
    (160, 17.5),
    (192, 19.0),
    (256, 19.5),
    (320, 20.0),
];

const AAC_CUTOFFS_KHZ: &[(u32, f64)] = &[(96, 15.5), (128, 17.0), (192, 19.0), (256, 20.0)];

const OPUS_EDGES_HZ: &[(u32, f64)] = &[(48, 8000.0), (64, 12000.0), (96, 12000.0), (128, 20000.0)];

#[derive(Debug, Clone)]
pub struct SpectralResult {
    pub evidence: Vec<Evidence>,
    pub cutoff_hz: f64,
    pub rolloff_hz: f64,
    pub edge_hz: f64,
    pub rolloff_steepness_db_per_oct: f64,
    pub full_band: bool,
    pub spectral_info_score: f64,
    pub codec_guess: Option<String>,
    pub est_bitrate_kbps: Option<u32>,
    pub codec_certainty: f64,
    pub suspicion: f64,
}

pub fn analyze(
    pcm: &PcmBuffer,
    window_secs: f64,
    window_count: usize,
    thresholds: &Thresholds,
) -> SpectralResult {
    let windows = if window_count > 0 {
        pcm.analysis_windows(window_secs, window_count)
    } else {
        vec![pcm.clone()]
    };

    let mono_windows: Vec<Vec<f32>> = windows.iter().map(|w| w.left()).collect();
    let rms: Vec<f32> = mono_windows.iter().map(|w| rms_of(w)).collect();
    let max_rms = rms.iter().copied().fold(0.0f32, f32::max);
    let gate = max_rms * 0.15;
    let refs: Vec<&[f32]> = mono_windows
        .iter()
        .zip(rms.iter())
        .filter(|(_, r)| max_rms <= 1e-6 || **r >= gate)
        .map(|(w, _)| w.as_slice())
        .collect();
    let refs: Vec<&[f32]> = if refs.is_empty() {
        mono_windows.iter().map(|v| v.as_slice()).collect()
    } else {
        refs
    };

    let fft_size = 4096usize;
    let (freqs, psd) = average_psd(&refs, pcm.sample_rate, fft_size);

    if freqs.is_empty() {
        return empty_result("no spectral data");
    }

    let nyquist = pcm.sample_rate as f64 / 2.0;
    let psd_db: Vec<f64> = psd.iter().map(|p| db_from_power(*p)).collect();
    let psd_db = smooth_db(&psd_db, 5);

    let lossy_gate = pcm.sample_rate <= thresholds.lossy_codec_max_sample_rate_hz;

    let (drop_cutoff, drop_strength) = if lossy_gate {
        detect_cutoff(&freqs, &psd_db, nyquist, thresholds)
    } else {
        (nyquist, 0.0)
    };
    let (deriv_cutoff, deriv_strength) = if lossy_gate {
        detect_derivative_cutoff(&freqs, &psd_db, nyquist, thresholds)
    } else {
        (nyquist, 0.0)
    };

    let cutoff_hz = if deriv_strength > drop_strength {
        deriv_cutoff
    } else {
        drop_cutoff
    };
    let brick_wall_strength = drop_strength.max(deriv_strength);

    let rolloff_hz = spectral_rolloff(&freqs, &psd, 0.95);
    let info_score = spectral_info_score(&freqs, &psd, nyquist);
    let edge_hz = detect_spectral_edge(&freqs, &psd_db, nyquist, thresholds);
    let steepness = rolloff_steepness(&freqs, &psd_db, cutoff_hz, nyquist);

    let steepness_boost = if steepness >= thresholds.steepness_codec_db_per_oct {
        ((steepness - thresholds.steepness_codec_db_per_oct)
            / thresholds.steepness_codec_db_per_oct)
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let combined_brick = (brick_wall_strength + steepness_boost * 0.4).clamp(0.0, 1.0);

    let full_band =
        edge_hz >= nyquist * thresholds.full_band_nyquist_ratio && combined_brick < 0.35;

    let shelf_score = if lossy_gate {
        detect_aac_shelf(&freqs, &psd_db, cutoff_hz, thresholds)
    } else {
        0.0
    };
    let sbr_score = if lossy_gate {
        detect_sbr_mirroring(&freqs, &psd, nyquist, thresholds)
    } else {
        0.0
    };

    let (codec_guess, est_bitrate, codec_certainty) = if lossy_gate {
        match_codec_signature(
            cutoff_hz,
            combined_brick,
            steepness,
            shelf_score,
            &freqs,
            &psd_db,
            nyquist,
            thresholds,
        )
    } else {
        (None, None, 0.0)
    };

    let mut suspicion = combined_brick * thresholds.weight_brick_wall
        + codec_certainty * thresholds.weight_codec_certainty
        + shelf_score * thresholds.weight_aac_shelf
        + sbr_score * thresholds.weight_sbr
        + steepness_boost * thresholds.weight_steepness;
    if combined_brick < 0.35 && rolloff_hz < nyquist * 0.75 && info_score < 0.35 {
        suspicion = suspicion.max(0.45);
    }
    suspicion = suspicion.clamp(0.0, 1.0);

    let mut evidence = vec![
        Evidence::new(
            "spectral",
            "spectral_info_score",
            info_score,
            0.0,
            format!("HF content score {:.2}", info_score),
        ),
        Evidence::new(
            "spectral",
            "cutoff_hz",
            cutoff_hz,
            0.2,
            format!("detected cutoff {:.0} Hz", cutoff_hz),
        ),
        Evidence::new(
            "spectral",
            "brick_wall",
            combined_brick,
            thresholds.weight_brick_wall,
            if combined_brick > 0.5 {
                format!("sharp spectral cliff near {:.0} Hz", cutoff_hz)
            } else {
                "no strong brick wall".to_string()
            },
        ),
        Evidence::new(
            "spectral",
            "rolloff_steepness",
            steepness,
            thresholds.weight_steepness,
            format!("rolloff steepness {:.1} dB/oct", steepness),
        ),
        Evidence::new(
            "spectral",
            "rolloff_hz",
            rolloff_hz,
            0.1,
            format!("95% energy rolloff at {:.0} Hz", rolloff_hz),
        ),
        Evidence::new(
            "spectral",
            "edge_hz",
            edge_hz,
            0.1,
            format!("content extends to {:.0} Hz", edge_hz),
        ),
    ];

    if full_band {
        evidence.push(Evidence::new(
            "spectral",
            "full_band",
            1.0,
            0.0,
            format!(
                "content reaches {:.0} Hz (~Nyquist); no lossy band-limit",
                edge_hz
            ),
        ));
    } else if combined_brick < 0.35 && edge_hz > 0.0 && edge_hz < nyquist * 0.75 {
        let deficit = ((nyquist * 0.85 - edge_hz) / (nyquist * 0.85)).clamp(0.0, 1.0);
        evidence.push(Evidence::new(
            "spectral",
            "early_rolloff",
            0.3 * deficit,
            0.4,
            format!("gradual rolloff, content ends near {:.0} Hz", edge_hz),
        ));
    }

    if codec_certainty > 0.3 {
        if let Some(ref codec) = codec_guess {
            evidence.push(Evidence::new(
                "spectral",
                "codec_guess",
                codec_certainty,
                thresholds.weight_codec_certainty,
                codec,
            ));
        }
        if let Some(br) = est_bitrate {
            evidence.push(Evidence::new(
                "spectral",
                "est_bitrate_kbps",
                br as f64,
                0.2,
                format!("estimated source {} kbps", br),
            ));
        }
    }
    if shelf_score > 0.3 {
        evidence.push(Evidence::new(
            "spectral",
            "aac_shelf",
            shelf_score,
            thresholds.weight_aac_shelf,
            "AAC-like shelf before cutoff",
        ));
        suspicion = suspicion.max(shelf_score * 0.8);
    }
    if sbr_score > 0.3 {
        evidence.push(Evidence::new(
            "spectral",
            "sbr_mirroring",
            sbr_score,
            thresholds.weight_sbr,
            "SBR-like HF mirroring of low band",
        ));
    }

    SpectralResult {
        evidence,
        cutoff_hz,
        rolloff_hz,
        edge_hz,
        rolloff_steepness_db_per_oct: steepness,
        full_band,
        spectral_info_score: info_score,
        codec_guess,
        est_bitrate_kbps: est_bitrate,
        codec_certainty,
        suspicion,
    }
}

fn empty_result(msg: &str) -> SpectralResult {
    SpectralResult {
        evidence: vec![Evidence::new(
            "spectral",
            "spectral_info_score",
            0.0,
            0.0,
            msg,
        )],
        cutoff_hz: 0.0,
        rolloff_hz: 0.0,
        edge_hz: 0.0,
        rolloff_steepness_db_per_oct: 0.0,
        full_band: false,
        spectral_info_score: 0.0,
        codec_guess: None,
        est_bitrate_kbps: None,
        codec_certainty: 0.0,
        suspicion: 0.0,
    }
}

fn detect_spectral_edge(
    freqs: &[f64],
    psd_db: &[f64],
    nyquist: f64,
    thresholds: &Thresholds,
) -> f64 {
    if freqs.len() < 8 {
        return 0.0;
    }
    let peak_db = psd_db
        .iter()
        .zip(freqs.iter())
        .filter(|(_, f)| **f < nyquist * 0.98)
        .map(|(p, _)| *p)
        .fold(f64::MIN, |a, b| a.max(b));
    let threshold = peak_db - thresholds.spectral_edge_dynamic_range_db;
    let run = 3usize;
    let mut i = freqs.len().saturating_sub(1);
    while i >= run {
        let above = (i - run + 1..=i).all(|j| psd_db[j] > threshold);
        if above {
            return freqs[i];
        }
        i -= 1;
    }
    0.0
}

fn rolloff_steepness(freqs: &[f64], psd_db: &[f64], cutoff_hz: f64, nyquist: f64) -> f64 {
    if cutoff_hz < 8000.0 || cutoff_hz >= nyquist * 0.98 {
        return 0.0;
    }
    let f_lo = cutoff_hz * 0.7;
    let f_hi = cutoff_hz.min(nyquist * 0.98);
    let mut lo_db = None;
    let mut hi_db = None;
    for (f, db) in freqs.iter().zip(psd_db.iter()) {
        if (*f - f_lo).abs() < 300.0 {
            lo_db = Some(*db);
        }
        if (*f - f_hi).abs() < 300.0 {
            hi_db = Some(*db);
        }
    }
    let (lo, hi) = match (lo_db, hi_db) {
        (Some(l), Some(h)) => (l, h),
        _ => return 0.0,
    };
    let octaves = (f_hi / f_lo).log2();
    if octaves <= 0.1 {
        return 0.0;
    }
    (lo - hi) / octaves
}

fn detect_derivative_cutoff(
    freqs: &[f64],
    psd_db: &[f64],
    nyquist: f64,
    thresholds: &Thresholds,
) -> (f64, f64) {
    let mut best_cutoff = nyquist;
    let mut best_strength = 0.0f64;
    for i in 8..freqs.len().saturating_sub(8) {
        let f = freqs[i];
        if f < 10000.0 || f > nyquist * 0.98 {
            continue;
        }
        let deriv = psd_db[i] - psd_db[i - 1];
        let drop = -deriv;
        if drop > thresholds.derivative_drop_db {
            let above = &psd_db[i + 1..(i + 8).min(psd_db.len())];
            if silent_floor_above(above) {
                let strength = ((drop - thresholds.derivative_drop_db)
                    / thresholds.brick_wall_strength_divisor)
                    .clamp(0.0, 1.0);
                if strength > best_strength {
                    best_strength = strength;
                    best_cutoff = f;
                }
            }
        }
    }
    (best_cutoff, best_strength)
}

fn detect_cutoff(
    freqs: &[f64],
    psd_db: &[f64],
    nyquist: f64,
    thresholds: &Thresholds,
) -> (f64, f64) {
    let mut best_cutoff = nyquist;
    let mut best_strength = 0.0f64;
    let start_idx = freqs.iter().position(|&f| f >= 8000.0).unwrap_or(0);
    let end_idx = freqs
        .iter()
        .position(|&f| f >= nyquist * 0.98)
        .unwrap_or(freqs.len());

    for i in start_idx..end_idx.saturating_sub(16) {
        let f = freqs[i];
        if f < 10000.0 {
            continue;
        }
        let below_mean: f64 = psd_db[i.saturating_sub(8)..i].iter().sum::<f64>() / 8.0;
        let above_slice = &psd_db[i + 1..(i + 16).min(psd_db.len())];
        if above_slice.len() < 8 {
            continue;
        }
        let above_mean: f64 = above_slice.iter().sum::<f64>() / above_slice.len() as f64;
        let drop = below_mean - above_mean;
        if drop > thresholds.brick_wall_drop_db && silent_floor_above(above_slice) {
            let strength = ((drop - thresholds.brick_wall_drop_db)
                / thresholds.brick_wall_strength_divisor)
                .clamp(0.0, 1.0);
            if strength > best_strength {
                best_strength = strength;
                best_cutoff = f;
            }
        }
    }
    (best_cutoff, best_strength)
}

fn silent_floor_above(psd_db_above: &[f64]) -> bool {
    if psd_db_above.len() < 4 {
        return false;
    }
    let mean = psd_db_above.iter().sum::<f64>() / psd_db_above.len() as f64;
    let var =
        psd_db_above.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / psd_db_above.len() as f64;
    mean < -42.0 && var.sqrt() < 4.0
}

fn detect_aac_shelf(freqs: &[f64], psd_db: &[f64], cutoff_hz: f64, thresholds: &Thresholds) -> f64 {
    if cutoff_hz < 12000.0 {
        return 0.0;
    }
    let shelf_lo = cutoff_hz - 3000.0;
    let shelf_hi = cutoff_hz;
    let mut shelf_vals = Vec::new();
    let mut above_vals = Vec::new();
    for (f, db) in freqs.iter().zip(psd_db.iter()) {
        if *f >= shelf_lo && *f < shelf_hi {
            shelf_vals.push(*db);
        } else if *f >= shelf_hi && *f < shelf_hi + 2000.0 {
            above_vals.push(*db);
        }
    }
    if shelf_vals.is_empty() || above_vals.is_empty() {
        return 0.0;
    }
    let shelf_mean: f64 = shelf_vals.iter().sum::<f64>() / shelf_vals.len() as f64;
    let above_mean: f64 = above_vals.iter().sum::<f64>() / above_vals.len() as f64;
    let drop = shelf_mean - above_mean;
    if drop > thresholds.aac_shelf_drop_min_db && drop < thresholds.aac_shelf_drop_max_db {
        ((drop - thresholds.aac_shelf_drop_min_db) / 20.0).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Cross-correlate low band energy pattern with HF band (SBR heuristic).
fn detect_sbr_mirroring(freqs: &[f64], psd: &[f64], nyquist: f64, thresholds: &Thresholds) -> f64 {
    let lo_lo = 4000.0;
    let lo_hi = 8000.0;
    let hf_lo = 12000.0;
    let hf_hi = nyquist * 0.95;

    let lo: Vec<f64> = freqs
        .iter()
        .zip(psd.iter())
        .filter(|(f, _)| **f >= lo_lo && **f < lo_hi)
        .map(|(_, p)| *p)
        .collect();
    let hf: Vec<f64> = freqs
        .iter()
        .zip(psd.iter())
        .filter(|(f, _)| **f >= hf_lo && **f < hf_hi)
        .map(|(_, p)| *p)
        .collect();
    if lo.len() < 8 || hf.len() < 8 {
        return 0.0;
    }

    let n = lo.len().min(hf.len());
    let lo = &lo[..n];
    let hf = &hf[..n];
    let mean_lo: f64 = lo.iter().sum::<f64>() / n as f64;
    let mean_hf: f64 = hf.iter().sum::<f64>() / n as f64;
    let mut num = 0.0;
    let mut den_lo = 0.0;
    let mut den_hf = 0.0;
    for i in 0..n {
        let dl = lo[i] - mean_lo;
        let dh = hf[i] - mean_hf;
        num += dl * dh;
        den_lo += dl * dl;
        den_hf += dh * dh;
    }
    let corr = if den_lo > 0.0 && den_hf > 0.0 {
        num / (den_lo * den_hf).sqrt()
    } else {
        0.0
    };
    if corr >= thresholds.sbr_correlation_min {
        ((corr - thresholds.sbr_correlation_min) / (1.0 - thresholds.sbr_correlation_min))
            .clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[allow(clippy::too_many_arguments)]
fn match_codec_signature(
    cutoff_hz: f64,
    brick_strength: f64,
    steepness: f64,
    shelf_score: f64,
    _freqs: &[f64],
    _psd_db: &[f64],
    _nyquist: f64,
    thresholds: &Thresholds,
) -> (Option<String>, Option<u32>, f64) {
    if brick_strength < 0.25 && steepness < thresholds.steepness_codec_db_per_oct * 0.8 {
        let vorbis = detect_vorbis_soft_rolloff(steepness, thresholds);
        if vorbis > 0.3 {
            return (
                Some("Vorbis (soft rolloff)".to_string()),
                None,
                vorbis * 0.7,
            );
        }
        return (None, None, 0.0);
    }

    let cutoff_khz = cutoff_hz / 1000.0;

    // MP3
    let mut best_mp3 = (0u32, f64::MAX, 0.0f64);
    for &(br, cf) in MP3_CUTOFFS_KHZ {
        let d = (cutoff_khz - cf).abs();
        if d < best_mp3.1 {
            let score = if d < thresholds.mp3_codec_match_tolerance_khz {
                1.0 - d / thresholds.mp3_codec_match_tolerance_khz
            } else {
                0.0
            };
            best_mp3 = (br, d, score);
        }
    }

    // AAC
    let mut best_aac = (0u32, f64::MAX, 0.0f64);
    for &(br, cf) in AAC_CUTOFFS_KHZ {
        let d = (cutoff_khz - cf).abs();
        if d < best_aac.1 {
            let score = if d < thresholds.mp3_codec_match_tolerance_khz {
                (1.0 - d / thresholds.mp3_codec_match_tolerance_khz) * (0.7 + shelf_score * 0.3)
            } else {
                0.0
            };
            best_aac = (br, d, score);
        }
    }

    // Opus hard edges
    let mut best_opus = (0u32, f64::MAX, 0.0f64);
    for &(br, edge) in OPUS_EDGES_HZ {
        let d = (cutoff_hz - edge).abs();
        if d < best_opus.1 {
            let score = if d < thresholds.opus_edge_tolerance_hz {
                1.0 - d / thresholds.opus_edge_tolerance_hz
            } else {
                0.0
            };
            best_opus = (br, d, score);
        }
    }

    let mp3_score = best_mp3.2 * brick_strength.max(0.4);
    let aac_score = best_aac.2 * (brick_strength.max(0.3) + shelf_score * 0.5).min(1.0);
    let opus_score = best_opus.2 * brick_strength.max(0.5);

    if mp3_score >= aac_score && mp3_score >= opus_score && mp3_score > 0.3 {
        (
            Some(format!("MP3 ~{} kbps", best_mp3.0)),
            Some(best_mp3.0),
            mp3_score,
        )
    } else if aac_score >= opus_score && aac_score > 0.3 {
        (
            Some(format!("AAC ~{} kbps", best_aac.0)),
            Some(best_aac.0),
            aac_score,
        )
    } else if opus_score > 0.3 {
        (
            Some(format!("Opus ~{} kbps", best_opus.0)),
            Some(best_opus.0),
            opus_score,
        )
    } else {
        let vorbis = detect_vorbis_soft_rolloff(steepness, thresholds);
        if vorbis > 0.35 {
            (
                Some("Vorbis (soft rolloff)".to_string()),
                None,
                vorbis * 0.6,
            )
        } else {
            (None, None, 0.0)
        }
    }
}

fn detect_vorbis_soft_rolloff(steepness: f64, thresholds: &Thresholds) -> f64 {
    if steepness > thresholds.steepness_gentle_db_per_oct
        && steepness < thresholds.vorbis_rolloff_max_db_per_oct
    {
        ((steepness - thresholds.steepness_gentle_db_per_oct)
            / (thresholds.vorbis_rolloff_max_db_per_oct - thresholds.steepness_gentle_db_per_oct))
            .clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn rms_of(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

fn smooth_db(psd_db: &[f64], win: usize) -> Vec<f64> {
    if psd_db.is_empty() || win <= 1 {
        return psd_db.to_vec();
    }
    let half = win / 2;
    psd_db
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(psd_db.len());
            psd_db[lo..hi].iter().sum::<f64>() / (hi - lo) as f64
        })
        .collect()
}

fn spectral_rolloff(freqs: &[f64], psd: &[f64], percentile: f64) -> f64 {
    let total: f64 = psd.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let mut cum = 0.0;
    for (f, p) in freqs.iter().zip(psd.iter()) {
        cum += p;
        if cum / total >= percentile {
            return *f;
        }
    }
    freqs.last().copied().unwrap_or(0.0)
}

fn spectral_info_score(freqs: &[f64], psd: &[f64], nyquist: f64) -> f64 {
    let bands = [(12000.0, 16000.0), (16000.0, 20000.0), (20000.0, nyquist)];
    let total: f64 = psd.iter().copied().sum::<f64>().max(1e-20);
    let mut hf = 0.0;
    for (lo, hi) in bands {
        for (f, p) in freqs.iter().zip(psd.iter()) {
            if *f >= lo && *f < hi {
                hf += p;
            }
        }
    }
    (hf / total).clamp(0.0, 1.0) * 2.0_f64.min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lossless_scan_core::{PcmBuffer, Thresholds};

    fn synth_fullband(sr: u32, secs: f64) -> PcmBuffer {
        let n = (sr as f64 * secs) as usize;
        let samples: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sr as f32;
                (2.0 * std::f32::consts::PI * 5000.0 * t).sin() * 0.3
                    + (2.0 * std::f32::consts::PI * 15000.0 * t).sin() * 0.2
            })
            .collect();
        PcmBuffer {
            samples,
            sample_rate: sr,
            channels: 1,
            bits_per_sample: Some(16),
        }
    }

    #[test]
    fn fullband_has_hf_info() {
        let pcm = synth_fullband(44100, 2.0);
        let r = analyze(&pcm, 1.0, 1, &Thresholds::default());
        assert!(r.spectral_info_score > 0.05);
    }

    #[test]
    fn wideband_noise_reads_full_band() {
        let n = 44100 * 2;
        let mut state = 0x1234_5678u32;
        let samples: Vec<f32> = (0..n)
            .map(|_| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                (state as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect();
        let pcm = PcmBuffer {
            samples,
            sample_rate: 44100,
            channels: 1,
            bits_per_sample: Some(16),
        };
        let r = analyze(&pcm, 1.0, 1, &Thresholds::default());
        assert!(r.full_band);
        assert!(r.edge_hz > 44100.0 / 2.0 * 0.9);
    }

    #[test]
    fn edge_ignores_bass_only_energy() {
        let sr = 44100u32;
        let n = sr as usize * 2;
        let samples: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sr as f32;
                (2.0 * std::f32::consts::PI * 200.0 * t).sin() * 0.5
                    + (2.0 * std::f32::consts::PI * 18000.0 * t).sin() * 0.02
            })
            .collect();
        let pcm = PcmBuffer {
            samples,
            sample_rate: sr,
            channels: 1,
            bits_per_sample: Some(16),
        };
        let r = analyze(&pcm, 1.0, 1, &Thresholds::default());
        assert!(r.edge_hz > 15000.0);
        assert!(r.rolloff_hz < 5000.0);
    }
}
