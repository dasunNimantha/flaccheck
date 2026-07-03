use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TranscodeVerdict {
  Genuine,
  Suspicious,
  Transcoded,
  Inconclusive,
}

impl std::fmt::Display for TranscodeVerdict {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Genuine => write!(f, "GENUINE"),
      Self::Suspicious => write!(f, "SUSPICIOUS"),
      Self::Transcoded => write!(f, "TRANSCODED"),
      Self::Inconclusive => write!(f, "INCONCLUSIVE"),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HiresVerdict {
  GenuineHires,
  Upsampled,
  PaddedDepth,
  Unknown,
}

impl std::fmt::Display for HiresVerdict {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::GenuineHires => write!(f, "GENUINE_HIRES"),
      Self::Upsampled => write!(f, "UPSAMPLED"),
      Self::PaddedDepth => write!(f, "PADDED_DEPTH"),
      Self::Unknown => write!(f, "UNKNOWN"),
    }
  }
}
