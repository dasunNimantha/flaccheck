//! Optional ONNX borderline classifier. Graceful no-op without model weights.

#[cfg(feature = "onnx")]
use lossless_scan_core::TranscodeVerdict;
use lossless_scan_core::{AnalysisResult, Evidence, PcmBuffer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MlError {
    #[error("ml feature not enabled; rebuild with --features ml")]
    NotEnabled,
    #[error("model load failed: {0}")]
    ModelLoad(String),
}

#[derive(Debug, Clone)]
pub struct MlConfig {
    pub model_path: Option<String>,
    pub enabled: bool,
}

pub struct MlClassifier {
    enabled: bool,
    #[cfg(feature = "onnx")]
    model_loaded: bool,
}

impl MlClassifier {
    pub fn new(config: &MlConfig) -> Self {
        #[cfg(feature = "onnx")]
        let model_loaded = config
            .enabled
            .then(|| config.model_path.as_deref())
            .flatten()
            .filter(|p| std::path::Path::new(p).exists())
            .is_some();

        Self {
            enabled: config.enabled,
            #[cfg(feature = "onnx")]
            model_loaded,
        }
    }

    pub fn available(&self) -> bool {
        if !self.enabled {
            return false;
        }
        #[cfg(feature = "onnx")]
        {
            return self.model_loaded;
        }
        #[cfg(not(feature = "onnx"))]
        {
            false
        }
    }

    pub fn refine_borderline(
        &self,
        pcm: &PcmBuffer,
        result: &mut AnalysisResult,
    ) -> Result<(), MlError> {
        if !self.enabled {
            return Ok(());
        }

        if result.spectral_info_score < 0.08 {
            result.evidence.push(Evidence::new(
                "ml",
                "ml_abstain",
                1.0,
                0.0,
                "ML abstained: low spectral information",
            ));
            return Ok(());
        }

        if !result.is_borderline() {
            return Ok(());
        }

        #[cfg(feature = "onnx")]
        {
            if self.model_loaded {
                let score = heuristic_mel_mid_side_score(pcm);
                result.evidence.push(Evidence::new(
                    "ml",
                    "onnx_borderline",
                    score,
                    0.8,
                    "mid/side mel heuristic (replace with tract-onnx when model weights present)",
                ));
                if score > 0.65 {
                    result.transcode_verdict = TranscodeVerdict::Suspicious;
                    result.confidence = result.confidence.max(score);
                }
                return Ok(());
            }
        }

        let _ = pcm;
        result.evidence.push(Evidence::new(
            "ml",
            "ml_noop",
            0.0,
            0.0,
            "ML enabled but no ONNX model found; train via research/train.py",
        ));
        Ok(())
    }
}

#[cfg(feature = "onnx")]
fn heuristic_mel_mid_side_score(pcm: &PcmBuffer) -> f64 {
    let l = pcm.left();
    let r = pcm.right();
    let n = l.len().min(r.len());
    if n < 100 {
        return 0.0;
    }
    let mut side_energy = 0.0f64;
    let mut mid_energy = 0.0f64;
    let step = (n / 1000).max(1);
    for i in (0..n).step_by(step) {
        let m = (l[i] + r[i]) as f64 * 0.5;
        let s = (l[i] - r[i]) as f64 * 0.5;
        mid_energy += m * m;
        side_energy += s * s;
    }
    if mid_energy < 1e-12 {
        return 0.0;
    }
    let ratio = side_energy / mid_energy;
    if ratio < 0.001 {
        0.7
    } else if ratio < 0.01 {
        0.45
    } else {
        0.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ml_noop_when_disabled() {
        let ml = MlClassifier::new(&MlConfig {
            enabled: false,
            model_path: None,
        });
        assert!(!ml.available());
    }
}
