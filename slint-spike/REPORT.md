# Slint 試作の報告（担当 A: 整理の画面と OS とのつなぎ）

- ブランチ: `spike/slint-organize`（main の 97309b3 から）。マージしない前提。
- Slint: **1.18.1**（crates.io の最新の安定版。2026-11-04 に crates.io の索引で確かめて選んだ。2026-10-16 の比較と同じ版で、その後の新しい版は出ていない）。`slint-build` も `=1.18.1` に固定。
- 確かめた環境: クラウドの Linux（画面なし）。`Xvfb` の上で、Slint のソフトウェアの描き方（`SLINT_BACKEND=winit-software`）で動かし、`Window::take_snapshot()` で画像にした。
- 毎回 `cargo build` と `cargo test`（`src/organize.rs` の単体テスト 10 本）を通した。画面の操作は `src/scenario.rs` が `Window::dispatch_event`（本物のマウスとキーと同じ入口）で流し、結果をログと画像で確かめた。

## 動かし方

```bash
cd slint-spike
./scripts/fetch-font.sh          # Plex Sans JP を fonts/ に（git で追跡しない）
cargo run --bin make_sample      # samples/pages-240.pdf（240 ページ）を作る
cargo run -- samples/pages-240.pdf
./scripts/screenshots.sh         # Xvfb で台本を流し、screenshots/ に画像を書く
```

- pdfium は、リポジトリの直下で `npm run pdfium:fetch` を実行したあとの `src-tauri/pdfium/linux/lib`（Windows は `src-tauri/pdfium/windows/bin`）を見る。環境変数 `PDFIUM_DIR` で変えられる。
- worker は別のバイナリにせず、この試作のバイナリを `--worker <pdfium のフォルダ>` で起こし直して `pdfconv_worker::server::run` を呼ぶ（`src/main.rs` の `main`）。レイアウトの検出は使わないので、onnxruntime とモデルの道は渡すだけで開かれない。
- フォント: npm の `@ibm/plex-sans-jp` には WOFF / WOFF2 しか入っていない。Slint の `import "….ttf"` は TTF / OTF を受けるので、`scripts/woff2ttf.py`（Python の標準ライブラリだけで WOFF 1.0 を TTF に戻す）で変換する。
- 試す PDF: `src/bin/make_sample.rs` が `pdfconv_core::PdfWriter` で作る（240 ページ、7 ページごとに横向き）。ライセンスはこのリポジトリと同じ。

## 1. 骨組み — 判定: そのままできる

- 作り: [`ui/app.slint`](ui/app.slint)（窓・上のタブ・言語の切り替え。172 行）、[`ui/tokens.slint`](ui/tokens.slint)（`src/styles/tokens.css` の色を写したもの。45 行）、[`translations/ja/LC_MESSAGES/slint-spike.po`](translations/ja/LC_MESSAGES/slint-spike.po)（日本語。`src/i18n/ja.ts` から 25 個ほど写した）。
- 色: `global T` に明るい・暗いの 2 組を置き、`Palette.color-scheme`（OS の設定）で切り替える。CSS の custom properties とほぼ同じ書き味。PDF ごとの色（`--organize-source-1〜6`）も配列で持てた。
- フォント: `import "../fonts/IBMPlexSansJP-Regular.ttf";` と `default-font-family: "IBM Plex Sans JP";` だけで、ソフトウェアの描き方でも日本語がきれいに出た（画像 01）。フォントはバイナリに埋め込まれる（1 書体 5.7 MB。3 書体で 17 MB ほど増える。今のアプリの woff2 より大きい。分割した woff2 を使う手はない）。
- 言語: `@tr("…")` と `slint_build::CompilerConfiguration::with_bundled_translations("translations")`、実行時に `slint::select_bundled_translation("ja" | "en")`。窓を作り直さずに、その場で全部の文言が切り替わった（画像 01 → 10）。複数形も `@tr("{n} page" | "{n} pages" % count)` で書け、日本語は `nplurals=1` で 1 つにできた。
  - 注意: 翻訳の文脈（`msgctxt`）は既定で**部品の名前**になる。同じ英語でも部品ごとに `.po` の項目が要る（`"{n} page"` が `Chip` と `OrganizeView` の 2 つ）。今回は手で `.po` を書いた。本番では `slint-tr-extractor` で `.pot` を抜き出す流れになる。
  - 今の `src/i18n/` のように「キー → 文言」ではなく「英語の原文 → 訳」になる。キーの型の検査（`Translations` 型）はなくなり、訳し漏れは英語のまま出る。
- 突き当たったこと:
  - 破線の枠がない（`border-style` がない）。ドロップの場所の破線は実線にした（画像 00）。
  - `ScrollView` の `viewport-*` は 1.18 で非推奨になり `content-*` に改名されていた（警告で分かる）。

![空の画面](screenshots/00-empty-ja.png)

## 2. 整理のサムネイルの格子 — 判定: 自作が要る（ただし手間は小さく、性能の心配は当たらなかった）

