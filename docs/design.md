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
└── .github/workflows/        ci.yml / release.yml / audit.yml / dependency-review.yml / pdfium-update.yml
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
- 画像のデータの誤り（`corrupt.png` など）は、krilla では文書の `finish` まで分からず、どの画像の誤りかも分からない。そのため、ページに加える前に `image` で画素を一度読み、読めなければ `DecodeFailed` としてその画像を加えない。
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
- フォルダは直下だけを見る。サブフォルダ、シンボリックリンク、隠しファイル（`.` で始まる名前）、対応しない拡張子は追加しない。§7.1 の `skipped` には、サブフォルダを `folders`、対応しない拡張子を `unsupported`、一覧にすでにあるものを `duplicates` として数える。隠しファイルとシンボリックリンクは、利用者が意図して選んだものではないので数えない。ファイルダイアログや D&D で直接選ばれたファイルは、名前が `.` で始まっていても追加する。
- 追加、フォルダの選択、「PDF を保存」のダイアログは、コマンドを呼んだウィンドウを親にして開く（`commands.rs` の `set_parent`）。親がないと、アプリのウィンドウを押したときにダイアログがその裏に隠れたまま開いていて、もう一度押すと重ねて開けるため。
- D&D は Rust の `WindowEvent::DragDrop` で受ける。落とされたものを拡張子で分け、画像は「画像 → PDF」の一覧に、PDF は「PDF → 画像」の一覧に入れる。フォルダは中を見て同じように分ける。フロントエンドは、追加があった一覧のタブに切り替える（両方に入ったときは、表示中のタブのまま）。
- 同じファイル（正規化したパスが同じ）がすでに一覧にあれば、追加しない。
- 追加のときに、画像は §4.1 の `probe`、PDF はワーカーの `Open` を行う。失敗したファイルも一覧に入れ、行にエラーの理由を表示する。エラーの行は変換の対象にしない。パスワード付きの PDF も、ここで分かる。
- 画像の一覧のサムネイルは、Rust が画像を縮小して PNG で返す。PDF のサムネイルはワーカーの `Thumbnail` で作る。どちらも長い辺 `THUMBNAIL_SIDE`（160 px）。サムネイルは表示されたときに 1 つずつ要求する（多数の追加で待たされないようにするため）。
- 並べ替え（画像の一覧のみ。結合の順になる）: ドラッグ、各行の ↑↓ ボタン、行にフォーカスがあるときの Alt + ↑ / ↓。順序はフロントエンドが持ち、変換を始めるときに ID の並びとして渡す。
- 変換中は一覧を変えられない（追加・削除・並べ替えのボタンを無効にし、D&D は `ConversionRunning` で拒む）。

### 6.2 画像 → PDF（FR-02、FR-03、FR-05）

- 「1 つの PDF」: 「PDF を保存」で保存ダイアログを開く。初期ファイル名は一覧の先頭の画像の名前の拡張子を `.pdf` にしたもの。ダイアログで既存のファイルを選んだ場合は、OS のダイアログで上書きの確認が済んでいるので上書きする。Linux では、ダイアログをホームのフォルダで開く（指定しないと GTK がプロセスの作業フォルダで開き、AppImage では書き込めない読み取り専用のマウント先になるため）。Windows は OS が前回のフォルダを覚えているので指定しない。追加やフォルダを選ぶダイアログは「最近使ったもの」で開くので指定しない。
- 「1 枚ずつ」: 保存先のフォルダを選び、「変換を開始」で一覧の画像を 1 つずつ PDF にする。出力名は §6.4。並列の数は §5.2 と同じ式（ワーカーは使わず、スレッドで行う）。
- どちらも、進捗を `job-progress`、1 件ごとの結果を `job-item` で通知し、最後に `job-finished` を送る（§7.2）。

### 6.3 PDF → 画像（FR-04、FR-05）

