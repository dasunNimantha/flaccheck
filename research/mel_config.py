#!/usr/bin/env python3
"""Shared mel-spectrogram parameters for Rust/Python parity."""

N_FFT = 2048
HOP_LENGTH = 512
N_MELS = 64
N_FRAMES = 128
F_MIN = 0.0
INPUT_CHANNELS = 2
INPUT_HEIGHT = N_MELS
INPUT_WIDTH = N_FRAMES
