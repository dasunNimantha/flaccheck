//! Empirical threshold calibration against a labeled manifest.
//!
//! Usage: lossless-scan-calibrate <manifest.json> [--out thresholds.json]

use lossless_scan_core::{
    fuse_evidence, AnalysisConfig, Evidence, ScanMode, Thresholds, TranscodeVerdict,
};
use lossless_scan_decode::decode_file;
use lossless_scan_detectors::analyze_pcm;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    path: String,
    label: String,
}

#[derive(Debug, Clone)]
struct LabeledRun {
    label: String,
    evidence: Vec<Evidence>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: lossless-scan-calibrate <manifest.json> [--out thresholds.json]")?;

    let out_path = std::env::args()
        .skip(2)
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "--out")
        .map(|w| PathBuf::from(&w[1]))
        .unwrap_or_else(|| PathBuf::from("thresholds.calibrated.json"));

    let manifest: Vec<ManifestEntry> =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)?;

    let config = AnalysisConfig::for_mode(ScanMode::Max);
    let mut runs = Vec::new();

    eprintln!("Calibrating on {} manifest entries...", manifest.len());
    for entry in &manifest {
        let path = Path::new(&entry.path);
        if !path.exists() {
            eprintln!("  skip missing: {}", entry.path);
            continue;
        }
        let pcm = match decode_file(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("  skip decode {}: {e}", entry.path);
                continue;
            }
        };
        let r = analyze_pcm(&entry.path, &pcm, &config)?;
        runs.push(LabeledRun {
            label: entry.label.clone(),
            evidence: r.evidence,
        });
        eprint!(".");
    }
    eprintln!();

    if runs.is_empty() {
        return Err("no files analyzed".into());
    }

    let baseline = score_thresholds(&runs, &Thresholds::default());
    eprintln!(
        "Baseline: accuracy {:.1}% precision {:.1}% recall {:.1}% inconclusive {:.1}%",
        baseline.accuracy * 100.0,
        baseline.precision * 100.0,
        baseline.recall * 100.0,
        baseline.inconclusive_rate * 100.0
    );

    let mut best = baseline;
    let mut best_thresholds = Thresholds::default();

    let suspicious_grid = [0.30, 0.35, 0.38, 0.40, 0.42, 0.45, 0.48];
    let transcode_grid = [0.58, 0.62, 0.65, 0.68, 0.72, 0.75];
    let quant_high_grid = [5e-5, 8e-5, 1e-4, 2e-4, 4e-4, 8e-4];
    let quant_promote_grid = [0.55, 0.65, 0.75, 0.85];

    for &vs in &suspicious_grid {
        for &vt in &transcode_grid {
            for &qh in &quant_high_grid {
                for &qp in &quant_promote_grid {
                    let t = Thresholds {
                        verdict_suspicious: vs,
                        verdict_transcoded: vt,
                        quant_residual_high: qh,
                        quant_residual_mid: qh * 5.0,
                        quant_residual_low: qh * 20.0,
                        quant_promote_transcoded: qp,
                        quant_promote_suspicious: qp - 0.15,
                        ..Default::default()
                    };
                    let m = score_thresholds(&runs, &t);
                    if m.score() > best.score() {
                        best = m;
                        best_thresholds = t;
                    }
                }
            }
        }
    }

    eprintln!(
        "Best: accuracy {:.1}% precision {:.1}% recall {:.1}% inconclusive {:.1}%",
        best.accuracy * 100.0,
        best.precision * 100.0,
        best.recall * 100.0,
        best.inconclusive_rate * 100.0
    );
    eprintln!(
        "  verdict_suspicious={:.2} verdict_transcoded={:.2}",
        best_thresholds.verdict_suspicious, best_thresholds.verdict_transcoded
    );
    eprintln!(
        "  quant_residual_high={:.2e} quant_promote_transcoded={:.2}",
        best_thresholds.quant_residual_high, best_thresholds.quant_promote_transcoded
    );

    std::fs::write(&out_path, best_thresholds.to_json_pretty()?)?;
    eprintln!("Wrote {}", out_path.display());
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Metrics {
    accuracy: f64,
    precision: f64,
    recall: f64,
    inconclusive_rate: f64,
}

impl Metrics {
    fn score(&self) -> f64 {
        // Favor recall on transcoded detection for calibration.
        self.accuracy * 0.25 + self.recall * 0.50 + self.precision * 0.25
            - self.inconclusive_rate * 0.10
    }
}

fn is_transcoded_label(label: &str) -> bool {
    matches!(
        label,
        "transcoded" | "fake" | "not_genuine" | "notgenuine" | "lossy"
    )
}

fn is_genuine_label(label: &str) -> bool {
    matches!(label, "genuine" | "lossless" | "true_lossless")
}

fn score_thresholds(runs: &[LabeledRun], thresholds: &Thresholds) -> Metrics {
    let mut tp = 0usize;
    let mut fp = 0usize;
    let mut fn_ = 0usize;
    let mut tn = 0usize;
    let mut inconclusive = 0usize;

    for run in runs {
        let (verdict, _, _, _) = fuse_evidence(&run.evidence, thresholds);
        let pred_transcoded = matches!(
            verdict,
            TranscodeVerdict::Transcoded | TranscodeVerdict::Suspicious
        );
        let pred_inconclusive = verdict == TranscodeVerdict::Inconclusive;
        let actual_transcoded = is_transcoded_label(&run.label);
        let actual_genuine = is_genuine_label(&run.label);

        if pred_inconclusive {
            inconclusive += 1;
        }

        if actual_transcoded {
            if pred_transcoded {
                tp += 1;
            } else {
                fn_ += 1;
            }
        } else if actual_genuine {
            if pred_transcoded {
                fp += 1;
            } else if !pred_inconclusive {
                tn += 1;
            }
        }
    }

    let total = runs.len().max(1) as f64;
    let precision = if tp + fp > 0 {
        tp as f64 / (tp + fp) as f64
    } else {
        0.0
    };
    let recall = if tp + fn_ > 0 {
        tp as f64 / (tp + fn_) as f64
    } else {
        0.0
    };
    let accuracy = (tp + tn) as f64 / total;

    Metrics {
        accuracy,
        precision,
        recall,
        inconclusive_rate: inconclusive as f64 / total,
    }
}
