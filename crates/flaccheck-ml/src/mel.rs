//! Mid/side mel-spectrogram front-end matching `research/mel_config.py`.

use flaccheck_core::PcmBuffer;
use rustfft::{num_complex::Complex, FftPlanner};

/// Locked mel parameters — must match Python training pipeline.
pub const N_FFT: usize = 2048;
pub const HOP_LENGTH: usize = 512;
pub const N_MELS: usize = 64;
pub const N_FRAMES: usize = 128;
pub const F_MIN: f64 = 0.0;

pub const INPUT_CHANNELS: usize = 2;
pub const INPUT_HEIGHT: usize = N_MELS;
pub const INPUT_WIDTH: usize = N_FRAMES;

/// Shape: `[2, N_MELS, N_FRAMES]` row-major (channel, mel, time).
pub fn mid_side_mel(pcm: &PcmBuffer) -> Vec<f32> {
    let l = pcm.left();
    let r = pcm.right();
    let n = l.len().min(r.len());
    if n < N_FFT {
        return vec![0.0; INPUT_CHANNELS * N_MELS * N_FRAMES];
    }

    let mid: Vec<f32> = l[..n]
        .iter()
        .zip(r[..n].iter())
        .map(|(a, b)| (a + b) * 0.5)
        .collect();
    let side: Vec<f32> = l[..n]
        .iter()
        .zip(r[..n].iter())
        .map(|(a, b)| (a - b) * 0.5)
        .collect();

    let mel_mid = channel_mel(&mid, pcm.sample_rate);
    let mel_side = channel_mel(&side, pcm.sample_rate);

    let mut out = vec![0.0f32; INPUT_CHANNELS * N_MELS * N_FRAMES];
    for c in 0..INPUT_CHANNELS {
        let src = if c == 0 { &mel_mid } else { &mel_side };
        for m in 0..N_MELS {
            for t in 0..N_FRAMES {
                out[c * N_MELS * N_FRAMES + m * N_FRAMES + t] = src[m * N_FRAMES + t];
            }
        }
    }
    out
}

fn channel_mel(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    let f_max = sample_rate as f64 / 2.0;
    let mel_filters = build_mel_filterbank(N_FFT, N_MELS, sample_rate, F_MIN, f_max);
    let frames = stft_power(samples);
    let n_frames_avail = frames.len();
    if n_frames_avail == 0 {
        return vec![0.0; N_MELS * N_FRAMES];
    }

    let mut mel = vec![0.0f32; N_MELS * n_frames_avail];
    for (t, frame) in frames.iter().enumerate() {
        for m in 0..N_MELS {
            let mut e = 0.0f64;
            for (k, &w) in mel_filters[m].iter().enumerate() {
                if k < frame.len() {
                    e += frame[k] as f64 * w;
                }
            }
            mel[m * n_frames_avail + t] = (e.max(1e-10)).ln() as f32;
        }
    }

    crop_or_pad_frames(&mel, N_MELS, n_frames_avail, N_FRAMES)
}

fn crop_or_pad_frames(
    mel: &[f32],
    n_mels: usize,
    n_frames_avail: usize,
    target_frames: usize,
) -> Vec<f32> {
    let mut out = vec![0.0f32; n_mels * target_frames];
    if n_frames_avail >= target_frames {
        let start = (n_frames_avail - target_frames) / 2;
        for m in 0..n_mels {
            for t in 0..target_frames {
                out[m * target_frames + t] = mel[m * n_frames_avail + start + t];
            }
        }
    } else {
        let pad_left = (target_frames - n_frames_avail) / 2;
        for m in 0..n_mels {
            for t in 0..n_frames_avail {
                out[m * target_frames + pad_left + t] = mel[m * n_frames_avail + t];
            }
        }
    }
    out
}

fn stft_power(samples: &[f32]) -> Vec<Vec<f32>> {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N_FFT);
    let mut frames = Vec::new();
    let mut start = 0usize;
    while start + N_FFT <= samples.len() {
        let mut buffer: Vec<Complex<f32>> = samples[start..start + N_FFT]
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let w = hann_window(i, N_FFT);
                Complex::new(s * w, 0.0)
            })
            .collect();
        fft.process(&mut buffer);
        let half = N_FFT / 2 + 1;
        let power: Vec<f32> = buffer[..half]
            .iter()
            .map(|c| c.norm_sqr() / N_FFT as f32)
            .collect();
        frames.push(power);
        start += HOP_LENGTH;
    }
    frames
}

fn hann_window(i: usize, n: usize) -> f32 {
    let x = i as f32 / (n.saturating_sub(1).max(1) as f32);
    0.5 * (1.0 - (2.0 * std::f32::consts::PI * x).cos())
}

fn hz_to_mel(hz: f64) -> f64 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f64) -> f64 {
    700.0 * (10.0f64.powf(mel / 2595.0) - 1.0)
}

fn build_mel_filterbank(
    n_fft: usize,
    n_mels: usize,
    sample_rate: u32,
    f_min: f64,
    f_max: f64,
) -> Vec<Vec<f64>> {
    let n_freqs = n_fft / 2 + 1;
    let mel_min = hz_to_mel(f_min);
    let mel_max = hz_to_mel(f_max);
    let mel_points: Vec<f64> = (0..=n_mels + 1)
        .map(|i| mel_min + (mel_max - mel_min) * i as f64 / (n_mels + 1) as f64)
        .collect();
    let hz_points: Vec<f64> = mel_points.iter().map(|m| mel_to_hz(*m)).collect();
    let bin_points: Vec<usize> = hz_points
        .iter()
        .map(|hz| ((n_fft + 1) as f64 * hz / sample_rate as f64).floor() as usize)
        .collect();

    let mut filters = vec![vec![0.0; n_freqs]; n_mels];
    for m in 0..n_mels {
        let left = bin_points[m];
        let center = bin_points[m + 1];
        let right = bin_points[m + 2];
        for k in left..center {
            if center > left {
                filters[m][k] = (k - left) as f64 / (center - left) as f64;
            }
        }
        for k in center..right {
            if right > center {
                filters[m][k] = (right - k) as f64 / (right - center) as f64;
            }
        }
    }
    filters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_output_shape() {
        let len = 44100 * 5;
        let left: Vec<f32> = (0..len).map(|i| (i as f32 * 0.001).sin()).collect();
        let right = left.clone();
        let mut interleaved = Vec::with_capacity(len * 2);
        for i in 0..len {
            interleaved.push(left[i]);
            interleaved.push(right[i]);
        }
        let pcm = flaccheck_core::PcmBuffer {
            samples: interleaved,
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: Some(16),
        };
        let mel = mid_side_mel(&pcm);
        assert_eq!(mel.len(), INPUT_CHANNELS * N_MELS * N_FRAMES);
    }
}
