#!/usr/bin/env python3
"""Compare flaccheck vs other tools on a labeled manifest; emit JSON + chart PNGs."""

from __future__ import annotations

import argparse
import json
import re
import sqlite3
import subprocess
import sys
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "datasets/output/realistic/manifest.json"
OUT_DIR = ROOT / "docs/benchmarks"
BENCH_DIR = ROOT / "benchmarks"
FLACCHECK_SCAN = BENCH_DIR / "flaccheck_per_file.json"
FLAC_DETECTIVE = BENCH_DIR / "flac_detective.json"
ISFLAC_JSON = BENCH_DIR / "isflac.json"
SOUNDAUDIT_DB = BENCH_DIR / "soundaudit.db"
AUDIOCHECKR_JSON = BENCH_DIR / "audiocheckr.json"
AUDIOCHECKR_BIN = BENCH_DIR / "vendor/audiocheckr/target/release/audiocheckr"
ISFLAC_BIN = Path.home() / ".cargo/bin/isflac"


@dataclass
class Pred:
    tool: str
    path: str
    label: str
    predicted_fake: bool | None
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


def isflac_fake(exit_code: int, stdout: str) -> bool | None:
    if exit_code == 2 or "probably transcoded" in stdout.lower():
        return True
    if exit_code == 0 and "genuine flac" in stdout.lower():
        return False
    return None


def audiocheckr_fake(data: dict) -> bool | None:
    verdict = data.get("verdict") or {}
    if isinstance(verdict, dict):
        if verdict.get("genuine") is False:
            return True
        if verdict.get("genuine") is True:
            return False
        label = str(verdict.get("label", "")).lower()
        if "lossy" in label or "fake" in label:
            return True
        if "genuine" in label or "lossless" in label:
            return False
    for det in data.get("detections", []):
        sev = str(det.get("severity", "")).lower()
        dtype = json.dumps(det.get("defect_type", {}))
        if sev in ("critical", "high") and "transcode" in dtype.lower():
            return True
    return False


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
    return [
        Pred(
            tool="flaccheck",
            path=row["path"],
            label=label_by_path.get(row["path"], "transcoded"),
            predicted_fake=flaccheck_fake(row.get("transcode_verdict", "")),
            codec=codec_from_path(row["path"]),
        )
        for row in data.get("results", [])
    ]


def load_flac_detective() -> list[Pred]:
    data = json.loads(FLAC_DETECTIVE.read_text())
    label_by_path = {e["path"]: e["label"] for e in load_manifest()}
    return [
        Pred(
            tool="FLAC Detective",
            path=row["filepath"],
            label=label_by_path.get(row["filepath"], "transcoded"),
            predicted_fake=flac_detective_fake(row.get("verdict", "")),
            codec=codec_from_path(row["filepath"]),
        )
        for row in data.get("results", [])
    ]


def load_isflac() -> list[Pred]:
    rows = json.loads(ISFLAC_JSON.read_text())
    label_by_path = {e["path"]: e["label"] for e in load_manifest()}
    return [
        Pred(
            tool="isflac",
            path=row["path"],
            label=label_by_path.get(row["path"], "transcoded"),
            predicted_fake=isflac_fake(row["exit_code"], row.get("stdout", "")),
            codec=codec_from_path(row["path"]),
        )
        for row in rows
    ]


def load_soundaudit() -> list[Pred]:
    label_by_path = {e["path"]: e["label"] for e in load_manifest()}
    conn = sqlite3.connect(SOUNDAUDIT_DB)
    preds: list[Pred] = []
    for abs_path, is_transcode in conn.execute("SELECT path, is_transcode FROM files"):
        rel = Path(abs_path)
        try:
            rel = rel.relative_to(ROOT)
        except ValueError:
            continue
        path = str(rel)
        if path not in label_by_path:
            continue
        preds.append(
            Pred(
                tool="soundaudit",
                path=path,
                label=label_by_path[path],
                predicted_fake=bool(is_transcode),
                codec=codec_from_path(path),
            )
        )
    return preds


def load_audiocheckr() -> list[Pred]:
    rows = json.loads(AUDIOCHECKR_JSON.read_text())
    label_by_path = {e["path"]: e["label"] for e in load_manifest()}
    return [
        Pred(
            tool="audiocheckr",
            path=row["path"],
            label=label_by_path.get(row["path"], "transcoded"),
            predicted_fake=audiocheckr_fake(row["result"]),
            codec=codec_from_path(row["path"]),
        )
        for row in rows
    ]