**結論を先に**: 心配したとおり、HTML ならブラウザーと数行の CSS がくれるもの（折り返す格子、見えている所だけを描く仕組み、ドラッグの当たり判定、差し込みの線、端での自動のスクロール）は Slint には部品がなく、**全部を自作した**。ただ、今の React の画面もこの部分はすでに自作（`organizeGrid.ts` が升の大きさを固定して見えている行だけを描き、`useOrganizeDrag.ts` がポインターのイベントで並べ替えている）なので、**自作の量は React とほぼ同じ**だった。むしろ、状態と幾何をすべて Rust に寄せたので、テストは書きやすくなった。手ざわりと速さは、Linux の画面なしで測った範囲では問題なかった。

### どう作ったか

| 部分 | ファイル | 行数 |
| --- | --- | --- |
| 状態と幾何（選択・並べ替え・取り消し・列の数・見えている範囲・差し込む位置・自動のスクロールの速さ）と単体テスト | [`src/organize.rs`](src/organize.rs) | 554（テスト 180） |
| 画面とのつなぎ（升の使い回し、ポインター、キー、サムネイルの受け取り） | [`src/main.rs`](src/main.rs) の `Ctx` | 約 450 |
| worker への問い合わせ（サムネイル・開く・保存） | [`src/thumbs.rs`](src/thumbs.rs) | 236 |
| 格子・升・札・帯・下部バーの見た目 | [`ui/organize.slint`](ui/organize.slint) | 約 500 |
| 画面なしで操作を流す台本 | [`src/scenario.rs`](src/scenario.rs) | 336 |

参考: 今の React の整理の画面は `src/features/organize/` で約 7,900 行（テスト込み）。この試作は分割・パスワード・通知などを含まないので、行数は比べられない。

#### 窓の幅で列の数を変える

- Slint には折り返す格子（CSS の `grid-template-columns: repeat(auto-fill, …)` や flex-wrap）がない。`GridLayout` は行と列を自分で書く静的な配置で、モデルから升を並べて折り返すことはできない。
- そこで升の位置（`x`, `y`）を Rust で計算して渡す（`organize::column_count`・`cell_origin`）。`ScrollView` の `visible-width` が変わると `changed visible-width => layout-changed()` が呼ばれ、Rust が列の数を数え直す。今の `organizeGrid.ts` と同じ式・同じ大きさ（升 124 px、間 16 px / 18 px）。
- 結果: 1,100 px で 6 列、720 px で 3 列（画像 01、09）。

#### 200 ページ以上でも重くしない（見えている所だけを描く）

- Slint の `ListView` は縦 1 列の行を仮想化するだけで、格子には使えない（升を行ごとにまとめて `ListView` に入れる手もあるが、列の数が変わるたびに行の作り直しになり、ドラッグの最中に行が消えるとポインターの捕捉も失う）。
- そこで自前で仮想化した。`ScrollView` の中身の高さだけを全ページ分にし、`for cell in cells` の `cells` には**見えている行と上下 2 行の分の升だけ**を入れる。升は使い回す（ページの番号 → 升の番号の表を持ち、画面から出た升を空きにして、入ってきたページに渡す）。データが変わった升だけ `set_row_data` するので、Slint が描き直すのもその升だけ。
- 測った値（台本のログ。デバッグビルド、ソフトウェアの描き方、4 コア）:
  - 240 ページで、作られた升は最大 60 個（最初の画面は 42 個）。ページが増えても升の数は変わらない。
  - 中身の高さ全体を 120 px ずつ 56 回動かしたときの Rust 側の計算: 合計 10 ms（1 回 0.2 ms 以下）。
  - 開いてから最初の画面のサムネイル 42 枚が揃うまで: 約 0.7〜0.8 秒（worker 2 つ）。60% の位置に飛んで揃うまで: 約 1.0 秒。
- 描き方の速さ（フレームの時間）は画面なしでは測れていない。Windows でスクロールの手ざわりを確かめてほしい（「オーナーが確かめること」）。

#### サムネイルを描けたものから差し替える（画面を止めない）

- `ThumbService`（`src/thumbs.rs`）が worker を 2 つ持ち、それぞれ別のスレッドで `Open` と `Thumbnail` を投げる。PNG はそのスレッドで解いて `SharedPixelBuffer` にし、`slint::Weak::upgrade_in_event_loop` で UI のスレッドに渡す。UI のスレッドは `Image::from_rgba8` で升に入れるだけ。
- 待ち行列は「新しい要求から先に」。画面に見えている (PDF, ページ) の組を毎回 worker 側に渡し、worker が取り出す時点で画面から消えていれば捨てる（今のアプリの `ORGANIZE_THUMBNAIL_DELAY_MS` で待ってから頼む仕組みと同じ狙い）。
- 一度取った絵は 1,000 枚まで覚える（`THUMB_CACHE_PAGES`。今のアプリの `ORGANIZE_THUMBNAIL_CACHE_PAGES` と同じ）。
- 回転: 今のアプリと同じく描き直さず、画面で回して見せる。ただし**ソフトウェアの描き方は `transform-rotation` を無視する**（`i-slint-renderer-software` の `fn rotate` が空で `// TODO (#6068)` とある。slint-ui/slint#6068）。GPU の描き方（femtovg / skia）では回るが、描き方によって見た目が変わるのは困るので、Rust で画素を 90° 単位に回した絵を作って覚えておく形にした（`Thumb::image`）。画像 03。

