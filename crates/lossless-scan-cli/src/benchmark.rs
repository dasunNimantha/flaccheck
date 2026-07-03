use crate::args::LegacyScanConfig;
use crate::scan::{analyze_one, FileOutcome};
use crate::ui::Ui;
use indicatif::{ProgressBar, ProgressStyle};
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

const AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "wav", "wave", "aiff", "aif", "mp3", "aac", "m4a", "ogg", "opus", "ape", "wv", "alac",
];

fn looks_like_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Validate manifest path before reading — catch common mistakes (directory, audio file).
pub fn validate_manifest_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("manifest not found: {}", path.display()));
    }
    if path.is_dir() {
        return Err(format!(
            "{} is a directory, not a JSON manifest.\n\n\
             To analyze audio files in that folder, run:\n\
               lossless-scan scan {}\n\n\
             Benchmark compares results against a labeled dataset manifest, e.g.:\n\
               lossless-scan benchmark datasets/output/synthetic/manifest.json",
            path.display(),
            path.display()
        ));
    }
    if looks_like_audio(path) {
        return Err(format!(
            "{} is an audio file, not a JSON manifest.\n\n\
                 To analyze this track, run:\n\
                   lossless-scan scan {}\n\n\
                 Benchmark needs a manifest.json listing files and labels (genuine/transcoded).",
            path.display(),
            path.display()
        ));
    }
    Ok(())
}

fn load_manifest(path: &Path) -> Result<Vec<ManifestEntry>, String> {
    validate_manifest_path(path)?;

    let data = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if data.is_empty() {
        return Err(format!("manifest is empty: {}", path.display()));
    }

    let text = std::str::from_utf8(&data).map_err(|_| {
        format!(
            "{} is not a UTF-8 text manifest (likely a binary file).\n\n\
             To analyze audio, use:\n\
               lossless-scan scan {}",
            path.display(),
            path.display()
        )
    })?;

    let trimmed = text.trim_start();
    if !trimmed.starts_with('[') && !trimmed.starts_with('{') {
        return Err(format!(
            "expected JSON manifest at {} (array of {{\"path\", \"label\"}} objects).\n\n\
             To scan audio instead:\n\
               lossless-scan scan {}",
            path.display(),
            path.display()
        ));
    }

    serde_json::from_str(text).map_err(|e| {
        format!(
            "invalid JSON manifest ({}): {e}\n\n\
             Example entry:\n  \
             {{\"path\": \"track.flac\", \"label\": \"genuine\"}}",
            path.display()
        )
    })
}

pub fn run_benchmark(
    manifest_path: &Path,
    args: &LegacyScanConfig,
    ui: &Ui,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = load_manifest(manifest_path)?;

    ui.print_banner(args.mode.label());
    ui.status(&format!(
        "benchmarking {} manifest entries ({})",
        entries.len(),
        manifest_path.display()
    ));

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

    let pb = if ui.quiet {
        None
    } else {
        let bar = ProgressBar::new(entries.len() as u64);
        bar.set_style(
            ProgressStyle::with_template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len}")
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );
        Some(bar)
    };

    for entry in &entries {
        *per_label.entry(entry.label.clone()).or_insert(0) += 1;
        let path = std::path::PathBuf::from(&entry.path);
        if let Some(ref bar) = pb {
            bar.inc(1);
        }
        if !path.exists() {
            ui.warn_line(&format!("skip missing: {}", entry.path));
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

    if let Some(bar) = pb {
        bar.finish_and_clear();
    }

    let total = tp + fp + fn_ + tn + inconclusive;
    let metrics = BenchmarkMetrics {
        mode: args.mode.label().to_string(),
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
        ui.success(&format!("benchmark metrics written to {}", p.display()));
    } else if args.format == crate::args::FormatArg::Json {
        println!("{out}");
    } else {
        print!(
            "{}",
            ui.format_benchmark_summary(
                &metrics.mode,
                metrics.total,
                metrics.precision_transcoded,
                metrics.recall_transcoded,
                metrics.inconclusive_rate,
            )
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_directory_as_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let err = validate_manifest_path(dir.path()).unwrap_err();
        assert!(err.contains("directory"));
        assert!(err.contains("lossless-scan scan"));
    }

    #[test]
    fn rejects_flac_as_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let flac = dir.path().join("track.flac");
        std::fs::write(&flac, b"fLaC").unwrap();
        let err = validate_manifest_path(&flac).unwrap_err();
        assert!(err.contains("audio file"));
        assert!(err.contains("lossless-scan scan"));
    }
}
