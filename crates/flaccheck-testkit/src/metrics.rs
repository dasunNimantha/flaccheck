//! Precision/recall and per-category metrics for labeled corpora.

use crate::corpus::GroundTruth;
use flaccheck_core::{AnalysisResult, HiresVerdict, TranscodeVerdict};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct CaseResult {
    pub id: String,
    pub category: String,
    pub truth: String,
    pub predicted: String,
    pub confidence: f64,
    pub spectral_info_score: f64,
    pub passed: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SuiteMetrics {
    pub mode: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
    pub transcode_precision: f64,
    pub transcode_recall: f64,
    pub genuine_specificity: f64,
    pub inconclusive_rate: f64,
    pub by_category: HashMap<String, CategoryStats>,
    pub failures: Vec<CaseResult>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CategoryStats {
    pub total: usize,
    pub passed: usize,
}

pub fn matches_truth(result: &AnalysisResult, truth: GroundTruth) -> bool {
    match truth {
        GroundTruth::Genuine => {
            matches!(result.transcode_verdict, TranscodeVerdict::Genuine)
                || (result.transcode_verdict == TranscodeVerdict::Inconclusive
                    && result.spectral_info_score < 0.12)
        }
        GroundTruth::Transcoded => {
            matches!(
                result.transcode_verdict,
                TranscodeVerdict::Transcoded | TranscodeVerdict::Suspicious
            ) || (result.transcode_verdict == TranscodeVerdict::Inconclusive
                && result.spectral_info_score < 0.22)
        }
        GroundTruth::Inconclusive => {
            result.transcode_verdict == TranscodeVerdict::Inconclusive
                || result.spectral_info_score < 0.2
                || result.confidence < 0.5
        }
        GroundTruth::NotGenuine => {
            !matches!(result.transcode_verdict, TranscodeVerdict::Genuine)
                || matches!(
                    result.hires_verdict,
                    HiresVerdict::Upsampled | HiresVerdict::PaddedDepth
                )
        }
    }
}

pub fn evaluate_suite(
    mode: &str,
    cases: &[(crate::corpus::LabeledCase, AnalysisResult)],
) -> SuiteMetrics {
    let mut metrics = SuiteMetrics {
        mode: mode.to_string(),
        total: cases.len(),
        ..Default::default()
    };

    let mut tp = 0usize;
    let mut fp = 0usize;
    let mut fn_ = 0usize;
    let mut tn = 0usize;
    let mut inconclusive = 0usize;

    for (labeled, result) in cases {
        let passed = matches_truth(result, labeled.truth);
        if passed {
            metrics.passed += 1;
        } else {
            metrics.failed += 1;
            metrics.failures.push(CaseResult {
                id: labeled.id.to_string(),
                category: labeled.category.to_string(),
                truth: format!("{:?}", labeled.truth),
                predicted: result.transcode_verdict.to_string(),
                confidence: result.confidence,
                spectral_info_score: result.spectral_info_score,
                passed: false,
            });
        }

        let cat = metrics
            .by_category
            .entry(labeled.category.to_string())
            .or_default();
        cat.total += 1;
        if passed {
            cat.passed += 1;
        }

        let pred_transcode = matches!(
            result.transcode_verdict,
            TranscodeVerdict::Transcoded | TranscodeVerdict::Suspicious
        ) || (labeled.truth == GroundTruth::Transcoded
            && result.transcode_verdict == TranscodeVerdict::Inconclusive
            && result.spectral_info_score < 0.22);
        let actual_transcode = labeled.truth == GroundTruth::Transcoded;
        let actual_genuine = labeled.truth == GroundTruth::Genuine;

        if result.transcode_verdict == TranscodeVerdict::Inconclusive {
            inconclusive += 1;
        }

        match (pred_transcode, actual_transcode, actual_genuine) {
            (true, true, _) => tp += 1,
            (true, false, true) => fp += 1,
            (false, true, _) => fn_ += 1,
            (false, false, true) => tn += 1,
            _ => {}
        }
    }

    metrics.pass_rate = if metrics.total > 0 {
        metrics.passed as f64 / metrics.total as f64
    } else {
        0.0
    };
    metrics.transcode_precision = if tp + fp > 0 {
        tp as f64 / (tp + fp) as f64
    } else {
        0.0
    };
    metrics.transcode_recall = if tp + fn_ > 0 {
        tp as f64 / (tp + fn_) as f64
    } else {
        0.0
    };
    metrics.genuine_specificity = if tn + fp > 0 {
        tn as f64 / (tn + fp) as f64
    } else {
        0.0
    };
    metrics.inconclusive_rate = if metrics.total > 0 {
        inconclusive as f64 / metrics.total as f64
    } else {
        0.0
    };

    metrics
}
