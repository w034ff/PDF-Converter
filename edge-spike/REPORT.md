# エッジケースの洗い出し 報告

ブランチ `spike/edge-cases`（main の 97309b3 から）。直しは 1 つの問題につき 1 つのコミットで、試験も同じコミットに入れた。どの直しのコミットでも `cargo fmt --check`、`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` は通る。`cargo test -p pdfconv-core` も全部通る。`cargo test --workspace --all-features --locked` は 1 件だけ落ちる（下の「環境のせいで落ちる試験」）。依存と `Cargo.lock` は変えていない（`edge-spike/` の外では）。

## 見つけたもの

| # | 場所 | 入力 | 分類 | 再現と試験 | 直したか |
| --- | --- | --- | --- | --- | --- |
| 1 | `crates/core/src/tiff.rs` `read_page` | `ICCProfile` タグに、先頭 16 バイトが何でもよくて、その後に `RGB `（または `GRAY`）があるだけのでたらめな中身を持つ TIFF | 黙って間違った結果（メインプロセス） | `repro/make_tiff.py` で作る `repro/tiff_junk_icc.tif`・`tiff_short_icc.tif`。試験は `tiff::tests::icc_profile_fits_needs_a_profile_header_of_the_page_colour_space` | b3f854b |
| 2 | `crates/core/src/probe.rs` `find_png_phys_dpi` | `IDAT` より前に空のチャンクを大量に並べた、正しい PNG（12 MB に 100 万個） | 遅い（メインプロセス。止まらないわけではない） | `repro/make_png_many_chunks.py OUT 1000000`。試験は `probe::tests::png_phys_search_reads_small_chunks_from_the_buffer` | 046f403 |
| 3 | `crates/core/src/probe.rs` `probe` | 名前が `x.png` の名前付きパイプ（FIFO）をダイアログで選ぶか、ドロップする | 止まらない（メインプロセスのブロッキングスレッド） | `mkfifo x.png`。試験は `probe::tests::read_failed_on_a_named_pipe_without_blocking` | b53c197 |
| 4 | `crates/core/src/layout.rs` `calculate_layout`（`PageSize::Fit`）と `pdf_write.rs` `commit_page` | 極端に細長い画像。96 dpi の 70,000,000×1 画素（`MAX_IMAGE_PIXELS` 以内） | 誤ったエラー（`DecodeFailed`） | `repro/make_png_strip.py OUT 70000000 1` | していない（下に案） |
| 5 | 同上 | 縦横比がおよそ 4,800:1 を超える画像を「画像と同じ大きさ」で書き出す | 黙って間違った結果（規格の推奨より小さいページ） | `repro/make_png_strip.py OUT 1000000 1` → ページは 14400 × 0.0144 pt | していない（下に案） |
| 6 | `crates/core/src/naming.rs`（出力名の決定） | 名前の長い PDF（拡張子を除いた名前が UTF-8 で 247 バイト以上。日本語ならおよそ 83 文字以上） | 誤ったエラー（`WriteFailed`） | 読んで確かめた（実行はしていない）。`<名前>_p001.png` が 255 バイトを超える | していない（下に案） |

### 1. TIFF の壊れた ICC プロファイルが PDF に入る

- 前は、色の空間の 4 バイト（16〜19 バイト目）だけを確かめて、タグのバイト列をそのまま krilla に渡していた。krilla は中身を読まずに `ICCBased` の色として埋め込む。通常の PDF にも PDF/A にも入った。poppler 24.02 は `not an ICC profile, invalid signature` と警告して RGB にフォールバックするので、見た目は変わらない。PDF/A の検証ソフト（veraPDF など）は通さないはず（試してはいない）。
- 直し: ICC の見出しの `acsp` の署名（36〜39 バイト目）があり、宣言された長さ（0〜3 バイト目）が「見出しの 128 バイト + タグの数の 4 バイト」以上で、タグのバイト数以下であることも求める。宣言の長さより後ろの詰め物は許す。`docs/design.md` §4.1.1 に 1 文を足した。
- 直したあと、PDF/A の出力の `ICCBased` は出力インテントの sRGB の 1 つだけになった（前は 2 つ）。

### 2. PNG の pHYs を探すのが、小さいチャンクが多いと遅い

- `BufReader::seek` はバッファを捨てるので、チャンクを 1 つ飛ばすたびに、シーク 1 回と 8 KiB の読み直しが起きていた。12 MB・100 万チャンクのファイルで、`probe`（ファイルから）が 0.65 秒。`image` が画素まで読む時間（0.08 秒）の 8 倍で、ファイルの大きさに比例して延びる。一覧に加えるときに、メインプロセスで動く。
- 直し: バッファの中のバイトは `consume` で飛ばし、残りが 64 KiB を超えるときだけシークする。同じファイルで 0.072 秒。
- 選ばなかった直し: チャンクの数に上限を置くこと（上限の定数が 1 つ増えるが、線形の時間で済むなら要らない）。

