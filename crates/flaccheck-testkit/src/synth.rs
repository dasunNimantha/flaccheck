//! Synthetic PCM generation for labeled test corpora.

use flaccheck_core::PcmBuffer;
use realfft::RealFftPlanner;
use rustfft::num_complex::Complex;

/// Deterministic LCG white noise in [-1, 1].
pub fn lcg_noise(n: usize, seed: u32) -> Vec<f32> {
    let mut state = seed;
    (0..n)
        .map(|_| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

pub fn mono_buffer(samples: Vec<f32>, sample_rate: u32, bits: u16) -> PcmBuffer {
    PcmBuffer {
        samples,
        sample_rate,
        channels: 1,
        bits_per_sample: Some(bits),
    }
}

pub fn stereo_from_mono(left: &[f32], right: &[f32]) -> PcmBuffer {
    let n = left.len().min(right.len());
    let mut samples = Vec::with_capacity(n * 2);
    for i in 0..n {
        samples.push(left[i]);
        samples.push(right[i]);
    }
    PcmBuffer {
        samples,
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: Some(16),
    }
}

/// Multi-tone wideband content with HF energy.
pub fn multi_tone(sr: u32, secs: f64, freqs: &[f64]) -> PcmBuffer {
    let n = (sr as f64 * secs) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / sr as f64;
            let mut s = 0.0f64;
            for &f in freqs {
                s += (2.0 * std::f64::consts::PI * f * t).sin() * 0.12;
            }
            s.clamp(-1.0, 1.0) as f32
        })
        .collect();
    mono_buffer(samples, sr, 16)
}

pub fn wideband_noise(sr: u32, secs: f64, seed: u32) -> PcmBuffer {
    let n = (sr as f64 * secs) as usize;
    mono_buffer(lcg_noise(n, seed), sr, 16)
}

/// Single-frequency tone (naturally band-limited).
pub fn pure_tone(sr: u32, secs: f64, freq_hz: f64) -> PcmBuffer {
    let n = (sr as f64 * secs) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| (2.0 * std::f32::consts::PI * freq_hz as f32 * i as f32 / sr as f32).sin() * 0.5)
        .collect();
    mono_buffer(samples, sr, 16)
}

/// Deterministic 16-bit-quantized pattern (reliable padded-depth test material).
pub fn quantized_pattern(n: usize, seed: u32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let v = (i as u32).wrapping_mul(seed.wrapping_add(1)) % 4096;
            (v as f32 / 2048.0) - 1.0
        })
        // Quantize onto the true 16-bit grid (full-scale 32768) so it matches
        // how real i16 PCM is scaled.
        .map(|s| (s * 32768.0).round() / 32768.0)
        .collect()
}

pub fn lowpass(samples: &[f32], alpha: f32) -> Vec<f32> {
    let mut y = 0.0f32;
    samples
        .iter()
        .map(|&x| {
            y = alpha * x + (1.0 - alpha) * y;
            y
        })
        .collect()
}

/// Cascaded one-pole lowpass — steep rolloff approximating a codec low-pass.
pub fn steep_lowpass(samples: &[f32], alpha: f32, poles: usize) -> Vec<f32> {
    let mut out = samples.to_vec();
    for _ in 0..poles.max(1) {
        out = lowpass(&out, alpha);
    }
    out
}

/// Simulate MP3-style brick wall: FFT, zero bins above cutoff, inverse FFT.
pub fn brick_wall(source: &[f32], sample_rate: u32, cutoff_hz: f64) -> Vec<f32> {
    const BLOCK: usize = 4096;
    if source.len() < BLOCK {
        return brick_wall_block(source, sample_rate, cutoff_hz);
    }
    let mut out = vec![0.0f32; source.len()];
    let hop = BLOCK / 2;
    let window: Vec<f32> = (0..BLOCK)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (BLOCK - 1) as f32).cos()))
        .collect();

    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(BLOCK);
    let c2r = planner.plan_fft_inverse(BLOCK);
    let mut scratch_fwd = r2c.make_scratch_vec();
    let mut scratch_inv = c2r.make_scratch_vec();
    let mut spectrum = r2c.make_output_vec();

    let mut start = 0usize;
    while start + BLOCK <= source.len() {
        let mut block: Vec<f32> = source[start..start + BLOCK]
            .iter()
            .zip(window.iter())
            .map(|(s, w)| s * w)
            .collect();

        r2c.process_with_scratch(&mut block, &mut spectrum, &mut scratch_fwd)
            .expect("fft fwd");

        let bin_hz = sample_rate as f64 / BLOCK as f64;
        for (i, c) in spectrum.iter_mut().enumerate() {
            let f = i as f64 * bin_hz;
            if f > cutoff_hz {
                *c = Complex::new(0.0, 0.0);
            }
        }

        c2r.process_with_scratch(&mut spectrum, &mut block, &mut scratch_inv)
            .expect("fft inv");
        let norm = 1.0 / BLOCK as f32;
        for (i, v) in block.iter_mut().enumerate() {
            *v *= norm * window[i];
            let idx = start + i;
            out[idx] += *v;
        }
        start += hop;
    }
    // normalize overlap-add peak
    let peak = out.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
    if peak > 1e-6 {
        for v in &mut out {
            *v /= peak * 1.05;
        }
    }
    out
}

