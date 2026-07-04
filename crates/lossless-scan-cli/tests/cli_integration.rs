//! CLI integration: decode WAV from testkit corpus and analyze end-to-end.

use lossless_scan_core::{AnalysisConfig, ScanMode, TranscodeVerdict};
use lossless_scan_ml::{MlClassifier, MlConfig};
use lossless_scan_testkit::{full_corpus, write_wav_f32_mono};
use std::process::Command;

fn lossless_scan_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lossless-scan"))
}

#[test]
fn cli_scan_synthetic_wav_fast() {
    let dir = tempfile::tempdir().unwrap();
    let case = full_corpus()
        .into_iter()
        .find(|c| c.id == "genuine_noise_1")
        .unwrap();
    let wav = dir.path().join("genuine.wav");
    write_wav_f32_mono(&wav, &case.pcm.left(), case.pcm.sample_rate).unwrap();

    let out = lossless_scan_bin()
        .args([
            "scan",
            "--mode",
            "fast",
            "--format",
            "json",
            "--quiet",
            wav.to_str().unwrap(),
        ])
        .output()
        .expect("run cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("GENUINE") || stdout.contains("genuine"));
}

#[test]
fn cli_scan_brick_wall_reads_suspicious_or_transcoded() {
    let dir = tempfile::tempdir().unwrap();
    let case = full_corpus()
        .into_iter()
        .find(|c| c.id == "transcode_brick_11k")
        .unwrap();
    let wav = dir.path().join("brick.wav");
    write_wav_f32_mono(&wav, &case.pcm.left(), case.pcm.sample_rate).unwrap();

    let out = lossless_scan_bin()
        .args([
            "scan",
            "--mode",
            "balanced",
            "--quiet",
            wav.to_str().unwrap(),
        ])
        .output()
        .expect("run cli");

    assert!(out.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("TRANSCODED")
            || combined.contains("SUSPICIOUS")
            || combined.contains("INCONCLUSIVE"),
        "brick-wall should not read as genuine: {combined}"
    );
}

#[test]
fn decode_roundtrip_matches_direct_analyze() {
    use lossless_scan_decode::decode_file;
    use lossless_scan_detectors::analyze_pcm;

    let dir = tempfile::tempdir().unwrap();
    // Use a robust wideband-noise case: its verdict is stable, so this exercises decode-path
    // consistency without depending on a borderline signal whose verdict can flip on tiny
    // float round-trip differences (e.g. synthetic multitones with silence above the top tone).
    let case = full_corpus()
        .into_iter()
        .find(|c| c.id == "genuine_noise_1")
        .unwrap();
    let wav = dir.path().join("tone.wav");
    write_wav_f32_mono(&wav, &case.pcm.left(), case.pcm.sample_rate).unwrap();

    let pcm = decode_file(&wav).expect("decode wav");
    let cfg = AnalysisConfig::for_mode(ScanMode::Fast);
    let direct = analyze_pcm("direct", &case.pcm, &cfg).unwrap();
    let decoded = analyze_pcm(wav.to_str().unwrap(), &pcm, &cfg).unwrap();

    assert_eq!(
        direct.transcode_verdict, decoded.transcode_verdict,
        "decode roundtrip verdict mismatch"
    );
}

#[test]
fn analyze_one_wideband_genuine() {
    use lossless_scan::analyze_one;
    use std::path::Path;

    let dir = tempfile::tempdir().unwrap();
    let case = full_corpus()
        .into_iter()
        .find(|c| c.id == "genuine_noise_42")
        .unwrap();
    let wav = dir.path().join("noise.wav");
    write_wav_f32_mono(&wav, &case.pcm.left(), case.pcm.sample_rate).unwrap();

    let cfg = AnalysisConfig::for_mode(ScanMode::Fast);
    let ml = MlClassifier::new(&MlConfig {
        enabled: false,
        model_path: None,
    });
    let outcome = analyze_one(Path::new(&wav), &cfg, &ml, false);
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let r = outcome.result.unwrap();
    assert!(!matches!(
        r.transcode_verdict,
        TranscodeVerdict::Transcoded | TranscodeVerdict::Suspicious
    ));
}
