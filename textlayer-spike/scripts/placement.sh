#!/usr/bin/env bash
# Item 5: placement variants on the nine pages (lopdf path, real font), full
# tables into out/placement_tables.md and one summary row per variant.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -q 2>/dev/null
T=target/release/textlayer-spike
: > out/placement_tables.md
for v in "line v" "tj v" "char v" "char upright" "line rot90"; do
  set -- $v
  { echo "### fit=$1 vertical=$2"; echo; $T lopdf --font real --fit $1 --vmode $2 --out "p_$1_$2"; } >> out/placement_tables.md
done
python3 -I scripts/summarize.py out/placement_tables.md