- 一覧に PDF が 1 つのときは、ページのサムネイルを並べ、範囲の指定で変換するページを示す。複数のときは、一覧の表とする（モックアップ `PdfSingle` と `PdfBatch`）。
- 範囲を入力するたびに `check_page_range` を呼び、変換するページの合計を下部バーに表示する（§10.1）。書式の誤りがあれば「変換を開始」をすぐに無効にし、誤りの文言は入力が 0.6 秒止まるか欄から離れたときに入力欄の下に赤で表示する。打ちかけの「1-」のような途中の文字で注意を出さないため。範囲に当てはまるページが全 PDF の合計で 0（`totalPages` が 0）のときは、下部バーの文言を赤にし、「変換を開始」も無効にする（押しても何も変換しないため）。一部の PDF だけにページがない（合計が 1 以上）ときは、そのまま変換でき、その PDF の行は「対象のページなし」になる。範囲の確認中は、書式の誤りのときと同じく無効にする。
- 新しい範囲の文字列が最初の `check_page_range` の答えを待つ間は、サムネイルの強調、表のページ数、下部バーの文言を、その前の表示のまま保つ。答えのない一瞬にすべてのページが外れたように見えて、ちらつくため。
- 「すべて」のときは、範囲として `1-{対象の PDF の最大のページ数}` を渡す。`parse_page_range` は空の文字列を受け付けず、ページ数を超える番号は PDF ごとに飛ばされる（§4.5）ので、これで全 PDF の全ページになる。
- PDF が 1 つのときのサムネイルの格子では、変換するページを強調する。どのページかは、`check_page_range` が返す `intervals`（正規化した区間）にページ番号が入るかで決める。フロントエンドは範囲の文字列を解釈しない（§4.5）。
- その格子では、サムネイルのクリックでも変換するページを選べる。クリックしたページを入れるか外すかを、今の `intervals` に対して切り替え、その結果から範囲の文字列を作り直して入力欄に入れる（「すべて」のときは「範囲を指定」に切り替え、そのページだけを外す）。作った文字列も、手で入力したときと同じく `check_page_range` で確かめる。文字列を作るだけで解釈はしないので、規則は §4.5 の 1 か所のままになる。範囲の誤りが出ている間は、手で入力した文字を上書きしないように、クリックでは選べない。
- 「範囲を指定」に切り替えたとき、範囲の欄が空なら `1-{対象の PDF の最大のページ数}` を入れる。空の範囲はどのページも選ばないので、そのままでは切り替えた途端にすべてのページが外れ、要るページを 1 つずつ選び直すことになるため。前に入力した文字があれば、それを残す。
- 変換の結果（その PDF の `job-item`）があるときは、選んだページのキャプションを結果の語にする。「届いたページ」は、選んだページ（`intervals`）の先頭から「保存した数（`outputs.length`）＋失敗した数（`failedPages.length`）」までで、ページは先頭から順に処理するため。画面は範囲の文字列を解釈しない（§4.5）。キャンセルした PDF の `job-item` にも、それまでに失敗したページがあれば `failedPages` を入れる（届いたページの数が合うように）。PDF ごと失敗した（`status` が `failed` で `failedPages` がない。PDF を開けない、など）ときは、選んだページすべてが失敗したものとして数え、キャプションは選んだページすべてが「✕ 失敗」（赤い枠も同じ）になる。理由は見出しに出る。選んでいないページは、どの場面でも番号だけ。結果は §10.1 のとおり設定や範囲が変わったら消え、キャプションも元の「✓ 変換する」に戻る。

  | ページ | キャプション（ja / en） |
  | --- | --- |
  | 保存できた | `{page}  ✓ 完了`（緑）/ `{page}  ✓ Done` |
  | 失敗した（`failedPages`） | `{page}  ✕ 失敗`（赤）/ `{page}  ✕ Failed` |
  | 届かなかった（キャンセル、§6.5 の打ち切り） | `{page}  未処理`（灰）/ `{page}  Not processed`（行の状態の「未処理」と同じ語） |

