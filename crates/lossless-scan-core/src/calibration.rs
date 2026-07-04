//! Empirically tunable detector thresholds (defaults calibrated on synthetic matrix).

use serde::{Deserialize, Serialize};

/// Central threshold bundle shared across all detector tiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    // --- Spectral (Tier 1) ---
    pub brick_wall_drop_db: f64,
    pub brick_wall_strength_divisor: f64,
    pub steepness_codec_db_per_oct: f64,
    pub steepness_gentle_db_per_oct: f64,
    pub derivative_drop_db: f64,
    pub spectral_edge_dynamic_range_db: f64,
    pub full_band_nyquist_ratio: f64,
    pub sufficient_bandwidth_hz: f64,
    pub mp3_codec_match_tolerance_khz: f64,
    pub aac_shelf_drop_min_db: f64,
    pub aac_shelf_drop_max_db: f64,
    pub sbr_correlation_min: f64,
    pub opus_edge_tolerance_hz: f64,
    pub vorbis_rolloff_max_db_per_oct: f64,
    pub lossy_codec_max_sample_rate_hz: u32,

    // --- Quantization (Tier 2) ---
    pub quant_residual_high: f64,
    pub quant_residual_mid: f64,
    pub quant_residual_low: f64,
    pub quant_likelihood_high: f64,
    pub quant_likelihood_mid: f64,
    pub quant_likelihood_low: f64,
    pub pqmf_subband_boundary_ratio: f64,
    pub pqmf_granule_energy_cv: f64,
    pub pqmf_likelihood_high: f64,
    pub pqmf_likelihood_mid: f64,

    // --- Artifacts (Tier 3) ---
    pub joint_stereo_ratio_high: f64,
    pub joint_stereo_ratio_mid: f64,
    pub joint_stereo_score_high: f64,
    pub joint_stereo_score_mid: f64,
    pub noise_floor_flatness_high: f64,
    pub noise_floor_flatness_mid: f64,

    // --- Hi-res (Tier 4) ---
    pub upsample_energy_ratio_high: f64,
    pub upsample_energy_ratio_mid: f64,
    pub resampler_imaging_ratio: f64,
    pub padded_grid_ratio_high: f64,
    pub padded_lsb_zero_ratio_high: f64,

    // --- Fusion ---
    pub verdict_transcoded: f64,
    pub verdict_suspicious: f64,
    pub quant_promote_transcoded: f64,
    pub quant_promote_suspicious: f64,
    pub abstain_edge_ceiling_ratio: f64,
    pub abstain_edge_ceiling_max_hz: f64,
    /// Brick-wall strength at/above which a sharp cliff to a silent floor is treated as a
    /// definitive lossy fingerprint (bypasses band-limited / low-HF abstention).
    pub brick_wall_transcoded_min: f64,

    // --- Evidence weights ---
    pub weight_brick_wall: f64,
    pub weight_steepness: f64,
    pub weight_pqmf: f64,
    pub weight_aac_shelf: f64,
    pub weight_sbr: f64,
    pub weight_codec_certainty: f64,
    pub weight_joint_stereo: f64,
    pub weight_quant: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            brick_wall_drop_db: 20.0,
            brick_wall_strength_divisor: 35.0,
            steepness_codec_db_per_oct: 50.0,
            steepness_gentle_db_per_oct: 25.0,
            derivative_drop_db: 15.0,
            spectral_edge_dynamic_range_db: 72.0,
            full_band_nyquist_ratio: 0.92,
            sufficient_bandwidth_hz: 18000.0,
            mp3_codec_match_tolerance_khz: 1.5,
            aac_shelf_drop_min_db: 8.0,
            aac_shelf_drop_max_db: 35.0,
            sbr_correlation_min: 0.55,
            opus_edge_tolerance_hz: 400.0,
            vorbis_rolloff_max_db_per_oct: 45.0,
            lossy_codec_max_sample_rate_hz: 48000,

            // After RMS-normalization the MDCT grid residual clusters near the
            // uniform-quantization variance (~1/12 ≈ 0.083) for genuine and lossy material
            // alike, so it only carries information when a file's residual is anomalously
            // low. These bands sit well below the empirical genuine floor (~0.065) so the
            // detector abstains on normal music instead of raising false positives.
            quant_residual_high: 0.010,
            quant_residual_mid: 0.020,
            quant_residual_low: 0.030,
            quant_likelihood_high: 0.85,
            quant_likelihood_mid: 0.6,
            quant_likelihood_low: 0.35,
            pqmf_subband_boundary_ratio: 2.5,
            pqmf_granule_energy_cv: 0.35,
            pqmf_likelihood_high: 0.8,
            pqmf_likelihood_mid: 0.5,

            joint_stereo_ratio_high: 0.02,
            joint_stereo_ratio_mid: 0.05,
            joint_stereo_score_high: 0.6,
            joint_stereo_score_mid: 0.3,
            noise_floor_flatness_high: 0.85,
            noise_floor_flatness_mid: 0.7,

            upsample_energy_ratio_high: 0.001,
            upsample_energy_ratio_mid: 0.01,
            resampler_imaging_ratio: 0.15,
            padded_grid_ratio_high: 0.92,
            padded_lsb_zero_ratio_high: 0.85,

            verdict_transcoded: 0.68,
            verdict_suspicious: 0.38,
            quant_promote_transcoded: 0.80,
            quant_promote_suspicious: 0.55,
            abstain_edge_ceiling_ratio: 0.62,
            abstain_edge_ceiling_max_hz: 13000.0,
            brick_wall_transcoded_min: 0.55,

            weight_brick_wall: 1.0,
            weight_steepness: 0.8,
            weight_pqmf: 1.0,
            weight_aac_shelf: 0.4,
            weight_sbr: 0.5,
            weight_codec_certainty: 0.3,
            weight_joint_stereo: 0.5,
            weight_quant: 1.2,
        }
    }
}

impl Thresholds {
    /// Load thresholds from JSON (output of calibrate binary).
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
