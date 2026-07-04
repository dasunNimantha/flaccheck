#!/usr/bin/env bash
# Build a realistic transcode calibration set from real music sources.
# Unlike generate.sh this uses no synthetic full-band masters, so lossy encodes
# exhibit true low-pass cutoffs and brick walls that the spectral tier can detect.
# Usage: ./datasets/generate_realistic.sh <source_dir> <output_dir>
set -uo pipefail

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

process_source() {
  local f="$1" base
  base=$(basename "$f" | sed 's/\.[^.]*$//' | tr ' ' '_' | tr -cd '[:alnum:]_-')
  [[ -n "$base" ]] || base="track"

  # Genuine reference (re-encode to canonical FLAC, no lossy stage).
  if ! ffmpeg -y -i "$f" -map 0:a:0 -c:a flac "$OUT/${base}_genuine.flac" -loglevel error 2>/dev/null; then
    echo "skip source: $f" >&2
    return 0
  fi
  add_entry "$OUT/${base}_genuine.flac" "genuine"

  # MP3 CBR + VBR
  for br in 128 192 320; do
    if ffmpeg -y -i "$f" -map 0:a:0 -c:a libmp3lame -b:a "${br}k" -f mp3 - 2>/dev/null | \
      ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_mp3_${br}k.flac" -loglevel error 2>/dev/null; then
      add_entry "$OUT/${base}_mp3_${br}k.flac" "transcoded"
    fi
  done
  if ffmpeg -y -i "$f" -map 0:a:0 -c:a libmp3lame -q:a 2 -f mp3 - 2>/dev/null | \
    ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_mp3_vbr2.flac" -loglevel error 2>/dev/null; then
    add_entry "$OUT/${base}_mp3_vbr2.flac" "transcoded"
  fi

  # AAC
  for br in 128 192 256; do
    if ffmpeg -y -i "$f" -map 0:a:0 -c:a aac -b:a "${br}k" -f adts - 2>/dev/null | \
      ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_aac_${br}k.flac" -loglevel error 2>/dev/null; then
      add_entry "$OUT/${base}_aac_${br}k.flac" "transcoded"
    fi
  done

  # Opus
  for br in 96 128; do
    if ffmpeg -y -i "$f" -map 0:a:0 -c:a libopus -b:a "${br}k" -f opus - 2>/dev/null | \
      ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_opus_${br}k.flac" -loglevel error 2>/dev/null; then
      add_entry "$OUT/${base}_opus_${br}k.flac" "transcoded"
    fi
  done

  # Vorbis
  if ffmpeg -y -i "$f" -map 0:a:0 -c:a libvorbis -q:a 5 -f ogg - 2>/dev/null | \
    ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_vorbis_q5.flac" -loglevel error 2>/dev/null; then
    add_entry "$OUT/${base}_vorbis_q5.flac" "transcoded"
  fi
}

shopt -s nullglob
for f in "$SRC"/*.flac "$SRC"/*.FLAC "$SRC"/*.wav "$SRC"/*.WAV; do
  [[ -f "$f" ]] || continue
  echo "source: $(basename "$f")"
  process_source "$f" || true
done

echo "" >> "$manifest"
echo "]" >> "$manifest"
echo "Wrote $manifest ($(grep -c '"path"' "$manifest" || echo 0) entries)"
