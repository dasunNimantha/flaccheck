#!/usr/bin/env python3
"""Minimal training sandbox exporting a demo ONNX model for borderline detection."""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np

try:
    import torch
    import torch.nn as nn
except ImportError as e:
    raise SystemExit("Install deps: pip install -r requirements.txt") from e


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


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=Path("../models/borderline.onnx"))
    args = parser.parse_args()

    model = TinyCnn()
    model.eval()

    # Demo weights only — replace with real training on transcode dataset
    dummy = torch.randn(1, 2, 128, 64)
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
    print("TODO: train on synthetic transcodes from datasets/generate.sh")


if __name__ == "__main__":
    main()
