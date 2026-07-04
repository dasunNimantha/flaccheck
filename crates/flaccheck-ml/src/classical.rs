//! Pure-Rust inference for logistic regression over detector evidence features.

use flaccheck_core::{evidence_to_feature_vector, Evidence};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassicalModel {
    pub kind: String,
    pub feature_names: Vec<String>,
    pub coefficients: Vec<f64>,
    pub intercept: f64,
    #[serde(default = "default_scaler_mean")]
    pub scaler_mean: Vec<f64>,
    #[serde(default = "default_scaler_scale")]
    pub scaler_scale: Vec<f64>,
    #[serde(default = "default_threshold")]
    pub threshold: f64,
}

fn default_threshold() -> f64 {
    0.5
}

fn default_scaler_mean() -> Vec<f64> {
    Vec::new()
}

fn default_scaler_scale() -> Vec<f64> {
    Vec::new()
}

impl ClassicalModel {
    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_path(path: &Path) -> Result<Self, ClassicalModelError> {
        let data = std::fs::read_to_string(path).map_err(|e| ClassicalModelError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Self::from_json_str(&data).map_err(ClassicalModelError::Json)
    }

    pub fn transcode_probability(&self, features: &[f64]) -> f64 {
        let n = self.feature_names.len().min(features.len());
        let mut logit = self.intercept;
        for i in 0..n {
            let x = if self.scaler_mean.len() == n && self.scaler_scale.len() == n {
                let scale = self.scaler_scale[i].abs().max(1e-12);
                (features[i] - self.scaler_mean[i]) / scale
            } else {
                features[i]
            };
            logit += self.coefficients.get(i).copied().unwrap_or(0.0) * x;
        }
        sigmoid(logit)
    }

    pub fn predict_from_evidence(&self, evidence: &[Evidence]) -> f64 {
        let features = evidence_to_feature_vector(evidence);
        self.transcode_probability(&features)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClassicalModelError {
    #[error("cannot read model at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid classical model JSON: {0}")]
    Json(#[from] serde_json::Error),
}

fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_made_model_predicts_high_for_transcode_pattern() {
        let model = ClassicalModel {
            kind: "logistic_regression".to_string(),
            feature_names: vec![
                "spectral.brick_wall".to_string(),
                "quant.mp3_pqmf".to_string(),
            ],
            coefficients: vec![2.0, 1.5],
            intercept: -1.0,
            scaler_mean: vec![0.0, 0.0],
            scaler_scale: vec![1.0, 1.0],
            threshold: 0.5,
        };
        let prob = model.transcode_probability(&[0.9, 0.8]);
        assert!(prob > 0.7, "expected high prob, got {prob}");
    }

    #[test]
    fn loads_json_round_trip() {
        let json = r#"{
            "kind": "logistic_regression",
            "feature_names": ["spectral.brick_wall"],
            "coefficients": [1.0],
            "intercept": 0.0,
            "scaler_mean": [0.0],
            "scaler_scale": [1.0],
            "threshold": 0.5
        }"#;
        let model = ClassicalModel::from_json_str(json).unwrap();
        assert!((model.transcode_probability(&[1.0]) - 0.731058).abs() < 1e-4);
    }
}
