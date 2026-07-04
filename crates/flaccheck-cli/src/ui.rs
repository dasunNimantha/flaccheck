//! Terminal styling, colors, and formatted output helpers.

use comfy_table::{presets::UTF8_FULL, Attribute, Cell, ContentArrangement, Table};
use console::{style, Emoji, Term};
use flaccheck_core::{AnalysisResult, HiresVerdict, TranscodeVerdict};
use std::path::Path;

static NOTE: Emoji<'_, '_> = Emoji("ℹ️ ", "i ");
static WARN: Emoji<'_, '_> = Emoji("⚠️ ", "! ");
static OK: Emoji<'_, '_> = Emoji("✓ ", "+ ");
static FAIL: Emoji<'_, '_> = Emoji("✗ ", "x ");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    pub fn from_arg(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

pub struct Ui {
    term: Term,
    pub color: bool,
    pub quiet: bool,
}

impl Ui {
    pub fn new(color_mode: ColorMode, quiet: bool) -> Self {
        let term = Term::stdout();
        let color = match color_mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => term.features().colors_supported(),
        };
        Self { term, color, quiet }
    }

    pub fn print_banner(&self, mode: &str) {
        if self.quiet {
            return;
        }
        let title = if self.color {
            style(" flaccheck ").bold().cyan().to_string()
        } else {
            " flaccheck ".to_string()
        };
        let subtitle = if self.color {
            style("lossless audio authenticity analyzer")
                .dim()
                .to_string()
        } else {
            "lossless audio authenticity analyzer".to_string()
        };
        let _ = self.term.write_line(&format!("{title}— {subtitle}"));
        let _ = self.term.write_line(&format!(
            "  mode: {mode}  ·  use --explain for detector evidence"
        ));
        let _ = self.term.write_line("");
    }

    pub fn progress_style() -> indicatif::ProgressStyle {
        indicatif::ProgressStyle::with_template(
            "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}",
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  ")
    }

    pub fn verdict_label(&self, v: TranscodeVerdict) -> String {
        let text = v.to_string();
        if !self.color {
            return text;
        }
        let styled = match v {
            TranscodeVerdict::Genuine => style(&text).green().bold(),
            TranscodeVerdict::Suspicious => style(&text).yellow().bold(),
            TranscodeVerdict::Transcoded => style(&text).red().bold(),
            TranscodeVerdict::Inconclusive => style(&text).dim(),
        };
        styled.to_string()
    }

    pub fn hires_label(&self, v: HiresVerdict) -> String {
        let text = v.to_string();
        if !self.color {
            return text;
        }
        let styled = match v {
            HiresVerdict::GenuineHires | HiresVerdict::Unknown => style(&text).dim(),
            HiresVerdict::Upsampled | HiresVerdict::PaddedDepth => style(&text).magenta().bold(),
        };
        styled.to_string()
    }

    pub fn format_results_table(&self, results: &[AnalysisResult], explain: bool) -> String {
        if results.is_empty() {
            return String::new();
        }

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("File").add_attribute(Attribute::Bold),
                Cell::new("Verdict").add_attribute(Attribute::Bold),
                Cell::new("Conf").add_attribute(Attribute::Bold),
                Cell::new("Hi-res").add_attribute(Attribute::Bold),
                Cell::new("Audio").add_attribute(Attribute::Bold),
            ]);

        if self.color {
            table.apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS);
        }

        for r in results {
            let file = display_path(&r.path);
            let audio = format!(
                "{} Hz · {} ch{}",
                r.sample_rate,
                r.channels,
                r.bits_per_sample
                    .map(|b| format!(" · {b}-bit"))
                    .unwrap_or_default()
            );
            let conf = if r.transcode_verdict == TranscodeVerdict::Inconclusive {
                "—".to_string()
            } else {
                format!("{:.0}%", r.confidence * 100.0)
            };
            table.add_row(vec![
                Cell::new(file),
                Cell::new(self.verdict_label(r.transcode_verdict)),
                Cell::new(conf),
                Cell::new(self.hires_label(r.hires_verdict)),
                Cell::new(audio),
            ]);
        }

        let mut out = table.to_string();

        if explain {
            out.push('\n');
            for r in results {
                if r.evidence.is_empty() {
                    continue;
                }
                let heading = if self.color {
                    style(display_path(&r.path)).underlined().to_string()
                } else {
                    display_path(&r.path)
                };
                out.push_str(&format!("\n{heading}\n"));
                for e in &r.evidence {
                    if e.weight <= 0.0
                        && !matches!(
                            e.signal.as_str(),
                            "brick_wall" | "early_rolloff" | "padded_depth" | "upsampled"
                        )
                    {
                        continue;
                    }
                    let line = format!(
                        "  {} [{}] {:.2} (w={:.2}) — {}",
                        if self.color {
                            style(&e.detector).dim().to_string()
                        } else {
                            e.detector.clone()
                        },
                        e.signal,
                        e.value,
                        e.weight,
                        e.note
                    );
                    out.push_str(&line);
                    out.push('\n');
                }
            }
        }

        out
    }

    pub fn print_summary(&self, total: usize, suspicious: usize, skipped: usize, errors: usize) {
        if self.quiet {
            return;
        }
        let _ = self.term.write_line("");
        let summary = if self.color {
            style(" Summary ").bold().on_cyan().black().to_string()
        } else {
            " Summary ".to_string()
        };
        let _ = self.term.write_line(&summary);

        let _ = self
            .term
            .write_line(&format!("  {OK} scanned: {total} file(s)"));
        if suspicious > 0 {
            let line = format!("  {WARN} flagged: {suspicious} suspicious or transcoded");
            let _ = self.term.write_line(&if self.color {
                style(line).yellow().to_string()
            } else {
                line
            });
        } else {
            let _ = self
                .term
                .write_line(&format!("  {OK} flagged: 0 suspicious or transcoded"));
        }
        if skipped > 0 {
            let _ = self
                .term
                .write_line(&format!("  {NOTE} skipped: {skipped}"));
        }
        if errors > 0 {
            let line = format!("  {FAIL} errors: {errors}");
            let _ = self.term.write_line(&if self.color {
                style(line).red().to_string()
            } else {
                line
            });
        }
    }

    pub fn status(&self, msg: &str) {
        if !self.quiet {
            let _ = self.term.write_line(&format!("{NOTE}{msg}"));
        }
    }

    pub fn success(&self, msg: &str) {
        if !self.quiet {
            let line = format!("{OK}{msg}");
            let _ = self.term.write_line(&if self.color {
                style(line).green().to_string()
            } else {
                line
            });
        }
    }

    pub fn warn_line(&self, msg: &str) {
        if !self.quiet {
            let line = format!("{WARN}{msg}");
            let _ = self.term.write_line(&if self.color {
                style(line).yellow().to_string()
            } else {
                line
            });
        }
    }

    pub fn error_line(&self, msg: &str) {
        let line = format!("{FAIL}{msg}");
        let _ = Term::stderr().write_line(&if self.color {
            style(line).red().to_string()
        } else {
            line
        });
    }

    pub fn format_skipped_errors(&self, skipped: &[String], errors: &[String]) -> String {
        let mut out = String::new();
        for s in skipped {
            let line = format!("{WARN} SKIP: {s}");
            out.push_str(&if self.color {
                style(line).yellow().dim().to_string()
            } else {
                line
            });
            out.push('\n');
        }
        for e in errors {
            let line = format!("{FAIL} ERROR: {e}");
            out.push_str(&if self.color {
                style(line).red().to_string()
            } else {
                line
            });
            out.push('\n');
        }
        out
    }

    pub fn format_benchmark_summary(
        &self,
        mode: &str,
        total: usize,
        precision: f64,
        recall: f64,
        inconclusive: f64,
    ) -> String {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_header(vec!["Metric", "Value"]);
        table.add_row(vec!["Mode", mode]);
        table.add_row(vec!["Samples", &total.to_string()]);
        table.add_row(vec![
            "Transcode precision",
            &format!("{:.1}%", precision * 100.0),
        ]);
        table.add_row(vec!["Transcode recall", &format!("{:.1}%", recall * 100.0)]);
        table.add_row(vec![
            "Inconclusive rate",
            &format!("{:.1}%", inconclusive * 100.0),
        ]);
        if self.color {
            format!("{}\n{}", style(" Benchmark results ").bold().cyan(), table)
        } else {
            format!("Benchmark results\n{table}")
        }
    }
}

fn display_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string()
}
