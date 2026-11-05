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

## 2. HTML（ブラウザーの選択を使う）

### どう作ったか

- [web/src/native/NativeView.tsx](web/src/native/NativeView.tsx): pdf.js の text layer と同じく、OCR の **1 行を 1 つの透明な `span`** にして、行の箱の左上に `position: absolute` で置く。字の大きさは行の太さ（横書きは高さ、縦書きは幅）。描いたあと自然な幅（縦書きは高さ）を測り、`transform: scaleX`（縦書きは `writing-mode: vertical-rl` と `scaleY`）で行の長さに合わせる。行の後ろに `<br>`、塊の後ろにもう 1 つ `<br>` を置く。
- 選択とコピーは `window.getSelection` とブラウザーのコピーにまかせた。**コピーの形（行の間は改行、塊の間は空行、縦書きは列の順）は、`<br>` の置き方だけで Chromium と WebKitGTK の両方で正解と一致した**（`all` の台本）。
- 帯はブラウザーの `::selection`（文字は透明、背景だけ色を付ける）。
- 縦書きの欄: 選択の focus のある塊が縦書きなら、横の欄に `writing-mode: vertical-rl` で行を `div` ごとに出す。欄の中の選択とコピーもブラウザーまかせ。
- 検索はブラウザーにないので、項目 3・4 と同じ自作の検索（[web/src/core/search.ts](web/src/core/search.ts)、字の列を NFKC と小文字にそろえ、塊の中の行はつないで探す）と、字の 4 点からの帯（[web/src/core/select.ts](web/src/core/select.ts) の `bands`）を使った。

### 工夫が要った所

1. **Ctrl+A**: ブラウザーの全選択は、道具の欄や横の欄まで選ぶ。`keydown` で Ctrl+A を取り、`Range.selectNodeContents` で text layer（欄にいるときは欄）だけを選ぶようにした（20 行）。道具の欄は `user-select: none`。
2. **すき間で選択が飛ぶ**: 工夫なしでは、塊の間・行の後ろ・余白にドラッグが出ると、**選択の端がページの最後（text layer の最後の要素）まで飛んだ**（Chromium と WebKitGTK の両方、下の表の 11 か所すべて）。pdf.js の工夫（`text_layer_builder.js` の `#enableGlobalSelectionListener`。押している間だけ text layer 全体を覆う空の `div` を後ろに敷き、選択が変わるたびにそれを DOM の中で選択の動く端の直後へ移す）を写した（[web/src/native/endOfContent.ts](web/src/native/endOfContent.ts)、42 行）。これで「最後まで飛ぶ」はなくなったが、**すき間では選択が「最後に字の上にあった位置」で止まる**（いちばん近い行の端へは伸びない）。行の右の余白へ素早く抜けると、行の途中で止まる。
3. **縦書きの字形（WebKitGTK）**: WebKitGTK 2.52.6 は、`writing-mode: vertical-rl` だけでは、フォントの縦書き用の字形（OpenType の `vert`）を使わなかった。句読点が横書きの位置に出て、かぎ括弧と長音が横向きのままになった（[screenshots/webkitgtk-vert-test.png](screenshots/webkitgtk-vert-test.png)。左から Plex Sans JP（webfont）、Noto Sans CJK JP（OS のフォント）、Plex に `font-feature-settings: "vert" 1`、`text-orientation: upright`、`lang="ja"`）。**明示して `font-feature-settings: "vert" 1` を付けると正しくなった**。Chromium は何も付けなくても正しい（[screenshots/chromium-vert-test.png](screenshots/chromium-vert-test.png)）。確かめのページは [fixtures/vtest.html](fixtures/vtest.html)。Linux の Tauri（WebKitGTK）で縦書きを出すなら、この 1 行が要る。
4. **フォント**: 入っていないフォント名（`"IBM Plex Sans JP"`）を、WebKitGTK は fontconfig の代わりのフォント（この入れ物では Inter）で受けてしまい、日本語は別のフォントに落ちた。アプリと同じく Plex Sans JP を webfont として同梱した（`@ibm/plex-sans-jp`、OFL-1.1、アプリと同じ依存）。webfont が読み込まれると字の幅が変わるので、`document.fonts.ready` のあとに `scaleX` を測り直す必要があった。
5. **欄のあふれ（WebKitGTK）**: `writing-mode: vertical-rl` の箱を横書きの欄に置くと、列があふれたとき WebKitGTK は右へはみ出し、最初の列が見えなくなった。欄そのものを `vertical-rl` にすると、あふれは左へ伸び、右から読み始められた（Chromium は両方で同じ見た目）。

