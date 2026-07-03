use crate::{Evidence, HiresVerdict, TranscodeVerdict};

/// Spectral information score in [0, 1]: how much high-frequency content exists.
pub fn spectral_information_score(evidence: &[Evidence]) -> f64 {
    evidence
        .iter()
        .find(|e| e.signal == "spectral_info_score")
        .map(|e| e.value.clamp(0.0, 1.0))
        .unwrap_or(0.5)
}

/// Fuse detector evidence into transcode verdict, confidence, and codec guess.
pub fn fuse_evidence(
    evidence: &[Evidence],
) -> (TranscodeVerdict, f64, Option<String>, Option<u32>) {
    if evidence.is_empty() {
        return (TranscodeVerdict::Inconclusive, 0.0, None, None);
    }

    let info_score = spectral_information_score(evidence);

    // Positive full-band signal: content reaches near Nyquist with no lossy cliff.
    let full_band = evidence
        .iter()
        .any(|e| e.signal == "full_band" && e.value >= 1.0);
    let edge_hz = evidence
        .iter()
        .find(|e| e.signal == "edge_hz")
        .map(|e| e.value)
        .unwrap_or(0.0);
    // Enough bandwidth that a hidden lossy cutoff is unlikely (>= ~18 kHz).
    let sufficient_bandwidth = full_band || edge_hz >= 18000.0;

    // Band-limited abstention (Tier 5) — never abstain when clearly full-band.
    if !sufficient_bandwidth {
        if let Some(e) = evidence.iter().find(|e| e.signal == "abstain_band_limited") {
            if e.value >= 1.0 {
                return (TranscodeVerdict::Inconclusive, 0.0, None, None);
            }
        }
        if info_score < 0.08 {
            return (TranscodeVerdict::Inconclusive, 0.1, None, None);
        }
    }

    let mut transcode_score = 0.0f64;
    let mut codec_guess: Option<String> = None;
    let mut est_bitrate: Option<u32> = None;
    let mut max_weight = 0.0f64;

    for e in evidence {
        if e.detector == "hires"
            || e.signal == "spectral_info_score"
            || e.signal == "abstain_band_limited"
        {
            continue;
        }
        // Informational signals carry physical units, not scores
        if matches!(
            e.signal.as_str(),
            "cutoff_hz"
                | "rolloff_hz"
                | "edge_hz"
                | "full_band"
                | "codec_guess"
                | "est_bitrate_kbps"
                | "mp3_pqmf"
                | "ml_abstain"
                | "ml_noop"
        ) || e.weight <= 0.0
        {
            continue;
        }
        if e.value > 1.0 {
            continue;
        }
        transcode_score += e.value * e.weight;
        max_weight += e.weight.abs();
        if e.signal == "codec_guess" && e.value > 0.0 {
            codec_guess = Some(e.note.clone());
        }
        if e.signal == "est_bitrate_kbps" {
            est_bitrate = Some(e.value.round() as u32);
        }
    }

    let normalized = if max_weight > 0.0 {
        (transcode_score / max_weight).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let verdict = if normalized >= 0.72 {
        TranscodeVerdict::Transcoded
    } else if normalized >= 0.42 {
        TranscodeVerdict::Suspicious
    } else if sufficient_bandwidth {
        // Full-band content with no transcode fingerprint reads genuine even if
        // the high-frequency energy share is modest (bass-heavy masters).
        TranscodeVerdict::Genuine
    } else if info_score < 0.15 {
        TranscodeVerdict::Inconclusive
    } else {
        TranscodeVerdict::Genuine
    };

    let confidence = match verdict {
        TranscodeVerdict::Transcoded => normalized,
        TranscodeVerdict::Suspicious => 0.5 + (normalized - 0.42) * 0.5,
        TranscodeVerdict::Genuine => {
            if full_band {
                (0.75 + 0.25 * (1.0 - normalized)).min(0.99)
            } else {
                (0.55 + 0.20 * (1.0 - normalized)).min(0.80)
            }
        }
        TranscodeVerdict::Inconclusive => 0.0,
    };

    (
        verdict,
        confidence.clamp(0.0, 1.0),
        codec_guess,
        est_bitrate,
    )
}

/// Fuse hi-res evidence into hires verdict.
pub fn fuse_hires_verdict(evidence: &[Evidence]) -> HiresVerdict {
    let upsampled = evidence
        .iter()
        .any(|e| e.detector == "hires" && e.signal == "upsampled" && e.value >= 0.7);
    let padded = evidence
        .iter()
        .any(|e| e.detector == "hires" && e.signal == "padded_depth" && e.value >= 0.7);

    if upsampled {
        HiresVerdict::Upsampled
    } else if padded {
        HiresVerdict::PaddedDepth
    } else if evidence.iter().any(|e| e.detector == "hires") {
        HiresVerdict::GenuineHires
    } else {
        HiresVerdict::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Evidence;

    #[test]
    fn fuse_strong_transcode_signal() {
        let ev = vec![
            Evidence::new("spectral", "brick_wall", 0.95, 1.0, "MP3 cliff at 16kHz"),
            Evidence::new("spectral", "spectral_info_score", 0.8, 0.0, "HF present"),
        ];
        let (v, conf, _, _) = fuse_evidence(&ev);
        assert_eq!(v, TranscodeVerdict::Transcoded);
        assert!(conf > 0.7);
    }

    #[test]
    fn fuse_band_limited_abstains() {
        let ev = vec![Evidence::new(
            "abstention",
            "abstain_band_limited",
            1.0,
            1.0,
            "rolloff below 7kHz",
        )];
        let (v, _, _, _) = fuse_evidence(&ev);
        assert_eq!(v, TranscodeVerdict::Inconclusive);
    }
}
