# 詳細設計書

対応する要件は [requirements.md](requirements.md) の ID（FR-xx / NFR-xx）で示す。画面の見た目は [mockup/project/](mockup/project/)（各画面の `.dc.html`）を正とする（`Accent*.dc.html` は色を選んだときの比較で、画面の仕様ではない）。方式の根拠は [spike-report.md](spike-report.md) にある。

## 1. 全体構成

```
┌──────────────── メインプロセス（Tauri アプリ） ────────────────┐
│  WebView（React + TypeScript）                                  │
│   ・一覧、設定、進捗の表示。ファイルパスを扱わない（ID のみ）    │
│            │ invoke / event（IPC）                              │
│  Rust（src-tauri）                                              │
│   ・ダイアログ、D&D、パスと ID の表、設定、変換の実行管理        │
│   ・画像 → PDF（crates/core を呼ぶ。pdfium は使わない）           │
│   ・ワーカーの起動・監視・強制終了                              │
│            │ 標準入出力（§5.1 のメッセージ）                    │
└────────────┼────────────────────────────────────────────────────┘
             │  1〜N 個
┌────────────┴──────── ワーカープロセス（同じ実行ファイル） ──────┐
│  `pdf-converter --pdf-worker` で起動。WebView を作らない          │
│   ・pdfium で PDF を開く、ページ数・大きさを返す                 │
│   ・ページを描画し、PNG / JPEG にして返す（サムネイルも同じ）    │
│   ・ファイルを書かない。読むのは渡された PDF 1 つだけ             │
└──────────────────────────────────────────────────────────────────┘
```

- PDF の読み込みと描画は、すべてワーカーで行う。pdfium が壊れた PDF で異常終了しても、終了するのはワーカーだけで、アプリは動き続ける（NFR-01）。無限ループとメモリの使いすぎも、時間とメモリの上限（§5.3）でワーカーごと止める。
- 画像 → PDF は、画像の読み込み（image）と PDF の書き出し（krilla）がどちらも純 Rust なので、メインプロセスで行う。
- 変換の処理は Tauri に依存しない crate `crates/core` に置く（SVG Tracer の `crates/tracer` と同じ考え方）。WebKitGTK なしに `cargo test` できる。
- フロントエンドはファイルパスを Rust に渡さない。ダイアログと D&D は Rust で受け、パスは Rust の表に登録して ID だけを返す（SVG Tracer §6 と同じ）。

## 2. 技術選定

| 領域 | 採用 | 理由 |
| --- | --- | --- |
| アプリ基盤 | Tauri 2 | 要件で決定済み |
| PDF の描画 | `pdfium-render` + bblanchon/pdfium-binaries（版を固定、§8） | スパイクで決定。暗号化された PDF を開ける、速い |
| PDF の書き出し | `krilla`（`raster-images` 機能） | スパイクで決定。JPEG を再圧縮せずに入れられる、純 Rust |
| 画像の読み込み | `image`（png、jpeg、webp、bmp 機能のみ） | 寸法・EXIF の向きの取得、BMP の読み込み、PDF → 画像の PNG / JPEG の書き出し |
| 並列処理 | `std::thread` とチャネル | ワーカーはプロセスなので、スレッドプールのライブラリは要らない |
| 一時ファイル | `tempfile` | 原子的な保存（§6.5） |
| IPC 型共有 | `ts-rs` | SVG Tracer と同じ |
| フロントエンド | React + TypeScript + Vite、素の CSS + CSS カスタムプロパティ | SVG Tracer と同じ。部品と仕組みを流用する |
| フロントエンドのテスト | Vitest + Testing Library + jsdom、`@tauri-apps/api/mocks` | SVG Tracer と同じ |
| ライセンス | `cargo-about`、`cargo-deny`、`license-checker-rseidelsohn`、pdfium の同梱ライセンス（§8.3） | NFR-05 |

- ライブラリは実装開始時点の最新安定版とし、`Cargo.lock` と `package-lock.json` をコミットして固定する。
- `pdfium-render` の pdfium の API の版を選ぶ機能（`pdfium_NNNN`）は、同梱する pdfium の版以下で最も新しいものを明示する（2026-10 時点では、同梱の 8076 に対して `pdfium_7881`。スパイクはこの組み合わせで動いた）。`pdfium_latest` は使わない（crate の更新で API の版が同梱の pdfium より新しくならないようにするため）。
- Tauri のプラグインは `tauri-plugin-dialog` だけを使い、Rust 側からだけ呼ぶ（§9）。

## 3. ディレクトリ構成

