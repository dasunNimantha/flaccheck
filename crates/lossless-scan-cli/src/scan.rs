use lossless_scan_core::{AnalysisConfig, AnalysisResult};
use lossless_scan_decode::{decode_file, DecodeError};
use lossless_scan_detectors::analyze_pcm;
use lossless_scan_ml::MlClassifier;
use std::path::Path;

pub struct FileOutcome {
  pub result: Option<AnalysisResult>,
  pub skipped: Option<String>,
  pub error: Option<String>,
}

pub fn analyze_one(
  path: &Path,
  config: &AnalysisConfig,
  ml: &MlClassifier,
  _explain: bool,
) -> FileOutcome {
  let path_str = path.display().to_string();
  match decode_file(path) {
    Ok(pcm) => match analyze_pcm(&path_str, &pcm, config) {
      Ok(mut result) => {
        let _ = ml.refine_borderline(&pcm, &mut result);
        FileOutcome {
          result: Some(result),
          skipped: None,
          error: None,
        }
      }
      Err(e) => FileOutcome {
        result: None,
        skipped: None,
        error: Some(format!("{path_str}: {e}")),
      },
    },
    Err(DecodeError::FfmpegRequired { ext }) => FileOutcome {
      result: None,
      skipped: Some(format!(
        "{path_str}: requires ffmpeg for .{ext} (install ffmpeg or skip)"
      )),
      error: None,
    },
    Err(e) => FileOutcome {
      result: None,
      skipped: None,
      error: Some(format!("{path_str}: {e}")),
    },
  }
}