- 変換に失敗したページは、枠を赤にし、サムネイルに薄い赤を重ねて、右上に ✕ の印を付ける。選んだページの枠（アクセントの色）は赤と系統が近く（§10.3）、枠の色だけでは見分けられないため。
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
- 一時ファイルは、保存先のフォルダに `.` で始まる名前で作る（ほかのアプリの一覧に途中のファイルが見えないようにするため）。
- Unix では、一時ファイルを `0666` で作る（OS が umask を引くので、通常は `0644` になる）。`tempfile` の既定は本人だけが読める `0600` で、名前を替えてもそのままなので、保存した PDF や画像を、ほかのアプリや別の利用者が読めなくなるため。設定ファイル（§6.7）は本人だけが読めればよいので、既定のままにする。Windows にはこの指定がない。
- 保存先のフォルダは、「画像 → PDF」と「PDF → 画像」で 1 つずつ Rust の状態に持つ。`pick_output_dir` と設定の復元（§6.7、T09）が入れ、`start_images_to_pdfs`・`start_pdfs_to_images` はそれを使う。まだ選ばれていなければ `InvalidParams` で拒む。
- 画面は、フォルダが未選択のまま「変換を開始」が押されたら、先に `pick_output_dir` でフォルダのダイアログを開き、選ばれたらそのまま変換を始める（選んだフォルダは設定パネルの欄にも出る）。ダイアログを取り消したら何も始めない。ボタンを無効にして先に設定パネルへ行かせるより、手順が 1 つ少ないため。
- `start_images_to_pdfs` と `start_pdfs_to_images` は、変換中かどうかの確かめのあと、変換を始める前に保存先のフォルダを確かめる。
  - 存在しない、またはフォルダでない: 何も始めず、§6.5 の Rust の状態と設定ファイルの `outputDir` を未選択（`null`）に戻して `OutputDirMissing` を返す。ほかの画面の保存先は変えない。
  - フォルダはあるが、そこに一時ファイル（上の `.` で始まる名前）を作れない: 何も始めず `OutputDirNotWritable` を返す。権限を直せば使えるので、保存先は未選択に戻さない。
  - `detail` は付けない（`io::Error` の文言にフォルダのパスが入り、フロントエンドにパスを渡さないため）。
- 画面は、`OutputDirMissing` が返ったら、保存先の欄を「未選択」にし、一覧の上の `ErrorDisplay` でこの文言を出す。フォルダのダイアログは自動では開かない（帯と同時に開くと、理由を読む前に選ばせることになるため）。次に「変換を開始」を押すと、フォルダが未選択のときと同じ流れ（`useEnsureOutputDir`）でダイアログが開く。`OutputDirNotWritable` は同じ `ErrorDisplay` で出して何も始めない。どちらも下部バーには繰り返さない。この帯には閉じるボタンを付けない。フォルダが使えない間は出しておき、保存先を選ぶ、設定や一覧を変える、変換を始めるのいずれかで消える（§10.1）。起動時の設定の復元（§6.7）が存在しないフォルダを未選択にするのは、これとは別に今のまま行う。
- キャンセルは共有フラグで行う。画像 → PDF は次のファイルの前に、PDF → 画像は次のページの前に確かめる。描画中のページは終わらせて保存する（pdfium の描画は途中で止められないため）。キャンセルした PDF は、保存済みのページ数を表示する。
- 変換中に出力の書き込みが `WriteFailed` で失敗したら、その変換を打ち切る。保存先は全項目で共通なので、続けても失敗し続けるだけのため。キャンセルと同じく、次の項目・次のページの前で止まる。
  - 画像 → PDF（1 枚ずつ）: 失敗した画像は「失敗」（`WriteFailed`）、残りの画像は「未処理」。
  - PDF → 画像: 失敗した PDF は、それまでに保存したページがあれば `partial`、なければ `failed`。`failedPages` は書き込みに失敗したページだけで、届かなかったページは入れない。残りの PDF は「未処理」。
  - `job-finished` は `cancelled: false` で、残りを `unprocessed` に数える。ほかのワーカーがすでに処理している PDF は、その PDF の最後まで進む（同じフォルダなら、そこで同じ失敗になる）。
  - 結合（1 つの PDF）は最後に 1 回書くだけなので、この扱いはない。
  - `WriteFailed` 以外の失敗は、その項目・ページだけが失敗して続ける（壊れた画像、描画の上限）。ワーカーの異常終了・時間切れは、§5.2 のとおりその PDF の残りのページを試さず、ほかの PDF は続ける。
- 結合（1 つの PDF）をキャンセルしたときは、PDF を書かない（`job-finished` は `cancelled: true`、`save_merged_pdf` は `savedName: null`）。画面は結果を消さず、帯と行の状態を残す（§10.1）。全部の画像が失敗して PDF を書かなかったときも同じ。
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
| OutputDirMissing | 変換を始める前に、保存先のフォルダがない、またはフォルダでない（§6.5） | 保存先のフォルダが見つかりません。フォルダを選び直してください / The output folder can't be found. Choose a folder again. |
| OutputDirNotWritable | 変換を始める前に、保存先のフォルダに一時ファイルを作れない（§6.5） | 保存先のフォルダに書き込めません / Can't write to the output folder |

- 「対象のページなし」はエラーではなく、`job-item` の状態 `noPages` で表す（FR-04）。
- ワーカーが同梱の pdfium を読み込めないとき（ワーカー内のコード `PdfiumUnavailable`）は、IPC では `WorkerCrashed` として返す。配布物が壊れている場合にしか起きず、利用者に見せる文言は同じでよいため。
- `TooLarge`、`TooManyPages` の `detail` には、Rust が上限の定数を 3 桁区切りの文字列にして入れる（上限の数値をフロントエンドに書き写さないため）。