```
/
├── crates/core/              変換の処理（Tauri 非依存）。パッケージ名は pdfconv-core
│   ├── src/
│   │   ├── lib.rs
│   │   ├── probe.rs          画像の形式・寸法・向き・解像度の取得（§4.1）
│   │   ├── layout.rs         ページの大きさと画像の配置（§4.2）
│   │   ├── pdf_write.rs      krilla で PDF を書く（§4.3）
│   │   ├── page_range.rs     ページの範囲の解釈（§4.5）
│   │   ├── naming.rs         出力名の決定（§6.4）
│   │   └── error.rs
│   ├── examples/gen_fixtures.rs   フィクスチャの生成（§11.1）
│   └── tests/
├── crates/worker/            ワーカー（pdfium を使う唯一の crate）。パッケージ名は pdfconv-worker
│   └── src/
│       ├── protocol.rs       メッセージの型と読み書き（メインプロセスと共有）
│       ├── server.rs         ワーカーの本体（§5）
│       ├── client.rs         メインプロセスの側の 1 つのワーカー（起動、要求、時間切れ、異常終了）
│       ├── windows_job.rs    Windows のジョブオブジェクト（§5.3）
│       └── main.rs           テスト用の単独のワーカー（`pdfconv-worker`）
├── src-tauri/
│   ├── src/
│   │   ├── main.rs           `--pdf-worker` なら worker を実行、それ以外はアプリ
│   │   ├── commands.rs       IPC コマンド（§7）
│   │   ├── items.rs          ファイルの ID の表と一覧の項目
│   │   ├── worker_pool.rs    ワーカーの数の管理と貸し出し（§5.2）。1 つのワーカーの扱いは crates/worker の client.rs
│   │   ├── jobs.rs           変換の実行・進捗・キャンセル（§6）
│   │   └── settings.rs       設定の読み書き（§6.7）
│   ├── pdfium/               取得した pdfium（コミットしない、§8.1）
│   ├── capabilities/main.json
│   └── tauri.conf.json
├── src/                      フロントエンド
│   ├── features/images-to-pdf/
│   ├── features/pdf-to-images/
│   ├── features/about/
│   ├── components/           一覧、切り替えボタン、進捗など
│   ├── ipc/、ipc/generated/
│   ├── i18n/
│   ├── licenses/
│   └── styles/
├── scripts/                  fetch-pdfium.ts、generate-licenses.ts
├── docs/
├── about.toml / deny.toml
├── GEMINI.md
└── .github/workflows/        ci.yml / release.yml / audit.yml / pdfium-update.yml
```

## 4. 変換の処理（crates/core）

### 4.1 画像の読み込み（FR-01、FR-02）

- 対象の拡張子は `png` `jpg` `jpeg` `webp` `bmp`（大文字小文字を区別しない）。定数 `IMAGE_EXTENSIONS` にまとめる。
- 形式は中身の先頭バイトで判定する。判定できない、または上記以外の形式なら `UnsupportedFormat`。
- 一覧に追加するときは、ヘッダーだけを読んで、形式、寸法、EXIF の向き、解像度（dpi）を得る（`probe`）。画素は読まない。読めなければ `DecodeFailed`。ヘッダーより後ろだけが壊れているファイル（`corrupt.png`）は `probe` を通り、画素を読む変換のとき（§4.3）に `DecodeFailed` になる。
- 総ピクセル数の上限 `MAX_IMAGE_PIXELS = 80_000_000`。A3 を 600 dpi で読み取った画像（約 7016×9921 = 6960 万画素）が収まる値。超えたら `TooLarge`。
- 解像度は、PNG の `pHYs`、JPEG の JFIF の密度または EXIF の `XResolution`、BMP のヘッダーから読む。WebP と、値がない・単位が不明・`MIN_DPI`（36）〜`MAX_DPI`（2400）の外にある場合は `DEFAULT_DPI`（96）とする。
  - JPEG は、JFIF の密度の単位が dpi か dpcm ならその値を、そうでなければ EXIF の `XResolution`（`ResolutionUnit` がインチかセンチメートルのとき。省略はインチ）を使う。
  - 横と縦の値が違うときは横の値を使う。値は整数の dpi に四捨五入してから範囲を確かめる（pHYs の 11811 画素/m は 300 dpi になる）。
- BMP は `image` で読み、`image` が返した RGBA をそのまま使う。32 ビットの BMP の 4 つ目の成分を透過として扱うかは `image` の解釈に従う（アルファのマスクを持つ BMP（`BITMAPV4HEADER` 以降）は透過ありとして読まれ、マスクのない 32 ビットの BMP（スパイクで使ったもの）は不透明として読まれ、透過の部分は黒になる）。BMP の透過の扱いは規格で曖昧なので、アプリでは手を加えない。

### 4.2 ページの大きさと配置（FR-02）

- EXIF の向きが 90 度・270 度の回転を含むとき、表示上の幅と高さを入れ替えてから以下を計算する。
- **画像に合わせる**: ページの大きさを「画素数 ÷ dpi × 72」pt とする（余白なし）。300 dpi の A4 の読み取り画像は A4 のページになる。
- **A4**: 210×297 mm（595.28×841.89 pt）。表示上の幅が高さより大きい画像は横向き、それ以外は縦向きにする。四辺に `A4_MARGIN_MM`（10 mm）の余白を取り、残りの領域に縦横比を保って収まる最大の大きさで、中央に置く。小さい画像も拡大する。
- 1 ページの辺の長さは `MAX_PAGE_SIDE_PT`（14400 pt = 200 インチ）までにする。Acrobat が扱えるページの大きさの上限で、これを超えると開けないビューアーがあるため。「画像に合わせる」でこれを超える場合は、縦横比を保って上限に収まるよう縮める。

### 4.3 PDF の書き出し（FR-02、FR-03）

