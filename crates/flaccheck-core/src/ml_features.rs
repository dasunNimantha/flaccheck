//! Stable ordered feature vector for classical ML over detector evidence.

use crate::Evidence;
use std::collections::HashMap;

/// Canonical feature keys (`detector.signal`) in fixed order for training and inference.
pub const ML_FEATURE_ORDER: &[&str] = &[
    "spectral.spectral_info_score",
    "spectral.cutoff_hz",
    "spectral.brick_wall",
    "spectral.rolloff_steepness",
    "spectral.rolloff_hz",
    "spectral.edge_hz",
    "spectral.full_band",
    "spectral.early_rolloff",
    "spectral.codec_guess",
    "spectral.est_bitrate_kbps",
    "spectral.aac_shelf",
    "spectral.sbr_mirroring",
    "artifacts.noise_floor",
    "artifacts.pre_echo",
    "artifacts.phase_discontinuity",
    "artifacts.joint_stereo",
    "hires.upsampled",
    "hires.resampler_imaging",
    "hires.padded_depth",
    "hires.genuine_hires",
    "quant.aac_mdct_residual",
    "quant.mp3_pqmf",
    "abstention.abstain_band_limited",
];

pub fn feature_key(detector: &str, signal: &str) -> String {
    format!("{detector}.{signal}")
}

/// Build a map of feature key → value from pipeline evidence (ML evidence excluded).
pub fn evidence_feature_map(evidence: &[Evidence]) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    for ev in evidence {
        if ev.detector == "ml" {
            continue;
        }
        map.insert(feature_key(&ev.detector, &ev.signal), ev.value);
    }
    map
}

/// Dense feature vector aligned to [`ML_FEATURE_ORDER`]; missing signals default to 0.0.
pub fn evidence_to_feature_vector(evidence: &[Evidence]) -> Vec<f64> {
    let map = evidence_feature_map(evidence);
    ML_FEATURE_ORDER
        .iter()
        .map(|key| map.get(*key).copied().unwrap_or(0.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_length_matches_order() {
        let v = evidence_to_feature_vector(&[]);
        assert_eq!(v.len(), ML_FEATURE_ORDER.len());
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn maps_evidence_by_key() {
        let ev = vec![
            Evidence::new("spectral", "brick_wall", 0.9, 1.0, ""),
            Evidence::new("quant", "mp3_pqmf", 0.5, 1.0, ""),
        ];
        let v = evidence_to_feature_vector(&ev);
        let brick_idx = ML_FEATURE_ORDER
            .iter()
            .position(|k| *k == "spectral.brick_wall")
            .unwrap();
        let pqmf_idx = ML_FEATURE_ORDER
            .iter()
            .position(|k| *k == "quant.mp3_pqmf")
            .unwrap();
        assert_eq!(v[brick_idx], 0.9);
        assert_eq!(v[pqmf_idx], 0.5);
    }
}
