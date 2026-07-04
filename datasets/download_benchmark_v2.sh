#!/usr/bin/env bash
# Download fresh public-domain / trade-friendly FLAC sources for benchmark v2.
# All items differ from datasets/output/realistic (v1) sources.
# Usage: ./datasets/download_benchmark_v2.sh [output_dir]
set -euo pipefail

OUT="${1:-$(dirname "$0")/benchmark_v2_sources}"
mkdir -p "$OUT"

# identifier|archive_filename|local_basename
SOURCES=(
  "flowpoetry2020-02-18|FlowPoetry2020-02-18.flac|flowpoetry_live_2020"
  "harsharmadillo2014-04-27.dpa4023-sbd.flac|harsharmadillo2014-04-27.dpa4023-sbd.d1t03.flac|harsharmadillo_live_2014"
  "Peridoni2015-08-06.Peridoni2015-08-06_1|01.Peridoni2015-08-06d1t01.flac|peridoni_live_2015"
  "FK2017-12-15|FK2017-12-15t-01.flac|fk_live_2017"
  "NextFab61519|06 Hunnybee.flac|nextfab_hunnybee_2019"
  "CW2019-07-04|01 Terrapin Reprise.flac|cw_terrapin_2019"
  "rice2021-08-07|01 Tom.flac|rice_live_2021"
  "td2018-10-05|The Dubois' - 10.05.18 t01.flac|dubois_live_2018_t01"
  "universepeoples2021-05-29|universepeoples2021-05-29-01.flac|universe_peoples_2021"
  "skellogg2011-10-09|02-Satisfied Man.flac|skellogg_live_2011"
  "scienceseattle2017-05-13.sbd|scienceseattle2017.05.130101time.flac|science_seattle_2017"
  "Tractor-Pomares|PomaresTractor.flac|tractor_pomares_netlabel"
)

download_one() {
  local id="$1" remote="$2" local_base="$3"
  local dest="$OUT/${local_base}.flac"
  if [[ -f "$dest" ]]; then
    echo "skip (exists): $(basename "$dest")"
    return 0
  fi
  local url="https://archive.org/download/${id}/$(python3 -c "import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1]))" "$remote")"
  echo "fetch: $local_base <- $id"
  curl -fsSL --retry 3 --retry-delay 2 -o "$dest" "$url"
  if ! ffprobe -v error -select_streams a:0 -show_entries stream=codec_name -of csv=p=0 "$dest" >/dev/null 2>&1; then
    echo "invalid audio: $dest" >&2
    rm -f "$dest"
    return 1
  fi
}

for row in "${SOURCES[@]}"; do
  IFS='|' read -r id remote local_base <<< "$row"
  download_one "$id" "$remote" "$local_base"
done

count=$(find "$OUT" -maxdepth 1 -name '*.flac' | wc -l)
echo "Wrote $count FLAC sources to $OUT"
