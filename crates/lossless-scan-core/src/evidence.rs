use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
  pub detector: String,
  pub signal: String,
  pub value: f64,
  pub weight: f64,
  pub note: String,
}

impl Evidence {
  pub fn new(
    detector: &str,
    signal: &str,
    value: f64,
    weight: f64,
    note: impl Into<String>,
  ) -> Self {
    Self {
      detector: detector.to_string(),
      signal: signal.to_string(),
      value,
      weight,
      note: note.into(),
    }
  }
}