- 1 つの画像を 1 ページにする。結合では一覧の順にページを並べる。
- JPEG は `Image::from_jpeg` で、元のバイト列をそのまま入れる（`DCTDecode`）。CMYK の JPEG もそのまま入れる。
- PNG は `Image::from_png`、WebP は `Image::from_webp`。どちらも画素を変えずに可逆の圧縮で入れ、透過を保つ。16 ビットの PNG は 16 ビットのまま入れる。
- BMP は §4.1 の RGBA を `Image::from_rgba8` で入れる。
- EXIF の向きは、画像のデータを変えずに、ページの中の変換行列で表す。8 通りの行列はスパイクの `write-bench` に実装済みで、そのまま使う。
- PDF の文書情報には、作成ソフト（`PDF Converter <版>`）だけを入れる。元のファイル名やパスは入れない（意図せずに情報が漏れないようにするため）。
- `krilla` の出力をメモリに作ってから §6.5 の方式で書き込む。結合する画像の数の上限は設けないが、§6.2 の進捗を 1 枚ごとに通知する。

### 4.4 PDF → 画像の書き出し（FR-04）

ワーカーが pdfium の描画結果（BGRA）を受け取り、次のように画像にする。

- 背景は白で塗り、その上に描画する（PNG と JPEG の両方）。PDF のページは紙を表すので、透過のまま出すと、表示するソフトによっては文字が黒い背景に溶けて読めなくなるため。
- PNG: 8 ビットの RGB。圧縮の強さは `image` の既定。
- JPEG: 品質 `JPEG_QUALITY`（90）。
- 描画の寸法は「ページの大きさ（pt）÷ 72 × dpi」を四捨五入した値。総ピクセル数が `MAX_RENDER_PIXELS`（100_000_000）を超えるページは描かずに `RenderTooLarge` とする。A0 を 300 dpi で描くと約 1.4 億画素になり、これは上限を超える。
- 解像度の選択肢は 72、150、300 dpi、既定は 150 dpi。定数 `DPI_CHOICES` と `DEFAULT_RENDER_DPI` に置く。

### 4.5 ページの範囲（FR-04）

- 書式: 区切りは `,`（全角の `，` と `、` も受け付ける）。各要素は `n` または `a-b`（`-` は全角の `－`、`〜`、`～` も受け付ける）。前後の空白（全角の空白を含む）は無視する。
- 数字は半角の `0`〜`9` と全角の `０`〜`９` だけを受け付ける。日本語の入力のまま打った数字を通すため。`+` などの符号は受け付けない。
- `n`、`a`、`b` は 1 以上の整数で、`a ≤ b`。満たさなければ `InvalidPageRange`（どの要素が悪いかを `detail` に入れる）。
- 範囲はページ番号に展開せず、区間（開始と終了）のまま持つ。`1-4000000000` のような入力でも、メモリと時間が入力の長さに比例する程度で済むようにするため（この関数は入力欄の変更のたびにメインプロセスで呼ばれる）。ページ数との突き合わせ（`pages_within`、変換するページの数）は、PDF のページ数（`MAX_PDF_PAGES` 以下）の範囲だけを数える。
- 重なりと順序は正規化する（`5, 1-3, 2` → 1、2、3、5）。
- PDF のページ数を超える番号は、エラーにせず、その PDF では飛ばす（FR-04 の「あるページだけを変換する」）。1 ページも残らなければ、その PDF は「対象のページなし」。
- 純粋関数 `parse_page_range(text) -> Result<PageSet, …>` と `PageSet::pages_within(page_count)` にして、単体テストで網羅する。フロントエンドには同じ処理を持たず、§7 のコマンドで Rust に問い合わせる（解釈の規則を 1 か所にするため）。

## 5. ワーカー（NFR-01）

### 5.1 メッセージ

メインプロセスとワーカーは、標準入力と標準出力でメッセージをやりとりする。標準エラー出力はログとして読み捨てる。

- 1 つのメッセージ = 4 バイトの長さ（リトルエンディアン）+ JSON のヘッダー + （あれば）バイナリの本体。本体の長さはヘッダーの `bodyLength` に入れる。画像を base64 にしないため。
- 要求と応答は 1 対 1 で、ワーカーは一度に 1 つの要求だけを処理する。

| 要求 | 内容 | 応答 |
| --- | --- | --- |
| `Open { path }` | PDF を開く。前に開いていたものは閉じる | `{ pageCount, pages: [{ widthPt, heightPt }] }` またはエラー |
| `Render { page, dpi, format }` | 開いている PDF の 1 ページを描画して書き出す | 本体に PNG / JPEG のバイト列 |
| `Thumbnail { page, maxSide }` | 長い辺を `maxSide` px にして描画し、PNG にする | 本体に PNG |
| `Close` | 開いている PDF を閉じる | `{}` |

- エラーの応答は `{ code, detail }`（§6.6 のコード）。pdfium のパスワードのエラーは `PasswordProtected`、それ以外で開けないものは `PdfOpenFailed`。
- ページ数が `MAX_PDF_PAGES`（10_000）を超える PDF は、`Open` で `TooManyPages` を返す。
- ワーカーはファイルを書かない。描画した画像はメインプロセスに返し、メインプロセスが §6.5 の方式で保存する。