def run_isflac_one(entry: dict) -> dict:
    path = ROOT / entry["path"]
    proc = subprocess.run(
        [str(ISFLAC_BIN), str(path)],
        capture_output=True,
        text=True,
        timeout=120,
    )
    return {
        "path": entry["path"],
        "exit_code": proc.returncode,
        "stdout": proc.stdout + proc.stderr,
    }


def run_audiocheckr_one(entry: dict) -> dict:
    path = ROOT / entry["path"]
    proc = subprocess.run(
        [str(AUDIOCHECKR_BIN), str(path), "--format", "json"],
        capture_output=True,
        text=True,
        timeout=180,
    )
    if proc.returncode != 0 and not proc.stdout.strip():
        return {"path": entry["path"], "error": proc.stderr}
    return {"path": entry["path"], "result": json.loads(proc.stdout)}


def collect_isflac(entries: list[dict]) -> None:
    if not ISFLAC_BIN.exists():
        raise SystemExit(f"isflac not found at {ISFLAC_BIN} — run: cargo install isflac")
    rows: list[dict] = []
    with ThreadPoolExecutor(max_workers=8) as pool:
        futures = [pool.submit(run_isflac_one, e) for e in entries]
        for fut in as_completed(futures):
            rows.append(fut.result())
    rows.sort(key=lambda r: r["path"])
    ISFLAC_JSON.write_text(json.dumps(rows, indent=2) + "\n")
    print(f"wrote {ISFLAC_JSON}")


def collect_soundaudit(entries: list[dict]) -> None:
    corpus = ROOT / "datasets/output/realistic"
    BENCH_DIR.mkdir(parents=True, exist_ok=True)
    if SOUNDAUDIT_DB.exists():
        SOUNDAUDIT_DB.unlink()
    subprocess.run(
        ["soundaudit", "scan", str(corpus), "--db", str(SOUNDAUDIT_DB), "-j", "4"],
        check=True,
    )
    subprocess.run(
        [
            "soundaudit",
            "analyze",
            "--db",
            str(SOUNDAUDIT_DB),
            "--transcodes",
            "--no-duplicates",
            "--workers",
            "4",
        ],
        check=True,
    )
    print(f"wrote {SOUNDAUDIT_DB}")


def collect_audiocheckr(entries: list[dict]) -> None:
    if not AUDIOCHECKR_BIN.exists():
        raise SystemExit(
            f"audiocheckr not built at {AUDIOCHECKR_BIN}\n"
            "  git clone https://github.com/abalajiksh/audiocheckr benchmarks/vendor/audiocheckr\n"
            "  # add [workspace] table to its Cargo.toml, then cargo build --release"
        )
    rows: list[dict] = []
    with ThreadPoolExecutor(max_workers=4) as pool:
        futures = [pool.submit(run_audiocheckr_one, e) for e in entries]
        for i, fut in enumerate(as_completed(futures), 1):
            rows.append(fut.result())
            if i % 20 == 0:
                print(f"  audiocheckr {i}/{len(entries)}")
    rows.sort(key=lambda r: r["path"])
    AUDIOCHECKR_JSON.write_text(json.dumps(rows, indent=2) + "\n")
    print(f"wrote {AUDIOCHECKR_JSON}")


