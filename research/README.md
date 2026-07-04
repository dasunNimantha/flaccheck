# Offline ML training sandbox

Train borderline classifiers for `flaccheck --ml`. ML only refines **borderline** tracks (suspicious or confidence 0.35–0.65); confident heuristic verdicts are unchanged.

## Setup

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

## Phase 1 — Classical model (recommended first)

Uses the ~23 detector evidence scalars already computed by the pipeline. Pure-Rust inference from a small JSON file — no ONNX dependency.

### 1. Dump features from a labeled manifest

```bash
cargo run -p flaccheck --release -- features \
  datasets/output/calibration/manifest.json \
  --mode max -o features.jsonl --quiet
```

### 2. Train logistic regression

```bash
cd research
python train_classical.py --features ../features.jsonl --out ../models/classical_model.json
```

Exports `models/classical_model.json` with feature order, coefficients, intercept, and StandardScaler params.

### 3. Scan with ML

```bash
cargo run -p flaccheck --release -- \
  --ml --model models/classical_model.json scan track.flac
```

### 4. Benchmark with ML

```bash
cargo run -p flaccheck --release -- \
  benchmark datasets/output/calibration/manifest.json \
  --mode max --ml --model models/classical_model.json --format json
```

## Phase 2 — Mel CNN (optional, higher ceiling)

Mid/side mel-spectrogram CNN via `tract-onnx`. Requires building with the `ml` feature.

### Mel parameters

Locked in [`mel_config.py`](mel_config.py) and [`../crates/flaccheck-ml/src/mel.rs`](../crates/flaccheck-ml/src/mel.rs): `n_fft=2048`, `hop=512`, `n_mels=64`, `n_frames=128`.

### Train and export ONNX

```bash
cd research
# Demo untrained export:
python train.py --demo --out ../models/borderline.onnx

# Train on calibration manifest:
python train.py --manifest ../datasets/output/calibration/manifest.json \
  --out ../models/borderline.onnx --epochs 30
```

### Scan with ONNX model

```bash
cargo run -p flaccheck --release --features ml -- \
  --ml --model models/borderline.onnx scan track.flac
```

## Model routing

| `--model` extension | Runtime        | Build flags   |
|---------------------|----------------|---------------|
| `.json`             | Pure Rust      | default       |
| `.onnx`             | tract-onnx CNN | `--features ml` |

## Feature list

Canonical order lives in `flaccheck-core` as `ML_FEATURE_ORDER` (`detector.signal` keys). Training and inference must use the same order.

## TODO

- Add real EAC/XLD-certified FLAC negatives alongside synthetic transcodes
- Parity test: Rust mel vs librosa on fixed fixture WAV