### 3. 名前付きパイプで `probe` が止まる

- `File::open` は、書き込む側が来るまで戻らない。`add_images` の `run_blocking` のスレッドが戻らないので、追加は終わらない。`/dev/zero` へのシンボリックリンク（`.png` の名前）をドロップした場合も、通常のファイルでないのに読みに行っていた（先頭のバイトで形式の判定が落ちるので、こちらはすぐ終わる）。
- 直し: `std::fs::metadata` が通常のファイルと答えなければ `ReadFailed`（リンクはたどる）。フォルダの走査（`scan_folder`）は、もとから通常のファイルだけを拾っている。
- ワーカーは画像を自分で開く（`crates/worker/src/image.rs`）ので、そのままにした。ワーカーで同じことが起きても `WorkerTimeout` で止まり、いまは `probe` が一覧に入る前に止める。

### 4・5. 「画像と同じ大きさ」で極端に細長い画像

- `calculate_layout` は長い辺だけを `MAX_PAGE_SIDE_PT` に縮め、短い辺には下限がない。
  - 縦横比がおよそ 4,800:1 を超えると、短い辺が 3 pt を下回る。PDF の仕様（ISO 32000 の付録 C）が勧める最小の大きさは 3×3 単位で、Acrobat はそれより小さいページを扱えないとされる（確かめていない）。poppler と mupdf は 1 画素の高さで描いた。
  - 96 dpi で約 70,000,000×1 より細長いと、短い辺が約 0.00024 pt（1/4096）を下回り、krilla（tiny-skia の `Size::from_wh`）がページの大きさを拒む。`commit_page` はこれを `DecodeFailed` にする。その画像はワーカーの画素の確認を通ったあとなので、「画像が壊れている」という誤った表示になる。幅 50,000,000（0.000288 pt）は通り、70,000,000（0.000206 pt）は落ちた。
- 案（設計の決まりに触れるのでコードは変えていない）:
  1. 「画像と同じ大きさ」で、短い辺にも下限（たとえば 3 pt）を置く。縦横比を保つと長い辺が `MAX_PAGE_SIDE_PT` を超えるので、その場合は用紙と同じく、余白を足して画像を中央に置くのが素直。
  2. または、縦横比の上限を `probe` に置いて、超えたら専用のエラー（`TooLarge` の流用でもよい）にする。どちらにしても、krilla が拒んだ場合は `DecodeFailed` ではなく別のコードにした方がよい。

### 6. 長い名前の PDF からの出力名

- `resolve_pdf_to_image_names` は `元の名前_p{nnn}.{ext}` を作るが、名前の長さを見ない。ext4・APFS の名前の上限は 255 バイト（UTF-8）、NTFS は 255 文字（UTF-16）。日本語の名前は 1 文字 3 バイトなので、Linux・macOS では拡張子を除いておよそ 83 文字以上の PDF の名前で上限を超え、書き込みが `WriteFailed` になるはず（作りから読んだ。実行はしていない）。重なりの番号（` (1)`）が付くと、画像 → PDF でも起きる。
- 案: 出力名の規則（design §6.4）に「名前の部分を、OS の上限に収まるよう文字の境界で切り詰める」を足す。どこで切るか（元の名前の末尾を切る）と、切ったあとの重なりの扱いは設計で決めること。

## 環境のせいで落ちる試験

- `src-tauri/tests/jobs.rs` の `output_dir_that_cannot_be_listed_rejects_and_resets_is_running` は、この入れ物では落ちる。入れ物は root（uid 0）で動いていて、`0o300` のフォルダでも中身を一覧できてしまうため。直しとは関係がない（直しは `crates/core` の画像の読み込みだけ）。CI（root でない）では通るはず。

## 試したが何も見つからなかった所

ファザーは `edge-spike/fuzz/`（cargo-fuzz、libFuzzer、nightly、`-O`、4 コア）。`cargo fuzz run -O --fuzz-dir fuzz <target>` で動かす（`edge-spike/` で）。種のコーパスは `crates/core/tests/fixtures` の画像（リポジトリの中で作ったもの）で、コーパスそのものはコミットしない。

