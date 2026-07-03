//! Tier 1: Spectral / bitrate fingerprint (D'Alessandro & Shi 2009).

use crate::dsp::{average_psd, db_from_power};
use lossless_scan_core::{Evidence, PcmBuffer};

const MP3_CUTOFFS_KHZ: &[(u32, f64)] = &[
    (128, 16.0),
    (160, 17.5),
    (192, 19.0),
    (256, 19.5),
    (320, 20.0),
];

#[derive(Debug, Clone)]
pub struct SpectralResult {
    pub evidence: Vec<Evidence>,
    pub cutoff_hz: f64,
    pub rolloff_hz: f64,
    /// Highest frequency with real content above the treble noise floor (Hz).
    pub edge_hz: f64,
    /// True when content extends close to Nyquist (no lossy band-limiting).
    pub full_band: bool,
    pub spectral_info_score: f64,
    pub codec_guess: Option<String>,
    pub est_bitrate_kbps: Option<u32>,
    pub suspicion: f64,
}

pub fn analyze(pcm: &PcmBuffer, window_secs: f64, window_count: usize) -> SpectralResult {
    let windows = if window_count > 0 {
        pcm.analysis_windows(window_secs, window_count)
    } else {
        vec![pcm.clone()]
    };

    let mono_windows: Vec<Vec<f32>> = windows.iter().map(|w| w.left()).collect();

    // Skip near-silent windows (intros, fades, gaps) so they don't dilute the
    // averaged PSD — lossy cutoffs and true bandwidth show up in loud passages.
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
        return SpectralResult {
            evidence: vec![Evidence::new(
                "spectral",
                "spectral_info_score",
                0.0,
                0.0,
                "no spectral data",
            )],
            cutoff_hz: 0.0,
            rolloff_hz: 0.0,
            edge_hz: 0.0,
            full_band: false,
            spectral_info_score: 0.0,
            codec_guess: None,
            est_bitrate_kbps: None,
            suspicion: 0.0,
        };
    }

    let nyquist = pcm.sample_rate as f64 / 2.0;
    let psd_db: Vec<f64> = psd.iter().map(|p| db_from_power(*p)).collect();
    let psd_db = smooth_db(&psd_db, 5);

    let (cutoff_hz, brick_wall_strength) = detect_cutoff(&freqs, &psd_db, nyquist);
    let rolloff_hz = spectral_rolloff(&freqs, &psd, 0.95);
    let info_score = spectral_info_score(&freqs, &psd, nyquist);

    // Noise-floor spectral edge: highest frequency with real content above the
    // treble noise floor. Robust to bass-heavy masters where 95%-energy rolloff
    // sits at a few kHz even though content reaches Nyquist.
    let edge_hz = detect_spectral_edge(&freqs, &psd_db, nyquist);
    // Nyquist-aware: content within ~8% of Nyquist means no lossy band-limiting.
    let full_band = edge_hz >= nyquist * 0.92 && brick_wall_strength < 0.35;

    let (codec_guess, est_bitrate, codec_match) =
        match_mp3_signature(cutoff_hz, brick_wall_strength);
    let shelf_score = detect_aac_shelf(&freqs, &psd_db, cutoff_hz);

    let mut suspicion = brick_wall_strength * 0.5 + codec_match * 0.3 + shelf_score * 0.2;
    if brick_wall_strength < 0.35 && rolloff_hz < nyquist * 0.75 && info_score < 0.35 {
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
            brick_wall_strength,
            1.0,
            if brick_wall_strength > 0.5 {
                format!("sharp spectral cliff near {:.0} Hz", cutoff_hz)
            } else {
                "no strong brick wall".to_string()
            },
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
        // Positive evidence: real content reaches near Nyquist with no cliff.
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
    } else if brick_wall_strength < 0.35 && edge_hz > 0.0 && edge_hz < nyquist * 0.75 {
        // Soft early rolloff without a cliff — ambiguous, not proof of transcoding.
        let deficit = ((nyquist * 0.85 - edge_hz) / (nyquist * 0.85)).clamp(0.0, 1.0);
        evidence.push(Evidence::new(
            "spectral",
            "early_rolloff",
            0.3 * deficit,
            0.4,
            format!("gradual rolloff, content ends near {:.0} Hz", edge_hz),
        ));
    }

    if let Some(ref codec) = codec_guess {
        evidence.push(Evidence::new("spectral", "codec_guess", 1.0, 0.3, codec));
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
    if shelf_score > 0.3 {
        evidence.push(Evidence::new(
            "spectral",
            "aac_shelf",
            shelf_score,
            0.4,
            "AAC-like shelf before cutoff",
        ));
        suspicion = suspicion.max(shelf_score * 0.8);
    }

    SpectralResult {
        evidence,
        cutoff_hz,
        rolloff_hz,
        edge_hz,
        full_band,
        spectral_info_score: info_score,
        codec_guess,
        est_bitrate_kbps: est_bitrate,
        suspicion,
    }
}

