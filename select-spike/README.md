# OCR の文字の選択の試作（捨てる前提）

ページの上の OCR の文字を塊をまたいで選ぶ機能と縦書きの表示を、HTML と Slint で作って比べる。マージしない。結果は [REPORT.md](REPORT.md)。

- `fixtures/`: ページの絵（A4 を 200 dpi、1654 × 2339 px）と `OcrPage` の JSON（ブランチ `spike/ocr-vertical` の `ocr-spike` の `page-json` の出力）。
- `web/`: Vite ＋ React ＋ TypeScript。`npm ci --engine-strict=false && npm run dev`。
- `slint/`: Slint 1.18.1。フォントを `slint/fonts/` に置いてから `cargo run --release`（下）。

## フォント（Slint、git に入れない）

`slint/fonts/` に `IBMPlexSansJP-Regular.ttf` と `IBMPlexSansJP-Bold.ttf` を置く（前の試作 `slint-spike/README.md` の「フォント」と同じ手順。変換の道具は `slint/tools/woff2ttf.py`）。

## 自動の確かめ

```bash
cd select-spike/web && npm ci --engine-strict=false && npx vite --port 5173
```

```bash
cd select-spike/web && node tests/gen-scenarios.mjs
```

```bash
cd select-spike/web && node tests/run-web.mjs chromium native 1
```

- 2 つ目の引数は `native`（pdf.js の工夫あり）、`native-nofix`、`custom`。3 つ目は倍率、4 つ目に置き場を書くと、台本ごとに画面の絵を撮る。
- WebKitGTK（Linux）: `apt install webkit2gtk-driver libwebkit2gtk-4.1-0 xclip xvfb`、`Xvfb :99 -screen 0 2400x2700x24 &`、`DISPLAY=:99 WebKitWebDriver --port=4444 &` のあと、`DISPLAY=:99 node tests/run-web.mjs webkitgtk custom 1`。
- Slint: `cd select-spike/slint && cargo build --release && DISPLAY=:99 SLINT_BACKEND=winit-software ./target/release/select-spike-slint --script out/z1`（`--zoom 2` で 200 %）。Linux では `libxkbcommon-x11-0` が要る。
