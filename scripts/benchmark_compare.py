#!/usr/bin/env python3
"""Compare flaccheck vs other tools on a labeled manifest; emit JSON + chart PNGs."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "datasets/output/realistic/manifest.json"
OUT_DIR = ROOT / "docs/benchmarks"
FLACCHECK_SCAN = ROOT / "benchmarks/flaccheck_per_file.json"
FLAC_DETECTIVE = ROOT / "benchmarks/flac_detective.json"
FLACCHECK_BIN = ROOT / "target/release/flaccheck"


@dataclass
class Pred:
    tool: str
    path: str
    label: str
    predicted_fake: bool | None  # None = inconclusive / unknown
    codec: str


def load_manifest() -> list[dict]:
    return json.loads(MANIFEST.read_text())


def codec_from_path(path: str) -> str:
    name = Path(path).name
    if name.endswith("_genuine.flac"):
        return "genuine"
    m = re.search(r"_(mp3|aac|opus|vorbis)_", name)
    return m.group(1) if m else "other"

def flaccheck_fake(verdict: str) -> bool | None:
    v = verdict.upper()
    if v == "INCONCLUSIVE":
        return None
    if v in ("TRANSCODED", "SUSPICIOUS"):
        return True
    if v == "GENUINE":
        return False
    return None


def flac_detective_fake(verdict: str) -> bool | None:
    v = verdict.upper()
    if v == "AUTHENTIC":
        return False
    if v in ("FAKE_CERTAIN", "FAKE", "SUSPICIOUS", "WARNING"):
        return True
    return None


def aucdtect_fake(stdout: str) -> bool | None:
    # auCDtect: "CDDA" = genuine, "MPEG" / "AR" / etc. = lossy
    upper = stdout.upper()
    if "CDDA" in upper and "MPEG" not in upper:
        return False
    if "MPEG" in upper or "LOSSY" in upper:
        return True
    return None


def run_aucdtect(path: Path) -> bool | None:
    for cmd in ("aucdtect", "auCDtect"):
        try:
            proc = subprocess.run(
                [cmd, str(path)],
                capture_output=True,
                text=True,
                timeout=120,
            )
        except (FileNotFoundError, subprocess.TimeoutExpired):
            continue
        if proc.returncode == 0 or proc.stdout:
            return aucdtect_fake(proc.stdout + proc.stderr)
    return None


def metrics(preds: list[Pred]) -> dict:
    tp = fp = fn = tn = inconclusive = 0
    for p in preds:
        actual_fake = p.label in ("transcoded", "fake")
        if p.predicted_fake is None:
            inconclusive += 1
            continue
        if p.predicted_fake and actual_fake:
            tp += 1
        elif p.predicted_fake and not actual_fake:
            fp += 1
        elif not p.predicted_fake and actual_fake:
            fn += 1
        else:
            tn += 1
    judged = tp + fp + fn + tn
    return {
        "total": len(preds),
        "judged": judged,
        "inconclusive": inconclusive,
        "tp": tp,
        "fp": fp,
        "fn": fn,
        "tn": tn,
        "precision": tp / (tp + fp) if tp + fp else None,
        "recall": tp / (tp + fn) if tp + fn else None,
        "genuine_precision": tn / (tn + fp) if tn + fp else None,
        "genuine_recall": tn / (tn + fn) if tn + fn else None,
    }


def by_codec(preds: list[Pred]) -> dict[str, dict]:
    groups: dict[str, list[Pred]] = defaultdict(list)
    for p in preds:
        if p.codec == "genuine":
            continue
        groups[p.codec].append(p)
    return {codec: metrics(items) for codec, items in sorted(groups.items())}


def load_flaccheck() -> list[Pred]:
    data = json.loads(FLACCHECK_SCAN.read_text())
    label_by_path = {e["path"]: e["label"] for e in load_manifest()}
    out: list[Pred] = []
    for row in data.get("results", []):
        path = row["path"]
        out.append(
            Pred(
                tool="flaccheck",
                path=path,
                label=label_by_path.get(path, "transcoded"),
                predicted_fake=flaccheck_fake(row.get("transcode_verdict", "")),
                codec=codec_from_path(path),
            )
        )
    return out


def load_flac_detective() -> list[Pred]:
    data = json.loads(FLAC_DETECTIVE.read_text())
    label_by_path = {e["path"]: e["label"] for e in load_manifest()}
    out: list[Pred] = []
    for row in data.get("results", []):
        path = row["filepath"]
        out.append(
            Pred(
                tool="FLAC Detective",
                path=path,
                label=label_by_path.get(path, "transcoded"),
                predicted_fake=flac_detective_fake(row.get("verdict", "")),
                codec=codec_from_path(path),
            )
        )
    return out


def load_aucdtect(limit: int | None = None) -> list[Pred]:
    entries = load_manifest()
    if limit:
        entries = entries[:limit]
    out: list[Pred] = []
    for entry in entries:
        path = ROOT / entry["path"]
        if not path.exists():
            continue
        out.append(
            Pred(
                tool="auCDtect",
                path=entry["path"],
                label=entry["label"],
                predicted_fake=run_aucdtect(path),
                codec=codec_from_path(entry["path"]),
            )
        )
    return out


def make_charts(summary: dict, tool_preds: list[tuple[str, list[Pred]]]) -> None:
    import matplotlib.pyplot as plt

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    plt.style.use("seaborn-v0_8-whitegrid")

    tools = [t["name"] for t in summary["tools"]]
    precision = [t["overall"]["precision"] or 0 for t in summary["tools"]]
    recall = [t["overall"]["recall"] or 0 for t in summary["tools"]]

    # Overall precision & recall
    fig, ax = plt.subplots(figsize=(9, 5))
    x = range(len(tools))
    w = 0.35
    ax.bar([i - w / 2 for i in x], [p * 100 for p in precision], w, label="Precision", color="#2563eb")
    ax.bar([i + w / 2 for i in x], [r * 100 for r in recall], w, label="Recall (transcodes)", color="#16a34a")
    ax.set_xticks(list(x))
    ax.set_xticklabels(tools, rotation=15, ha="right")
    ax.set_ylim(0, 105)
    ax.set_ylabel("Percent")
    ax.set_title("Fake-lossless detection on real-music corpus (n=121)")
    ax.legend(loc="lower right")
    fig.tight_layout()
    fig.savefig(OUT_DIR / "overall_precision_recall.png", dpi=160)
    plt.close(fig)

    # Per-codec recall (flaccheck vs FLAC Detective)
    codec_tools = [t for t in summary["tools"] if t["name"] in ("flaccheck", "FLAC Detective")]
    codecs = ["mp3", "aac", "opus", "vorbis"]
    fig, ax = plt.subplots(figsize=(9, 5))
    w = 0.35
    for idx, tool in enumerate(codec_tools):
        offsets = [i + (idx - 0.5) * w for i in range(len(codecs))]
        vals = []
        for codec in codecs:
            m = tool["by_codec"].get(codec, {})
            r = m.get("recall")
            vals.append((r or 0) * 100)
        ax.bar(offsets, vals, w, label=tool["name"])
    ax.set_xticks(range(len(codecs)))
    ax.set_xticklabels([c.upper() for c in codecs])
    ax.set_ylim(0, 105)
    ax.set_ylabel("Recall on transcodes (%)")
    ax.set_title("Recall by source codec (lossy → FLAC)")
    ax.legend()
    fig.tight_layout()
    fig.savefig(OUT_DIR / "recall_by_codec.png", dpi=160)
    plt.close(fig)

    # Genuine false-positive rate (judged genuine files only)
    fig, ax = plt.subplots(figsize=(8, 4.5))
    fp_rates = []
    for t in summary["tools"]:
        preds = next(p for name, p in tool_preds if name == t["name"])
        genuine = [p for p in preds if p.label == "genuine"]
        fp = sum(1 for p in genuine if p.predicted_fake is True)
        tn = sum(1 for p in genuine if p.predicted_fake is False)
        fp_rates.append((fp / (fp + tn) * 100) if fp + tn else 0)
    ax.bar(tools, fp_rates, color="#dc2626")
    ax.set_ylabel("False positive rate on genuine FLAC (%)")
    ax.set_title("False alarms on authentic sources (lower is better)")
    plt.xticks(rotation=15, ha="right")
    fig.tight_layout()
    fig.savefig(OUT_DIR / "genuine_false_positive_rate.png", dpi=160)
    plt.close(fig)


def main() -> int:
    if not FLACCHECK_SCAN.exists() or not FLAC_DETECTIVE.exists():
        print("Run flaccheck scan + flac-detective first (see README benchmarks section).", file=sys.stderr)
        return 1

    tool_preds: list[tuple[str, list[Pred]]] = [
        ("flaccheck", load_flaccheck()),
        ("FLAC Detective", load_flac_detective()),
    ]

    aucdtect_preds = load_aucdtect()
    if aucdtect_preds and any(p.predicted_fake is not None for p in aucdtect_preds):
        tool_preds.append(("auCDtect", aucdtect_preds))
    else:
        print("auCDtect not available — skipping from charts", file=sys.stderr)

    summary = {
        "corpus": str(MANIFEST.relative_to(ROOT)),
        "description": "Real-music sources transcoded to FLAC (ffmpeg); 11 genuine + 110 transcodes",
        "tools": [],
    }
    for name, preds in tool_preds:
        overall = metrics(preds)
        summary["tools"].append(
            {
                "name": name,
                "overall": overall,
                "by_codec": by_codec(preds),
            }
        )

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_DIR / "comparison.json").write_text(json.dumps(summary, indent=2) + "\n")

    try:
        import matplotlib  # noqa: F401

        make_charts(summary, tool_preds)
    except ImportError:
        print("matplotlib not installed; wrote comparison.json only", file=sys.stderr)

    for t in summary["tools"]:
        o = t["overall"]
        print(
            f"{t['name']}: precision={o['precision']:.1%} recall={o['recall']:.1%} "
            f"inconclusive={o['inconclusive']}/{o['total']}"
            if o["precision"] is not None
            else f"{t['name']}: insufficient data"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