#### クリック、Shift / Ctrl のクリックで複数を選ぶ

- 格子全体を 1 つの `TouchArea` で受け、`pointer-event` の `modifiers.shift` / `modifiers.control` と、押した位置から Rust が升を当てる（`Ctx::hit_test`）。選択の規則は `Organizer::press` / `click_released`（Explorer と同じく、選んである升を修飾なしで押したときは、ドラッグに備えて離すまで選択を絞らない）。
- 台本: 2 番目をクリック → 4 番目を Shift+クリック → 9 番目を Ctrl+クリックで 4 ページが選ばれた（画像 02）。

#### ドラッグで並べ替える（差し込む線・複数・端での自動のスクロール）

- Slint 1.18 には `DragArea` / `DropArea` があるが、これは「データを運ぶ」ための仕組み（`data-transfer`）で、並べ替えの差し込む位置の計算や線は出してくれない。HTML でも並べ替えは自作するのが普通（今のアプリもポインターのイベントで自作）なので、同じ作りにした。
- 押した位置から 4 px 動いたらドラッグを始める。`organize::drop_target` が、ポインターの位置から「どの升の前に入れるか」と線の位置を返す。行の右端より右では、次の行の先頭ではなく、その行の末尾に線を出す（設計 organize §4 の区切りの線と同じ考え）。動かしているページは薄くし、ポインターの横に「4 ページ」の札を出す（画像 04）。
- 複数を選んだまま動かすと、選んだページが元の順のまま 1 かたまりで入る（`Organizer::move_pages`）。台本では p.2〜p.4 と p.9 を 13 番目の前に落とし、`[1, 5, 6, 7, 8, 10, 11, 12, 2, 3, 4, 9, 13, …]` になった（画像 05）。
- 端での自動のスクロール: ポインターが格子の上下 40 px に入ると、`slint::Timer`（16 ms ごと）で中身を動かし、端に近いほど速くする（`drag_scroll_step`。今のアプリの `ORGANIZE_DRAG_SCROLL_EDGE_PX` / `…_MAX_STEP_PX` と同じ値）。ポインターが止まっていても差し込む位置を数え直す。台本では下の端で 1.5 秒止めて 1,060 px ほど進んだ（画像 07）。
- `TouchArea` はボタンを押している間ポインターを捕まえるので、窓の外まで動かしても `moved` が届く。升を使い回して画面から消えても、捕まえているのは格子全体の `TouchArea` なので切れない（升ごとに `TouchArea` を置くと、自動のスクロールで升が消えたときにドラッグが切れる）。
- `ScrollView` はデスクトップではマウスのドラッグで中身を動かさない（fluent の `ScrollView` の `mouse-drag-pan-enabled: false`）ので、並べ替えのドラッグとぶつからない。

#### キーボード

- 格子を `FocusScope` で包み、`key-pressed` を Rust に渡す（`Ctx::grid_key`）。今のアプリの `useOrganizeGridKeys.ts` と同じ割り当て:
  - 矢印で移る（上下は 1 行）、Home / End、Shift+矢印で選択を広げる、Space で入れ外し、Esc で選択を外す、Ctrl+A ですべて。
  - Alt+矢印で選んだページを 1 つ（上下は 1 行）動かす。R / L で回す。Delete / Backspace で削除。Ctrl+Z / Ctrl+Y（Ctrl+Shift+Z）で取り消し・やり直し。
  - フォーカスのある升が画面の外に出たら、見える所まで動かす（`Ctx::reveal`）。
- 台本: Home → → → Shift+→ → Alt+→ で `[1, 5, 8, 6, 7, …]`、Ctrl+Z で元に戻った（画像 06）。フォーカスの升は濃い枠（格子にフォーカスがあるときだけ）。
- 注意: 升そのものはフォーカスを持たない（格子の `FocusScope` が 1 つだけ持ち、どの升が「今の升」かは Rust の状態）。これはスクリーンリーダーに効く（項目 4）。

### 突き当たった Slint の制約

- 折り返す格子・仮想化した格子の部品がない（`ListView` は縦 1 列、`GridLayout` は静的）。大きな画像の格子の例を求める issue が開いたまま（slint-ui/slint#12481 "Add large image grid example"）。
- ソフトウェアの描き方は `transform-rotation` を無視する（slint-ui/slint#6068。`i-slint-renderer-software-1.18.1/lib.rs` の `fn rotate`）。
- `DragArea` / `DropArea` は 1.18 で入ったばかりの「データを運ぶ」仕組みで、並べ替えには使えない（slint-ui/slint#11400 "Implement in-process drag-and-drop using a design that can be extended for cross-application support"）。
- 文字の幅で省略する（`overflow: elide`）は効くが、升の下の札の「ファイル名」は `max-width` で固定した。CSS の `min-width: 0` と flex の縮みのような「残りの幅に合わせて縮める」はレイアウトの stretch で書くことになる。

