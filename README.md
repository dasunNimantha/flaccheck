# lossless-scan

CLI for **lossless audio authenticity analysis** — detect fake lossless files (for example, MP3 or AAC transcoded to FLAC) using research-backed spectral, quantization, and artifact detectors.

## Features

- **Three scan modes** — `fast`, `balanced` (default), and `max` trade speed for depth
- **Multiple output formats** — text, JSON, CSV, and HTML reports
- **Parallel scanning** — configurable worker count for directories
- **Broad decode support** — FLAC, WAV, AIFF, ALAC, AAC, MP3, Vorbis via symphonia; optional ffmpeg for APE, WavPack, Opus
- **Optional ML layer** — classical JSON model (default) or ONNX mel-CNN (`--features ml`) for borderline refinement
- **Feature dump** — export labeled evidence vectors for training (`features` subcommand)
- **Benchmark harness** — evaluate against labeled manifests for regression tracking

### Detection tiers

| Tier | Method | Reference |
|------|--------|-----------|
| 1 | Spectral cutoff / bitrate fingerprint | D'Alessandro & Shi, ACM MM&Sec 2009 |
| 2 | MDCT quantization residual | Derrien, JAES 2019 |
| 3 | Pre-echo, phase, joint-stereo artifacts | Lacroix et al., AES 2015 |
| 4 | Fake hi-res (upsample, padded depth) | Lacroix et al., AES 2015 |
| 5 | Abstention on band-limited content | — |

## Install

**Requirements:** Rust stable (2021 edition).

### From source (release binary)

```bash
git clone https://github.com/YOUR_ORG/lossless-scan.git
cd lossless-scan
cargo build --release -p lossless-scan
```

Binary: `target/release/lossless-scan`

Optional ML support (tract-onnx):

```bash
cargo build --release -p lossless-scan --features ml
```

### Install into `~/.cargo/bin`

```bash
cargo install --path crates/lossless-scan-cli
# with ML:
cargo install --path crates/lossless-scan-cli --features ml
```

## Usage

Commands use subcommands: `scan` (analyze files), `benchmark` (evaluate a manifest), `features` (dump ML training data), and `serve` (web UI).

```bash
# Web UI — drag-and-drop in browser
lossless-scan serve
# → http://127.0.0.1:8787

# Scan a single file (colored table output)
lossless-scan scan track.flac

# Directory scan, HTML report
lossless-scan scan /path/to/music --mode balanced --format html -o report.html

# Exhaustive analysis with detector evidence
lossless-scan scan track.flac --mode max --explain

# Fast library sweep, 8 workers
lossless-scan scan /music --mode fast --workers 8

# JSON for scripts (no progress noise)
lossless-scan scan album.flac --format json --quiet -o results.json

# Disable colors (CI / logs)
lossless-scan scan album.flac --color never

# Classical ML borderline refinement (no extra build flags)
lossless-scan scan --ml --model models/classical_model.json track.flac

# ONNX mel-CNN (requires --features ml build)
lossless-scan scan --ml --model models/borderline.onnx track.flac

# Dump features for training
lossless-scan features datasets/output/calibration/manifest.json --mode max -o features.jsonl

# Benchmark against a labeled manifest
lossless-scan benchmark datasets/output/synthetic/manifest.json --mode balanced -o metrics.json
```

### Verdict colors (text mode)

| Verdict | Meaning |
|---------|---------|
| **GENUINE** (green) | No strong transcoding evidence |
| **SUSPICIOUS** (yellow) | Some lossy indicators |
| **TRANSCODED** (red) | Strong lossy fingerprint |
| **INCONCLUSIVE** (dim) | Too band-limited to judge |

### Scan modes

| Mode | What it runs | Typical cost |
|------|----------------|--------------|
| `fast` | Tier 1 + 4 + light artifacts | Seconds per file |
| `balanced` | + Tier 2 on suspects, full artifacts | Default; good library sweep |
| `max` | Exhaustive Tier 2 search, all tiers on every file | Slowest; highest recall |

## Decode support

**Native (symphonia):** FLAC, WAV, AIFF, ALAC, AAC, MP3, Vorbis

**Optional ffmpeg:** `.ape`, `.wv`, `.opus` — skipped with a clear message if ffmpeg is not installed

## Benchmarks and datasets

Synthetic transcode matrices for evaluation:

```bash
# Requires ffmpeg and a directory of source WAV/FLAC files
./datasets/generate.sh /path/to/lossless/sources datasets/output

# Run the benchmark harness against a manifest
lossless-scan benchmark tests/golden/manifest.example.json --mode balanced -o benchmark.json
```

The workspace includes an in-memory labeled corpus (60+ synthetic cases) exercised by `cargo test --workspace`. Tier 2 thresholds are structurally correct but **uncalibrated** until validated on a larger labeled database.

## Honest limitations

- `GENUINE` means *no evidence of transcoding*, not a cryptographic guarantee.
- Content bandwidth is measured with a **noise-floor spectral edge** (highest frequency
  with real content above the treble noise floor), not a 95%-energy rolloff. This is
  Nyquist-aware and robust to bass-heavy masters, so full-band audio no longer gets
  wrongly flagged as band-limited.
- `INCONCLUSIVE` is reserved for genuinely narrow-band material with no lossy cliff —
  where a hidden transcode cutoff cannot be told apart from a naturally dark source.
  Its confidence is intentionally shown as `—`.
- High-bitrate transcodes (256 kbps CBR / V0) whose cutoff sits near 19–20 kHz are an
  inherently ambiguous zone and may read `SUSPICIOUS` rather than a hard verdict.
- Tier 2 thresholds are calibrated via `lossless-scan-calibrate` against an ffmpeg
  transcode matrix (`datasets/generate.sh`). Baked defaults target balanced recall.
- MP3 PQMF and joint-stereo detectors are heuristic — require a lossy cliff or
  corroborating quant evidence for high-confidence transcode verdicts.

## Research / ML training

Offline ONNX export and training notes: [`research/README.md`](research/README.md) and `research/train.py`. The CLI runs without model weights (graceful no-op).

## Development

```bash
# Format, lint, test, release build (same as CI)
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p lossless-scan
```

## License

MIT — see workspace `Cargo.toml` (`license = "MIT"`).
