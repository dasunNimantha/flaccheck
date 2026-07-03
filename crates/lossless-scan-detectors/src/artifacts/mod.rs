//! Tier 3: Transform-codec artifact forensics.

use crate::dsp::{hilbert_envelope, welch_psd};
use lossless_scan_core::{Evidence, PcmBuffer};

#[derive(Debug, Clone)]
pub struct ArtifactResult {
  pub evidence: Vec<Evidence>,
  pub suspicion: f64,
}

pub fn analyze(pcm: &PcmBuffer, light_only: bool) -> ArtifactResult {
  let mut evidence = Vec::new();
  let mut suspicion = 0.0f64;

  let noise_score = noise_floor_shape(pcm);
  evidence.push(Evidence::new(
    "artifacts",
    "noise_floor",
    noise_score,
    if light_only { 0.3 } else { 0.6 },
    "digital silent floor above cutoff",
  ));
  suspicion += noise_score * if light_only { 0.2 } else { 0.25 };

  if !light_only {
    let pre_echo = pre_echo_score(pcm);
    evidence.push(Evidence::new(
      "artifacts",
      "pre_echo",
      pre_echo,
      0.7,
      "pre-transient smear before attacks",
    ));
    suspicion += pre_echo * 0.25;

    let phase = phase_discontinuity_score(pcm);
    evidence.push(Evidence::new(
      "artifacts",
      "phase_discontinuity",
      phase,
      0.6,
      "phase jumps at frame-like boundaries",
    ));
    suspicion += phase * 0.25;

    if pcm.channels >= 2 {
      let joint = joint_stereo_score(pcm);
      evidence.push(Evidence::new(
        "artifacts",
        "joint_stereo",
        joint,
        0.5,
        "mid/side correlation pattern suggestive of joint stereo",
      ));
      suspicion += joint * 0.25;
    }
  }

  ArtifactResult {
    evidence,
    suspicion: suspicion.clamp(0.0, 1.0),
  }
}

fn noise_floor_shape(pcm: &PcmBuffer) -> f64 {
  let mono = pcm.left();
  let _nyquist = pcm.sample_rate as f64 / 2.0;
  let (freqs, psd) = welch_psd(&mono, pcm.sample_rate, 4096);
  if freqs.is_empty() {
    return 0.0;
  }
  let cutoff_region = freqs.iter().position(|f| *f >= 16000.0).unwrap_or(0);
  let above: Vec<f64> = psd[cutoff_region..].to_vec();
  if above.len() < 4 {
    return 0.0;
  }
  let mean = above.iter().sum::<f64>() / above.len() as f64;
  let max = above.iter().cloned().fold(0.0f64, f64::max);
  let flatness = if max > 0.0 { mean / max } else { 0.0 };
  if flatness > 0.85 && mean < 1e-8 {
    0.7
  } else if flatness > 0.7 && mean < 1e-6 {
    0.4
  } else {
    0.0
  }
}

fn pre_echo_score(pcm: &PcmBuffer) -> f64 {
  let mono = pcm.left();
  let env = hilbert_envelope(&mono);
  let frame = (pcm.sample_rate as f64 * 0.023) as usize; // ~MP3 frame
  if env.len() < frame * 4 {
    return 0.0;
  }

  let mut hits = 0usize;
  let mut tests = 0usize;
  for i in (frame * 2..env.len().saturating_sub(frame)).step_by(frame) {
    let pre: f64 = env[i.saturating_sub(frame / 4)..i]
      .iter()
      .map(|&e| e as f64)
      .sum::<f64>()
      / (frame / 4) as f64;
    let post: f64 = env[i..i + frame / 4]
      .iter()
      .map(|&e| e as f64)
      .sum::<f64>()
      / (frame / 4) as f64;
    if post > pre * 3.0 && post > 0.05 {
      hits += 1;
    }
    tests += 1;
  }
  if tests == 0 {
    return 0.0;
  }
  (hits as f64 / tests as f64).clamp(0.0, 1.0)
}

fn phase_discontinuity_score(pcm: &PcmBuffer) -> f64 {
  let mono = pcm.left();
  let frame = 1152usize; // MP3 granule
  if mono.len() < frame * 3 {
    return 0.0;
  }

  let mut jumps = 0usize;
  let mut tests = 0usize;
  for i in (frame..mono.len().saturating_sub(frame)).step_by(frame) {
    let a = mono[i - 1];
    let b = mono[i];
    if (b - a).abs() > 0.15 {
      jumps += 1;
    }
    tests += 1;
  }
  if tests == 0 {
    return 0.0;
  }
  ((jumps as f64 / tests as f64) * 2.0).clamp(0.0, 1.0)
}

fn joint_stereo_score(pcm: &PcmBuffer) -> f64 {
  let l = pcm.left();
  let r = pcm.right();
  let n = l.len().min(r.len());
  if n < 1000 {
    return 0.0;
  }

  let mut mid_var = 0.0f64;
  let mut side_var = 0.0f64;
  let step = n / 2000;
  let step = step.max(1);
  let mut mids = Vec::new();
  let mut sides = Vec::new();
  for i in (0..n).step_by(step) {
    let m = (l[i] + r[i]) as f64 * 0.5;
    let s = (l[i] - r[i]) as f64 * 0.5;
    mids.push(m);
    sides.push(s);
  }
  let mean_m: f64 = mids.iter().sum::<f64>() / mids.len() as f64;
  let mean_s: f64 = sides.iter().sum::<f64>() / sides.len() as f64;
  for (&m, &s) in mids.iter().zip(sides.iter()) {
    mid_var += (m - mean_m).powi(2);
    side_var += (s - mean_s).powi(2);
  }
  mid_var /= mids.len() as f64;
  side_var /= sides.len() as f64;

  if mid_var > 1e-10 && side_var / mid_var < 0.02 {
    0.6
  } else if mid_var > 1e-10 && side_var / mid_var < 0.05 {
    0.3
  } else {
    0.0
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use lossless_scan_core::PcmBuffer;

  #[test]
  fn artifacts_run_on_stereo() {
    let n = 44100;
    let mut samples = Vec::with_capacity(n * 2);
    for i in 0..n {
      let s = (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 44100.0).sin();
      samples.push(s);
      samples.push(s * 0.9);
    }
    let pcm = PcmBuffer {
      samples,
      sample_rate: 44100,
      channels: 2,
      bits_per_sample: Some(16),
    };
    let r = analyze(&pcm, false);
    assert!(!r.evidence.is_empty());
  }
}
