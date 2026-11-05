#!/usr/bin/env bash
# Item 3: every krilla variant on the nine pages; tables into out/krilla_tables.md.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -q 2>/dev/null
T=target/release/textlayer-spike
{
for v in "real char transparent upright" "glyphless char transparent upright" "real line transparent upright" "real tj transparent upright" \
         "real char under upright" "real line transparent rot90"; do
  set -- $v
  echo "### font=$1 fit=$2 invisible=$3 vertical=$4"; echo
  $T krilla --font $1 --fit $2 --invisible $3 --vmode $4 --out "k_$1_$2_$3_$4"
  echo "大きさ: $(du -cb out/k_$1_$2_$3_$4/*.pdf | tail -1 | cut -f1) バイト（9 ページの計）"; echo
done
} > out/krilla_tables.md
cat out/krilla_tables.md
