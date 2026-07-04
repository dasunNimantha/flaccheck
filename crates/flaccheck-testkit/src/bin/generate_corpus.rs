//! Write synthetic labeled corpus to disk as WAV + manifest.json.

use flaccheck_testkit::{full_corpus, write_wav_f32_mono};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
struct ManifestEntry {
    path: String,
    label: String,
    id: String,
    category: String,
}

fn label_for_truth(truth: flaccheck_testkit::GroundTruth) -> &'static str {
    use flaccheck_testkit::GroundTruth;
    match truth {
        GroundTruth::Genuine => "genuine",
        GroundTruth::Transcoded | GroundTruth::NotGenuine => "transcoded",
        GroundTruth::Inconclusive => "inconclusive",
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("datasets/output/synthetic"));

    std::fs::create_dir_all(&out_dir)?;
    let corpus = full_corpus();
    let mut manifest = Vec::with_capacity(corpus.len());

    for case in &corpus {
        let mono = case.pcm.left();
        let wav_path = out_dir.join(format!("{}.wav", case.id));
        write_wav_f32_mono(&wav_path, &mono, case.pcm.sample_rate)?;
        manifest.push(ManifestEntry {
            path: wav_path.display().to_string(),
            label: label_for_truth(case.truth).to_string(),
            id: case.id.to_string(),
            category: case.category.to_string(),
        });
    }

    let manifest_path = out_dir.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    eprintln!(
        "Wrote {} WAV files and manifest ({} cases) to {}",
        corpus.len(),
        corpus.len(),
        out_dir.display()
    );
    Ok(())
}
