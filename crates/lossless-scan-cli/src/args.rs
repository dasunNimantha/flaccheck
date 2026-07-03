use clap::{Parser, ValueEnum};
use lossless_scan_core::ScanMode;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "lossless-scan", about = "Research-backed lossless audio authenticity analyzer")]
pub struct Args {
  pub path: PathBuf,
  #[arg(long, value_enum, default_value = "balanced")]
  pub mode: ModeArg,
  #[arg(long, value_enum, default_value = "text")]
  pub format: FormatArg,
  #[arg(short, long)]
  pub output: Option<PathBuf>,
  #[arg(long, default_value_t = 4)]
  pub workers: usize,
  #[arg(long)]
  pub ml: bool,
  #[arg(long)]
  pub model: Option<PathBuf>,
  #[arg(long)]
  pub explain: bool,
  #[arg(long)]
  pub benchmark: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ModeArg {
  Fast,
  Balanced,
  Max,
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

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum FormatArg {
  Text,
  Json,
  Csv,
  Html,
}
