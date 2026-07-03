#!/usr/bin/env bash
# Generate synthetic transcode matrix for benchmarking and calibration.
# Usage: ./datasets/generate.sh <source_dir> <output_dir>
set -uo pipefail

SRC="${1:?source directory with WAV/FLAC}"
OUT="${2:?output directory}"
mkdir -p "$OUT"
SYNTH="$OUT/_synth_masters"
mkdir -p "$SYNTH"

manifest="$OUT/manifest.json"
echo "[" > "$manifest"
first=1

add_entry() {
  local path="$1" label="$2"
  if [[ $first -eq 1 ]]; then first=0; else echo "," >> "$manifest"; fi
  printf '  {"path":"%s","label":"%s"}' "$path" "$label" >> "$manifest"
}

echo "Generating synthetic lossless masters..."
ffmpeg -y -f lavfi -i "sine=frequency=440:duration=30" -f lavfi -i "sine=frequency=8000:duration=30" \
  -f lavfi -i "sine=frequency=15000:duration=30" -filter_complex "[0:a][1:a][2:a]amix=inputs=3:duration=first" \
  "$SYNTH/master_tones.wav" -loglevel error 2>/dev/null || true
ffmpeg -y -f lavfi -i "anoisesrc=duration=30:color=white:amplitude=0.3" \
  "$SYNTH/master_noise.wav" -loglevel error 2>/dev/null || true
ffmpeg -y -f lavfi -i "sine=frequency=1000:duration=30" -f lavfi -i "anoisesrc=duration=30:color=pink:amplitude=0.15" \
  -filter_complex "[0:a][1:a]amix=inputs=2:duration=first" \
  "$SYNTH/master_mixed.wav" -loglevel error 2>/dev/null || true

process_source() {
  local f="$1"
  local base
  base=$(basename "$f" | sed 's/\.[^.]*$//' | tr ' ' '_' | tr -cd '[:alnum:]_-')
  [[ -n "$base" ]] || base="track"

  if ! ffmpeg -y -i "$f" -map 0:a:0 -c:a flac "$OUT/${base}_genuine.flac" -loglevel error 2>/dev/null; then
    echo "skip source: $f" >&2
    return 0
  fi
  add_entry "$OUT/${base}_genuine.flac" "genuine"

  for br in 128 192 256 320; do
    if ffmpeg -y -i "$f" -map 0:a:0 -b:a "${br}k" -f mp3 - 2>/dev/null | \
      ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_mp3_${br}k.flac" -loglevel error 2>/dev/null; then
      add_entry "$OUT/${base}_mp3_${br}k.flac" "transcoded"
    fi
  done

  if ffmpeg -y -i "$f" -map 0:a:0 -c:a libmp3lame -q:a 0 -f mp3 - 2>/dev/null | \
    ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_mp3_vbr0.flac" -loglevel error 2>/dev/null; then
    add_entry "$OUT/${base}_mp3_vbr0.flac" "transcoded"
  fi
  if ffmpeg -y -i "$f" -map 0:a:0 -c:a libmp3lame -q:a 2 -f mp3 - 2>/dev/null | \
    ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_mp3_vbr2.flac" -loglevel error 2>/dev/null; then
    add_entry "$OUT/${base}_mp3_vbr2.flac" "transcoded"
  fi

  if ffmpeg -y -i "$f" -map 0:a:0 -c:a libmp3lame -b:a 192k -joint_stereo 1 -f mp3 - 2>/dev/null | \
    ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_mp3_joint192.flac" -loglevel error 2>/dev/null; then
    add_entry "$OUT/${base}_mp3_joint192.flac" "transcoded"
  fi

  for br in 128 192 256; do
    if ffmpeg -y -i "$f" -map 0:a:0 -c:a aac -b:a "${br}k" -f adts - 2>/dev/null | \
      ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_aac_${br}k.flac" -loglevel error 2>/dev/null; then
      add_entry "$OUT/${base}_aac_${br}k.flac" "transcoded"
    fi
  done

  for br in 96 128; do
    if ffmpeg -y -i "$f" -map 0:a:0 -c:a libopus -b:a "${br}k" -f opus - 2>/dev/null | \
      ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_opus_${br}k.flac" -loglevel error 2>/dev/null; then
      add_entry "$OUT/${base}_opus_${br}k.flac" "transcoded"
    fi
  done

  if ffmpeg -y -i "$f" -map 0:a:0 -c:a libvorbis -q:a 5 -f ogg - 2>/dev/null | \
    ffmpeg -y -i pipe:0 -c:a flac "$OUT/${base}_vorbis_q5.flac" -loglevel error 2>/dev/null; then
    add_entry "$OUT/${base}_vorbis_q5.flac" "transcoded"
  fi
}

for f in "$SYNTH"/*.wav; do
  [[ -f "$f" ]] || continue
  process_source "$f" || true
done

shopt -s nullglob
for f in "$SRC"/*.{wav,flac,WAV,FLAC}; do
  [[ -f "$f" ]] || continue
  process_source "$f" || true
done

echo "" >> "$manifest"
echo "]" >> "$manifest"
echo "Wrote $manifest ($(grep -c '"path"' "$manifest" || echo 0) entries)"