### 画像

| | |
| --- | --- |
| ![格子](screenshots/01-grid-ja.png) 01 最初の画面（240 ページ、6 列） | ![選択](screenshots/02-selection.png) 02 クリック・Shift・Ctrl で 4 ページ |
| ![回転](screenshots/03-rotated.png) 03 R で右に回した | ![ドラッグ](screenshots/04-dragging.png) 04 ドラッグの最中（差し込む線と札） |
| ![落とした後](screenshots/05-dropped.png) 05 13 番目の前に落とした | ![キー](screenshots/06-keyboard.png) 06 Alt+→ で動かした |
| ![自動のスクロール](screenshots/07-autoscroll.png) 07 下の端で自動のスクロール | ![飛んだ先](screenshots/08-scrolled.png) 08 60% の位置に飛んだ |
| ![狭い窓](screenshots/09-narrow.png) 09 720 px で 3 列 | ![英語](screenshots/10-english.png) 10 英語に切り替えた |
| ![暗い](screenshots/11-dark.png) 11 暗い色 | |

## 3. OS とのつなぎ

### ファイルのダイアログ — 判定: そのままできる

- 作り: [`src/main.rs`](src/main.rs) の `on_add_pdfs` / `on_save`（各 15 行ほど）。`rfd` 0.15.4 の `AsyncFileDialog` を `slint::spawn_local` の中で `await` する。`set_parent(&app.window().window_handle())`（slint の `raw-window-handle-06` 機能）で、ダイアログを窓の子にする。
- 開く: PDF の絞り込みで複数を選ぶ → 選んだ道を Rust がそのまま worker の `Open` に渡す。保存: 初期のファイル名を付けて保存先を選ぶ → 項目 5 の `Organize` で書き出す。
- 同期の `FileDialog` でも動くが、ダイアログの間イベントループが止まるので非同期の版にした。Windows の `rfd` は IFileDialog を別のスレッドで開く。
- Linux の `rfd` の既定は XDG デスクトップポータル（D-Bus）。このクラウドにはポータルがなく、ダイアログは出ずに `None` が返った。イベントループは止まらなかった（台本のログ「add-pdfs dialog: event loop still running」）。Linux で GTK のダイアログが要るなら `rfd` の `gtk3` 機能に替える（GTK の開発用ファイルが要る）。
- 今のアプリの決まり「道はダイアログと D&D からだけ Rust に入る」（design §1, §6.1）は、IPC がなくなるので自然に守られる。

### OS からのファイルの D&D — 判定: 工夫すればできる（2026-10-16 から変わらず）

- **最新の 1.18.1 でも `DropArea` は OS からのファイルを受けない。** 確かめた所:
  - `DragArea` / `DropArea` は 1.18 で入った（slint-ui/slint#11400 "Implement in-process drag-and-drop using a design that can be extended for cross-application support"）。名前のとおり**同じアプリの中**の D&D で、`DropArea` に届く `MouseEvent::DragMove` / `Drop` は、`DragArea` からのドラッグのときにだけ Slint の中で作られる（`i-slint-core-1.18.1/items/drag_n_drop.rs`）。
  - 外から入る口もない: 公開の `slint::platform::WindowEvent` にドラッグの種類はなく、内部の `InternalEvent` もマウス・キー・タッチだけ（`i-slint-core-1.18.1/platform.rs`）。winit のバックエンドは `WindowEvent::DroppedFile` / `HoveredFile` を扱っていない（`i-slint-backend-winit-1.18.1` に `DroppedFile` の文字列がない）。
- そこで前回と同じく、winit の窓のイベントを直接受ける（`slint::winit_030::WinitWindowAccessor::on_winit_window_event`、`unstable-winit-030` 機能）。`HoveredFile` で「PDF をドロップ」の覆いを出し、`HoveredFileCancelled` で消し、`DroppedFile(path)` で、ダイアログと同じ `add_paths` に渡す。[`src/main.rs`](src/main.rs) の 25 行ほど。覆いの見た目は画像 12。
- 工夫が要る点・危ない点:
  - `unstable-winit-030` は名前のとおり安定版の約束の外。Slint の版を上げると変わりうる。winit の版も Slint が決める（1.18.1 は winit 0.30.13）。
  - winit 0.30 の `DroppedFile` には**位置がない**（ファイルごとに道だけが届く）。「格子の、落とした位置に差し込む」はできない。今のアプリも整理では末尾に足す（organize §4）ので困らないが、位置で振り分ける画面を作るなら別の手（OS の API を直接）が要る。
  - フォルダを落とすと、フォルダの道が 1 つ届く。中の PDF を集めるのは自分で書く（今の `items.rs` と同じ）。試作では `.pdf` 以外を無視しただけ。
  - 開いたままの issue に slint-ui/slint#12437 "Crash when dragging file onto winit window." がある（題だけ読んだ。中身は確かめていない）。Windows で必ず試してほしい。
  - 画面なしの Xvfb には D&D の送り手がないので、`DroppedFile` の道筋はここでは動かしていない（`add_paths` から先は台本で通っている）。

