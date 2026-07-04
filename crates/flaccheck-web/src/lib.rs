use axum::{
    extract::{DefaultBodyLimit, Multipart, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use flaccheck_core::{AnalysisConfig, AnalysisResult, ScanMode, TranscodeVerdict};
use flaccheck_decode::{decode_file, DecodeError};
use flaccheck_detectors::analyze_pcm;
use flaccheck_ml::{MlClassifier, MlConfig};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
struct AppState {
    default_model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ScanQuery {
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default)]
    explain: bool,
    #[serde(default)]
    ml: bool,
}

fn default_mode() -> String {
    "balanced".to_string()
}

#[derive(Debug, Serialize)]
pub struct ScanReport {
    pub results: Vec<AnalysisResult>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ScanStats {
    pub total: usize,
    pub flagged: usize,
    pub genuine: usize,
    pub suspicious: usize,
    pub transcoded: usize,
    pub inconclusive: usize,
    pub skipped: usize,
    pub errors: usize,
    pub duration_ms: u64,
    pub mode: String,
}

#[derive(Debug, Serialize)]
struct ScanResponse {
    report: ScanReport,
    stats: ScanStats,
}

#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("multipart error: {0}")]
    Multipart(String),
    #[error("no audio files uploaded")]
    NoFiles,
    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            WebError::NoFiles => (StatusCode::BAD_REQUEST, self.to_string()),
            WebError::Multipart(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            WebError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub model_path: Option<String>,
}

pub async fn run_server(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let state = Arc::new(AppState {
        default_model: config.model_path,
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/assets/app.js", get(serve_js))
        .route("/assets/style.css", get(serve_css))
        .route("/api/scan", post(scan_handler))
        .layer(DefaultBodyLimit::max(512 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("flaccheck UI → http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(include_str!("assets/index.html"))
}

async fn serve_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("assets/app.js"),
    )
}

async fn serve_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("assets/style.css"),
    )
}

struct UploadItem {
    path: PathBuf,
    display_name: String,
}

async fn scan_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ScanQuery>,
    mut multipart: Multipart,
) -> Result<Json<ScanResponse>, WebError> {
    let started = Instant::now();
    let mode = parse_mode(&query.mode);
    let mode_label = query.mode.clone();
    let config = AnalysisConfig::for_mode(mode);
    let model_path = state.default_model.clone();
    let ml = MlClassifier::new(&MlConfig {
        enabled: query.ml || model_path.is_some(),
        model_path,
    });

    let temp_dir = tempfile::tempdir().map_err(|e| WebError::Internal(e.to_string()))?;
    let mut uploads: Vec<UploadItem> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| WebError::Multipart(e.to_string()))?
    {
        let display_name = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("upload_{}.flac", uploads.len()));
        let safe_name = sanitize_filename(&display_name);
        let path = temp_dir.path().join(&safe_name);
        let data = field
            .bytes()
            .await
            .map_err(|e| WebError::Multipart(e.to_string()))?;
        if data.is_empty() {
            continue;
        }
        std::fs::write(&path, &data).map_err(|e| WebError::Internal(e.to_string()))?;
        if is_audio_path(&path) {
            uploads.push(UploadItem {
                path,
                display_name,
            });
        }
    }

    if uploads.is_empty() {
        return Err(WebError::NoFiles);
    }

    let mut results = Vec::new();
    let mut skipped = Vec::new();
    let mut errors = Vec::new();

    for item in &uploads {
        analyze_path(
            &item.path,
            &item.display_name,
            &config,
            &ml,
            &mut results,
            &mut skipped,
            &mut errors,
        );
    }

    let report = ScanReport {
        results,
        skipped,
        errors,
    };
    let stats = build_stats(&report, &mode_label, started.elapsed().as_millis() as u64);

    Ok(Json(ScanResponse { report, stats }))
}

fn build_stats(report: &ScanReport, mode: &str, duration_ms: u64) -> ScanStats {
    let mut genuine = 0usize;
    let mut suspicious = 0usize;
    let mut transcoded = 0usize;
    let mut inconclusive = 0usize;

    for r in &report.results {
        match r.transcode_verdict {
            TranscodeVerdict::Genuine => genuine += 1,
            TranscodeVerdict::Suspicious => suspicious += 1,
            TranscodeVerdict::Transcoded => transcoded += 1,
            TranscodeVerdict::Inconclusive => inconclusive += 1,
        }
    }

    ScanStats {
        total: report.results.len(),
        flagged: suspicious + transcoded,
        genuine,
        suspicious,
        transcoded,
        inconclusive,
        skipped: report.skipped.len(),
        errors: report.errors.len(),
        duration_ms,
        mode: mode.to_string(),
    }
}

fn analyze_path(
    path: &Path,
    display_name: &str,
    config: &AnalysisConfig,
    ml: &MlClassifier,
    results: &mut Vec<AnalysisResult>,
    skipped: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    let path_str = path.display().to_string();
    match decode_file(path) {
        Ok(pcm) => match analyze_pcm(&path_str, &pcm, config) {
            Ok(mut result) => {
                let _ = ml.refine_borderline(&pcm, &mut result);
                result.path = display_name.to_string();
                results.push(result);
            }
            Err(e) => errors.push(format!("{display_name}: {e}")),
        },
        Err(DecodeError::FfmpegRequired { ext }) => {
            skipped.push(format!(
                "{display_name}: requires ffmpeg for .{ext} (install ffmpeg or skip)"
            ));
        }
        Err(e) => errors.push(format!("{display_name}: {e}")),
    }
}

fn parse_mode(s: &str) -> ScanMode {
    match s.to_lowercase().as_str() {
        "fast" => ScanMode::Fast,
        "max" => ScanMode::Max,
        _ => ScanMode::Balanced,
    }
}

fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload.flac");
    base.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn is_audio_path(path: &Path) -> bool {
    const EXT: &[&str] = &[
        "flac", "wav", "wave", "aiff", "aif", "mp3", "aac", "m4a", "ogg", "opus", "ape", "wv",
        "alac",
    ];
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