### 6.7 設定の保存（FR-07、FR-10）

- 保存先は `app_config_dir()` 配下の `settings.json`。読み書きの方式、壊れた設定や未知の `schemaVersion` で既定値に戻すこと、フォルダの扱いは SVG Tracer §5.6 と同じ。
- 内容: `schemaVersion`、`language`、`imagesToPdf: { output: "merge" | "each", pageSize: "fit" | "a4", outputDir }`、`pdfToImages: { format: "png" | "jpeg", dpi, outputDir }`。
- 一覧の中身とページの範囲は保存しない（次回は空の一覧で始める）。
- 既定値: 「1 つの PDF」、「画像に合わせる」、PNG、150 dpi。言語は未設定（`null`。画面が `navigator.language` から決める）。フォルダは未選択（`null`）。
- `language` は `"ja"`、`"en"`、`null` のいずれか。`outputDir` はフォルダの絶対パスか `null`。
- `get_settings` はパスを返さない。返すのは `{ language, imagesToPdf: { output, pageSize, outputDir }, pdfToImages: { format, dpi, outputDir } }` で、`outputDir` は `pick_output_dir` と同じ `{ dirLabel }`（未選択なら `null`）。`dirLabel` はフォルダの名前（パスの最後の要素）。
- `save_settings` は、`get_settings` の形から `outputDir` を除いたものを受け取る。`dpi` が §4.4 の `DPI_CHOICES` にないなどの不正な値は `InvalidParams` で拒み、何も保存しない。書き込みに失敗したら `WriteFailed` を返す。
- `pick_output_dir` は、選ばれたフォルダを §6.5 の Rust の状態と設定に入れ、設定ファイルを書き直す。書き込みに失敗しても、選んだフォルダはそのセッションで使い、`{ dirLabel }` を返す（設定ファイルの問題で変換を始められなくならないようにするため）。
- 起動時、保存されたフォルダが存在すればそれを §6.5 の状態に入れ、存在しなければ未選択に戻す。
- 読み込んだ値は項目ごとに確かめ、不正な項目（未知の値、型の違い、欠けた項目、存在しないフォルダ）だけを既定値に戻して、ほかの項目は残す。SVG Tracer のプリセットとパラメータのような組になった項目はない。JSON として読めない場合と、`schemaVersion` が `1` でない場合は、すべてを既定値にする。

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
| `save_merged_pdf` | `ids, pageSize` | `{ savedName: string \| null } \| null`（保存ダイアログを開く）。`null` は保存ダイアログを取り消したときだけで、何も実行していない。変換を実行したときは必ずオブジェクトを返し、`savedName` は PDF を書いたときだけそのファイル名、キャンセルや 1 枚も成功しなかったときは `null`。 |
| `start_images_to_pdfs` | `ids, pageSize` | –（§6.2 の「1 枚ずつ」） |
| `check_page_range` | `text, ids` | `{ totalPages, intervals: [start, end][] }` または `InvalidPageRange` |
| `start_pdfs_to_images` | `ids, range, format, dpi` | – |
| `cancel_job` | – | – |
| `get_about` | – | `{ version, pdfiumReady, pdfiumError }`（`pdfiumReady` は、ワーカーが pdfium を読み込めたか。pdfium の版は返さず、「このアプリについて」の第三者ライセンスの一覧に出る） |

- `ImageItem`: `{ id, name, width, height, format, bytes, error }`。`width` と `height` は EXIF の向きを反映した表示上の寸法（サムネイルと同じ向き）。`PdfItem`: `{ id, name, pageCount, firstPageSizePt, bytes, error }`。`error` は失敗した項目だけに入る `{ code, detail }`。失敗した項目では、`width`・`height`・`pageCount` は 0、`format`・`firstPageSizePt` は `null`。
- どのコマンドもパスを引数に取らない。`dirLabel` は表示用で、送り返されない。
- 変換は同時に 1 つだけ。実行中の `save_merged_pdf`、`start_*`、`add_*`、`remove_items` は `ConversionRunning` で拒む。

### 7.2 イベント（Rust → フロントエンド）

