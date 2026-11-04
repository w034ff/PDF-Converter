# Slint 切り出しの試作（捨てる前提）

切り出しの画面と OCR の下ごしらえを Slint で作れるかを確かめる試作。マージしない。結果は [REPORT.md](REPORT.md)。

## 準備（Linux と Windows 共通）

リポジトリの直下で次を実行する（Node.js 24.15 以上。`npm ci` が古い Node で止まるときも、下の 2 つは動く）。

```bash
npm run pdfium:fetch
```

```bash
npm run layout:fetch
```

`src-tauri/pdfium/<os>/`、`src-tauri/onnxruntime/<os>/`、`src-tauri/layout-model/` に置かれ、試作はそこを見る。別の場所にあるときは環境変数 `SPIKE_PDFIUM_DIR`（pdfium のライブラリのあるフォルダ）、`SPIKE_ONNXRUNTIME`（onnxruntime のファイル）、`SPIKE_LAYOUT_MODEL`（モデルのファイル）で指す。レイアウトの解析が使えないときは、写真の範囲だけを候補に出す。

### フォント（git で追跡しない）

`slint-spike/fonts/` に `IBMPlexSansJP-Regular.ttf` と `IBMPlexSansJP-Bold.ttf` を置く。npm のパッケージには woff と woff2 しかないので、woff を TTF に戻す（Python の標準ライブラリだけで動く）。

```bash
cd slint-spike/fonts && npm pack @ibm/plex-sans-jp && tar xzf ibm-plex-sans-jp-*.tgz
```

```bash
for w in Regular Bold; do python3 ../tools/woff2ttf.py package/fonts/complete/woff/hinted/IBMPlexSansJP-$w.woff IBMPlexSansJP-$w.ttf; done
```

Windows の PowerShell では `python ..\tools\woff2ttf.py <woff> <ttf>` を 2 回実行する。

### 試験用の PDF（220 ページ）

```bash
cd slint-spike && cargo run --release --bin gen-pdf
```

`testdata/sample-220.pdf` ができる（git で追跡しない）。文字は Plex Sans JP で埋め込み、各ページに埋め込みの画像（写真）、ベクターの棒グラフ、表がある。

## 動かす

```bash
cd slint-spike && cargo run --release -- testdata/sample-220.pdf
```

- `--dpi 300`: ページを 300 dpi で描く（既定は 150）。
- 描き方は環境変数で選ぶ: `SLINT_BACKEND=winit-femtovg`（OpenGL）、`SLINT_BACKEND=winit-software`（ソフトウェア）。
- `--screenshot DIR`: 決まった操作を本物のポインターのイベントで流し、画面の画像を `DIR` に保存して終わる（`screenshots/` はこれで作った）。

## 確かめ方

```bash
cd slint-spike && cargo build && cargo test
```

`tests/ui.rs` は Slint の試験用のバックエンドで窓を作り、ポインターのイベントを送って範囲の描画・候補の採用・重なった所のドラッグ・つまみでの大きさの変更を確かめる。
