# OCR の文字の選択の試作の報告（HTML と Slint）

- ブランチ: `spike/ocr-select`（`origin/spike/slint-crop` から。マージしない）
- 確かめた環境: クラウドの Linux（画面なし、4 vCPU）。Node v22.23.4、Rust 1.99.0。

## 1. 骨組みと入力

### 入力のページ

`spike/ocr-vertical`（6a75db4）を別の作業木でビルドし、`ocr-spike/scripts/fetch.sh` でモデル（PP-OCRv6 small／medium）を、作業木の中の `node scripts/fetch-layout.ts` で onnxruntime 1.26.0 を取った。フォントは `@ibm/plex-sans-jp` の woff を fontTools で OTF に戻し、`scripts/vert_map.py` で `plex_vert.json` を作った（どれも作業木の `data/` の中だけ。git に入れない）。

| ページ | 作り方 | `page-json` の引数 | 塊 |
| --- | --- | --- | --- |
| `h1_two_columns`（横書き） | [fixtures/gen_pages.py](fixtures/gen_pages.py)（Pillow、IBM Plex Sans JP）。見出し、2 段組みの本文、4 行 3 列の表、最後の 1 行 | なし | 10 |
| `v2_two_tiers`（縦書き） | `ocr-spike` の `vertical`（横書きの柱、縦書きの 2 段、ノンブル） | `--vcut --vcut-guard 1,3,0.1` | 5 |
| `v4_dialogue`（縦書き） | 同上（かぎ括弧の会話の入った 1 塊） | 同上 | 2 |
| `m1_mixed`（混ざったもの） | `gen_pages.py`（縦書きは Pillow の raqm の `direction="ttb"` と `vert`） | 同上 | 6 |

- 画像は A4 を 200 dpi（1654 × 2339 px）。`page-json` は A4 の 595 × 842 pt として座標を出す。
- **2 段組みの読み順**: `ocr-spike` の読み順（`order.rs` の規則 4）は、横書きの塊が横に並んだ帯を「行ごとに帯をまたいで」読む（帳票のため）。そのため 2 段組みの本文が「左の 1 行、右の 1 行、…」と交互になった（[fixtures/h1_two_columns.raw.json](fixtures/h1_two_columns.raw.json)）。選択の試験では段の順に読むほうが意味があるので、[fixtures/regroup.py](fixtures/regroup.py) で本文の帯だけを左右の 2 塊に組み直した。**OCR の画面を作るときは、読み順の規則 4 を、段組み（同じ幅の行が縦に続く）と帳票で分ける必要がある**。
- OCR の読み違いもそのまま残した（`値` → `值`、縦書きの `ー` → `一`、2 段目の柱の位置に `6` など）。選択の正解は `OcrPage` の文字なので、読み違いは比べる妨げにならない。

### 骨組み

- [web/](web/): Vite 8.3.6 ＋ React 19.3.1 ＋ TypeScript 6.0.3（アプリの `package.json` と同じ系列）。`fixtures/` を Vite の `publicDir` にして、ページの絵と JSON を読む。100 % は 1 pt = 96/72 px（pdf.js と同じ）。
- [slint/](slint/): Slint 1.18.1、自分の `[workspace]`。同じ `fixtures/` を読む。フォントは前の試作と同じく Plex Sans JP の TTF（git に入れない）。

### 確かめの道具

- **Chromium**: 入れ物の Playwright 1.63.0 と Chromium（`/opt/pw-browsers`）。
- **WebKit**: Playwright の WebKit は取れなかった（`playwright install webkit` が `cdn.playwright.dev` と `playwright.download.prss.microsoft.com` への接続をネットワークの決まりで断られた）。代わりに `apt` で **WebKitGTK 2.52.6**（`libwebkit2gtk-4.1-0`、Tauri が Linux で使うものと同じ）と `webkit2gtk-driver`（`WebKitWebDriver`）を入れ、付属の `MiniBrowser` を Xvfb の上で WebDriver（W3C の Actions）から動かした。Playwright の WebKit より、**Linux の Tauri の WebView そのもの**に近い。
- **Slint**: Xvfb と `Window::dispatch_event`（前の試作の `drive.rs` を写す）。
