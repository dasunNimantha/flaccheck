# lossless-scan

Research-backed CLI to detect fake lossless audio (e.g. MP3/AAC transcoded to FLAC).

## Papers implemented

| Tier | Method | Reference |
|------|--------|-----------|
| 1 | Spectral cutoff / bitrate fingerprint | D'Alessandro & Shi, ACM MM&Sec 2009 |
| 2 | MDCT quantization residual | Derrien, JAES 2019 |
| 3 | Pre-echo, phase, joint-stereo artifacts | Lacroix et al. AES 2015 (distortion list) |
| 4 | Fake hi-res (upsample, padded depth) | Lacroix et al. AES 2015 |
| 5 | Abstention on band-limited content | — |

## Install

```bash
cd lossless-scan
cargo build --release
# optional ML (tract-onnx):
cargo build --release --features ml
```

## Usage

```bash
cargo run -p lossless-scan -- /path/to/music --mode balanced --format html -o report.html
cargo run -p lossless-scan -- track.flac --mode max --explain
cargo run -p lossless-scan -- /music --mode fast --workers 8
cargo run -p lossless-scan -- --ml --model models/borderline.onnx track.flac
```

### Modes

- `fast` — Tier 1 + 4 + light artifacts (~seconds per file)
- `balanced` — + Tier 2 on suspects, full artifacts (default)
- `max` — exhaustive Tier 2 search, all tiers on every file

## Decode support

**Native (symphonia):** FLAC, WAV, AIFF, ALAC, AAC, MP3, Vorbis

**Optional ffmpeg:** `.ape`, `.wv`, `.opus` — skipped with a clear message if ffmpeg is absent

## Honest limits

- `GENUINE` means *no evidence of transcoding*, not a cryptographic guarantee.
- Naturally band-limited recordings (rolloff &lt; 7 kHz) return `INCONCLUSIVE`.
- Tier 2 thresholds are structurally correct but **uncalibrated** until run against a labeled DB (see `datasets/`).
- MP3 PQMF detector is stubbed (`TODO` in evidence).

## Benchmarks

```bash
# Generate synthetic transcodes (requires ffmpeg + source WAV/FLAC)
./datasets/generate.sh /path/to/lossless/sources datasets/output

# Run benchmark harness
cargo run -p lossless-scan -- --benchmark tests/golden/manifest.example.json --mode balanced -o benchmark.json /dev/null
```

## Research / ML training

See `research/README.md` and `research/train.py` for offline ONNX export. The CLI runs without model weights (graceful no-op).

## Tests

```bash
cargo test --workspace
```