![ドロップの覆い](screenshots/12-file-hover.png)

## 4. アクセシビリティ — 判定: 工夫すればできる

### Slint が出すもの

- Slint は AccessKit を通して OS に出す（`accessibility` 機能。既定で入っている）。Windows は UI Automation、Linux は AT-SPI、macOS は NSAccessibility。
- 要素ごとに書ける性質（1.18.1）: `accessible-role`、`-label`、`-description`、`-enabled`、`-checkable` / `-checked`、`-expandable` / `-expanded`、`-value`（`-minimum` / `-maximum` / `-step`）、`-read-only`、`-placeholder-text`、`-orientation`、`-live-region`、`-item-selectable` / `-item-selected` / `-item-index` / `-item-count`、`-delegate-focus`、`-id`、それに `accessible-action-default` などの動作。
- 役割は 30 ほど（`button`・`checkbox`・`list`・`list-item`・`tab`・`tab-list`・`tab-panel`・`table`・`tree`・`text-input`・`slider`・`radio-button`・ランドマーク（`main`・`navigation`・`region` など）ほか）。`grid` / `gridcell` はない。AccessKit への対応は `i-slint-backend-winit-1.18.1/accesskit.rs`（`list` → ListBox、`list-item` → ListBoxOption）。
- 標準の部品（`Button`・`LineEdit`・`ScrollView` など）は役割と名前を自分で付ける。この試作は見た目を合わせるために自作の部品（`FlatButton`・`TabButton`）にしたので、役割と名前は自分で書いた。

### 格子の項目に役割と名前を付けられるか — 付けられた

[`scripts/a11y-dump.sh`](scripts/a11y-dump.sh) で、Xvfb と AT-SPI のバスの上で窓を開き、pyatspi で木を読んだ（全文は [`a11y-tree.txt`](a11y-tree.txt)）。2〜4 ページ目を選んだあと → で 5 ページ目に移った状態:

```
page tab list '変換の種類'
  page tab '整理' [focusable, selected, selectable]
panel '言語'
  toggle button '日本語'
list item 'pages-240.pdf 240 ページ'
label '1 ページを選択中'
push button '左に回す'
list box 'ページ'
  list item 'pages-240.pdf 1 ページ' [focusable, selectable] {'posinset': '1'}
  …
  list item 'pages-240.pdf 5 ページ' [focused, focusable, selected, selectable] {'posinset': '5'}
focused: list item 'pages-240.pdf 5 ページ'
  list selection count: 1
```

- 格子は `list`（AT-SPI の list box）、升は `list-item`。名前は今のアプリの `cellAria`（「{name} {page} ページ」、回したら「、{degrees}°」）と同じ文言を `@tr` で付けた。
- 選択: `accessible-item-selectable` / `-item-selected` で、升に selectable / selected が付き、list box の Selection インターフェースで選んだ数（1）が読めた。
- 位置: `accessible-item-index` が posinset になった（1 から数えた値で出た）。
- **フォーカス**: 升は一つ一つフォーカスを持たない（キーは格子の `FocusScope` 1 つが受ける）。そのままではスクリーンリーダーには「ページ」の list box にフォーカスがあるとしか伝わらない。`accessible-delegate-focus: <番号>`（フォーカスのある要素の、何番目のアクセシブルな子をフォーカスとして報告するか）に、Rust で「今の升」の番号を渡すと、キーで動かした升（5 ページ目）が focused として出た。今のアプリの roving tabindex に当たるものを、こうして作れる。
- 仮想化との兼ね合い:
  - 木に出るのは作られている升（見えている所と上下 2 行）だけ。今のアプリも同じ（`aria-setsize` / `aria-posinset` で全体の数と位置を伝える）。
  - 使っていない升（使い回しの空き）が空の list item として出ないように、`if cell.index >= 0: PageCell { … }` で包んだ（`accessible-role` は定数でなければならず、式で `none` にできない）。
  - フォーカスのある升は画面の外でも作り続ける（organize §4 と同じ）。消すと、報告するフォーカスがなくなるため。
  - `accessible-delegate-focus` の番号は「アクセシブルな子の中での順番」で、升の使い回しの順（ページの順ではない）。Rust で数えて渡す必要がある（`Ctx::refresh` の `delegate`）。

### 直した所・足りない所