| イベント | ペイロード |
| --- | --- |
| `items-dropped` | `{ images: ImageItem[], pdfs: PdfItem[], skipped, error: IpcError \| null }`（変換中に落とされたときは何も加えず、`error` を `ConversionRunning` にする） |
| `job-progress` | `{ done, total, current: string \| null }`（`total` は画像 → PDF は画像の数、PDF → 画像はページの合計） |
| `job-item` | `{ id, status: "ok" \| "failed" \| "partial" \| "noPages" \| "cancelled", outputs: string[], error?, failedPages? }` |
| `job-finished` | `{ succeeded, failed, noPages, unprocessed, cancelled }` |

- `failedPages` は、PDF → 画像で書き込みやページの描画に失敗したページの番号。`partial` と `failed` のほか、キャンセルした PDF の `cancelled` にも、それまでに失敗したページがあれば入る。PDF を開けないなど PDF ごとの失敗では入らない。

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

- **依存の入手経路**: 悪意のある版が公開されても、多くは数日のうちに見つかって取り下げられる。そこで、公開から 7 日たっていない版は入れない（Dependabot の `cooldown`、npm の `min-release-age`。Dependabot のセキュリティ更新は待たない）。Tauri の crate と `@tauri-apps/*` の PR が同時に出るよう、cargo と npm の日数はそろえる。npm の依存のインストール時スクリプトは実行しない（`ignore-scripts`）。Rust の `build.rs` と proc-macro はビルドのたびに実行され、止められないため、入口を絞って補う。crate は crates.io からだけ取る（`cargo-deny` の `sources`）。CI とインストーラーのビルドは `Cargo.lock` と `package-lock.json` のとおりに入れる（`--locked`、`npm ci`）。pdfium は §8.1 のとおり SHA-256 で照合する。
- **リリースの書き込みの権限**: `release.yml` で Release を書ける権限を持つのは、下書きを作る最後のジョブだけにする。検査とビルドのジョブは外部の action と依存のビルドのコード（`build.rs`、npm のパッケージ）を実行するので、読み取りの権限だけで動かし、`actions/checkout` にもトークンを残させない（`persist-credentials: false`）。最後のジョブは GitHub CLI だけを実行し、ビルドの成果物を受け取って下書きに添付する。下書きは、検査と両 OS のビルドがすべて通ってから作る。タグは main にあるコミットに限る（main は両 OS の CI を通った PR からしか変わらないため）。
- PDF はワーカーでだけ開く（§1、§5）。メインプロセスに pdfium を読み込まない。`crates/worker` 以外の crate が `pdfium-render` に依存していないことを、`cargo-deny` の `bans`（`wrappers` の指定）で確かめる。
- ワーカーには、開く PDF のパスと pdfium の場所だけを渡す。ワーカーはファイルを書かない。
- サムネイルと描画した画像は、`blob:` URL にして `<img>` で表示する。
- WebView2 の起動オプションの `--host-resolver-rules` は、SVG Tracer の `MAP * ~NOTFOUND` に `EXCLUDE localhost` を加える。`tauri dev` の画面は Vite（`devUrl` の `http://localhost:5173`）から読むので、`localhost` の名前解決まで失敗させると Windows で開発用の画面が開かない（`ERR_NAME_NOT_RESOLVED`）。製品版で `localhost` の名前解決が通っても、CSP が許す接続先は画面自身（`'self'`）と IPC だけなので、画面から `localhost` には接続できない。PAC や外部のサーバーへの名前解決は、これまでどおりすべて失敗する。

## 10. フロントエンド

### 10.1 画面構成

見た目と配置はモックアップに従う。要点:

