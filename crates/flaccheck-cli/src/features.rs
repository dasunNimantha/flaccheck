use crate::args::LegacyScanConfig;
use crate::benchmark::{load_manifest, ManifestEntry};
use crate::scan::{analyze_one, FileOutcome};
use crate::ui::Ui;
use flaccheck_core::{
    evidence_feature_map, evidence_to_feature_vector, ScanMode, ML_FEATURE_ORDER,
};
use flaccheck_ml::{MlClassifier, MlConfig};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Debug, Serialize)]
struct FeatureMetaLine {
    #[serde(rename = "type")]
    line_type: &'static str,
    feature_order: Vec<String>,
    mode: String,
}

#[derive(Debug, Serialize)]
struct FeatureSampleLine {
    #[serde(rename = "type")]
    line_type: &'static str,
    path: String,
    label: String,
    features: std::collections::HashMap<String, f64>,
    feature_vector: Vec<f64>,
    transcode_verdict: String,
    confidence: f64,
    borderline: bool,
}

pub fn run_features(
    manifest_path: &Path,
    args: &LegacyScanConfig,
    ui: &Ui,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = load_manifest(manifest_path)?;
    let out_path = args
        .output
        .clone()
        .unwrap_or_else(|| Path::new("features.jsonl").to_path_buf());

    ui.print_banner(args.mode.label());
    ui.status(&format!(
        "dumping features for {} manifest entries ({})",
        entries.len(),
        manifest_path.display()
    ));

    let mode: ScanMode = args.mode.into();
    let config = flaccheck_core::AnalysisConfig::for_mode(mode);
    let ml = MlClassifier::new(&MlConfig {
        enabled: false,
        model_path: None,
    });

    let file = std::fs::File::create(&out_path)?;
    let mut writer = BufWriter::new(file);

    let meta = FeatureMetaLine {
        line_type: "meta",
        feature_order: ML_FEATURE_ORDER.iter().map(|s| (*s).to_string()).collect(),
        mode: args.mode.label().to_string(),
    };
    writeln!(writer, "{}", serde_json::to_string(&meta)?)?;

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

    let mut written = 0usize;
    for entry in &entries {
        if let Some(ref bar) = pb {
            bar.inc(1);
        }
        if let Some(sample) = dump_one(entry, &config, &ml)? {
            writeln!(writer, "{}", serde_json::to_string(&sample)?)?;
            written += 1;
        } else {
            ui.warn_line(&format!("skip missing or failed: {}", entry.path));
        }
    }

    if let Some(bar) = pb {
        bar.finish_and_clear();
    }
    writer.flush()?;

    ui.success(&format!(
        "wrote {} feature rows (+ meta) to {}",
        written,
        out_path.display()
    ));
    Ok(())
}

fn dump_one(
    entry: &ManifestEntry,
    config: &flaccheck_core::AnalysisConfig,
    ml: &MlClassifier,
) -> Result<Option<FeatureSampleLine>, Box<dyn std::error::Error>> {
    let path = std::path::PathBuf::from(&entry.path);
    if !path.exists() {
        return Ok(None);
    }
    let outcome: FileOutcome = analyze_one(&path, config, ml, false);
    let Some(result) = outcome.result else {
        return Ok(None);
    };

    Ok(Some(FeatureSampleLine {
        line_type: "sample",
        path: entry.path.clone(),
        label: entry.label.clone(),
        features: evidence_feature_map(&result.evidence),
        feature_vector: evidence_to_feature_vector(&result.evidence),
        transcode_verdict: format!("{:?}", result.transcode_verdict),
        confidence: result.confidence,
        borderline: result.is_borderline(),
    }))
}
