#[cfg(feature = "onnx")]
mod onnx_infer;

#[cfg(feature = "onnx")]
mod mel;

mod classical;

pub use classical::{ClassicalModel, ClassicalModelError};

#[cfg(feature = "onnx")]
pub use mel::{mid_side_mel, INPUT_CHANNELS, INPUT_HEIGHT, INPUT_WIDTH, N_FRAMES, N_MELS};

use lossless_scan_core::{AnalysisResult, Evidence, PcmBuffer, TranscodeVerdict};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MlError {
    #[error("ml feature not enabled; rebuild with --features ml")]
    NotEnabled,
    #[error("model load failed: {0}")]
    ModelLoad(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadedModelKind {
    None,
    Classical,
    Onnx,
}

#[derive(Debug, Clone)]
pub struct MlConfig {
    pub model_path: Option<String>,
    pub enabled: bool,
}

pub struct MlClassifier {
    enabled: bool,
    model_kind: LoadedModelKind,
    classical: Option<ClassicalModel>,
    #[cfg(feature = "onnx")]
    onnx_loaded: bool,
    #[cfg(feature = "onnx")]
    onnx: Option<onnx_infer::OnnxModel>,
}

impl MlClassifier {
    pub fn new(config: &MlConfig) -> Self {
        let mut classical = None;
        let mut model_kind = LoadedModelKind::None;
        #[cfg(feature = "onnx")]
        let mut onnx = None;
        #[cfg(feature = "onnx")]
        let mut onnx_loaded = false;

        if config.enabled {
            if let Some(path) = config.model_path.as_deref() {
                let p = std::path::Path::new(path);
                if p.exists() {
                    if p.extension().and_then(|e| e.to_str()) == Some("json") {
                        match ClassicalModel::from_path(p) {
                            Ok(m) => {
                                classical = Some(m);
                                model_kind = LoadedModelKind::Classical;
                            }
                            Err(e) => {
                                eprintln!("warning: failed to load classical model: {e}");
                            }
                        }
                    }
                    #[cfg(feature = "onnx")]
                    if p.extension().and_then(|e| e.to_str()) == Some("onnx") {
                        match onnx_infer::OnnxModel::load(p) {
                            Ok(m) => {
                                onnx = Some(m);
                                onnx_loaded = true;
                                model_kind = LoadedModelKind::Onnx;
                            }
                            Err(e) => {
                                eprintln!("warning: failed to load ONNX model: {e}");
                            }
                        }
                    }
                }
            }
        }

        Self {
            enabled: config.enabled,
            model_kind,
            classical,
            #[cfg(feature = "onnx")]
            onnx_loaded,
            #[cfg(feature = "onnx")]
            onnx,
        }
    }

    pub fn available(&self) -> bool {
        if !self.enabled {
            return false;
        }
        self.model_kind != LoadedModelKind::None
    }

    pub fn model_kind(&self) -> LoadedModelKind {
        self.model_kind
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

        if let Some(ref model) = self.classical {
            let prob = model.predict_from_evidence(&result.evidence);
            apply_ml_score(result, prob, model.threshold, "classical_prob", "classical logistic regression");
            return Ok(());
        }

        #[cfg(feature = "onnx")]
        if let Some(ref onnx) = self.onnx {
            let mel = mel::mid_side_mel(pcm);
            let prob = onnx.predict(&mel).map_err(|e| MlError::ModelLoad(e.to_string()))?;
            apply_ml_score(result, prob, 0.5, "onnx_borderline", "mid/side mel CNN (tract-onnx)");
            return Ok(());
        }

        #[cfg(feature = "onnx")]
        if self.onnx_loaded {
            let score = heuristic_mel_mid_side_score(pcm);
            apply_ml_score(
                result,
                score,
                0.65,
                "onnx_borderline",
                "mid/side mel heuristic (ONNX load failed)",
            );
            return Ok(());
        }

        let _ = pcm;
        result.evidence.push(Evidence::new(
            "ml",
            "ml_noop",
            0.0,
            0.0,
            "ML enabled but no model found; train via research/train_classical.py or research/train.py",
        ));
        Ok(())
    }
}

fn apply_ml_score(
    result: &mut AnalysisResult,
    prob: f64,
    threshold: f64,
    signal: &str,
    note: &str,
) {
    result.evidence.push(Evidence::new("ml", signal, prob, 0.85, note));

    if prob >= threshold + 0.15 {
        result.transcode_verdict = TranscodeVerdict::Transcoded;
        result.confidence = result.confidence.max(prob);
    } else if prob >= threshold {
        result.transcode_verdict = TranscodeVerdict::Suspicious;
        result.confidence = result.confidence.max(prob);
    } else if prob < threshold - 0.15 {
        result.transcode_verdict = TranscodeVerdict::Genuine;
        result.confidence = result.confidence.max(1.0 - prob);
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
        assert_eq!(ml.model_kind(), LoadedModelKind::None);
    }

    #[test]
    fn classical_model_routes_by_json_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.json");
        let json = r#"{
            "kind": "logistic_regression",
            "feature_names": ["spectral.brick_wall"],
            "coefficients": [2.0],
            "intercept": -0.5,
            "scaler_mean": [0.0],
            "scaler_scale": [1.0],
            "threshold": 0.5
        }"#;
        std::fs::write(&path, json).unwrap();
        let ml = MlClassifier::new(&MlConfig {
            enabled: true,
            model_path: Some(path.display().to_string()),
        });
        assert!(ml.available());
        assert_eq!(ml.model_kind(), LoadedModelKind::Classical);
    }

    #[test]
    fn classical_refine_borderline_adds_evidence() {
        use lossless_scan_core::{AnalysisResult, HiresVerdict, ScanMode, TranscodeVerdict};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.json");
        let json = r#"{
            "kind": "logistic_regression",
            "feature_names": ["spectral.brick_wall", "quant.mp3_pqmf"],
            "coefficients": [3.0, 2.0],
            "intercept": -1.0,
            "scaler_mean": [0.0, 0.0],
            "scaler_scale": [1.0, 1.0],
            "threshold": 0.5
        }"#;
        std::fs::write(&path, json).unwrap();

        let ml = MlClassifier::new(&MlConfig {
            enabled: true,
            model_path: Some(path.display().to_string()),
        });

        let mut result = AnalysisResult {
            path: "test.flac".to_string(),
            transcode_verdict: TranscodeVerdict::Suspicious,
            hires_verdict: HiresVerdict::GenuineHires,
            confidence: 0.5,
            evidence: vec![
                lossless_scan_core::Evidence::new("spectral", "brick_wall", 0.9, 1.0, ""),
                lossless_scan_core::Evidence::new("spectral", "spectral_info_score", 0.5, 0.0, ""),
            ],
            codec_guess: None,
            est_source_bitrate_kbps: None,
            spectral_info_score: 0.5,
            mode: ScanMode::Balanced,
            duration_secs: 180.0,
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: Some(16),
        };

        let pcm = lossless_scan_core::PcmBuffer {
            samples: vec![0.0; 4096],
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: Some(16),
        };

        ml.refine_borderline(&pcm, &mut result).unwrap();
        assert!(
            result
                .evidence
                .iter()
                .any(|e| e.signal == "classical_prob"),
            "expected classical_prob evidence"
        );
    }

    #[cfg(feature = "onnx")]
    #[test]
    #[ignore = "requires models/borderline.onnx from research/train.py --demo"]
    fn onnx_demo_model_loads() {
        let path = std::path::Path::new("models/borderline.onnx");
        if !path.exists() {
            return;
        }
        let onnx = crate::onnx_infer::OnnxModel::load(path).unwrap();
        let mel = vec![0.0f32; crate::mel::INPUT_CHANNELS * crate::mel::INPUT_HEIGHT * crate::mel::INPUT_WIDTH];
        let prob = onnx.predict(&mel).unwrap();
        assert!((0.0..=1.0).contains(&prob));
    }
}
