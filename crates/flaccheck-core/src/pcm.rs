use serde::{Deserialize, Serialize};

/// Decoded PCM audio in normalized float32 [-1, 1].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcmBuffer {
    /// Interleaved samples: [L0, R0, L1, R1, ...] or mono [S0, S1, ...]
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: Option<u16>,
}

impl PcmBuffer {
    pub fn duration_secs(&self) -> f64 {
        if self.channels == 0 {
            return 0.0;
        }
        let frames = self.samples.len() as f64 / self.channels as f64;
        frames / self.sample_rate as f64
    }

    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }

    /// Left channel (or mono) as contiguous slice.
    pub fn left(&self) -> Vec<f32> {
        let ch = self.channels as usize;
        if ch == 1 {
            return self.samples.clone();
        }
        self.samples.chunks(ch).map(|f| f[0]).collect()
    }

    /// Right channel; for mono returns same as left.
    pub fn right(&self) -> Vec<f32> {
        let ch = self.channels as usize;
        if ch == 1 {
            return self.left();
        }
        self.samples
            .chunks(ch)
            .map(|f| f.get(1).copied().unwrap_or(f[0]))
            .collect()
    }

    /// Extract a window [start_frame, start_frame + len) in frames.
    pub fn window_frames(&self, start_frame: usize, len_frames: usize) -> PcmBuffer {
        let ch = self.channels as usize;
        let start = start_frame * ch;
        let end = (start_frame + len_frames).min(self.frame_count()) * ch;
        PcmBuffer {
            samples: self.samples.get(start..end).unwrap_or(&[]).to_vec(),
            sample_rate: self.sample_rate,
            channels: self.channels,
            bits_per_sample: self.bits_per_sample,
        }
    }

    /// Multiple evenly-spaced analysis windows (multi-window spectral analysis).
    pub fn analysis_windows(&self, window_secs: f64, count: usize) -> Vec<PcmBuffer> {
        let frames_total = self.frame_count();
        if frames_total == 0 || count == 0 {
            return vec![];
        }
        let window_frames = (window_secs * self.sample_rate as f64) as usize;
        let window_frames = window_frames.min(frames_total).max(1024);
        if count == 1 || frames_total <= window_frames {
            return vec![self.window_frames(0, window_frames)];
        }
        let max_start = frames_total.saturating_sub(window_frames);
        (0..count)
            .map(|i| {
                let start = (max_start * i) / (count - 1);
                self.window_frames(start, window_frames)
            })
            .collect()
    }
}