### 5.2 ワーカーの管理（`worker_pool.rs`）

- ワーカーは必要になったときに起動し、使い終わったら次の要求のために残す。数の上限は `max(1, min(論理 CPU 数 - 1, MAX_WORKERS))`、`MAX_WORKERS` は 4。pdfium はスレッドの並列に対応しないので、並列はプロセスの数で行う。
- 一覧の追加（ページ数の取得）、サムネイル、変換は、どれもプールからワーカーを借りて行う。
- 応答の待ちには時間の上限を設ける: `Open` は `OPEN_TIMEOUT`（30 秒）、`Render` と `Thumbnail` は `RENDER_TIMEOUT`（60 秒）。超えたらワーカーを強制終了し、その要求を `WorkerTimeout` で失敗させる。
- ワーカーが応答の途中で終了した、または読めない応答を返した場合は、その要求を `WorkerCrashed` で失敗させ、ワーカーを捨てる。次の要求では新しいワーカーを起動する。
- 1 つの PDF でワーカーが続けて落ちるのを避けるため、`WorkerCrashed` / `WorkerTimeout` になった PDF は、その一括変換の中で再試行しない。
- アプリの終了時には、すべてのワーカーを終了させる。

### 5.3 メモリの上限

- ワーカー 1 つが使えるメモリを `WORKER_MEMORY_LIMIT`（2 GiB）までにする。超えた確保は失敗し、ワーカーは終了する（`WorkerCrashed` として扱う）。
- Linux: ワーカーが起動直後、pdfium を読み込む前に `setrlimit(RLIMIT_AS)` で自分に上限を掛ける。
- Windows: メインプロセスがワーカーを起動したあと、ジョブオブジェクトに入れて `JOB_OBJECT_LIMIT_PROCESS_MEMORY` を掛ける。あわせて `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` を付け、メインプロセスが異常終了したときにワーカーが残らないようにする。最初の要求を送る前にジョブに入れるので、上限の外で pdfium が動くことはない。
- Windows のジョブオブジェクトの操作は `crates/worker/src/windows_job.rs` に閉じ込め、ワークスペースで unsafe を許すのはこのモジュールだけにする（ワークスペースの `unsafe_code` は、このモジュールが例外を宣言できるよう `deny` にする）。`JOB_OBJECT_LIMIT_PROCESS_MEMORY` を設定できる安全な包みの crate がないため（`win32job` 2.0 はワーキングセットの制限だけで、これは確保を失敗させない）。
- Linux で親が異常終了した場合は、ワーカーの標準入力が閉じ、ワーカーは次の読み込みで終了する。pdfium の処理の途中で止まっているワーカーは、その処理が終わるか時間切れになるまで残りうる。
- スパイクでの最大は約 84 MB（23 ページの論文を 300 dpi）で、上限は十分な余裕を持つ。上限の値と仕組みが Windows でも働くことを、作業計画の最初のタスクで確かめる。

### 5.4 ワーカーの見つけ方

- メインプロセスは `std::env::current_exe()` を `--pdf-worker` 付きで起動する。別の実行ファイルを同梱しない。
- ワーカーは、同梱した pdfium のライブラリ（§8.2）の場所を引数で受け取る。Tauri のリソースのフォルダはメインプロセスだけが知っているため。

## 6. 各機能の設計

### 6.1 一覧（FR-01）

- 「画像 → PDF」と「PDF → 画像」は、それぞれ 1 つの一覧を持つ。一覧の項目は Rust の表に登録し、フロントエンドには ID と表示用の情報だけを渡す。
- 追加の方法: 「画像を追加」「PDF を追加」（複数選択のファイルダイアログ）、「フォルダを追加」（フォルダダイアログ）、D&D。
- フォルダは直下だけを見る。サブフォルダ、シンボリックリンク、隠しファイル（`.` で始まる名前）、対応しない拡張子は追加せず、件数を返す。
- D&D は Rust の `WindowEvent::DragDrop` で受ける。落とされたものを拡張子で分け、画像は「画像 → PDF」の一覧に、PDF は「PDF → 画像」の一覧に入れる。フォルダは中を見て同じように分ける。フロントエンドは、追加があった一覧のタブに切り替える（両方に入ったときは、表示中のタブのまま）。
- 同じファイル（正規化したパスが同じ）がすでに一覧にあれば、追加しない。
- 追加のときに、画像は §4.1 の `probe`、PDF はワーカーの `Open` を行う。失敗したファイルも一覧に入れ、行にエラーの理由を表示する。エラーの行は変換の対象にしない。パスワード付きの PDF も、ここで分かる。
- 画像の一覧のサムネイルは、Rust が画像を縮小して PNG で返す。PDF のサムネイルはワーカーの `Thumbnail` で作る。どちらも長い辺 `THUMBNAIL_SIDE`（160 px）。サムネイルは表示されたときに 1 つずつ要求する（多数の追加で待たされないようにするため）。
- 並べ替え（画像の一覧のみ。結合の順になる）: ドラッグ、各行の ↑↓ ボタン、行にフォーカスがあるときの Alt + ↑ / ↓。順序はフロントエンドが持ち、変換を始めるときに ID の並びとして渡す。
- 変換中は一覧を変えられない（追加・削除・並べ替えのボタンを無効にし、D&D は `ConversionRunning` で拒む）。

