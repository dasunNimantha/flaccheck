//! Unit tests for testkit metrics and corpus helpers.

use flaccheck_core::{AnalysisResult, TranscodeVerdict};
use flaccheck_testkit::{evaluate_suite, full_corpus, matches_truth, GroundTruth, LabeledCase};

fn stub_result(verdict: TranscodeVerdict, confidence: f64, info: f64) -> AnalysisResult {
    AnalysisResult {
        path: "test".into(),
        transcode_verdict: verdict,
        hires_verdict: flaccheck_core::HiresVerdict::Unknown,
        confidence,
        spectral_info_score: info,
        codec_guess: None,
        est_source_bitrate_kbps: None,
        evidence: vec![],
        mode: flaccheck_core::ScanMode::Balanced,
        duration_secs: 3.0,
        sample_rate: 44100,
        channels: 1,
        bits_per_sample: Some(16),
    }
}

#[test]
fn matches_truth_genuine() {
    let r = stub_result(TranscodeVerdict::Genuine, 0.9, 0.8);
    assert!(matches_truth(&r, GroundTruth::Genuine));
}

#[test]
fn matches_truth_transcoded_accepts_suspicious() {
    let r = stub_result(TranscodeVerdict::Suspicious, 0.7, 0.5);
    assert!(matches_truth(&r, GroundTruth::Transcoded));
}

#[test]
fn evaluate_suite_counts_failures() {
    let cases: Vec<(LabeledCase, AnalysisResult)> = full_corpus()
        .into_iter()
        .take(5)
        .map(|c| {
            let r = stub_result(TranscodeVerdict::Genuine, 0.9, 0.8);
            (c, r)
        })
        .collect();
    let m = evaluate_suite("test", &cases);
    assert_eq!(m.total, 5);
    assert!(m.pass_rate <= 1.0);
}