- アプリ名は「PDF Converter」。ウィンドウの既定の大きさは 1200×800、最小は 960×640。
- 上部バー: アプリ名、タブ（画像 → PDF / PDF → 画像）、言語の切り替え、「このアプリについて」。
- 左の設定パネル: 画像 → PDF は「出力」「ページの大きさ」と、「1 枚ずつ」のときの保存先フォルダ。PDF → 画像は「ページ」「形式」「解像度」「保存先フォルダ」。
- 右の一覧: 空のときはドロップ領域。画像は行の一覧（番号、サムネイル、名前、寸法、↑↓、外す）。PDF は 1 つならページのサムネイルの格子、複数なら表。
- 下部バー: 状態の説明と、実行のボタン（「PDF を保存」または「変換を開始」）。変換中は進捗バーと「キャンセル」。
- 下部バーの左には、変換していない間、その画面で実行のボタンを押すと何が起きるかを 1 行で出す（「3 ページの PDF になります」「24 ページを PNG で保存します」など。数えるのはエラーのない項目だけ）。「範囲を指定」のページ数は `check_page_range` の `totalPages` を使い、画面で範囲を数えない（§4.5）。保存するファイル名の例は出さない（名前の規則は §6.4 の `naming.rs` だけに置くため）。変換中・変換のあと・エラーのときは、その表示が優先される。変換のあとは結果を色と記号で示す（すべて成功なら緑の「✓ すべて完了しました」、失敗が 1 件でもあれば赤の「✕ 失敗があります」、キャンセルや対象のページなしだけなら記号なし）。変換を始められなかったときなど、結果より後に起きたエラーは結果より優先する。
- 一覧の上（見出しより上）には、その画面の最後の変換の結果を `JobSummaryBanner` で出す（件数の 1 行）。両方の画面で同じ部品・同じ位置にする。0 件の分類は出さない（「成功 0 件」も出さない）。数え方は 4 つある（`formatJobSummary`）。先頭の「変換が終わりました：」「キャンセルしました：」は共通。
  - 件で数える（複数の PDF、画像 → PDF の「1 枚ずつ」、全部の画像が失敗して PDF を書かなかった「1 つの PDF」）: 「成功 3 件 · 失敗 2 件」。
  - ページで数える（PDF → 画像で、変換した PDF が 1 つのとき）: 「3 ページ保存しました」「2 ページ保存しました · 失敗 1 ページ」「18 ページ保存しました · 未処理 182 ページ」「失敗 3 ページ」。保存 = `outputs.length`、失敗 = `failedPages.length`、未処理 = 選んだページの数 − 保存 − 失敗（選んだページの数は `intervals` から数え、範囲の文字列は解釈しない）。PDF ごと失敗したとき（キャプションの説明を参照）は、選んだページすべてを失敗に数える（「失敗 200 ページ」）。キャンセルした PDF の失敗したページも失敗に数える。英語は 1 ページのとき単数形（`1 page saved`）。
  - 「1 つの PDF」で PDF を書いたとき: 「10 ページの PDF を保存しました」。失敗した画像があれば「 · 失敗 1 枚」を続ける。保存したファイル名は出さない。英語は `Saved a 10-page PDF`（1 ページは `Saved a 1-page PDF`）。
  - 「1 つの PDF」をキャンセルしたとき: 「キャンセルしました：PDF は保存していません」（英語は `Cancelled: the PDF was not saved`）。PDF のファイルがないので、処理した画像の件数は出さない（出すと保存したように読める）。保存ダイアログを取り消したときは何も実行していないので、帯は出さない。
- 結果の帯は、失敗があれば左の縁と「✕」を赤、すべて成功なら緑にする。理由は帯に書かず、一覧の各行（画像の表・行の一覧、PDF の表、単一 PDF の失敗したページ）に出す。
- WebView2 のブラウザーのショートカットのうち、検索（Ctrl+F、F3、Ctrl+G、Ctrl+Shift+G）、印刷（Ctrl+P、Ctrl+Shift+P）、再読み込み（Ctrl+R、Ctrl+Shift+R、F5、Ctrl+F5）、ダウンロードの一覧（Ctrl+J）、キャレット ブラウズ（F7）は、`window` の `keydown` で `preventDefault` して止める（`useBlockBrowserShortcuts`。止めるキーは `BLOCKED_BROWSER_SHORTCUTS`）。入力欄にフォーカスがあっても同じ。止めるのは、Windows のインストール版で押して何かが出たキーだけにする（Ctrl+S、Ctrl+U、F11、Alt+←/→、Ctrl+N/T/W/H/D は何も出ないので入れない）。拡大・縮小、コピー、貼り付け、全選択、元に戻すなどは止めない。macOS の Command も Ctrl と同じに扱う。WebView2 の `AreBrowserAcceleratorKeysEnabled` は Tauri 2.12.1 から変えられず、`unsafe` を足すことになるので使わない。
- 結果は、その画面の一覧か設定が変わったら消し、下部バーは予告の 1 行に戻す（件数・行の状態・失敗したページが、もうない項目や設定を指すことになるため）。変換中は消さない。

### 10.2 状態管理、多言語

SVG Tracer §8.3、§8.4 と同じ（`useReducer` と Context、`ja.ts` を基準にした型付きの辞書）。reducer は「画像 → PDF」「PDF → 画像」「変換の実行」「言語」に分ける。

「変換の実行」は両方の画面で共通にする（変換は同時に 1 つだけで、タブを切り替えても続くため）。

