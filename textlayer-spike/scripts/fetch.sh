#!/usr/bin/env bash
# Fonts (SIL OFL 1.1) into data/fonts. Noto Sans JP is the font the text layer embeds;
# Noto Serif JP and IBM Plex Sans JP only draw the generated pages.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p data/fonts
N=https://raw.githubusercontent.com/notofonts/noto-cjk/main
[[ -s data/fonts/NotoSansJP-Regular.otf ]] || curl -sSfL -o data/fonts/NotoSansJP-Regular.otf $N/Sans/SubsetOTF/JP/NotoSansJP-Regular.otf
[[ -s data/fonts/NotoSerifJP-Regular.otf ]] || curl -sSfL -o data/fonts/NotoSerifJP-Regular.otf $N/Serif/SubsetOTF/JP/NotoSerifJP-Regular.otf
# IBM Plex Sans JP and the *_vert.json maps come from ocr-spike (sets/README.md there):
# copy data/fonts/{IBMPlexSansJP-Regular.otf,plex_vert.json,noto_serif_vert.json} from it.
ls -la data/fonts
