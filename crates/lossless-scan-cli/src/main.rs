use clap::Parser;
use indicatif::{ParallelProgressIterator, ProgressStyle};
use lossless_scan::report::{OutputFormat, ScanReport};
use lossless_scan::{analyze_one, Args, FileOutcome};
use lossless_scan_core::{AnalysisConfig, ScanMode};
use lossless_scan_decode::collect_audio_files;
use lossless_scan_ml::{MlClassifier, MlConfig};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn resolve_inputs(path: &Path) -> Vec<PathBuf> {
  if path.is_file() {
    return vec![path.to_path_buf()];
  }
  collect_audio_files(path)
}

fn main() -> ExitCode {
  match run() {
    Ok(()) => ExitCode::SUCCESS,
    Err(e) => {
      eprintln!("error: {e}");
      ExitCode::FAILURE
    }
  }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
  let args = Args::parse();

  if let Some(bench_path) = &args.benchmark {
    return lossless_scan::benchmark::run_benchmark(bench_path, &args);
  }

  let files = resolve_inputs(&args.path);
  if files.is_empty() {
    return Err("no supported audio files found".into());
  }

  let mode: ScanMode = args.mode.into();
  let config = AnalysisConfig::for_mode(mode);
  let ml = MlClassifier::new(&MlConfig {
    enabled: args.ml,
    model_path: args.model.as_ref().map(|p| p.display().to_string()),
  });

  let pool = rayon::ThreadPoolBuilder::new()
    .num_threads(args.workers.max(1))
    .build()?;

  let outcomes: Vec<FileOutcome> = pool.install(|| {
    files
      .par_iter()
      .progress_with_style(
        ProgressStyle::with_template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len}")
          .unwrap()
          .progress_chars("=>-"),
      )
      .map(|path| analyze_one(path, &config, &ml, args.explain))
      .collect()
  });

  let report = ScanReport {
    results: outcomes.iter().filter_map(|o| o.result.clone()).collect(),
    skipped: outcomes.iter().filter_map(|o| o.skipped.clone()).collect(),
    errors: outcomes.iter().filter_map(|o| o.error.clone()).collect(),
  };

  let body = report.render(OutputFormat::from_format_arg(args.format), args.explain)?;
  if let Some(p) = &args.output {
    std::fs::write(p, body)?;
    eprintln!("wrote {}", p.display());
  } else {
    print!("{body}");
  }

  let bad = report
    .results
    .iter()
    .filter(|r| {
      matches!(
        r.transcode_verdict,
        lossless_scan_core::TranscodeVerdict::Transcoded
          | lossless_scan_core::TranscodeVerdict::Suspicious
      )
    })
    .count();
  eprintln!(
    "scanned {} files, {} suspicious/transcoded, {} skipped, {} errors",
    report.results.len(),
    bad,
    report.skipped.len(),
    report.errors.len()
  );

  Ok(())
}