### 結果（倍率 100 %、[results/](results/)）

台本は項目 5 のもの。○ は正解と同じ、△ は端が 1〜2 字ずれた、× は違う。正解は「字の 4 点からいちばん近い行の、字の境目」（項目 3 の決まり）なので、すき間で終わる台本（`end-*`）の × は「ブラウザーは別の所で止めた」の意味。

| 台本 | Chromium 工夫なし | Chromium pdf.js の工夫 | WebKitGTK 工夫なし | WebKitGTK pdf.js の工夫 |
| --- | --- | --- | --- | --- |
| Ctrl+A → Ctrl+C（4 ページ） | ○○○○ | ○○○○ | ○○○○ | ○○○○ |
| 塊をまたぐドラッグ（4） | ○○○○ | ○○○○ | ○○○○ | ○○○○ |
| 余白を通って字で終わる（4） | ○○○○ | ○○○○ | ○○○○ | ○○○○ |
| Shift+クリック（4） | ○○○△ | ○○○△ | ○○○○ | ○○○○ |
| 行の後ろのすき間で終わる（4） | ×××× | ×△×△ | ×××× | ×○×○ |
| 塊の間のすき間で終わる（4） | ×××× | △×△× | ×××× | ××△× |
| 余白で終わる（4） | ××○× | ×○×× | ××○× | ××○× |
| 行をまたぐ検索（4） | ○○○○ | ○○○○ | ○○○○ | ○○○○ |
| 縦書きの欄: Ctrl+A、ドラッグ（3 ページ × 2） | ○○○○○○ | ○○○○○○ | ○○○○○○ | ○○○○○○ |
| 合計 38 | 26 ○、1 △ | 26 ○、5 △ | 27 ○ | 29 ○、1 △ |

- 工夫なしの × はすべて「ページの最後まで飛ぶ」だった（コピーした字が 100〜300 字多い）。
- △ の多くは、**ブラウザーが置いた字の位置と OCR の字の位置の差**。1 行を 1 つの `span` にして `scaleX` で伸ばすので、ブラウザーの中では字が等間隔に並ぶ。OCR の字の 4 点は等間隔ではない（句読点や英数字は狭い）ので、行の途中では 1〜2 字ずれる。字ごとに `span` を置けばずれはなくなるはずだが、要素が 10 倍になり、pdf.js もそうしていない（試していない）。
- Chromium と WebKitGTK の差は、すき間で止まる位置の細部だけで、コピーの形と縦書きの選択は同じだった。

### 行数

| 部分 | ファイル | 行数 |
| --- | --- | --- |
| text layer、Ctrl+A、縦書きの欄 | [native/NativeView.tsx](web/src/native/NativeView.tsx) | 116 |
| すき間の工夫（pdf.js の写し） | [native/endOfContent.ts](web/src/native/endOfContent.ts) | 42 |
| 検索と帯（項目 4 と共用） | [core/search.ts](web/src/core/search.ts)、[core/select.ts](web/src/core/select.ts) の `bands`、[shared/](web/src/shared/) | 約 60 |
| 型と字の列（項目 4 と共用） | [core/ocr.ts](web/src/core/ocr.ts) | 115（うち型 50） |

（空行とコメントだけの行を除く。試験と台本 `web/tests/` を除く。）

- かかった時間: 約 1 時間半（うち WebKitGTK の字形・フォント・あふれの調べに約 40 分）。
