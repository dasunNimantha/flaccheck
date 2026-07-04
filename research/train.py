#!/usr/bin/env python3
"""Train mid/side mel CNN and export ONNX for lossless-scan --ml --model borderline.onnx."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np

try:
    import torch
    import torch.nn as nn
    from torch.utils.data import DataLoader, TensorDataset
    import librosa
    import soundfile as sf
except ImportError as e:
    raise SystemExit("Install deps: pip install -r requirements.txt") from e

from mel_config import F_MIN, HOP_LENGTH, N_FFT, N_FRAMES, N_MELS

POSITIVE_LABELS = {"transcoded", "fake"}


class TinyCnn(nn.Module):
    def __init__(self) -> None:
        super().__init__()
        self.net = nn.Sequential(
            nn.Conv2d(2, 8, 3, padding=1),
            nn.ReLU(),
            nn.AdaptiveAvgPool2d(1),
            nn.Flatten(),
            nn.Linear(8, 1),
            nn.Sigmoid(),
        )

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.net(x)


def load_manifest(path: Path) -> list[dict]:
    return json.loads(path.read_text(encoding="utf-8"))


def mid_side_mel(path: Path, max_secs: float = 30.0) -> np.ndarray:
    audio, sr = sf.read(path, always_2d=True)
    if audio.shape[1] == 1:
        audio = np.repeat(audio, 2, axis=1)
    max_samples = int(max_secs * sr)
    if audio.shape[0] > max_samples:
        start = (audio.shape[0] - max_samples) // 2
        audio = audio[start : start + max_samples]

    mid = (audio[:, 0] + audio[:, 1]) * 0.5
    side = (audio[:, 0] - audio[:, 1]) * 0.5

    def channel_mel(channel: np.ndarray) -> np.ndarray:
        mel = librosa.feature.melspectrogram(
            y=channel.astype(np.float32),
            sr=sr,
            n_fft=N_FFT,
            hop_length=HOP_LENGTH,
            n_mels=N_MELS,
            fmin=F_MIN,
            fmax=sr / 2.0,
            power=2.0,
        )
        mel = np.log(np.maximum(mel, 1e-10))
        # mel shape: (n_mels, time)
        t = mel.shape[1]
        out = np.zeros((N_MELS, N_FRAMES), dtype=np.float32)
        if t >= N_FRAMES:
            start = (t - N_FRAMES) // 2
            out = mel[:, start : start + N_FRAMES].astype(np.float32)
        else:
            pad = (N_FRAMES - t) // 2
            out[:, pad : pad + t] = mel.astype(np.float32)
        return out

    mel_mid = channel_mel(mid)
    mel_side = channel_mel(side)
    # NCHW for TinyCnn: (2, time, mel) -> conv expects (C, H, W) = (2, 128, 64)
    stacked = np.stack([mel_mid.T, mel_side.T], axis=0)  # (2, 128, 64)
    return stacked


def build_dataset(manifest: Path) -> tuple[np.ndarray, np.ndarray]:
    entries = load_manifest(manifest)
    xs: list[np.ndarray] = []
    ys: list[int] = []
    for entry in entries:
        p = Path(entry["path"])
        if not p.exists():
            continue
        xs.append(mid_side_mel(p))
        ys.append(1 if entry["label"] in POSITIVE_LABELS else 0)
    if not xs:
        raise SystemExit(f"no audio found for manifest {manifest}")
    return np.stack(xs, axis=0), np.asarray(ys, dtype=np.float32)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("../datasets/output/calibration/manifest.json"),
    )
    parser.add_argument("--out", type=Path, default=Path("../models/borderline.onnx"))
    parser.add_argument("--epochs", type=int, default=30)
    parser.add_argument("--batch-size", type=int, default=8)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--demo", action="store_true", help="export untrained demo ONNX only")
    args = parser.parse_args()

    model = TinyCnn()
    model.eval()

    if args.demo:
        dummy = torch.randn(1, 2, N_FRAMES, N_MELS)
        args.out.parent.mkdir(parents=True, exist_ok=True)
        torch.onnx.export(
            model,
            dummy,
            str(args.out),
            input_names=["mel_mid_side"],
            output_names=["transcode_prob"],
            dynamic_axes={"mel_mid_side": {0: "batch"}, "transcode_prob": {0: "batch"}},
            opset_version=17,
        )
        print(f"Wrote demo ONNX to {args.out}")
        return

    x, y = build_dataset(args.manifest)
    print(f"Training on {len(y)} clips, shape {x.shape}")

    x_t = torch.from_numpy(x)
    y_t = torch.from_numpy(y).unsqueeze(1)
    loader = DataLoader(TensorDataset(x_t, y_t), batch_size=args.batch_size, shuffle=True)
    opt = torch.optim.Adam(model.parameters(), lr=args.lr)
    loss_fn = nn.BCELoss()

    model.train()
    for epoch in range(args.epochs):
        total = 0.0
        for xb, yb in loader:
            opt.zero_grad()
            pred = model(xb)
            loss = loss_fn(pred, yb)
            loss.backward()
            opt.step()
            total += loss.item() * len(xb)
        print(f"epoch {epoch + 1}/{args.epochs} loss={total / len(y):.4f}")

    model.eval()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    dummy = torch.randn(1, 2, N_FRAMES, N_MELS)
    torch.onnx.export(
        model,
        dummy,
        str(args.out),
        input_names=["mel_mid_side"],
        output_names=["transcode_prob"],
        dynamic_axes={"mel_mid_side": {0: "batch"}, "transcode_prob": {0: "batch"}},
        opset_version=17,
    )
    print(f"Wrote trained ONNX to {args.out}")


if __name__ == "__main__":
    main()
