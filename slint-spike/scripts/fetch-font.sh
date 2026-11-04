#!/usr/bin/env bash
# Puts IBM Plex Sans JP (OFL) as TTF into slint-spike/fonts/ (git-ignored).
# The npm package ships only WOFF/WOFF2 and Slint imports TTF/OTF, so the WOFF
# files are unpacked to TTF. Uses the root node_modules after `npm ci` when
# present (main depends on @ibm/plex-sans-jp), else `npm pack`.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
pkg="$here/../node_modules/@ibm/plex-sans-jp"
if [ ! -d "$pkg/fonts/complete/woff" ]; then
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  (cd "$work" && npm pack --silent @ibm/plex-sans-jp@3 >/dev/null && tar xzf ibm-plex-sans-jp-*.tgz)
  pkg="$work/package"
fi
py="$(command -v python3 || command -v python)"
mkdir -p "$here/fonts"
for w in Regular Medium SemiBold; do
  "$py" -I "$here/scripts/woff2ttf.py" "$pkg/fonts/complete/woff/hinted/IBMPlexSansJP-$w.woff" "$here/fonts/IBMPlexSansJP-$w.ttf"
done
cp "$pkg/LICENSE.txt" "$here/fonts/LICENSE.txt"
ls -l "$here/fonts"