- 状態は `state/job.ts`。`job-progress`・`job-item`・`job-finished` は `useJobEvents` が `App` で 1 回だけ購読する。
- 画面は `useJobRunner` で変換を始め、キャンセルする。始める前に状態を「実行中」にするので、コマンドの戻りより先に届いたイベントも失われない。キャンセルは押した直後に「キャンセル中…」にする（§6.5）。
- 行の状態（待機、変換中…、結果）は `jobRowStatus` で得る。`job-progress` の `current` はファイル名だけで ID を持たないので、変換中の行は名前と渡した順から割り出す。同じ名前の 2 つが同時に変換中のときは、1 つだけが変換中に見える。
- 下部バーは `JobFooter`。変換中は進捗とキャンセル、終わったら結果（§10.1）を出す。結果の帯は `JobSummaryBanner` で、成否の分け方（`jobOutcome`）は下部バーと共有する。古い結果を消すのは `appReducer` で、画面の状態が実際に変わった操作のときだけ行う。各画面は自分のボタン（`ImagesToPdfAction`・`PdfToImagesAction`）だけを持つ。

一覧まわりで両方の画面が使うものも、共通にする。

- D&D（`items-dropped`）は `useItemsDropped` が `App` で 1 回だけ受け、両方の一覧に加えてタブを切り替える（§6.1）。拒まれたとき（`ConversionRunning`）は、`App` が一覧の上にエラーを出す。
- サムネイルは `useThumbnail(id, page?)`。要素が画面に入ってから要求し（§6.1）、`blob:` URL にして表示し（§9）、項目が変わるか消えたら解放する。
- 保存先フォルダの欄は `OutputDirField`。`pick_output_dir` を呼び、フォルダの名前だけを表示する。選んだ名前は各画面の状態に持ち、起動時は `get_settings` の `outputDir` から入れる。

設定（§6.7）の読み込みと保存:

- `App` は `get_settings` の応答を待ってから画面を出す。既定値で出してから保存した値に変わると、言語や切り替えボタンが一瞬ちらつくため。読めなかったときは既定値で始める。
- 言語の状態は、表示している言語と、利用者が選んだ言語（`null` なら OS の言語に従う）を別に持つ。保存するのは後者で、一度も選んでいなければ `null` のままにする。
- 変更は `useSettingsAutoSave` が、最後の変更から 1 秒たってから `save_settings` で 1 回にまとめて保存する。起動時の値と同じなら保存しない。保存の失敗は表示しない（FR-10 は Should で、変換の妨げにしない）。

### 10.3 テーマ

- 色は `styles/tokens.css` の CSS カスタムプロパティで定義する。SVG Tracer の tokens.css を元にし、アクセントの色をテラコッタ（ライト: `#b4492b`）に替える。ダークのアクセントは `#e07353`（面の背景 `#262522` との比 4.92）。ライトのアクセントの上の文字は白（比 5.35）だが、ダークでは白だと 3.11 で足りないので、ボタンの文字を `#1c1b18` にする（比 5.53）。どちらも `tokens.test.ts` が `tokens.css` の値で 4.5 以上を確かめる。
- 成功は緑、失敗は赤に「✓」「✕」の記号を添える。実行中の行は本文と同じ色にする。「キャンセル」と「対象のページなし」も、記号は付けず、実行中と同じ本文の色（太さ 500）にする。理由の灰色の補足と見分けるため（単一 PDF の見出しと PDF の表で同じ）。待機と未処理は灰色のまま。アクセントの色（テラコッタ）は状態の表示には使わない。
- PDF → 画像の見出し（`.pdf-view-header`）は 14px にそろえる。状態の文字には大きさの指定がなく、ブラウザーの既定の 16px になって、ファイル名（14px）や情報（12px）とずれるため。
- 失敗の赤は、アクセントのテラコッタと見分けられるよう、色相を青寄りにした深紅にする（ライト `#b3163a`、ダーク `#ff7a8a`）。明るさはアクセントと近い（比 1.3 程度）ので、色相と「✕」の記号で見分ける。文字としての比は、ライトで面の背景 `#ffffff` に 6.79、ダークで `#262522` に 6.13。`tokens.test.ts` が、失敗と成功の色を、アプリが使うすべての背景（`bg-app`・`bg-surface`・`bg-subtle`）に対して 4.5 以上か確かめる。
- エラーの見せ方は 2 つだけにする。利用者の操作（追加、外す、開始など）の失敗と、1 つだけの PDF が読み込めないときは `ErrorDisplay`（記号付きの赤い枠の帯。閉じるボタンを持つものは、記号・文・ボタンを上下の中央に揃える）。入力欄や下部バーの 1 行のエラーは `.error-text`（赤の文字に「✕」）。灰色の `.hint` でエラーを出さない。
- 追加のときに読み込めなかった項目は「✕ 読み込めません」とし、変換で失敗した項目の「✕ 失敗」と分ける。どちらも状態の語を赤、理由を灰色の補足の文で出す（行の一覧、画像の表、PDF の表で同じ）。
- 「1 つの PDF」をキャンセルしたときは、PDF が保存されないので、すでに処理した行も「キャンセル」とする。
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
| `mixed_sizes.pdf` | 3 ページの PDF。1 ページ目と 3 ページ目は A4、2 ページ目は 3000×3000 pt（300 dpi で描くと `MAX_RENDER_PIXELS` を超える） | 途中のページだけ失敗する PDF → 画像（`partial`） |
| `shapes_150dpi_p1.png`〜`p3.png` | `shapes.pdf` の各ページを 150 dpi で描いたときの期待の画像（`gen_fixtures` が同じ図形から計算する） | §11.2 の PDF → 画像の比較 |

