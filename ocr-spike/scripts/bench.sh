#!/usr/bin/env bash
# Item 3: Rust and Python alternately, so that the noisy cloud CPU affects both alike.
# usage: scripts/bench.sh MODEL ROUNDS [extra args for both]
set -euo pipefail
cd "$(dirname "$0")/.."
MODEL=${1:-small}; ROUNDS=${2:-3}; shift 2 || true
IM="data/ref/ja.jpg data/xfund/val/ja_val_0.jpg data/xfund/val/ja_val_1.jpg data/xfund/val/ja_val_2.jpg data/xfund/val/ja_val_3.jpg"
mkdir -p out
for r in $(seq 1 "$ROUNDS"); do
  ./target/release/ocr-spike bench $IM --model "$MODEL" "$@" --json "out/bench_rs_${MODEL}_r$r.json" 2>&1 | tail -1
  .venv/bin/python -I scripts/ocr_min.py $IM --model "$MODEL" "$@" --json "out/bench_py_${MODEL}_r$r.json" 2>&1 | tail -1
done
