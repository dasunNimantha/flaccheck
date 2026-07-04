#[cfg(feature = "onnx")]
use crate::mel::{INPUT_CHANNELS, INPUT_HEIGHT, INPUT_WIDTH};
use std::path::Path;
use thiserror::Error;
use tract_onnx::prelude::*;

#[derive(Debug, Error)]
pub enum OnnxError {
    #[error("tract model error: {0}")]
    Tract(#[from] TractError),
    #[error("unexpected output shape")]
    BadOutput,
}

pub struct OnnxModel {
    model: SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>,
}

impl OnnxModel {
    pub fn load(path: &Path) -> Result<Self, OnnxError> {
        let model = tract_onnx::onnx()
            .model_for_path(path)?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self { model })
    }

    /// `mel` is `[2, N_MELS, N_FRAMES]` from [`crate::mel::mid_side_mel`].
    pub fn predict(&self, mel: &[f32]) -> Result<f64, OnnxError> {
        let expected = INPUT_CHANNELS * INPUT_HEIGHT * INPUT_WIDTH;
        if mel.len() != expected {
            return Err(OnnxError::BadOutput);
        }

        // ONNX TinyCnn expects NCHW with H=time (128), W=mel (64).
        let mut tensor_data = vec![0.0f32; expected];
        for c in 0..INPUT_CHANNELS {
            for t in 0..INPUT_WIDTH {
                for m in 0..INPUT_HEIGHT {
                    let src = c * INPUT_HEIGHT * INPUT_WIDTH + m * INPUT_WIDTH + t;
                    let dst = c * INPUT_WIDTH * INPUT_HEIGHT + t * INPUT_HEIGHT + m;
                    tensor_data[dst] = mel[src];
                }
            }
        }

        use ndarray::Array4;

        let arr =
            Array4::from_shape_vec((1, INPUT_CHANNELS, INPUT_WIDTH, INPUT_HEIGHT), tensor_data)
                .map_err(|_| OnnxError::BadOutput)?;
        let input = arr.into_tvalue();

        let outputs = self.model.run(tvec!(input))?;
        let view = outputs[0]
            .to_array_view::<f32>()
            .map_err(|_| OnnxError::BadOutput)?;
        let prob = view.iter().next().copied().unwrap_or(0.0);
        Ok(prob as f64)
    }
}
