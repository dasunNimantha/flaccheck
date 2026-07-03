# Offline ML training sandbox

Train a borderline classifier and export ONNX for `lossless-scan --ml`.

## Setup

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

## Train (synthetic demo)

```bash
python train.py --out ../models/borderline.onnx
```

The shipped CLI works without weights. When `models/borderline.onnx` exists:

```bash
cargo run -p lossless-scan --features ml -- --ml --model models/borderline.onnx track.flac
```

## Features

Stereo mid/side mel-spectrogram (per FLAC Detective v0.14 lesson: MP3 fingerprints survive in side channel).

## TODO

- Train on EAC/XLD-certified FLAC + synthetic transcode matrix from `datasets/generate.sh`
- Replace heuristic fallback in `lossless-scan-ml` with tract-onnx inference
