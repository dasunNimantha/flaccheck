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
  let refs: Vec<&[f32]> = mono_windows.iter().map(|v| v.as_slice()).collect();

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

  let (codec_guess, est_bitrate, codec_match) = match_mp3_signature(cutoff_hz, brick_wall_strength);
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
  ];

  if brick_wall_strength < 0.35 && rolloff_hz < nyquist * 0.75 && info_score < 0.35 {
    evidence.push(Evidence::new(
      "spectral",
      "early_rolloff",
      0.45,
      0.4,
      format!("early spectral rolloff at {:.0} Hz", rolloff_hz),
    ));
  }

  if let Some(ref codec) = codec_guess {
    evidence.push(Evidence::new(
      "spectral",
      "codec_guess",
      1.0,
      0.3,
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
    spectral_info_score: info_score,
    codec_guess,
    est_bitrate_kbps: est_bitrate,
    suspicion,
  }
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
    if drop > 25.0 && silent_floor_above(above_slice) {
      let strength = ((drop - 25.0) / 40.0).clamp(0.0, 1.0);
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
  let var = psd_db_above
    .iter()
    .map(|x| (x - mean).powi(2))
    .sum::<f64>()
    / psd_db_above.len() as f64;
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
}
