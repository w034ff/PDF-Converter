#!/usr/bin/env bash
# Runs the scripted scenario under Xvfb with the software renderer and writes
# slint-spike/screenshots/*.png. Needs Xvfb and libxkbcommon-x11.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q
[ -f samples/pages-240.pdf ] || cargo run -q --bin make_sample
SLINT_BACKEND="${SLINT_BACKEND:-winit-software}" SPIKE_SCREENSHOTS=screenshots SPIKE_PDF=samples/pages-240.pdf \
  timeout 300 xvfb-run -a -s "-screen 0 1400x1000x24" ./target/debug/slint-spike 2>&1 | grep -v "xdg desktop settings"
