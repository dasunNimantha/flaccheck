#!/usr/bin/env bash
# Generate synthetic transcode matrix for benchmarking.
# Usage: ./datasets/generate.sh <source_dir> <output_dir>
set -euo pipefail

SRC="${1:?source directory with WAV/FLAC}"
OUT="${2:?output directory}"
mkdir -p "$OUT"

manifest="$OUT/manifest.json"
echo "[" > "$manifest"
first=1

add_entry() {
  local path="$1" label="$2"
  if [[ $first -eq 1 ]]; then first=0; else echo "," >> "$manifest"; fi
  printf '  {"path":"%s","label":"%s"}' "$path" "$label" >> "$manifest"
}

for f in "$SRC"/*.{wav,flac,WAV,FLAC} 2>/dev/null; do
  [[ -f "$f" ]] || continue
  base=$(basename "$f" | sed 's/\.[^.]*$//')
  cp "$f" "$OUT/${base}_genuine.flac" 2>/dev/null || ffmpeg -y -i "$f" -c:a flac "$OUT/${base}_genuine.flac" -loglevel error
  add_entry "$OUT/${base}_genuine.flac" "genuine"

  for br in 128 192 320; do
    ffmpeg -y -i "$f" -b:a "${br}k" -f mp3 - | ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_mp3_${br}k.flac" -loglevel error
    add_entry "$OUT/${base}_mp3_${br}k.flac" "transcoded"
  done

  for br in 128 256; do
    ffmpeg -y -i "$f" -c:a aac -b:a "${br}k" -f adts - | ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_aac_${br}k.flac" -loglevel error
    add_entry "$OUT/${base}_aac_${br}k.flac" "transcoded"
  done
done

echo "" >> "$manifest"
echo "]" >> "$manifest"
echo "Wrote $manifest"
