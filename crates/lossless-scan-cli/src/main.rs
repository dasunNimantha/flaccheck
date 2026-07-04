use clap::Parser;
use indicatif::ParallelProgressIterator;
use lossless_scan::args::{BenchmarkArgs, Command, FeaturesArgs, LegacyScanConfig, OutputOpts, ScanArgs, ServeArgs};
use lossless_scan::benchmark::run_benchmark;
use lossless_scan::features::run_features;
use lossless_scan_web::ServerConfig;
use lossless_scan::report::{OutputFormat, ScanReport};
use lossless_scan::scan::{analyze_one, FileOutcome};
use lossless_scan::ui::{ColorMode, Ui};
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
            let ui = Ui::new(ColorMode::Auto, false);
            ui.error_line(&format!("{e}"));
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = lossless_scan::args::Cli::parse();

    match cli.command {
        Command::Scan(args) => run_scan(&args),
        Command::Benchmark(args) => run_benchmark_cmd(&args),
        Command::Features(args) => run_features_cmd(&args),
        Command::Serve(args) => run_serve_cmd(&args),
    }
}

fn ui_from_opts(opts: &OutputOpts) -> Ui {
    let color = ColorMode::from_arg(&opts.color).unwrap_or(ColorMode::Auto);
    Ui::new(color, opts.quiet)
}

fn run_scan(args: &ScanArgs) -> Result<(), Box<dyn std::error::Error>> {
    let ui = ui_from_opts(&args.opts);
    let files = resolve_inputs(&args.path);
    if files.is_empty() {
        return Err(format!("no supported audio files found in {}", args.path.display()).into());
    }

    let mode: ScanMode = args.opts.mode.into();
    let config = AnalysisConfig::for_mode(mode);
    let ml = MlClassifier::new(&MlConfig {
        enabled: args.opts.ml,
        model_path: args.opts.model.as_ref().map(|p| p.display().to_string()),
    });

    let format = OutputFormat::from_format_arg(args.opts.format);
    if matches!(format, OutputFormat::Text) {
        ui.print_banner(args.opts.mode.label());
        if !ui.quiet {
            ui.status(&format!(
                "scanning {} file(s) from {}",
                files.len(),
                args.path.display()
            ));
        }
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(args.opts.workers.max(1))
        .build()?;

    let outcomes: Vec<FileOutcome> = pool.install(|| {
        if ui.quiet || files.len() <= 1 {
            files
                .par_iter()
                .map(|path| analyze_one(path, &config, &ml, args.opts.explain))
                .collect()
        } else {
            files
                .par_iter()
                .progress_with_style(Ui::progress_style())
                .map(|path| analyze_one(path, &config, &ml, args.opts.explain))
                .collect()
        }
    });

    let report = ScanReport {
        results: outcomes.iter().filter_map(|o| o.result.clone()).collect(),
        skipped: outcomes.iter().filter_map(|o| o.skipped.clone()).collect(),
        errors: outcomes.iter().filter_map(|o| o.error.clone()).collect(),
    };

    let body = report.render(format, args.opts.explain, &ui)?;
    if let Some(p) = &args.opts.output {
        std::fs::write(p, &body)?;
        ui.success(&format!("wrote {}", p.display()));
    } else {
        print!("{body}");
        if matches!(format, OutputFormat::Text) && !body.ends_with('\n') && !body.is_empty() {
            println!();
        }
    }

    ui.print_summary(
        report.results.len(),
        report.suspicious_count(),
        report.skipped.len(),
        report.errors.len(),
    );

    Ok(())
}

fn run_benchmark_cmd(args: &BenchmarkArgs) -> Result<(), Box<dyn std::error::Error>> {
    let ui = ui_from_opts(&args.opts);
    let legacy = LegacyScanConfig::from(&args.opts);
    run_benchmark(&args.manifest, &legacy, &ui)
}

fn run_features_cmd(args: &FeaturesArgs) -> Result<(), Box<dyn std::error::Error>> {
    let ui = ui_from_opts(&args.opts);
    let legacy = LegacyScanConfig::from(&args.opts);
    run_features(&args.manifest, &legacy, &ui)
}

fn run_serve_cmd(args: &ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(lossless_scan_web::run_server(ServerConfig {
        host: args.host.clone(),
        port: args.port,
        model_path: args.model.as_ref().map(|p| p.display().to_string()),
    }))
}
