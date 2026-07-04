use clap::{Parser, Subcommand, ValueEnum};
use lossless_scan_core::ScanMode;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "lossless-scan",
    about = "Analyze audio for lossless authenticity",
    long_about = "Detect fake lossless files (e.g. MP3/AAC transcoded to FLAC) using \
                  spectral, quantization, and artifact detectors.",
    version,
    author,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scan a file or directory for transcoding fingerprints
    Scan(ScanArgs),
    /// Evaluate precision/recall against a labeled JSON manifest
    Benchmark(BenchmarkArgs),
    /// Dump detector feature vectors for ML training (JSONL)
    Features(FeaturesArgs),
    /// Launch local web UI in the browser
    Serve(ServeArgs),
}

#[derive(Parser, Debug)]
pub struct ScanArgs {
    /// Audio file or directory to scan
    pub path: PathBuf,

    #[command(flatten)]
    pub opts: OutputOpts,
}

#[derive(Parser, Debug)]
pub struct BenchmarkArgs {
    /// JSON manifest file (not audio). Each entry: `{"path": "...", "label": "genuine"|"transcoded"}`
    #[arg(value_name = "MANIFEST.json")]
    pub manifest: PathBuf,

    #[command(flatten)]
    pub opts: OutputOpts,
}

#[derive(Parser, Debug)]
pub struct FeaturesArgs {
    /// JSON manifest file (not audio). Each entry: `{"path": "...", "label": "genuine"|"transcoded"}`
    #[arg(value_name = "MANIFEST.json")]
    pub manifest: PathBuf,

    #[command(flatten)]
    pub opts: OutputOpts,
}

#[derive(Parser, Debug)]
pub struct ServeArgs {
    /// Host address to bind
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// TCP port
    #[arg(short, long, default_value_t = 8787)]
    pub port: u16,

    /// Path to ML model (`.json` classical or `.onnx` CNN)
    #[arg(long, value_name = "FILE")]
    pub model: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct OutputOpts {
    /// Analysis depth: fast (quick), balanced (default), max (exhaustive)
    #[arg(long, value_enum, default_value = "balanced", global = true)]
    pub mode: ModeArg,

    /// Report format
    #[arg(long, value_enum, default_value = "text", global = true)]
    pub format: FormatArg,

    /// Write report to file instead of stdout
    #[arg(short, long, global = true)]
    pub output: Option<PathBuf>,

    /// Parallel worker threads (directory scans)
    #[arg(long, default_value_t = 4, global = true)]
    pub workers: usize,

    /// Include per-detector evidence in the report
    #[arg(long, global = true)]
    pub explain: bool,

    /// Suppress progress bar and summary (stdout report only)
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Color output: auto, always, or never
    #[arg(long, default_value = "auto", global = true, value_name = "WHEN")]
    pub color: String,

    /// Enable ONNX borderline classifier (requires `ml` feature + model file)
    #[arg(long, global = true)]
    pub ml: bool,

    /// Path to ML model weights (`.json` classical or `.onnx` CNN)
    #[arg(long, global = true, value_name = "FILE")]
    pub model: Option<PathBuf>,
}

/// Legacy flat args — kept for lib/benchmark internal use.
#[derive(Debug, Clone)]
pub struct LegacyScanConfig {
    pub mode: ModeArg,
    pub format: FormatArg,
    pub output: Option<PathBuf>,
    pub workers: usize,
    pub explain: bool,
    pub quiet: bool,
    pub color: String,
    pub ml: bool,
    pub model: Option<PathBuf>,
}

impl From<&OutputOpts> for LegacyScanConfig {
    fn from(o: &OutputOpts) -> Self {
        Self {
            mode: o.mode,
            format: o.format,
            output: o.output.clone(),
            workers: o.workers,
            explain: o.explain,
            quiet: o.quiet,
            color: o.color.clone(),
            ml: o.ml,
            model: o.model.clone(),
        }
    }
}

// Back-compat type alias used by benchmark module
pub type Args = LegacyScanConfig;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ModeArg {
    /// Tier 1+4 + light artifacts — fastest
    Fast,
    /// Tier 2 on suspects + full artifacts — recommended
    Balanced,
    /// Exhaustive search on every file — slowest
    Max,
}

impl ModeArg {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::Max => "max",
        }
    }
}

impl From<ModeArg> for ScanMode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::Fast => ScanMode::Fast,
            ModeArg::Balanced => ScanMode::Balanced,
            ModeArg::Max => ScanMode::Max,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    /// Colored table (default, best for terminals)
    Text,
    /// Machine-readable JSON
    Json,
    /// Spreadsheet-friendly CSV
    Csv,
    /// Standalone HTML report
    Html,
}