- 中の `Text` は、何もしないと label として木に出る（升の中の「p.5」「pages-240.pdf」、ボタンの中の文字が二重に読まれる）。`accessible-role: none;` を付けて消した。HTML ならボタンの中の文字はボタンの名前になるだけなので、ここは Slint の方が手間が多い。
- タブの「選ばれている」は `accessible-item-selected` だけでは出ず、`accessible-item-selectable: true` も要った。言語のボタンの「押されている」は `accessible-checkable: true` で toggle button になった。
- **全体の数（setsize）が升に出ていない**: `accessible-item-count: 240` を付けたが、AT-SPI の属性に `setsize` がなかった（最初の版では、升の中の label の方に `setsize:240` が出ていた）。AccessKit か Slint のつなぎの不具合かもしれない。Windows の UI Automation の SizeOfSet に出るかを、ナレーターか NVDA で確かめてほしい（「240 個中 5 個目」と読むか）。
- 下部バーの文言（保存の進み具合など）に `accessible-live-region: polite` を付けた。読み上げられるかは画面なしでは確かめられない。
- 大きな list の性能の issue が開いたまま（slint-ui/slint#3867 "`accessibility` feature have big performance impact on big listview"。題だけ読んだ）。この試作は升を 60 個ほどに抑えているので当たりにくい。
- 並べ替えの結果（「4 ページを 13 番目に移しました」のような知らせ）は出していない。今のアプリが出しているなら、live region の文言で作る。

## 5. 整理の保存 — 判定: そのままできる

- 作り: [`src/thumbs.rs`](src/thumbs.rs) の `organize`（約 60 行）と、[`src/main.rs`](src/main.rs) の `save_to` / `organize_request`（約 50 行）。
- 画面の並び（`Organizer::pages` の source・page・rotation）を `OrganizePage` にして、worker の `Organize` を `WorkerProcess::request_stream` で投げる。届いた `OrganizeChunk` を `.pdf.part` に書き、`Organized` が来たら名前を変える。進み具合（`OrganizeProgress`）は `upgrade_in_event_loop` で下部バーに出す。別のスレッドで動かすので、保存の間も画面は止まらない。
- `crates/worker` の `client.rs` / `protocol.rs` は**そのまま**使えた。Tauri に依存する所はなかった。
- 確かめたこと（台本のログ）: 並べ替え・回転のあとの 240 ページを保存 → 592,170 バイト、約 100 ms。保存した PDF を worker で開き直し、**ページごとの大きさ（回したページは幅と高さが入れ替わる）が画面の並びと全部一致した**（元の横向き 34 ページ＋回した 4 ページ＝横向き 38 ページ）。
- 試作に入れていないもの: 一時ファイルの置き場所と置き換えの規則（design §6.5。今は `src-tauri/src/organize_export.rs` にある）、キャンセル、分割、パスワード、見積もりの上限。これらは `organize_export.rs` を流用すれば足りる（下の「まとめ」の 2）。

## オーナーが確かめること（Windows）

### 前提

1. Rust（rustup と MSVC の Build Tools）。`slint-spike/` はリポジトリの下にあるので、ルートの `rust-toolchain.toml`（1.99.0）がそのまま効く。
2. Node 24 で、リポジトリの直下で `npm ci` → `npm run pdfium:fetch`（`src-tauri/pdfium/windows/bin/pdfium.dll` ができる。試作はここを見る。別の場所なら環境変数 `PDFIUM_DIR` にそのフォルダ）。
3. フォント: Git Bash で `slint-spike/scripts/fetch-font.sh`（`npm ci` 済みなら `node_modules/@ibm/plex-sans-jp` の WOFF を使う。Python 3 が要る）。PowerShell だけなら、`slint-spike` で次を 3 書体（Regular・Medium・SemiBold）分:
   `python -I scripts\woff2ttf.py ..\node_modules\@ibm\plex-sans-jp\fonts\complete\woff\hinted\IBMPlexSansJP-Regular.woff fonts\IBMPlexSansJP-Regular.ttf`
4. `cd slint-spike` → `cargo run --bin make_sample` → `cargo run --release -- samples\pages-240.pdf`。
   - 描き方は既定（Windows では femtovg、OpenGL）。比べるなら PowerShell で `$env:SLINT_BACKEND="winit-software"` を付けてもう一度。

### 手で確かめること

