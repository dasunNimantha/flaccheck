use crate::ui::Ui;
use lossless_scan_core::AnalysisResult;
use minijinja::{context, Environment};
use serde::Serialize;

#[derive(Clone, Copy)]
pub enum OutputFormat {
    Text,
    Json,
    Csv,
    Html,
}

impl OutputFormat {
    pub fn from_format_arg(f: crate::args::FormatArg) -> Self {
        match f {
            crate::args::FormatArg::Text => Self::Text,
            crate::args::FormatArg::Json => Self::Json,
            crate::args::FormatArg::Csv => Self::Csv,
            crate::args::FormatArg::Html => Self::Html,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ScanReport {
    pub results: Vec<AnalysisResult>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

impl ScanReport {
    pub fn render(
        &self,
        format: OutputFormat,
        explain: bool,
        ui: &Ui,
    ) -> Result<String, Box<dyn std::error::Error>> {
        match format {
            OutputFormat::Json => Ok(serde_json::to_string_pretty(self)?),
            OutputFormat::Csv => Ok(render_csv(&self.results)),
            OutputFormat::Html => Ok(render_html(self, explain)?),
            OutputFormat::Text => Ok(render_text(self, explain, ui)),
        }
    }

    pub fn suspicious_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| {
                matches!(
                    r.transcode_verdict,
                    lossless_scan_core::TranscodeVerdict::Transcoded
                        | lossless_scan_core::TranscodeVerdict::Suspicious
                )
            })
            .count()
    }
}

fn render_text(report: &ScanReport, explain: bool, ui: &Ui) -> String {
    let mut out = ui.format_results_table(&report.results, explain);
    if !out.is_empty() && !report.skipped.is_empty() || !report.errors.is_empty() {
        out.push('\n');
    }
    out.push_str(&ui.format_skipped_errors(&report.skipped, &report.errors));
    out
}

fn render_csv(results: &[AnalysisResult]) -> String {
    let mut out = String::from(
        "path,transcode_verdict,hires_verdict,confidence,spectral_info_score,sample_rate,channels,codec_guess,est_bitrate_kbps\n",
    );
    for r in results {
        out.push_str(&format!(
            "\"{}\",{},{},{:.4},{:.4},{},{},\"{}\",{}\n",
            r.path.replace('"', "\"\""),
            r.transcode_verdict,
            r.hires_verdict,
            r.confidence,
            r.spectral_info_score,
            r.sample_rate,
            r.channels,
            r.codec_guess.as_deref().unwrap_or(""),
            r.est_source_bitrate_kbps
                .map(|b| b.to_string())
                .unwrap_or_default()
        ));
    }
    out
}

fn render_html(report: &ScanReport, explain: bool) -> Result<String, Box<dyn std::error::Error>> {
    let env = Environment::new();
    let tmpl = env.template_from_str(
        r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>lossless-scan report</title>
<style>
body{font-family:system-ui,sans-serif;margin:2rem;background:#0f1419;color:#e7e9ea}
h1{font-weight:600}
table{border-collapse:collapse;width:100%;margin-top:1rem}
th,td{border:1px solid #2f3336;padding:.6rem .75rem;text-align:left;font-size:.9rem}
th{background:#16181c;color:#8b98a5;text-transform:uppercase;font-size:.75rem;letter-spacing:.04em}
tr:hover td{background:#16181c}
.GENUINE{color:#00ba7c;font-weight:600}
.SUSPICIOUS{color:#ffad1f;font-weight:600}
.TRANSCODED{color:#f4212e;font-weight:600}
.INCONCLUSIVE{color:#8b98a5}
.UPSAMPLED,.PADDED_DEPTH{color:#7856ff;font-weight:600}
.summary{display:flex;gap:1.5rem;margin:1rem 0}
.stat{background:#16181c;border-radius:8px;padding:1rem 1.25rem;min-width:6rem}
.stat b{display:block;font-size:1.5rem}
details{margin:.25rem 0}
</style></head><body>
<h1>lossless-scan report</h1>
<div class="summary">
  <div class="stat"><b>{{ results|length }}</b> analyzed</div>
  <div class="stat"><b>{{ flagged }}</b> flagged</div>
  <div class="stat"><b>{{ skipped|length }}</b> skipped</div>
  <div class="stat"><b>{{ errors|length }}</b> errors</div>
</div>
<table>
<tr><th>File</th><th>Verdict</th><th>Hi-res</th><th>Confidence</th><th>HF score</th><th>Format</th></tr>
{% for r in results %}
<tr>
<td>{{ r.path }}</td>
<td class="{{ r.transcode_verdict }}">{{ r.transcode_verdict }}</td>
<td class="{{ r.hires_verdict }}">{{ r.hires_verdict }}</td>
<td>{{ "%.0f"|format(r.confidence * 100) }}%</td>
<td>{{ "%.2f"|format(r.spectral_info_score) }}</td>
<td>{{ r.sample_rate }} Hz · {{ r.channels }}ch{% if r.bits_per_sample %} · {{ r.bits_per_sample }}-bit{% endif %}</td>
</tr>
{% if explain %}
<tr><td colspan="6">
<details><summary>Evidence</summary><ul>
{% for e in r.evidence %}
<li><b>{{ e.detector }}</b> · {{ e.signal }} = {{ e.value }} (w={{ e.weight }}) — {{ e.note }}</li>
{% endfor %}
</ul></details>
</td></tr>
{% endif %}
{% endfor %}
</table>
{% if skipped %}<h2>Skipped</h2><ul>{% for s in skipped %}<li>{{ s }}</li>{% endfor %}</ul>{% endif %}
{% if errors %}<h2>Errors</h2><ul>{% for e in errors %}<li>{{ e }}</li>{% endfor %}</ul>{% endif %}
</body></html>"#,
    )?;

    let flagged = report
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

    Ok(tmpl.render(context! {
        results => &report.results,
        skipped => &report.skipped,
        errors => &report.errors,
        explain => explain,
        flagged => flagged,
    })?)
}