/// Highest frequency carrying real content above the treble noise floor.
///
/// Uses a noise floor estimated from the upper half of the spectrum (the treble
/// region), then finds the highest frequency where a short run of bins sits
/// clearly above that floor. This is robust to bass-heavy material and avoids
/// the "ratio trap" — it measures absolute content bandwidth, not energy share.
fn detect_spectral_edge(freqs: &[f64], psd_db: &[f64], nyquist: f64) -> f64 {
    if freqs.len() < 8 {
        return 0.0;
    }

    let peak_db = psd_db
        .iter()
        .zip(freqs.iter())
        .filter(|(_, f)| **f < nyquist * 0.98)
        .map(|(p, _)| *p)
        .fold(f64::MIN, |a, b| a.max(b));

    // Content is present where the PSD is within a sane dynamic range of the
    // overall peak. 72 dB keeps faint but real HF content, yet excludes the
    // near-silent floor a lossy encoder leaves above its cutoff. Scale-invariant
    // and works on flat spectra (white noise) where a noise-floor-relative
    // threshold would collapse.
    let threshold = peak_db - 72.0;

    // Scan downward from Nyquist; require a run of bins above threshold.
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

fn detect_cutoff(freqs: &[f64], psd_db: &[f64], nyquist: f64) -> (f64, f64) {
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
        if drop > 20.0 && silent_floor_above(above_slice) {
            let strength = ((drop - 20.0) / 35.0).clamp(0.0, 1.0);
            if strength > best_strength {
                best_strength = strength;
                best_cutoff = f;
            }
        }
    }

    (best_cutoff, best_strength)
}

/// Lossy codecs leave a flat, near-silent floor above the cliff — unlike analog rolloff or noise.
fn silent_floor_above(psd_db_above: &[f64]) -> bool {
    if psd_db_above.len() < 4 {
        return false;
    }
    let mean = psd_db_above.iter().sum::<f64>() / psd_db_above.len() as f64;
    let var =
        psd_db_above.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / psd_db_above.len() as f64;
    let std = var.sqrt();
    mean < -42.0 && std < 4.0
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

fn match_mp3_signature(cutoff_hz: f64, brick_strength: f64) -> (Option<String>, Option<u32>, f64) {
    if brick_strength < 0.35 {
        return (None, None, 0.0);
    }
    let cutoff_khz = cutoff_hz / 1000.0;
    let mut best_br = 0u32;
    let mut best_dist = f64::MAX;
    for &(br, cf) in MP3_CUTOFFS_KHZ {
        let d = (cutoff_khz - cf).abs();
        if d < best_dist {
            best_dist = d;
            best_br = br;
        }
    }
    let match_score = if best_dist < 1.5 {
        1.0 - best_dist / 1.5
    } else {
        0.0
    };
    if match_score > 0.3 {
        (
            Some(format!("MP3 ~{} kbps", best_br)),
            Some(best_br),
            match_score,
        )
    } else {
        (None, None, 0.0)
    }
}

fn detect_aac_shelf(freqs: &[f64], psd_db: &[f64], cutoff_hz: f64) -> f64 {
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
    if drop > 8.0 && drop < 35.0 {
        ((drop - 8.0) / 20.0).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lossless_scan_core::PcmBuffer;

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

    fn lowpass(samples: &[f32], sr: u32, cutoff: f64) -> Vec<f32> {
        // Simple one-pole lowpass
        let rc = 1.0 / (2.0 * std::f64::consts::PI * cutoff);
        let dt = 1.0 / sr as f64;
        let alpha = dt / (rc + dt);
        let mut y = 0.0f32;
        samples
            .iter()
            .map(|&x| {
                y = (alpha as f32) * x + (1.0 - alpha as f32) * y;
                y
            })
            .collect()
    }

    #[test]
    fn detects_brick_walled_fake() {
        let full = synth_fullband(44100, 2.0);
        let lp = lowpass(&full.samples, 44100, 15500.0);
        let fake = PcmBuffer {
            samples: lp,
            ..full
        };
        let r = analyze(&fake, 1.0, 1);
        assert!(
            r.suspicion > 0.2 || r.cutoff_hz < 18000.0 || r.rolloff_hz < 17500.0,
            "suspicion {} cutoff {} rolloff {}",
            r.suspicion,
            r.cutoff_hz,
            r.rolloff_hz
        );
    }

    #[test]
    fn fullband_has_hf_info() {
        let pcm = synth_fullband(44100, 2.0);
        let r = analyze(&pcm, 1.0, 1);
        assert!(r.spectral_info_score > 0.05);
    }

    #[test]
    fn wideband_noise_reads_full_band() {
        // White-ish noise reaches Nyquist → should be flagged full-band.
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
        let r = analyze(&pcm, 1.0, 1);
        assert!(r.full_band, "edge {} should be near Nyquist", r.edge_hz);
        assert!(r.edge_hz > 44100.0 / 2.0 * 0.9);
    }

    #[test]
    fn edge_ignores_bass_only_energy() {
        // Low tone + faint but real HF content: edge should reflect the HF, not
        // the 95%-energy rolloff which would sit near the bass tone.
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
        let r = analyze(&pcm, 1.0, 1);
        assert!(
            r.edge_hz > 15000.0,
            "edge {} should capture 18 kHz content despite bass-dominant energy",
            r.edge_hz
        );
        assert!(
            r.rolloff_hz < 5000.0,
            "energy rolloff sits near the bass tone"
        );
    }
}