### 6.2 画像 → PDF（FR-02、FR-03、FR-05）

- 「1 つの PDF」: 「PDF を保存」で保存ダイアログを開く。初期ファイル名は一覧の先頭の画像の名前の拡張子を `.pdf` にしたもの。ダイアログで既存のファイルを選んだ場合は、OS のダイアログで上書きの確認が済んでいるので上書きする。
- 「1 枚ずつ」: 保存先のフォルダを選び、「変換を開始」で一覧の画像を 1 つずつ PDF にする。出力名は §6.4。並列の数は §5.2 と同じ式（ワーカーは使わず、スレッドで行う）。
- どちらも、進捗を `job-progress`、1 件ごとの結果を `job-item` で通知し、最後に `job-finished` を送る（§7.2）。

### 6.3 PDF → 画像（FR-04、FR-05）

- 一覧に PDF が 1 つのときは、ページのサムネイルを並べ、範囲の指定で変換するページを示す。複数のときは、一覧の表とする（モックアップ `PdfSingle` と `PdfBatch`）。
- 範囲を入力するたびに `check_page_range` を呼び、変換するページの合計を表示する。書式の誤りは入力欄の下に表示し、「変換を開始」を無効にする。
- 「変換を開始」で、保存先のフォルダと、形式、解像度、範囲を渡す。PDF ごとにワーカーを 1 つ借り、`Open` → 対象のページごとに `Render` → 保存、を行う。PDF の単位で並列にする。
- 出力名: `元の名前_p{番号}.{png|jpg}`。番号は、その PDF のページ数の桁数で 0 を詰める（8 ページなら `p1`〜`p8`、120 ページなら `p001`〜`p120`）。ファイルの一覧で名前の順に並べたとき、ページの順になるようにするため。
- 1 つの PDF の途中のページで失敗したときは、その PDF の残りのページを続け、その PDF を「一部失敗」として失敗のページ番号を表示する。

### 6.4 出力名の決定

SVG Tracer の §5.3 と同じ規則にする。

- 候補名が「保存先のフォルダに既にある名前」または「この変換で割り当て済みの名前」と重なったら、`name (1).pdf`、`name (2).pdf` … と番号を付ける。PDF → 画像では `report_p1 (1).png` の形になる。
- 重なりの判定は大文字小文字を区別しない。名前は**開始時に全件分まとめて決める**。
- 保存の瞬間に他のプロセスが同名のファイルを作っていたら、上書きせずに次の番号で保存し直す。
- 純粋関数として `naming.rs` に置き、単体テストで網羅する。

### 6.5 保存とキャンセル

- 保存は、保存先のフォルダに一時ファイルを作って全内容を書き、`persist_noclobber` で最終名にする（SVG Tracer §5.4 と同じ）。書き込みの途中で失敗・中断しても、不完全なファイルは残らない。
- キャンセルは共有フラグで行う。画像 → PDF は次のファイルの前に、PDF → 画像は次のページの前に確かめる。描画中のページは終わらせて保存する（pdfium の描画は途中で止められないため）。キャンセルした PDF は、保存済みのページ数を表示する。
- 結合（1 つの PDF）をキャンセルしたときは、PDF を書かない。
- キャンセルを押した直後に「キャンセル中…」を表示し、ボタンを無効にする（NFR-02）。

### 6.6 エラー（FR-09）

IPC では `{ code, detail }` の形で返す（SVG Tracer §5.5 と同じ。型の合わない引数を `InvalidParams` に変換するフロントエンドの仕組みも同じ）。

| コード | 状況 | 表示文言（ja / en） |
| --- | --- | --- |
| UnsupportedFormat | 非対応の形式 | 対応していない形式です / Unsupported file format |
| DecodeFailed | 画像を読めない（壊れたファイル） | 画像を読み込めませんでした。ファイルが壊れている可能性があります / Couldn't read the image. The file may be damaged. |
| PdfOpenFailed | PDF を開けない（壊れたファイル） | PDF を読み込めませんでした。ファイルが壊れている可能性があります / Couldn't read the PDF. The file may be damaged. |
| PasswordProtected | パスワード付きの PDF | パスワードで保護されているため開けません / This PDF is password-protected |
| TooLarge | 画像のピクセル数が上限超過 | 画像が大きすぎます（上限 {detail} ピクセル） / Image is too large (limit: {detail} pixels) |
| TooManyPages | PDF のページ数が上限超過 | ページ数が多すぎます（上限 {detail} ページ） / Too many pages (limit: {detail}) |
| RenderTooLarge | 描画の大きさが上限超過 | この解像度では大きすぎて画像にできません。解像度を下げてください / Too large at this resolution. Choose a lower resolution. |
| WorkerCrashed | ワーカーが異常終了した | PDF の処理中に問題が起きました。ファイルが壊れている可能性があります / Something went wrong while processing the PDF. The file may be damaged. |
| WorkerTimeout | ワーカーが時間内に応答しない | PDF の処理に時間がかかりすぎたため中止しました / Processing the PDF took too long and was stopped |
| ReadFailed | 読み込みの失敗 | ファイルの読み込みに失敗しました / Failed to read file |
| WriteFailed | 書き込みの失敗（権限、容量など） | ファイルの書き込みに失敗しました / Failed to write file |
| InvalidPageRange | 範囲の書式の誤り | ページの範囲の書き方が正しくありません（{detail}） / Invalid page range ({detail}) |
| ConversionRunning | 変換中に一覧や変換を操作しようとした | 変換中は操作できません / Not available during conversion |
| UnknownHandle | 存在しない ID | ファイルをもう一度追加してください / Please add the file again |
| InvalidParams | 引数が範囲外（UI からは通常送られない） | 無効な設定です / Invalid settings |

