use crate::{Evidence, HiresVerdict, Thresholds, TranscodeVerdict};

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
    thresholds: &Thresholds,
) -> (TranscodeVerdict, f64, Option<String>, Option<u32>) {
    if evidence.is_empty() {
        return (TranscodeVerdict::Inconclusive, 0.0, None, None);
    }

    let info_score = spectral_information_score(evidence);

    let brick_wall = evidence
        .iter()
        .find(|e| e.signal == "brick_wall")
        .map(|e| e.value)
        .unwrap_or(0.0);
    let codec_certainty_hint = evidence
        .iter()
        .find(|e| e.signal == "codec_guess")
        .map(|e| e.value)
        .unwrap_or(0.0);
    let aac_shelf = evidence
        .iter()
        .find(|e| e.signal == "aac_shelf")
        .map(|e| e.value)
        .unwrap_or(0.0);
    let cutoff_hz = evidence
        .iter()
        .find(|e| e.signal == "cutoff_hz")
        .map(|e| e.value)
        .unwrap_or(0.0);
    let steepness = evidence
        .iter()
        .find(|e| e.signal == "rolloff_steepness")
        .map(|e| e.value)
        .unwrap_or(0.0);

    // AAC often leaves a pre-cutoff spectral shelf with a gentler cliff than MP3. When the
    // shelf is strong and the cutoff sits below Nyquist, treat it as a lossy fingerprint even
    // if `brick_wall` alone is below the MP3-oriented threshold.
    let strong_aac = aac_shelf >= thresholds.aac_shelf_transcoded_min
        && cutoff_hz > 0.0
        && cutoff_hz <= thresholds.aac_cutoff_max_hz
        && (brick_wall >= 0.10 || steepness >= thresholds.steepness_gentle_db_per_oct);

    // A sharp spectral cliff to a silent floor below Nyquist is a definitive lossy
    // fingerprint, no matter how little high-frequency energy the track carries (a
    // bass-heavy transcode still has almost no HF energy). Such files must bypass the
    // band-limited / low-info abstention paths below, which otherwise mask real transcodes.
    let strong_cliff = brick_wall >= thresholds.brick_wall_transcoded_min
        || (brick_wall >= 0.35 && codec_certainty_hint >= 0.6);

    let strong_lossy = strong_cliff || strong_aac;

    let full_band = evidence
        .iter()
        .any(|e| e.signal == "full_band" && e.value >= 1.0);
    let edge_hz = evidence
        .iter()
        .find(|e| e.signal == "edge_hz")
        .map(|e| e.value)
        .unwrap_or(0.0);
    let sufficient_bandwidth = full_band || edge_hz >= thresholds.sufficient_bandwidth_hz;

    if !sufficient_bandwidth && !strong_lossy {
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
    let mut codec_certainty = 0.0f64;

    for e in evidence {
        if e.detector == "hires"
            || e.signal == "spectral_info_score"
            || e.signal == "abstain_band_limited"
        {
            continue;
        }
        if matches!(
            e.signal.as_str(),
            "cutoff_hz"
                | "rolloff_hz"
                | "edge_hz"
                | "full_band"
                | "est_bitrate_kbps"
                | "ml_abstain"
                | "ml_noop"
                // Steepness (dB/oct) is a raw magnitude, not a [0,1] score; its transcode
                // signal is already folded into `brick_wall` via steepness_boost. Scoring it
                // raw here would swamp the normalized sum, so exclude it.
                | "rolloff_steepness"
        ) || e.weight <= 0.0
        {
            continue;
        }
        if e.signal == "codec_guess" {
            codec_certainty = e.value;
            codec_guess = Some(e.note.clone());
            transcode_score += e.value * e.weight;
            max_weight += e.weight.abs();
            continue;
        }
        if e.value <= 0.0 {
            continue;
        }
        if e.value > 1.0 && e.signal != "rolloff_steepness" {
            if e.signal == "est_bitrate_kbps" {
                est_bitrate = Some(e.value.round() as u32);
            }
            continue;
        }
        transcode_score += e.value * e.weight;
        max_weight += e.weight.abs();
        if e.signal == "est_bitrate_kbps" {
            est_bitrate = Some(e.value.round() as u32);
        }
    }

    let normalized = if max_weight > 0.0 {
        (transcode_score / max_weight).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Content reaching ~18 kHz without a lossy cliff reads genuine unless codec ID is strong.
    if !strong_lossy
        && (full_band || edge_hz >= thresholds.sufficient_bandwidth_hz)
        && brick_wall < 0.35
        && aac_shelf < thresholds.aac_shelf_transcoded_min
        && codec_certainty < 0.45
    {
        let confidence = (0.80 + 0.15 * (1.0 - normalized)).min(0.99);
        return (
            TranscodeVerdict::Genuine,
            confidence,
            codec_guess,
            est_bitrate,
        );
    }

    let verdict = if strong_lossy {
        TranscodeVerdict::Transcoded
    } else if full_band && normalized < thresholds.verdict_transcoded && codec_certainty < 0.35 {
        TranscodeVerdict::Genuine
    } else if normalized >= thresholds.verdict_transcoded {
        TranscodeVerdict::Transcoded
    } else if normalized >= thresholds.verdict_suspicious {
        TranscodeVerdict::Suspicious
    } else if sufficient_bandwidth {
        TranscodeVerdict::Genuine
    } else if info_score < 0.15 {
        TranscodeVerdict::Inconclusive
    } else {
        TranscodeVerdict::Genuine
    };

    let confidence = match verdict {
        TranscodeVerdict::Transcoded => normalized
            .max(codec_certainty * 0.5)
            .max(brick_wall)
            .max(aac_shelf * 0.85),
        TranscodeVerdict::Suspicious => {
            0.5 + (normalized - thresholds.verdict_suspicious) * 0.5 + codec_certainty * 0.15
        }
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
        let (v, conf, _, _) = fuse_evidence(&ev, &Thresholds::default());
        assert_eq!(v, TranscodeVerdict::Transcoded);
        assert!(conf > 0.7);
    }

    #[test]
    fn fuse_joint_stereo_evidence() {
        let ev = vec![
            Evidence::new("spectral", "early_rolloff", 0.042, 0.4, ""),
            Evidence::new("artifacts", "phase_discontinuity", 1.0, 0.6, ""),
            Evidence::new("artifacts", "joint_stereo", 0.6, 0.5, ""),
            Evidence::new("quant", "aac_mdct_residual", 0.35, 1.2, ""),
            Evidence::new("quant", "mp3_pqmf", 0.5, 1.0, ""),
            Evidence::new("spectral", "spectral_info_score", 0.249, 0.0, ""),
            Evidence::new("spectral", "edge_hz", 16106.0, 0.1, ""),
        ];
        let (v, c, _, _) = fuse_evidence(&ev, &Thresholds::default());
        assert!(
            matches!(
                v,
                TranscodeVerdict::Suspicious | TranscodeVerdict::Transcoded
            ),
            "got {:?} conf {c}",
            v
        );
    }

    #[test]
    fn fuse_strong_aac_shelf() {
        let ev = vec![
            Evidence::new("spectral", "brick_wall", 0.12, 1.0, "gentle AAC cliff"),
            Evidence::new("spectral", "aac_shelf", 0.95, 0.4, "AAC shelf"),
            Evidence::new("spectral", "cutoff_hz", 21500.0, 0.2, ""),
            Evidence::new("spectral", "rolloff_steepness", 45.0, 0.8, ""),
            Evidence::new("spectral", "edge_hz", 24000.0, 0.1, ""),
            Evidence::new("spectral", "spectral_info_score", 0.3, 0.0, ""),
        ];
        let (v, conf, _, _) = fuse_evidence(&ev, &Thresholds::default());
        assert_eq!(v, TranscodeVerdict::Transcoded);
        assert!(conf > 0.6);
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
        let (v, _, _, _) = fuse_evidence(&ev, &Thresholds::default());
        assert_eq!(v, TranscodeVerdict::Inconclusive);
    }
}
