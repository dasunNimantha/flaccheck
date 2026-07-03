#!/usr/bin/env bash
# Rebuild manifest.json from FLAC files in a calibration output directory.
OUT="${1:?output directory}"
manifest="$OUT/manifest.json"
echo "[" > "$manifest"
first=1
for f in "$OUT"/*.flac; do
  [[ -f "$f" ]] || continue
  base=$(basename "$f")
  if [[ "$base" == *"_genuine.flac" ]]; then
    label="genuine"
  else
    label="transcoded"
  fi
  if [[ $first -eq 1 ]]; then first=0; else echo "," >> "$manifest"; fi
  printf '  {"path":"%s","label":"%s"}' "$f" "$label" >> "$manifest"
done
echo "" >> "$manifest"
echo "]" >> "$manifest"
echo "Wrote $manifest"