- 「対象のページなし」はエラーではなく、`job-item` の状態 `noPages` で表す（FR-04）。
- `TooLarge`、`TooManyPages` の `detail` には、Rust が上限の定数を 3 桁区切りの文字列にして入れる（上限の数値をフロントエンドに書き写さないため）。

### 6.7 設定の保存（FR-07、FR-10）

- 保存先は `app_config_dir()` 配下の `settings.json`。読み書きの方式、壊れた設定や未知の `schemaVersion` で既定値に戻すこと、フォルダの扱いは SVG Tracer §5.6 と同じ。
- 内容: `schemaVersion`、`language`、`imagesToPdf: { output: "merge" | "each", pageSize: "fit" | "a4", outputDir }`、`pdfToImages: { format: "png" | "jpeg", dpi, outputDir }`。
- 一覧の中身とページの範囲は保存しない（次回は空の一覧で始める）。
- 既定値: 「1 つの PDF」、「画像に合わせる」、PNG、150 dpi。

## 7. IPC

### 7.1 コマンド

| コマンド | 引数 | 戻り値 |
| --- | --- | --- |
| `get_settings` / `save_settings` | – / 設定（フォルダを除く） | §6.7 |
| `add_images` | `source: "files" \| "folder"` | `{ added: ImageItem[], skipped: { unsupported, folders, duplicates } } \| null` |
| `add_pdfs` | `source: "files" \| "folder"` | `{ added: PdfItem[], skipped: … } \| null` |
| `remove_items` | `ids` | – |
| `get_thumbnail` | `id, page?` | PNG のバイナリ（`tauri::ipc::Response`） |
| `pick_output_dir` | `kind: "imagesToPdf" \| "pdfToImages"` | `{ dirLabel } \| null` |
| `save_merged_pdf` | `ids, pageSize` | `{ savedName } \| null`（保存ダイアログを開く） |
| `start_images_to_pdfs` | `ids, pageSize` | –（§6.2 の「1 枚ずつ」） |
| `check_page_range` | `text, ids` | `{ totalPages }` または `InvalidPageRange` |
| `start_pdfs_to_images` | `ids, range, format, dpi` | – |
| `cancel_job` | – | – |
| `get_about` | – | `{ version, pdfiumVersion }` |

- `ImageItem`: `{ id, name, width, height, format, bytes, error }`。`PdfItem`: `{ id, name, pageCount, firstPageSizePt, bytes, error }`。`error` は失敗した項目だけに入る `{ code, detail }`。
- どのコマンドもパスを引数に取らない。`dirLabel` は表示用で、送り返されない。
- 変換は同時に 1 つだけ。実行中の `save_merged_pdf`、`start_*`、`add_*`、`remove_items` は `ConversionRunning` で拒む。

### 7.2 イベント（Rust → フロントエンド）

| イベント | ペイロード |
| --- | --- |
| `items-dropped` | `{ images: ImageItem[], pdfs: PdfItem[], skipped }` |
| `job-progress` | `{ done, total, current: string \| null }`（`total` は画像 → PDF は画像の数、PDF → 画像はページの合計） |
| `job-item` | `{ id, status: "ok" \| "failed" \| "partial" \| "noPages" \| "cancelled", outputs: string[], error?, failedPages? }` |
| `job-finished` | `{ succeeded, failed, noPages, unprocessed, cancelled }` |

## 8. pdfium の同梱と更新

### 8.1 取得

- `scripts/fetch-pdfium.ts` が、`PDFIUM_RELEASE`（例: `chromium/8076`）の `pdfium-win-x64.tgz` / `pdfium-linux-x64.tgz` を GitHub Releases から取得し、`PDFIUM_SHA256` の値と照合してから `src-tauri/pdfium/<os>/` に展開する。版とハッシュは `scripts/pdfium-version.json` の 1 か所に置く。
- 開発者は `npm run pdfium:fetch` を一度実行する。CI と release.yml も同じスクリプトを使う。取得したファイルはコミットしない。
- アプリは通信しない（NFR-01）。取得するのはビルドのときだけ。

### 8.2 同梱

- `tauri.conf.json` の `bundle.resources` で、その OS のライブラリ（`pdfium.dll` / `libpdfium.so`）をリソースとして同梱する。MSI、NSIS、AppImage、.deb のすべてで、メインプロセスが `resource_dir()` から場所を求め、ワーカーに渡す（§5.4）。
- 開発時（`tauri dev`）も、Tauri がリソースを `target/<profile>/pdfium/` に写すので、同じ `resource_dir()` の仕組みで見つかる。リソースの指定は `tauri.linux.conf.json` と `tauri.windows.conf.json` に OS ごとに置く。

