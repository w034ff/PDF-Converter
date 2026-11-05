# OCR の文字の選択の試作（捨てる前提）

ページの上の OCR の文字を塊をまたいで選ぶ機能と縦書きの表示を、HTML と Slint で作って比べる。マージしない。結果は [REPORT.md](REPORT.md)。

- `fixtures/`: ページの絵（A4 を 200 dpi、1654 × 2339 px）と `OcrPage` の JSON（ブランチ `spike/ocr-vertical` の `ocr-spike` の `page-json` の出力）。
- `web/`: Vite ＋ React ＋ TypeScript。`npm ci --engine-strict=false && npm run dev`。
- `slint/`: Slint 1.18.1。フォントを `slint/fonts/` に置いてから `cargo run --release`（下）。
