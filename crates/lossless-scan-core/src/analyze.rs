//! Orchestration configuration types.

use crate::Thresholds;

#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    pub mode: crate::ScanMode,
    pub window_secs: f64,
    pub window_count: usize,
    pub full_file: bool,
    pub ml_enabled: bool,
    pub thresholds: Thresholds,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            mode: crate::ScanMode::Balanced,
            window_secs: 10.0,
            window_count: 3,
            full_file: false,
            ml_enabled: false,
            thresholds: Thresholds::default(),
        }
    }
}

impl AnalysisConfig {
    pub fn for_mode(mode: crate::ScanMode) -> Self {
        match mode {
            crate::ScanMode::Fast => Self {
                mode,
                window_secs: 8.0,
                window_count: 2,
                full_file: false,
                ml_enabled: false,
                thresholds: Thresholds::default(),
            },
            crate::ScanMode::Balanced => Self {
                mode,
                window_secs: 10.0,
                window_count: 3,
                full_file: false,
                ml_enabled: false,
                thresholds: Thresholds::default(),
            },
            crate::ScanMode::Max => Self {
                mode,
                window_secs: 15.0,
                window_count: 5,
                full_file: true,
                ml_enabled: false,
                thresholds: Thresholds::default(),
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("empty audio buffer")]
    EmptyBuffer,
}