1. **スクロールの手ざわり**: 240 ページをホイール・スクロールバーのつまみで上下する。カクつかないか。サムネイルが描けたものから出て、その間も操作できるか。スクロールバーで大きく飛んだとき、止まった所のサムネイルが先に出るか。
2. **並べ替え**: クリック、Shift+クリック、Ctrl+クリックで選び、ドラッグで動かす。差し込む線が思った位置に出るか。行の右端の外では、その行の末尾に線が出るか。格子の上下の端で止めると自動でスクロールするか（速さは端に近いほど速い）。窓の外まで出しても切れないか。
3. **キー**: 矢印・Home・End・Shift+矢印・Space・Esc・Ctrl+A・Alt+矢印・R / L・Delete・Ctrl+Z / Ctrl+Y。Tab で格子に入れるか。
4. **回転の見た目**: R で回したサムネイルが回って見えるか（femtovg と software の両方で同じか）。
5. **OS からの D&D**: エクスプローラーから PDF を 1 つ・複数落とす。覆い（「PDF をドロップ」）が出て、離すと札が増えるか。落とさずに窓の外へ戻すと覆いが消えるか。**落としたときに落ちないか**（slint-ui/slint#12437）。
6. **ダイアログ**: 「PDF を追加」で複数を選べるか、ダイアログが窓の前に出て、その間に窓を触れないか。「PDF を保存」で保存した PDF を別のビューアーで開き、並びと回転が画面どおりか。
7. **スクリーンリーダー**（ナレーターか NVDA）: 格子に入ると「ページ リスト」と読むか。矢印で動くと「pages-240.pdf 5 ページ、選択」と読むか。**「240 個中 5 個目」のように全体の数を読むか**（Linux では setsize が出ていなかった。項目 4）。タブ（「整理、選択」）と言語のボタン（オン・オフ）を読むか。
8. **高い DPI**: 表示の拡大を 150%・200% にして、文字とサムネイルがぼけないか（サムネイルは描く大きさの 2 倍で取っている）。
9. **言語と色**: 右上で日本語 / English を切り替えて全部変わるか。Windows を暗い色にすると、起動の時点で暗い色になるか。
10. IME: この試作には文字の入力欄がない（2026-10-16 の比較で `LineEdit` の IME は確かめ済み）。

## まとめ

### 1. 画面ごとの移行の手間の見積もり

慣れた人が 1 人で作るとして、テストの書き直しを含めた日数のおおよそ。担当 B（切り出し・OCR）の結果で大きく変わるので、切り出しは幅を広く取った。

| 画面・部分 | 今の React（テストを除く行数） | 見積もり | 根拠 |
| --- | --- | --- | --- |
| 共通の土台（窓・タブ・色・フォント・文言の移し替え（`ja.ts` の約 460 項目を `.po` に）・ダイアログ（このアプリについて・パスワード・確認・キーの一覧）・通知の帯・進み具合・設定の保存のつなぎ） | components・state・i18n・job ほか 約 8,000 | 8〜12 日 | 文言は「キー → 文言」から「英語の原文 → 訳」に変わり、形の検査がなくなる。`ContextMenuArea`・`PopupWindow` はある |
| 整理 | 約 3,100 | 6〜10 日 | 格子・選択・並べ替え・キー・保存はこの試作でできた（Rust 約 1,200 行・`.slint` 約 500 行）。残りは札の状態（パスワード・読めない）、分割と区切りの線、右クリックのメニュー、通知、キャンセル |
| 画像 → PDF | 約 1,200 | 4〜6 日 | 一覧と並べ替えは整理の格子の仕組みを流用できる |
| PDF → 画像 | 約 1,900 | 3〜5 日 | 一覧と設定。ページの範囲の入力は `LineEdit` |
| 切り出し（OCR を含む） | 約 4,400 | 10〜25 日 | 担当 B の報告による。範囲の描画は `TouchArea` と矩形で作れる見込みだが、文字の選択と縦書きは試していない |
| Rust 側のつなぎ（`commands.rs`・`lib.rs` を Slint の callback に置き換え、`ts-rs` の derive を外す） | Rust 約 1,100 | 3〜5 日 | 下の 2 |
| テスト | React のテスト 約 30,000 行 | 上の各行に含めたが、同じ網羅には届かない | 状態を Rust に寄せれば単体テストにできる。画面の結合テストは `Window::dispatch_event` の台本（この試作の `scenario.rs`）か Slint の testing バックエンドになる |
| **合計** | | **およそ 35〜65 日** | |

### 2. `src-tauri/src/` のうち Tauri なしでそのまま使えそうなもの

`use tauri` があるのは `commands.rs`（776 行）・`lib.rs`（204 行）・`layout_files.rs`（86 行）・`pdfium.rs`（18 行）の 4 つだけだった。ほかは Tauri の型を使っていない。

| ファイル | 行数 | 移すときにすること |
| --- | --- | --- |
| `worker_pool.rs`・`decode_budget.rs` | 683・405 | そのまま |
| `jobs/`（`mod.rs`・`queue.rs`・`output.rs`・`options.rs`・`images_to_pdfs.rs`・`pdfs_to_images.rs`・`merged.rs`・`image_check.rs`） | 約 1,860 | 進み具合は閉包（`JobCallbacks`・`FinishJob`）で渡す作りなので、閉包の中を `upgrade_in_event_loop` に替えるだけ。`ts-rs` の derive を外す |
| `items.rs`・`organize.rs`・`organize_export.rs`・`crop.rs`・`crop_export.rs`・`settings.rs`・`error.rs` | 994・858・591・903・479・713・186 | `ts-rs` の derive と、IPC 用の `serde` の名前付け（`camelCase`）は不要になる。`AppState`（`lib.rs` にある素の構造体）を別のファイルに移す。`IpcError` は画面の文言の選び分けに使い続けられる |
| `layout_files.rs`・`pdfium.rs` | 86・18 | Tauri の resource_dir の代わりに、実行ファイルの隣を見るように書き直す（数行） |
| `commands.rs`・`lib.rs` | 980 | 消える。代わりに Slint の callback をつなぐ層を書く |