def make_charts(summary: dict, tool_preds: list[tuple[str, list[Pred]]]) -> None:
    import matplotlib.pyplot as plt

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    plt.style.use("seaborn-v0_8-whitegrid")

    tools = [t["name"] for t in summary["tools"]]
    precision = [t["overall"]["precision"] or 0 for t in summary["tools"]]
    recall = [t["overall"]["recall"] or 0 for t in summary["tools"]]

    fig, ax = plt.subplots(figsize=(12, 5.5))
    x = range(len(tools))
    w = 0.35
    ax.bar([i - w / 2 for i in x], [p * 100 for p in precision], w, label="Precision", color="#2563eb")
    ax.bar([i + w / 2 for i in x], [r * 100 for r in recall], w, label="Recall (transcodes)", color="#16a34a")
    ax.set_xticks(list(x))
    ax.set_xticklabels(tools, rotation=20, ha="right")
    ax.set_ylim(0, 105)
    ax.set_ylabel("Percent")
    ax.set_title("Fake-lossless detection on real-music corpus (n=121)")
    ax.legend(loc="lower right")
    fig.tight_layout()
    fig.savefig(OUT_DIR / "overall_precision_recall.png", dpi=160)
    plt.close(fig)

    codecs = ["mp3", "aac", "opus", "vorbis"]
    palette = ["#2563eb", "#7c3aed", "#ea580c", "#0891b2", "#16a34a", "#dc2626"]
    fig, ax = plt.subplots(figsize=(12, 5.5))
    n_tools = len(summary["tools"])
    group_w = 0.8
    bar_w = group_w / max(n_tools, 1)
    for t_idx, tool in enumerate(summary["tools"]):
        offsets = [i + (t_idx - (n_tools - 1) / 2) * bar_w for i in range(len(codecs))]
        vals = [(tool["by_codec"].get(codec, {}).get("recall") or 0) * 100 for codec in codecs]
        ax.bar(offsets, vals, bar_w * 0.95, label=tool["name"], color=palette[t_idx % len(palette)])
    ax.set_xticks(range(len(codecs)))
    ax.set_xticklabels([c.upper() for c in codecs])
    ax.set_ylim(0, 105)
    ax.set_ylabel("Recall on transcodes (%)")
    ax.set_title("Recall by source codec (lossy → FLAC)")
    ax.legend(fontsize=8, ncol=2)
    fig.tight_layout()
    fig.savefig(OUT_DIR / "recall_by_codec.png", dpi=160)
    plt.close(fig)

    fig, ax = plt.subplots(figsize=(12, 4.5))
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
    plt.xticks(rotation=20, ha="right")
    fig.tight_layout()
    fig.savefig(OUT_DIR / "genuine_false_positive_rate.png", dpi=160)
    plt.close(fig)


def build_summary(tool_preds: list[tuple[str, list[Pred]]]) -> dict:
    summary = {
        "corpus": str(MANIFEST.relative_to(ROOT)),
        "description": "Real-music sources transcoded to FLAC (ffmpeg); 11 genuine + 110 transcodes",
        "tools": [],
    }
    for name, preds in tool_preds:
        summary["tools"].append(
            {"name": name, "overall": metrics(preds), "by_codec": by_codec(preds)}
        )
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--collect",
        action="store_true",
        help="Run isflac, soundaudit, and audiocheckr on the manifest (slow)",
    )
    args = parser.parse_args()

    entries = load_manifest()
    if args.collect:
        collect_isflac(entries)
        collect_soundaudit(entries)
        collect_audiocheckr(entries)

    missing = []
    if not FLACCHECK_SCAN.exists():
        missing.append(str(FLACCHECK_SCAN))
    if not FLAC_DETECTIVE.exists():
        missing.append(str(FLAC_DETECTIVE))
    if missing:
        print("Missing prerequisite outputs:", ", ".join(missing), file=sys.stderr)
        print("See README benchmarks section.", file=sys.stderr)
        return 1

    tool_preds: list[tuple[str, list[Pred]]] = [
        ("flaccheck", load_flaccheck()),
        ("FLAC Detective", load_flac_detective()),
    ]
    for name, path, loader in (
        ("isflac", ISFLAC_JSON, load_isflac),
        ("soundaudit", SOUNDAUDIT_DB, load_soundaudit),
        ("audiocheckr", AUDIOCHECKR_JSON, load_audiocheckr),
    ):
        if path.exists():
            tool_preds.append((name, loader()))
        else:
            print(f"skip {name}: {path} not found (run with --collect)", file=sys.stderr)

    summary = build_summary(tool_preds)
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_DIR / "comparison.json").write_text(json.dumps(summary, indent=2) + "\n")

    try:
        import matplotlib  # noqa: F401

        make_charts(summary, tool_preds)
    except ImportError:
        print("matplotlib not installed; wrote comparison.json only", file=sys.stderr)

    for t in summary["tools"]:
        o = t["overall"]
        prec = f"{o['precision']:.1%}" if o["precision"] is not None else "n/a"
        rec = f"{o['recall']:.1%}" if o["recall"] is not None else "n/a"
        print(f"{t['name']}: precision={prec} recall={rec} inconclusive={o['inconclusive']}/{o['total']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