| 対象 | 何を確かめたか | 実行 | 結果 |
| --- | --- | --- | --- |
| `image_pipeline` | どんな入力でも、`probe_reader` → `verify_image` → `thumbnail_png` → `PdfWriter::add_checked_image` → `finish`（通常の PDF とページの大きさ「画像」、PDF/A と A4 の 2 通り）が panic しない。画素の確認を通ったら `finish` は失敗しない（design §4.3 の「失敗したら作りの誤り」） | 40 分、168,111 回、最大 RSS 792 MB。カバレッジは最後まで少しずつ増えていた | 何もなし |
| `jpeg_strip` | `strip_jpeg_metadata` が panic しない、出力が入力より大きくならない、2 回掛けても同じ | 15 分、38,678,187 回 | 何もなし |
| `page_range` | `parse_page_range` が panic しない。通ったら、ページ数 0・1・7・100 のそれぞれで `pages_within` の長さが `count_within` と同じ、昇順で重なりがない、1〜ページ数に収まる | 15 分、5,857,327 回 | 何もなし |
| `protocol` | `read_message` が `Request`・`Response` のどちらでも panic しない | 10 分、15,401,556 回、最大 RSS 228 MB | 何もなし |
| `naming` | `resolve_image_to_pdf_names` の出力が、大文字小文字を区別せずに重ならず、既にある名前と重ならない | 5 分、432,568 回 | 何もなし |

- `image_pipeline` は、ファザーのメモリの上限に収めるため、総ピクセル数が 400 万を超える入力を `probe` のあとで捨てている。`MAX_IMAGE_PIXELS`（8,000 万）に近い大きさの画像は、手で作った入力（下）でだけ試した。
- 手で作った入力:
  - 8944×8944（`MAX_IMAGE_PIXELS` ちょうど付近）、9000×8000 のグレーの PNG は、`add_checked_image` が 0.27 秒以内で通った。
  - 300,000×1、1×300,000、100,000×100 の PNG は通った（短い辺の問題は上の 4・5）。
- 上限がきちんと効いていたもの（読んで確かめた。ファザーでも panic・タイムアウト・メモリの超過は出ていない）:
  - `probe` は寸法をヘッダーだけから読み、`MAX_IMAGE_PIXELS` を画素より前に確かめる。TIFF はページごとに確かめ、ページ数は `MAX_TIFF_PAGES` で止まる。TIFF の展開は `decoding_buffer_size` で抑えている。
  - `read_message` は、ヘッダーを読む前に `MAX_HEADER_BYTES` を、本体を読む前に `MAX_BODY_BYTES` を確かめ、本体は `take` で読むので、偽りの `bodyLength` で一度に大きく確保しない。途中で切れたメッセージは `UnexpectedEof`。長さの 4 バイトの途中で切れたものは「正しく閉じた」（`Ok(None)`）と読むが、クライアントはどちらも `Crashed` にするので害はない。
  - `client.rs` の `request_stream` は、進捗のたびに期限を延ばす。多ページの TIFF の確認（`CheckImage`）は、ページごとに `timeout_for` の時間を持つので、全体の時間の上限はない（キャンセルは効く）。メインプロセスでの 2 回目の展開は、ワーカーが通したものだけ。
  - `settings.rs` は、JSON でない・版が違う・型が違う項目を既定に戻す。serde_json の入れ子の深さの上限（128）で、深い入れ子も既定になる。
  - `page_range.rs` は区間のまま持ち、`1-4000000000` も一瞬。
  - 切り出し（`crop.rs` の `render_region`）は、範囲が空・逆・ページの外なら `InvalidParams`、大きすぎれば `RenderTooLarge`。JSON に NaN は書けないので、NaN の範囲は来ない（来ても比較が偽になり `InvalidParams`）。
- 調べなかった・浅いもの（クレジットの都合）:
  - ワーカーの PDF の処理（pdfium・lopdf）に壊れた PDF を与えるファザー。ワーカーで動くので異常終了は想定内で、整理のしおりの輪・深さ（`organize_outline_ring.pdf`・`organize_outline_deep.pdf`）や注釈の参照の上限は、既存の試験が見ている。lopdf はメインプロセスでは krilla が書いた PDF しか読まない（`encrypt_pdf`）。
  - `items.rs` のフォルダの走査は入れ子をたどらない（直下だけ）ので、深い入れ子やリンクの輪は起きない。読めないエントリが 1 つあると `read_dir` の走査全体が誤りになる作り（`entry?`）は、Linux ではまず起きないので見送った。
  - `settings.json` の先頭の BOM（古いメモ帳で保存したもの）は serde_json が受け付けず、全部が既定に戻る。手で編集した場合だけなので、報告だけにする。

## 上限の値の見直しの案

- 値を変える必要は見つからなかった。足すなら次の 2 つ（どちらも設計の判断が要る）:
  - 「画像と同じ大きさ」のページの短い辺の下限（3 pt）。上の 4・5。
  - 出力名の長さの上限（OS の 255）。上の 6。

## edge-spike の中身

- `fuzz/`: ファザー（`[workspace]` を別に持ち、`../rust-toolchain.toml` で nightly）。`fuzz/tools/img2pdf.rs` は、1 つの画像についてメインプロセスの画像 → PDF の手順を動かし、手順ごとの時間を出す道具（`cargo build --release --manifest-path fuzz/Cargo.toml --bin img2pdf`）。
- `repro/`: 再現の入力とその作り方（すべてここで作ったもの）。