合わせて約 9,600 行（テストを含む）が、ほぼそのまま残せる。セキュリティの決まり（道はダイアログと D&D からだけ、PDF は worker だけが開く）は、IPC がなくなっても同じ形で守れる。

### 3. 移行するかの見立て

- **項目 2 の心配について**: 当たった。折り返す格子・見えている所だけを描く仕組み・並べ替えの当たり判定と差し込む線・端での自動のスクロールは、Slint では部品がなく自作になった。ただし今の React も同じ部分を自作している（`organizeGrid.ts`・`useOrganizeDrag.ts`）ので、**HTML からの上積みの手間は小さい**。性能（240 ページで升 60 個、スクロール 1 回の計算 0.2 ms 以下）と、worker とのつなぎ（`crates/worker` をそのまま、画面を止めずに）は問題がなかった。状態を Rust に寄せた分、テストは書きやすい。
- 逆に、HTML なら何もしなくてよい所で手間が出た: 破線の枠がない、ソフトウェアの描き方で回転が効かない（#6068）、OS からの D&D は `unstable-` の winit の API 頼み、スクリーンリーダー向けに中の文字を隠す・フォーカスを委ねる番号を自分で数える、setsize が出ない疑い。どれも回避できたが、版を上げるたびに確かめ直す所が増える。
- 見立て: **整理の画面は、移行を止める理由にはならない**。移行するかは、(1) 担当 B の切り出し・OCR（文字の選択と縦書き）の結果と、(2) 上の 35〜65 日と React のテスト約 3 万行を捨てる重さを、起動の速さとメモリ（2026-10-16 に測り済み）に見合うと見るか、で決まる。整理だけを見た私の判断は「**今すぐの移行は勧めない。B の結果で文字の選択・縦書きが『工夫すればできる』以上なら、移行は現実的**」。決めるなら、Windows の手ざわり（上の 1〜2）とスクリーンリーダー（7）を先に確かめてほしい。ここが悪いと、自作の量がさらに増える。

### Slint のライセンス（Royalty-free 2.0）

`slint-1.18.1/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md` の本文を全部読んだ。Slint の crate は `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` の 3 つから選ぶ。このアプリ（公開のリポジトリ、MIT、無料で配るデスクトップのアプリ）に効く条件:

- **使ってよい範囲**: デスクトップ・モバイル・Web の「アプリの一部として」なら、無料で、使う・変える・配ることができる（§1）。料金や売り上げの条件はない。公開のリポジトリか、無料かは条件に入っていない。
- **表示の義務（§2）**: 次のどちらか。
  - (a) アプリの「このアプリについて」（一番上のメニューから開けるもの）に `AboutSlint` の部品を出す。なければ起動の画面に出す。今のアプリには「このアプリについて」があるので、ここに置ける。
  - (b) 公開の Web ページ（できれば配布のページ。GitHub のリリースや README）に「Made with Slint」のバッジを、見つけやすく出す。
- **してはいけないこと（§3）**: Slint をアプリに組み込まずに単体で配ること（それには GPL-3.0 を使う）。組み込み機器での利用。Slint の API を外に出すアプリ（プラグインの仕組みなどで Slint の API を公開する）。ソースの中のライセンスや著作権の表示を消す・変えること。このアプリはどれにも当たらない。
- **保証と責任（§4）**: 保証なし、責任なし（法律で求められる場合を除く）。
- このリポジトリの手続きに効くこと:
  - アプリのコード自体は MIT のまま。ただし配るバイナリには MIT でない Slint が入る。Slint は「オープンソースでないライセンス」を選んで使うことになる（GPL を選ぶとアプリ全体が GPL になる）。
  - `deny.toml` の `allow` に `LicenseRef-Slint-Royalty-free-2.0` を足す（3 つの OR のうちこれを選ぶ）。`about.toml` の `accepted` にも足し、本文を同梱のライセンスの一覧に入れる。
  - Slint を入れると、ほかに `BSL-1.0`（`clipboard-win`・`error-code`。Windows の依存）が増え、今の `allow` にない。足すかを決める必要がある（`cargo metadata` で Windows の依存を数えた結果。ほかは MIT / Apache-2.0 / BSD / Zlib / ISC / Unicode-3.0 で、今の一覧に入っている）。
  - フォークした人も、同じ条件で SixtyFPS から直接ライセンスを受ける（このリポジトリが再許諾するわけではない）。README にその旨を書いておくとよい。

## 途中で分かった、このクラウドでの事情

- `git push` が 403 で断られ、`spike/slint-organize` を GitHub に送れなかった（読み取りはできる）。コミットはこの環境の `spike/slint-organize` にある。
- Xvfb の上で動かすには `libxkbcommon-x11-0` が要った（`apt-get install` した）。AT-SPI の木を読むのに `at-spi2-core`・`python3-pyatspi`・`dbus-x11`・`xdotool` を入れた。
