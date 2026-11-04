#!/usr/bin/env bash
# Puts IBM Plex Sans JP (OFL) as TTF into slint-spike/fonts/ (git-ignored).
# The npm package ships only WOFF/WOFF2, so the WOFF files are unpacked to TTF.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
(cd "$work" && npm pack --silent @ibm/plex-sans-jp@3 >/dev/null && tar xzf ibm-plex-sans-jp-*.tgz)
mkdir -p "$here/fonts"
for w in Regular Medium SemiBold; do
  python3 -I "$here/scripts/woff2ttf.py" "$work/package/fonts/complete/woff/hinted/IBMPlexSansJP-$w.woff" "$here/fonts/IBMPlexSansJP-$w.ttf"
done
cp "$work/package/LICENSE.txt" "$here/fonts/LICENSE.txt"
ls -l "$here/fonts"
