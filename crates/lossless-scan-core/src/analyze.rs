//! Orchestration configuration types.

#[derive(Debug, Clone)]
pub struct AnalysisConfig {
  pub mode: crate::ScanMode,
  pub window_secs: f64,
  pub window_count: usize,
  pub full_file: bool,
  pub ml_enabled: bool,
}

impl Default for AnalysisConfig {
  fn default() -> Self {
    Self {
      mode: crate::ScanMode::Balanced,
      window_secs: 10.0,
      window_count: 3,
      full_file: false,
      ml_enabled: false,
    }
  }
}

impl AnalysisConfig {
  pub fn for_mode(mode: crate::ScanMode) -> Self {
    let mut cfg = Self::default();
    cfg.mode = mode;
    match mode {
      crate::ScanMode::Fast => {
        cfg.window_secs = 8.0;
        cfg.window_count = 2;
        cfg.full_file = false;
      }
      crate::ScanMode::Balanced => {
        cfg.window_secs = 10.0;
        cfg.window_count = 3;
        cfg.full_file = false;
      }
      crate::ScanMode::Max => {
        cfg.window_secs = 15.0;
        cfg.window_count = 5;
        cfg.full_file = true;
      }
    }
    cfg
  }
}

#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
  #[error("empty audio buffer")]
  EmptyBuffer,
}