fn brick_wall_block(source: &[f32], sample_rate: u32, cutoff_hz: f64) -> Vec<f32> {
    let n = source.len().next_power_of_two().max(256);
    let mut padded = source.to_vec();
    padded.resize(n, 0.0);
    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(n);
    let c2r = planner.plan_fft_inverse(n);
    let mut scratch_fwd = r2c.make_scratch_vec();
    let mut scratch_inv = c2r.make_scratch_vec();
    let mut spectrum = r2c.make_output_vec();
    r2c.process_with_scratch(&mut padded, &mut spectrum, &mut scratch_fwd)
        .expect("fft");
    let bin_hz = sample_rate as f64 / n as f64;
    for (i, c) in spectrum.iter_mut().enumerate() {
        if i as f64 * bin_hz > cutoff_hz {
            *c = Complex::new(0.0, 0.0);
        }
    }
    c2r.process_with_scratch(&mut spectrum, &mut padded, &mut scratch_inv)
        .expect("ifft");
    let norm = 1.0 / n as f32;
    padded
        .iter()
        .take(source.len())
        .map(|v| (v * norm).clamp(-1.0, 1.0))
        .collect()
}

/// MP3-like fake: wideband source brick-walled at cutoff.
pub fn fake_mp3_transcode(sr: u32, secs: f64, cutoff_hz: f64, seed: u32) -> PcmBuffer {
    let src = lcg_noise((sr as f64 * secs) as usize, seed);
    let walled = brick_wall_hard(&src, sr, cutoff_hz);
    mono_buffer(walled, sr, 16)
}

/// Full-buffer FFT brick wall — sharper cliff than overlap-add for test corpora.
pub fn brick_wall_hard(source: &[f32], sample_rate: u32, cutoff_hz: f64) -> Vec<f32> {
    let n = source.len().next_power_of_two().max(4096);
    let mut padded = source.to_vec();
    padded.resize(n, 0.0);
    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(n);
    let c2r = planner.plan_fft_inverse(n);
    let mut scratch_fwd = r2c.make_scratch_vec();
    let mut scratch_inv = c2r.make_scratch_vec();
    let mut spectrum = r2c.make_output_vec();
    r2c.process_with_scratch(&mut padded, &mut spectrum, &mut scratch_fwd)
        .expect("fft");
    let bin_hz = sample_rate as f64 / n as f64;
    for (i, c) in spectrum.iter_mut().enumerate() {
        if i as f64 * bin_hz > cutoff_hz {
            *c = Complex::new(0.0, 0.0);
        }
    }
    c2r.process_with_scratch(&mut spectrum, &mut padded, &mut scratch_inv)
        .expect("ifft");
    let norm = 1.0 / n as f32;
    let mut out: Vec<f32> = padded.iter().take(source.len()).map(|v| v * norm).collect();
    let peak = out.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
    if peak > 1e-6 {
        for v in &mut out {
            *v /= peak * 1.02;
        }
    }
    out
}

/// 16-bit samples stored as if 24-bit (LSB zeros).
pub fn padded_24bit_from_16(source: &[f32]) -> PcmBuffer {
    // Snap to the true 16-bit grid (full-scale 32768) so a genuine 16→24-bit
    // padding is simulated exactly as real i16 PCM would scale.
    let samples: Vec<f32> = source
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * 32768.0).round() / 32768.0)
        .collect();
    PcmBuffer {
        samples,
        sample_rate: 44100,
        channels: 1,
        bits_per_sample: Some(24),
    }
}

/// Upsample by zero-stuffing + brick wall at original Nyquist (fake 44.1 -> 96).
pub fn fake_upsample_44k_to_96k(secs: f64, seed: u32) -> PcmBuffer {
    let sr_in = 44100u32;
    let sr_out = 96000u32;
    let src = lcg_noise((sr_in as f64 * secs) as usize, seed);
    let ratio = sr_out as f64 / sr_in as f64;
    let mut up: Vec<f32> = Vec::with_capacity((src.len() as f64 * ratio) as usize);
    for &s in &src {
        up.push(s);
        let extra = ratio.round() as usize - 1;
        for _ in 0..extra {
            up.push(s);
        }
    }
    let walled = brick_wall_hard(&up, sr_out, 22050.0);
    mono_buffer(walled, sr_out, 24)
}

/// Joint-stereo-like: side channel nearly silent.
pub fn joint_stereo_collapsed(sr: u32, secs: f64, seed: u32) -> PcmBuffer {
    let n = (sr as f64 * secs) as usize;
    // Simulate MP3 joint stereo: side channel near-zero (L ≈ R).
    let left = lcg_noise(n, seed);
    let right: Vec<f32> = left.iter().map(|&l| l * 0.9995).collect();
    stereo_from_mono(&left, &right)
}

/// Wideband noise with collapsed stereo — typical of joint-stereo MP3 re-wrapped as FLAC.
pub fn joint_stereo_transcode(sr: u32, secs: f64, seed: u32, cutoff_hz: f64) -> PcmBuffer {
    let n = (sr as f64 * secs) as usize;
    let mono = brick_wall_hard(&lcg_noise(n, seed), sr, cutoff_hz);
    let right: Vec<f32> = mono.iter().map(|&l| l * 0.9998).collect();
    let mut samples = Vec::with_capacity(n * 2);
    for i in 0..n {
        samples.push(mono[i]);
        samples.push(right[i]);
    }
    PcmBuffer {
        samples,
        sample_rate: sr,
        channels: 2,
        bits_per_sample: Some(16),
    }
}

/// Vinyl-like gentle rolloff (not a brick wall) — should stay genuine or inconclusive.
pub fn gentle_rolloff_noise(sr: u32, secs: f64, seed: u32) -> PcmBuffer {
    let raw = lcg_noise((sr as f64 * secs) as usize, seed);
    let filtered = lowpass(&raw, 0.12);
    mono_buffer(filtered, sr, 16)
}
