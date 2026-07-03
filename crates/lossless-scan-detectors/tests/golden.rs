//! Golden-file harness using in-memory synthetic PCM (no external files).

use lossless_scan_core::{AnalysisConfig, ScanMode, TranscodeVerdict};
use lossless_scan_detectors::analyze_pcm;

fn band_limited_pcm() -> lossless_scan_core::PcmBuffer {
    let sr = 44100u32;
    let n = sr as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| (2.0 * std::f32::consts::PI * 3000.0 * i as f32 / sr as f32).sin() * 0.5)
        .collect();
    lossless_scan_core::PcmBuffer {
        samples,
        sample_rate: sr,
        channels: 1,
        bits_per_sample: Some(16),
    }
}

fn wideband_noise_pcm() -> lossless_scan_core::PcmBuffer {
    let sr = 44100u32;
    let n = sr as usize * 3;
    let mut state = 0xC0FFEE_u32;
    let samples: Vec<f32> = (0..n)
        .map(|_| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect();
    lossless_scan_core::PcmBuffer {
        samples,
        sample_rate: sr,
        channels: 1,
        bits_per_sample: Some(16),
    }
}

#[test]
fn golden_band_limited_is_inconclusive_or_low_confidence() {
    let pcm = band_limited_pcm();
    let cfg = AnalysisConfig::for_mode(ScanMode::Balanced);
    let r = analyze_pcm("golden_band_limited.flac", &pcm, &cfg).unwrap();
    assert!(
        r.transcode_verdict == TranscodeVerdict::Inconclusive
            || r.spectral_info_score < 0.2
            || r.confidence < 0.5,
        "got {:?} conf {}",
        r.transcode_verdict,
        r.confidence
    );
}

#[test]
fn golden_wideband_noise_not_transcoded() {
    let pcm = wideband_noise_pcm();
    let cfg = AnalysisConfig::for_mode(ScanMode::Fast);
    let r = analyze_pcm("golden_wideband.flac", &pcm, &cfg).unwrap();
    assert!(
        !matches!(
            r.transcode_verdict,
            TranscodeVerdict::Transcoded | TranscodeVerdict::Suspicious
        ),
        "wideband noise should read genuine (got {:?} conf {:.2})",
        r.transcode_verdict,
        r.confidence
    );
    assert!(r.spectral_info_score > 0.1);
}

#[test]
fn golden_lowpassed_reads_as_fake() {
    let sr = 44100u32;
    let n = sr as usize * 2;
    // Steep (cascaded) lowpass produces a codec-like band limit. A gentle
    // one-pole rolloff is intentionally *not* flagged — it is indistinguishable
    // from bright-vs-dark mastering.
    let alpha = 0.08f32;
    let mut samples: Vec<f32> = (0..n)
        .map(|i| ((i as u32).wrapping_mul(1103515245).wrapping_add(12345) >> 8) as f32 / 16777216.0)
        .collect();
    for _ in 0..4 {
        let mut y = 0.0f32;
        samples = samples
            .iter()
            .map(|&x| {
                y = alpha * x + (1.0 - alpha) * y;
                y
            })
            .collect();
    }
    let pcm = lossless_scan_core::PcmBuffer {
        samples,
        sample_rate: sr,
        channels: 1,
        bits_per_sample: Some(16),
    };
    let cfg = AnalysisConfig::for_mode(ScanMode::Fast);
    let r = analyze_pcm("golden_lowpassed.flac", &pcm, &cfg).unwrap();
    assert!(
        matches!(
            r.transcode_verdict,
            TranscodeVerdict::Transcoded
                | TranscodeVerdict::Suspicious
                | TranscodeVerdict::Inconclusive
        ),
        "heavily lowpassed signal should not read as genuine (got {:?})",
        r.transcode_verdict
    );
}
