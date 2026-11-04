#!/usr/bin/env bash
# Starts the window under Xvfb with an AT-SPI bus and prints its accessibility
# tree (Linux: AccessKit -> AT-SPI). Needs at-spi2-core and python3-pyatspi.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q
[ -f samples/pages-240.pdf ] || cargo run -q --bin make_sample
dbus-run-session -- bash -c '
  /usr/libexec/at-spi-bus-launcher --launch-immediately &
  sleep 1
  dbus-send --session --dest=org.a11y.Bus --print-reply /org/a11y/bus \
    org.freedesktop.DBus.Properties.Set string:org.a11y.Status string:IsEnabled variant:boolean:true >/dev/null
  dbus-send --session --dest=org.a11y.Bus --print-reply /org/a11y/bus \
    org.freedesktop.DBus.Properties.Set string:org.a11y.Status string:ScreenReaderEnabled variant:boolean:true >/dev/null
  Xvfb :97 -screen 0 1400x1000x24 >/dev/null 2>&1 &
  xvfb=$!
  sleep 1
  DISPLAY=:97 SLINT_BACKEND=winit-software SPIKE_A11Y=samples/pages-240.pdf \
    ./target/debug/slint-spike > target/a11y-app.log 2>&1 &
  app=$!
  # Without a window manager the window never gets the OS focus, and
  # AccessKit reports no focused node; give it the focus by hand.
  sleep 1
  DISPLAY=:97 xdotool search --name "Slint spike" windowfocus --sync 2>/dev/null || true
  for _ in $(seq 1 100); do grep -q "a11y: ready" target/a11y-app.log && break; sleep 0.2; done
  python3.12 -I scripts/a11y_dump.py || true
  kill $app $xvfb 2>/dev/null || true
'