### 8.3 ライセンス

- `scripts/generate-licenses.ts` が、取得した tgz の `LICENSE` と `licenses/` の 14 件を第三者ライセンス一覧に加える。版は `pdfium-version.json` から入れる。
- FreeType のライセンスが求める表示は、「このアプリについて」の一覧で満たす。
- ライセンスの許可リストの検査（NFR-05）と第三者ライセンス一覧は、配布物に入る依存だけを対象にする。テストとフィクスチャの生成にだけ使う dev-dependencies は除く（`deny.toml` の `[graph] exclude-dev = true`）。配布物に入らないものに同梱の条件は掛からないため。例: `jpeg-encoder` は IJG ライセンスを含むが、フィクスチャの生成にだけ使う。

### 8.4 更新の確認

- Dependabot は pdfium の版を見ないので、`.github/workflows/pdfium-update.yml` を月 1 回実行し、bblanchon/pdfium-binaries の最新の版が `pdfium-version.json` より新しければ Issue を作る（すでに同じ版の Issue があれば作らない）。
- 更新は手で行う: 版とハッシュを書き換え、`pdfium-render` の `pdfium_NNNN` 機能を §2 の規則で選び直し、§11 のテストを通してから PR にする。
- pdfium の脆弱性は、Chromium のセキュリティ修正として公開される。月 1 回の更新で取り込む。

## 9. セキュリティ（NFR-01）

SVG Tracer §7 と同じにする（capabilities は `core:*` の最小集合、CSP、HTTP クライアントを `cargo-deny` の `bans` で禁止、WebView2 の起動オプション、止められない通信の README への記載、既知の脆弱性の検査）。加えて次のとおり。

- PDF はワーカーでだけ開く（§1、§5）。メインプロセスに pdfium を読み込まない。`crates/worker` 以外の crate が `pdfium-render` に依存していないことを、`cargo-deny` の `bans`（`wrappers` の指定）で確かめる。
- ワーカーには、開く PDF のパスと pdfium の場所だけを渡す。ワーカーはファイルを書かない。
- サムネイルと描画した画像は、`blob:` URL にして `<img>` で表示する。

## 10. フロントエンド

### 10.1 画面構成

見た目と配置はモックアップに従う。要点:

- アプリ名は「PDF Converter」。ウィンドウの既定の大きさは 1200×800、最小は 960×640。
- 上部バー: アプリ名、タブ（画像 → PDF / PDF → 画像）、言語の切り替え、「このアプリについて」。
- 左の設定パネル: 画像 → PDF は「出力」「ページの大きさ」と、「1 枚ずつ」のときの保存先フォルダ。PDF → 画像は「ページ」「形式」「解像度」「保存先フォルダ」。
- 右の一覧: 空のときはドロップ領域。画像は行の一覧（番号、サムネイル、名前、寸法、↑↓、外す）。PDF は 1 つならページのサムネイルの格子、複数なら表。
- 下部バー: 状態の説明と、実行のボタン（「PDF を保存」または「変換を開始」）。変換中は進捗バーと「キャンセル」。

### 10.2 状態管理、多言語

SVG Tracer §8.3、§8.4 と同じ（`useReducer` と Context、`ja.ts` を基準にした型付きの辞書）。reducer は「画像 → PDF」「PDF → 画像」「変換の実行」「言語」に分ける。

### 10.3 テーマ

- 色は `styles/tokens.css` の CSS カスタムプロパティで定義する。SVG Tracer の tokens.css を元にし、アクセントの色をテラコッタ（ライト: `#b4492b`）に替える。ダーク用のアクセントの色は、背景との明るさの差を確かめて実装時に決め、決めた値をこの節に書き足す。
- 成功は緑、失敗は赤に「✓」「✕」の記号を添える。実行中の行は本文と同じ色にする。アクセントの色（テラコッタ）は失敗の赤と系統が近いので、状態の表示には使わない。
- フォントは SVG Tracer §8.5 と同じ（同梱しない）。

## 11. テスト

### 11.1 フィクスチャ

第三者のファイルを使わず、`crates/core/examples/gen_fixtures.rs` で作ってコミットする。

