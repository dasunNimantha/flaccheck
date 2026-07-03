use crate::args::Args;
use crate::scan::{analyze_one, FileOutcome};
use lossless_scan_core::ScanMode;
use lossless_scan_ml::{MlClassifier, MlConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct ManifestEntry {
  path: String,
  label: String,
}

#[derive(Debug, Serialize)]
struct BenchmarkMetrics {
  mode: String,
  total: usize,
  precision_transcoded: f64,
  recall_transcoded: f64,
  precision_genuine: f64,
  recall_genuine: f64,
  inconclusive_rate: f64,
  per_label: HashMap<String, usize>,
}

pub fn run_benchmark(manifest_path: &Path, args: &Args) -> Result<(), Box<dyn std::error::Error>> {
  let data = std::fs::read_to_string(manifest_path)?;
  let entries: Vec<ManifestEntry> = serde_json::from_str(&data)?;

  let mode: ScanMode = args.mode.into();
  let config = lossless_scan_core::AnalysisConfig::for_mode(mode);
  let ml = MlClassifier::new(&MlConfig {
    enabled: false,
    model_path: None,
  });

  let mut tp = 0usize;
  let mut fp = 0usize;
  let mut fn_ = 0usize;
  let mut tn = 0usize;
  let mut inconclusive = 0usize;
  let mut per_label: HashMap<String, usize> = HashMap::new();

  for entry in &entries {
    *per_label.entry(entry.label.clone()).or_insert(0) += 1;
    let path = std::path::PathBuf::from(&entry.path);
    if !path.exists() {
      eprintln!("skip missing: {}", entry.path);
      continue;
    }
    let outcome: FileOutcome = analyze_one(&path, &config, &ml, false);
    let Some(result) = outcome.result else {
      continue;
    };

    let actual_transcode = entry.label == "transcoded" || entry.label == "fake";

    match (result.transcode_verdict, actual_transcode) {
      (lossless_scan_core::TranscodeVerdict::Inconclusive, _) => inconclusive += 1,
      (
        lossless_scan_core::TranscodeVerdict::Transcoded
        | lossless_scan_core::TranscodeVerdict::Suspicious,
        true,
      ) => tp += 1,
      (
        lossless_scan_core::TranscodeVerdict::Transcoded
        | lossless_scan_core::TranscodeVerdict::Suspicious,
        false,
      ) => fp += 1,
      (lossless_scan_core::TranscodeVerdict::Genuine, true) => fn_ += 1,
      (lossless_scan_core::TranscodeVerdict::Genuine, false) => tn += 1,
    }
  }

  let total = tp + fp + fn_ + tn + inconclusive;
  let metrics = BenchmarkMetrics {
    mode: format!("{:?}", args.mode),
    total,
    precision_transcoded: if tp + fp > 0 {
      tp as f64 / (tp + fp) as f64
    } else {
      0.0
    },
    recall_transcoded: if tp + fn_ > 0 {
      tp as f64 / (tp + fn_) as f64
    } else {
      0.0
    },
    precision_genuine: if tn + fn_ > 0 {
      tn as f64 / (tn + fn_) as f64
    } else {
      0.0
    },
    recall_genuine: if tn + fp > 0 {
      tn as f64 / (tn + fp) as f64
    } else {
      0.0
    },
    inconclusive_rate: if total > 0 {
      inconclusive as f64 / total as f64
    } else {
      0.0
    },
    per_label,
  };

  let out = serde_json::to_string_pretty(&metrics)?;
  if let Some(p) = &args.output {
    std::fs::write(p, &out)?;
    eprintln!("benchmark metrics written to {}", p.display());
  } else {
    println!("{out}");
  }

  Ok(())
}
