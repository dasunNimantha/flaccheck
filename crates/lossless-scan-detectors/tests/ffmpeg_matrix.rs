//! FFmpeg-generated transcode matrix tests (run with `cargo test -- --ignored`).

use lossless_scan_core::{AnalysisConfig, ScanMode, TranscodeVerdict};
use lossless_scan_decode::decode_file;
use lossless_scan_detectors::analyze_pcm;
use std::path::PathBuf;

fn matrix_dir() -> Option<PathBuf> {
    let p = PathBuf::from("datasets/output/calibration");
    if p.join("manifest.json").exists() {
        Some(p)
    } else {
        None
    }
}

#[test]
#[ignore = "requires datasets/output/calibration from ./datasets/generate.sh"]
fn ffmpeg_matrix_transcoded_detected() {
    let dir = matrix_dir().expect("calibration matrix not generated");
    let manifest: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();

    let config = AnalysisConfig::for_mode(ScanMode::Balanced);
    let mut tp = 0usize;
    let mut total = 0usize;

    for entry in manifest {
        let label = entry["label"].as_str().unwrap_or("");
        if label != "transcoded" {
            continue;
        }
        let path = entry["path"].as_str().unwrap();
        let pcm = decode_file(PathBuf::from(path).as_path()).unwrap();
        let r = analyze_pcm(path, &pcm, &config).unwrap();
        total += 1;
        if matches!(
            r.transcode_verdict,
            TranscodeVerdict::Transcoded | TranscodeVerdict::Suspicious
        ) {
            tp += 1;
        }
    }

    assert!(total > 0, "no transcoded entries in matrix");
    let recall = tp as f64 / total as f64;
    assert!(
        recall >= 0.70,
        "ffmpeg matrix transcode recall {:.1}% ({tp}/{total}) below 70%",
        recall * 100.0
    );
}

#[test]
#[ignore = "requires datasets/output/calibration from ./datasets/generate.sh"]
fn ffmpeg_matrix_genuine_not_flagged() {
    let dir = matrix_dir().expect("calibration matrix not generated");
    let manifest: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();

    let config = AnalysisConfig::for_mode(ScanMode::Balanced);
    let mut ok = 0usize;
    let mut total = 0usize;

    for entry in manifest {
        let label = entry["label"].as_str().unwrap_or("");
        if label != "genuine" {
            continue;
        }
        let path = entry["path"].as_str().unwrap();
        let pcm = decode_file(PathBuf::from(path).as_path()).unwrap();
        let r = analyze_pcm(path, &pcm, &config).unwrap();
        total += 1;
        if r.transcode_verdict == TranscodeVerdict::Genuine
            || r.transcode_verdict == TranscodeVerdict::Inconclusive
        {
            ok += 1;
        }
    }

    assert!(total > 0);
    let rate = ok as f64 / total as f64;
    assert!(
        rate >= 0.80,
        "genuine specificity {:.1}% below 80%",
        rate * 100.0
    );
}