| ファイル | 内容 | 目的 |
| --- | --- | --- |
| `logo_alpha.png` / `.webp` | 透過背景の図形 | 透過の保持 |
| `photo.jpg` | 色の多い RGB の JPEG | JPEG のバイト列の保持 |
| `photo_cmyk.jpg` | CMYK の JPEG（`jpeg-encoder` で作る） | CMYK の JPEG |
| `rotate0.jpg`、`flip_h.jpg`、`rotate180.jpg`、`flip_v.jpg`、`rotate90_flip_h.jpg`、`rotate90.jpg`、`rotate270_flip_h.jpg`、`rotate270.jpg` | EXIF の向き 1〜8 の JPEG。名前は `image::metadata::Orientation` に合わせる。向きを反映するとすべて同じ絵になる | 向きの反映 |
| `deep16.png` | 16 ビットの PNG | 16 ビットの保持 |
| `opaque.bmp` / `alpha32.bmp` | 24 ビットと 32 ビットの BMP | BMP の読み込み |
| `dpi300.png` / `dpi300.jpg` | 解像度 300 dpi の情報付き | 「画像に合わせる」のページの大きさ |
| `shapes.pdf` | 図形と画像だけの 3 ページの PDF（krilla で作る。A4 縦、A4 横、Letter） | 描画の正しさ、ページの大きさ |
| `encrypted.pdf` | 閲覧のパスワード付きの PDF | `PasswordProtected` |
| `restricted.pdf` | 閲覧のパスワードなし、制限だけを掛けた暗号化の PDF | 開けること |
| `corrupt.pdf` / `corrupt.png` | 途中で切れたファイル | `PdfOpenFailed` / `DecodeFailed` |
| `shapes_150dpi_p1.png`〜`p3.png` | `shapes.pdf` の各ページを 150 dpi で描いたときの期待の画像（`gen_fixtures` が同じ図形から計算する） | §11.2 の PDF → 画像の比較 |

- 暗号化された PDF は、標準のセキュリティハンドラー（revision 3、128 ビットの RC4）を `gen_fixtures` に自前で書いて作る。`lopdf` の暗号化は乱数を使うので、生成し直すと同じファイルにならないため（T02 で決定）。
- `crates/core/tests/fixtures.rs` が、生成し直したバイト列とコミット済みのファイルを比べる。生成プログラムとフィクスチャがずれないようにするため。
- 文字を含む PDF はフォントのライセンスの扱いが要るので、フィクスチャには入れない。文字の描画の確認は手動の確認（§11.3）で行う。

### 11.2 品質テスト（NFR-03）

- **画像 → PDF**: 書いた PDF を pdfium で、元の画像の dpi で描画し直し、白の背景に重ねた元の画像と比べる。不一致の画素（RGB のいずれかの差が `COLOR_TOLERANCE_IMAGE`（8）を超える）の割合が `MAX_MISMATCH_RATIO_IMAGE`（0.1%）以下。T02 の実測では、CMYK を除く全フィクスチャで差の最大が 2、不一致は 0% だった。JPEG は、加えて元のバイト列が PDF の中にそのまま含まれることを確かめる。CMYK の JPEG は色の変換が表示側に依存するので、バイト列の確認だけにする。
- **PDF → 画像**: `shapes.pdf` を描画し、`gen_fixtures` が同じ図形から計算した期待の画像と比べる。図形の縁のぼかし方の差を許すため、`COLOR_TOLERANCE_RENDER`（32）、`MAX_MISMATCH_RATIO_RENDER`（0.5%）とする。T02 の実測では最も悪いページで 0.12% だった（pdfium は画像を x 方向に 1 画素ずらして描く、A4 の高さが整数の画素にならない、などの 1 画素の差が図形の縁に出る）。4 倍の余裕を残しつつ、描画の劣化を見逃しにくい値にしている。
- 出力した PDF が一般的なビューアーで開けること（NFR-03）は、手動の確認でブラウザー内蔵のビューアーで開いて確かめる。

### 11.3 テストの一覧

| 対象 | 種類 | 主な内容 |
| --- | --- | --- |
| crates/core | 単体 | probe（形式、寸法、向き、dpi、上限）、ページの配置（画像に合わせる、A4 の縦横、余白、上限）、範囲の解釈（書式、全角、正規化、誤り）、出力名（重複、大文字小文字、番号の桁数） |
| crates/core | 品質 | §11.2 の画像 → PDF |
| crates/worker | 品質 | §11.2 の PDF → 画像 |
| crates/worker | 単体 | メッセージの読み書き、`Open` / `Render` / `Thumbnail`、パスワード、壊れた PDF、ページ数の上限、描画の上限 |
| crates/worker | 結合 | 単独のワーカー（`pdfconv-worker`）を起動し、テスト用の要求（`test-hooks` 機能の `CrashForTest`、`HangForTest`、`AllocateForTest`）で、異常終了、時間切れ、メモリの上限のあとにメインプロセスの側が続けて動くこと、新しいワーカーが起動することを確かめる（T01） |
| src-tauri | 単体 | ワーカーの数の管理（T06）。変換: 成功・失敗・対象のページなしの混在、キャンセル後に一時ファイルが残らないこと。設定の読み書き |
| フロントエンド | 単体 | 一覧の追加・並べ替え・削除、範囲の入力と開始ボタンの有効・無効、変換中の無効化、エラーコードから文言、状態の色と記号 |
| 全体 | 手動 | 受け入れ基準（両 OS でインストーラーから）。文字を含む実際の PDF の描画、ブラウザーでの PDF の表示。`docs/manual-test.md` |

- 性能（NFR-02）: `shapes.pdf` の A4 のページを 150 dpi で描画する時間を `--release` のテストで測る。CI では 3 秒を超えたら失敗とし、1 秒以内は基準環境（開発者の PC）で手動確認する。
- CI は Windows と Ubuntu で、SVG Tracer と同じ検査に加え、`npm run pdfium:fetch` のあとにワーカーのテストを実行する。
