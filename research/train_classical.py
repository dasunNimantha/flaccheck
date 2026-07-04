#!/usr/bin/env python3
"""Train a classical borderline classifier on features.jsonl and export JSON for Rust."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np

try:
    from sklearn.linear_model import LogisticRegression
    from sklearn.ensemble import RandomForestClassifier
    from sklearn.metrics import (
        average_precision_score,
        classification_report,
        precision_recall_fscore_support,
    )
    from sklearn.model_selection import train_test_split
    from sklearn.preprocessing import StandardScaler
except ImportError as e:
    raise SystemExit("Install deps: pip install -r requirements.txt") from e


POSITIVE_LABELS = {"transcoded", "fake"}


def load_features(path: Path) -> tuple[list[str], np.ndarray, np.ndarray]:
    feature_order: list[str] | None = None
    rows: list[list[float]] = []
    labels: list[int] = []

    with path.open(encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            if obj.get("type") == "meta":
                feature_order = obj["feature_order"]
                continue
            if obj.get("type") != "sample":
                continue
            y = 1 if obj["label"] in POSITIVE_LABELS else 0
            labels.append(y)
            if "feature_vector" in obj:
                rows.append(obj["feature_vector"])
            else:
                order = feature_order or list(obj["features"].keys())
                rows.append([obj["features"].get(k, 0.0) for k in order])

    if feature_order is None:
        raise SystemExit("features.jsonl missing meta line with feature_order")

    if not rows:
        raise SystemExit(f"no samples in {path}")

    return feature_order, np.asarray(rows, dtype=np.float64), np.asarray(labels, dtype=np.int32)


def export_logreg(
    feature_names: list[str],
    scaler: StandardScaler,
    clf: LogisticRegression,
    threshold: float,
    out: Path,
) -> None:
    payload = {
        "kind": "logistic_regression",
        "feature_names": feature_names,
        "coefficients": clf.coef_[0].tolist(),
        "intercept": float(clf.intercept_[0]),
        "scaler_mean": scaler.mean_.tolist(),
        "scaler_scale": scaler.scale_.tolist(),
        "threshold": threshold,
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote classical model to {out}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--features",
        type=Path,
        default=Path("../features.jsonl"),
        help="JSONL from: lossless-scan features manifest.json -o features.jsonl",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("../models/classical_model.json"),
    )
    parser.add_argument("--test-size", type=float, default=0.25)
    parser.add_argument("--seed", type=int, default=42)
    args = parser.parse_args()

    feature_names, x, y = load_features(args.features)
    print(f"Loaded {len(y)} samples, {len(feature_names)} features")

    x_train, x_test, y_train, y_test = train_test_split(
        x, y, test_size=args.test_size, random_state=args.seed, stratify=y if len(set(y)) > 1 else None
    )

    scaler = StandardScaler()
    x_train_s = scaler.fit_transform(x_train)
    x_test_s = scaler.transform(x_test)

    logreg = LogisticRegression(max_iter=2000, class_weight="balanced")
    logreg.fit(x_train_s, y_train)
    prob = logreg.predict_proba(x_test_s)[:, 1]
    pred = (prob >= 0.5).astype(int)
    p, r, f1, _ = precision_recall_fscore_support(y_test, pred, average="binary", zero_division=0)
    ap = average_precision_score(y_test, prob) if len(set(y_test)) > 1 else 0.0
    print(f"LogReg hold-out: precision={p:.3f} recall={r:.3f} f1={f1:.3f} AP={ap:.3f}")
    print(classification_report(y_test, pred, target_names=["genuine", "transcoded"], zero_division=0))

    rf = RandomForestClassifier(n_estimators=100, class_weight="balanced", random_state=args.seed)
    rf.fit(x_train_s, y_train)
    rf_pred = rf.predict(x_test_s)
    p2, r2, f1_2, _ = precision_recall_fscore_support(
        y_test, rf_pred, average="binary", zero_division=0
    )
    print(f"RandomForest hold-out (comparison): precision={p2:.3f} recall={r2:.3f} f1={f1_2:.3f}")

    # Retrain on full data for export
    x_full_s = scaler.fit_transform(x)
    logreg.fit(x_full_s, y)
    export_logreg(feature_names, scaler, logreg, threshold=0.5, out=args.out)


if __name__ == "__main__":
    main()
