use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ScanMode {
  #[default]
  Fast,
  Balanced,
  Max,
}

impl std::str::FromStr for ScanMode {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "fast" => Ok(Self::Fast),
      "balanced" => Ok(Self::Balanced),
      "max" => Ok(Self::Max),
      _ => Err(format!("unknown mode: {s}")),
    }
  }
}

impl std::fmt::Display for ScanMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Fast => write!(f, "fast"),
      Self::Balanced => write!(f, "balanced"),
      Self::Max => write!(f, "max"),
    }
  }
}
