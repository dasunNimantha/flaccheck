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
  pub fn render(&self, format: OutputFormat, explain: bool) -> Result<String, Box<dyn std::error::Error>> {
    match format {
      OutputFormat::Json => Ok(serde_json::to_string_pretty(self)?),
      OutputFormat::Csv => Ok(render_csv(&self.results)),
      OutputFormat::Html => Ok(render_html(self, explain)?),
      OutputFormat::Text => Ok(render_text(self, explain)),
    }
  }
}

fn render_text(report: &ScanReport, explain: bool) -> String {
  let mut out = String::new();
  for r in &report.results {
    out.push_str(&format!(
      "{} | {} | conf {:.2} | hires {} | {} Hz {}ch\n",
      r.path,
      r.transcode_verdict,
      r.confidence,
      r.hires_verdict,
      r.sample_rate,
      r.channels
    ));
    if explain {
      for e in &r.evidence {
        out.push_str(&format!(
          "  [{}] {} = {:.3} w={:.2}: {}\n",
          e.detector, e.signal, e.value, e.weight, e.note
        ));
      }
    }
  }
  for s in &report.skipped {
    out.push_str(&format!("SKIP: {s}\n"));
  }
  for e in &report.errors {
    out.push_str(&format!("ERROR: {e}\n"));
  }
  out
}

fn render_csv(results: &[AnalysisResult]) -> String {
  let mut out =
    String::from("path,transcode_verdict,hires_verdict,confidence,spectral_info_score,sample_rate,channels,codec_guess,est_bitrate_kbps\n");
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
      r.est_source_bitrate_kbps.map(|b| b.to_string()).unwrap_or_default()
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
body{font-family:system-ui,sans-serif;margin:2rem;background:#111;color:#eee}
table{border-collapse:collapse;width:100%}
th,td{border:1px solid #333;padding:.5rem;text-align:left}
.GENUINE{color:#4ade80}.SUSPICIOUS{color:#fbbf24}.TRANSCODED{color:#f87171}.INCONCLUSIVE{color:#94a3b8}
details{margin:.25rem 0}
</style></head><body>
<h1>lossless-scan report</h1>
<p>{{ results|length }} files analyzed, {{ skipped|length }} skipped, {{ errors|length }} errors</p>
<table>
<tr><th>Path</th><th>Transcode</th><th>Hi-res</th><th>Conf</th><th>Info</th></tr>
{% for r in results %}
<tr>
<td>{{ r.path }}</td>
<td class="{{ r.transcode_verdict }}">{{ r.transcode_verdict }}</td>
<td>{{ r.hires_verdict }}</td>
<td>{{ "%.2f"|format(r.confidence) }}</td>
<td>{{ "%.2f"|format(r.spectral_info_score) }}</td>
</tr>
{% if explain %}
<tr><td colspan="5">
<details><summary>Evidence</summary><ul>
{% for e in r.evidence %}
<li><b>{{ e.detector }}</b> {{ e.signal }} = {{ e.value }} — {{ e.note }}</li>
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

  Ok(tmpl.render(context! {
    results => &report.results,
    skipped => &report.skipped,
    errors => &report.errors,
    explain => explain,
  })?)
}