- 暗号化された PDF は、標準のセキュリティハンドラー（revision 3、128 ビットの RC4）を `gen_fixtures` に自前で書いて作る。`lopdf` の暗号化は乱数を使うので、生成し直すと同じファイルにならないため（T02 で決定）。
- `crates/core/tests/fixtures.rs` が、生成し直したバイト列とコミット済みのファイルを比べる。生成プログラムとフィクスチャがずれないようにするため。
- 文字を含む PDF はフォントのライセンスの扱いが要るので、フィクスチャには入れない。文字の描画の確認は手動の確認（§11.3）で行う。

### 11.2 品質テスト（NFR-03）

- **画像 → PDF**: 「画像に合わせる」で書いた PDF を pdfium で、元の画像の dpi で描画し直し、白の背景に重ねた元の画像と比べる（画素が 1 対 1 に対応する）。不一致の画素（RGB のいずれかの差が `COLOR_TOLERANCE_IMAGE`（8）を超える）の割合が `MAX_MISMATCH_RATIO_IMAGE`（0.1%）以下。T02 の実測では、CMYK を除く全フィクスチャで差の最大が 2、不一致は 0% だった。JPEG は、加えて元のバイト列が PDF の中にそのまま含まれることを確かめる。CMYK の JPEG は色の変換が表示側に依存するので、バイト列の確認だけにする。
- **画像 → PDF の A4**: 画像が拡大・縮小され、画素の途中の位置に置かれるので、縁がぼけて画素ごとの比較は成り立たない（T04 の前の試行で、photo.jpg の不一致が 9.8%）。代わりに配置と向きを確かめる。
  - 白の画素を含まないフィクスチャ（向きの 8 つ、`dpi300.png`、`dpi300.jpg`、`opaque.bmp`）を A4 で書いて描画し、白以外の画素の範囲を求める。それが §4.2 の配置の矩形の外接矩形（開始は切り捨て、終了は切り上げ）と、各辺 1 画素以内で一致すること。試行では 11 個すべてで満たした。
  - 向きのフィクスチャは、4 つの色の領域の中心の色が元の画像と同じであること（差が `COLOR_TOLERANCE_RENDER` 以内）。
- 画像 → PDF の品質テストは、pdfium をテストのプロセスに読み込んで描画する。`pdfium-render` は `crates/core` の dev-dependencies で、アプリのメインプロセスには入らない（§9 の検査は dev-dependencies を除く）。
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

- 一括変換の中で特定の PDF だけでワーカーが落ちる状況は、`test-hooks` 機能のときだけ、ワーカーが名前 `CRASH_ON_OPEN_FILE_NAME`（`crash-on-open-for-test.pdf`）の PDF の `Open` で異常終了することで作る（T08）。機能を付けないビルドには含まれない。
- 性能（NFR-02）: `shapes.pdf` の A4 のページを 150 dpi で描画する時間を `--release` のテストで測る。CI では 3 秒を超えたら失敗とし、1 秒以内は基準環境（開発者の PC）で手動確認する。
- CI は Windows と Ubuntu で、SVG Tracer と同じ検査に加え、`npm run pdfium:fetch` のあとにワーカーのテストを実行する。
