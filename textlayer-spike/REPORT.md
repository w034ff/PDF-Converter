# OCR の結果を PDF の透明な文字の層にする試作

捨てる前提の試作（ブランチ `spike/ocr-textlayer`、マージしない）。コードは `textlayer-spike/` だけにある。

## 項目 1: 骨組みと OCR の結果

### できたこと

- `spike/ocr-vertical` を作業木（`git worktree add ../ocr-vertical origin/spike/ocr-vertical`）に取り出し、`ocr-spike` を release でビルドした。モデルは `ocr-spike/scripts/fetch.sh`（Hugging Face）で取れた。onnxruntime はアプリの `npm run layout:fetch` の 1.31.1 を `ORT_DYLIB` で渡した（`ocr-spike` は `api-26` で作ってあるが、1.31.1 で動いた）。
- 入力のページ 9 枚（`data/pages/`、git に入れない）。生成は [`src/pages.rs`](src/pages.rs)（`cargo run --release -- gen <ocr-spike のディレクトリ>`）。字の組み方は `ocr-spike/src/vertical.rs` を [`src/typeset.rs`](src/typeset.rs) に写したもの。

| ページ | 種類 | 中身 | 行 | 字 |
| --- | --- | --- | --- | --- |
| `h1_horizontal` | (b) 横書き | 見出し（IBM Plex Sans JP）、本文（Noto Serif JP、ベタ組み）、英数字の行、ノンブル | 18 | 439 |
| `ja_ref` | (b) 横書き | `ocr-spike` の `data/ref/ja.jpg`（写真。A4 に引き伸ばして扱う） | 56 | 235 |
| `t1_vertical` | (a) 縦書き | `gen-sets` の `val/t1_merosu_mincho`（句読点、「」、長音、小さいかな） | 19 | 906 |
| `v2_two_tiers` | (d) 混在 | `gen-sets` の `dev/v2_two_tiers`（横書きの柱、縦書き 2 段、ノンブル） | 14 | 242 |
| `v4_dialogue` | (d) 混在 | `gen-sets` の `dev/v4_dialogue`（横書きの柱、会話の括弧） | 7 | 239 |
| `m1_mixed` | (d) 混在 | 横書きの見出しと段落、その下に縦書き | 8 | 239 |
| `h1_horizontal_rot2` | (c) 傾き | `h1_horizontal` を 2° 回した | 18 | 439 |
| `t1_vertical_rot2` | (c) 傾き | `t1_vertical` を 2° 回した | 19 | 906 |
| `m1_mixed_rot2` | (c) 傾き | `m1_mixed` を −2° 回した | 8 | 239 |

- OCR の結果（`fixtures/*.json`）は `ocr-spike page-json <画像> --vcut --vcut-guard 1,3,0.1` で作った。画像は A4（595 × 842 pt）として扱われる。
- 確かめの道具: `pdftotext`（poppler-utils、もとから入っていた）、pdfium chromium/8125（`npm run pdfium:fetch`）、`mutool` と `qpdf`（apt）、veraPDF 1.28.2（[`scripts/verapdf.sh`](scripts/verapdf.sh)）。

### 突き当たったこと

- **2° 傾けた縦書きのページは、OCR の試作が読めなかった。** `t1_vertical_rot2` は 19 列のうち 17 列を横書きと取り違え、句読点の並び（「。。、。」など）になった（906 字 → 110 字）。`m1_mixed_rot2` も 7 列のうち 6 列が横書き扱いで、小さい「っ」と句読点が落ちた。傾いた検出の 4 点の頂点の順（左下から始まる）で縦か横かを決めているらしい。文字の層の問題ではないので、この 2 枚は傾けていない結果を同じ角度で幾何的に回した結果（[`scripts/rotate_fixture.py`](scripts/rotate_fixture.py)）で代え、OCR の生の結果は `fixtures/ocr_misread/` に残した。回し方の向きは、OCR が正しく読めた `h1_horizontal_rot2` で、回した結果と実際の OCR の 4 点が 0.5 pt 以内で合うことで確かめた。**OCR の側への宿題**: 傾いた縦の列の向きの判定。
- veraPDF は GitHub の Releases（`veraPDF/veraPDF-apps` はこのセッションの GitHub のアクセスの外）も `software.verapdf.org`（プロキシが 403）も取れなかった。Maven Central の `org.verapdf.apps:greenfield-apps:1.28.2` を `mvn dependency:copy-dependencies` で取り、`GreenfieldCliWrapper` を直接動かした。
- IBM Plex Sans JP は npm の `@ibm/plex-sans-jp@3` の最新を OTF に戻したので、`ocr-spike/sets/README.md` の SHA-256 と違う（版の違い）。ページの絵の見た目にしか効かない。
- 埋め込む本物のフォントは Noto Sans JP（SIL OFL 1.1、`notofonts/noto-cjk` の `Sans/SubsetOTF/JP/NotoSansJP-Regular.otf`、4.5 MB、CFF の輪郭、SHA-256 `dff723ba…5073`）。取り方は [`scripts/fetch.sh`](scripts/fetch.sh)。

## 項目 2: 確かめの道具

### どう作ったか

[`src/check.rs`](src/check.rs)。書いた PDF の各ページを、そのページの `OcrPage` と比べる。以後の表の列は次のとおり。

| 列 | 数え方 |
| --- | --- |
| 字 | OCR の字の数（空白を除く） |
| 全文 pdfium | pdfium のページ全文（`FPDFText_GetText`）と OCR の字を、空白・改行を除いて読み順のまま編集距離で並べ、`1 − 編集距離 / 字` |
| 全文 pdftotext | 同じことを `pdftotext`（poppler、既定の読み順の組み）で |
| 箱の中心が 4 点の中 | OCR の字と同じ字に並んだ pdfium の字（`FPDFText_GetLooseCharBox`）の箱の中心が、OCR の字の 4 点の中にある割合 |
| IoU | その箱と、OCR の字の 4 点の外接の箱の重なり（平均） |
| 検索 | pdfium の `FPDFText_FindStart`（大文字小文字を区別）で、各行の文字列と、各行を 4 字ずつに切った断片（2 字以上）が見つかる割合。括弧の中は問い合わせの数 |
| 記号を含む検索 | そのうち句読点・括弧・長音・小さいかなを含む断片 |
| 縦書きの検索 | そのうち縦書きの行のもの |
| 余計な空白 | pdfium の全文の空白の数 − OCR の語の間の空白の数。pdfium は字の間が広いと空白を足す（`is_generated` の字）。コピーした文字に入る |

- 箱は「緩い箱」（loose）を使う。選択の青い帯はこの箱で描かれる。「きつい箱」（`FPDFText_GetCharBox`）は字形の輪郭から決まるので、字形のないフォントでは大きさがない。
- pdftotext の数字は、縦書きでは低く出る（既定の組みは横の行に並べ直すので、縦の列が 1 字ずつの行になる）。`pdftotext -raw` なら内容の順になるが、アプリが使うのは pdfium なので、pdftotext は「ほかのビューアーで読み順が崩れるか」の目安としてだけ見る。
- `cargo run --release -- dump <PDF>` で、pdfium と pdftotext の全文と、最初の 40 字の箱を出す。

## 項目 3: krilla で書く（画像 → PDF の経路）

### できたこと

[`src/krilla_write.rs`](src/krilla_write.rs)（`cargo run --release -- krilla --font real|glyphless --fit line|tj|char --invisible transparent|under --vmode upright|rot90`、全部の組み合わせは [`scripts/krilla_tables.sh`](scripts/krilla_tables.sh)）。ページの画像を描き、その上に OCR の字を `Surface::draw_glyphs` で書く。字の箱は `KrillaGlyph` の `glyph_id`・`text_range`（ToUnicode のもと）・`x_advance` で決める。

**9 ページすべてで、pdfium の全文 100 %、検索 100 %、字の箱の中心が OCR の 4 点に入る割合 100 %（字ごとに置いた場合）**。縦書き・傾き・混在のページも同じ。

| 書き方（krilla） | 横書き h1 の箱の中心 | ja_ref の箱の中心 / IoU | 縦書き t1 の全文 pdfium | h1 の余計な空白 | 9 ページの大きさ |
| --- | ---: | ---: | ---: | ---: | ---: |
| 本物のフォント、字ごと、透明、縦は字を立てて | 100 % | 100 % / 0.96 | 100 % | 0 | 2,095,052 B |
| 字形のないフォント（四角の字形）、同上 | 100 % | 100 % / 0.95 | 100 % | 0 | 1,890,307 B |
| 本物、行ごと（変換で伸ばす＝`Tz` 相当） | 94.5 % | 98.7 % / 0.81 | 100 % | 0 | 2,076,767 B |
| 本物、行ごと＋字の送り（`TJ`） | 99.8 % | 98.3 % / 0.73 | 100 % | 17 | 2,079,515 B |
| 本物、字ごと、画像の下に黒で | 100 % | 100 % / 0.96 | 100 % | 0 | 2,094,132 B |
| 本物、行ごと、縦の列を 90° 回す | 94.5 % | 98.7 % / 0.81 | **11.0 %** | 0 | 2,048,009 B |

全部の表は `scripts/krilla_tables.sh` が `out/krilla_tables.md` に出す（項目 5 で位置の合わせ方として改めて比べる）。

### krilla 0.8.2 でできないこと（ソースとイシュー）

- **描き方 3（`3 Tr`）は公開の API にない。** `content.rs` の `fill_stroke_glyph_group` は `TextRenderingMode::{Fill, Stroke, FillStroke}` しか書かない。代わりに不透明度 0 の塗り（`Fill { opacity: NormalizedF32::ZERO }`、`/ca 0` の ExtGState と `0 Tr` になる）か、文字を先に描いて画像で覆う。pdfium の取り出しはどちらでも同じ。イシューでは、`Tr 3` の補助を足す PR [LaurenzV/krilla#382](https://github.com/LaurenzV/krilla/pull/382)（2026-05、マージされず閉じた）に、作者が「塗りの不透明度を 0 にすればよいのでは」と答えている。つまり krilla としては足す気がない。
  - 透明な塗りの心配: `/ca` を無視する古いビューアーや印刷経路では文字が黒く出る（字形のないフォントなら四角が出る）。画像の下に置けば見えないが、画像に透明な部分があると透ける。lopdf の経路の `3 Tr` にはこの心配がない。
- **縦書き（`Identity-V`、`WMode 1`）は書けない。** `text/cid.rs` は CMap に `WMode::Horizontal` を固定で書き、エンコーディングは `Identity-H` だけ。`Glyph::y_advance` は輪郭として描く経路（`content.rs` の 1340 行あたり）でしか使われない。PR [LaurenzV/krilla#45](https://github.com/LaurenzV/krilla/pull/45)「Add support for vertical writing」（2024）と [#447](https://github.com/LaurenzV/krilla/pull/447)（2026-10）はこの y の送りの話で、`WMode` を検索しても何も出なかった。PDF/A の説明（`configure/PDF_A4.md`）にも「krilla does not use fonts in vertical writing mode」とある。
  - 代わりに、縦の列の字を 1 字ずつ立てて（横書きのフォントのまま）字の 4 点に置いた。pdfium はこれを列ごとの 1 行にまとめて正しい順で取り出し、検索も選択も通った（`t1_vertical` で全文 100 %、箱 100 %）。
  - 列を 90° 回した 1 行（字が横に寝る）にすると、pdfium は列を**左から右へ**並べて読み順が逆になった（全文 11 %）。検索は通る。縦書きは字を立てるほうが良い。
- **`Tz`・`Tc` はない。** 行を伸ばすのは `push_transform` の拡大（`cm`）で代え、字ごとの位置は `x_advance`（`TJ` の調整になる）で与えた。krilla は `draw_glyphs` のたびに `q … cm /g0 gs BT … Tf … Tm … Tj ET Q` を書くので、字ごとに置くと内容のストリームが大きくなる（項目 6）。
- 本物のフォント（Noto Sans JP、CFF）は、krilla が `subsetter` でサブセットにし、`CIDFontType0` / `FontFile3`（`CIDFontType0C`）で埋め込む。ToUnicode は `text_range` から字形ごとに作られる。

### 突き当たったこと

- **輪郭が空の字形は、pdfium の文字の取り出しから落ちる。** 字形のないフォントを、Tesseract のように輪郭のない字形で作ると、pdfium は 1 字も取り出さなかった（全文 0 %、検索 0 %。pdftotext は 100 % 取れた）。字形に em の箱いっぱいの四角（4 点の輪郭 1 つ）を入れると 100 % になった。見えない文字なので形は関係なく、きつい箱も em の箱になる。lopdf の経路でも確かめる（項目 4）。
- krilla は ToUnicode を字形ごとに決め、字形はフォントの `cmap` から選ぶので、「全部の字を 1 つの字形に写す」Tesseract の作り方はそのままでは使えない。字形のないフォントは、文書に出る字ごとに 1 つずつ四角の字形を持たせて作った（[`src/fonts.rs`](src/fonts.rs) の `glyphless_ttf`。TrueType の表を手で組む）。それでも 9 ページで本物のフォントより 200 KB 小さい。
- `ja_ref` は横長の写真を A4 に引き伸ばしたので字が縦長（高さ 38 pt、幅 10 pt）。字の送りだけ合わせて幅を縮めないと、箱が字の 4 倍の幅になる（箱の中心 2 %）。行を伸ばす拡大と字の送りを組み合わせて直した。
- OCR の結果に、Noto Sans JP にない字はこの 9 ページにはなかった。ただし OCR の辞書は中国語の簡体字も含む（`fixtures/ocr_misread/` に「请」）。本物のフォントにない字は .notdef になり、ToUnicode も正しく作れない（krilla の PDF/A の検査では `ContainsNotDefGlyph` で失敗する）。字形のないフォントにはこの問題がない。

## 項目 4: lopdf で書く（すでにある PDF の経路）

### できたこと

[`src/lopdf_write.rs`](src/lopdf_write.rs)（`cargo run --release -- lopdf --font real|glyphless|glyphless-empty --fit line|tj|char --vmode v|upright|rot90`）。krilla で画像だけの PDF を作り（スキャンの PDF の代わり）、lopdf で読んで次を足す。

- ページの内容: 元の内容の前に `q`、後ろに `Q` と文字の層のストリーム（`BT 3 Tr … ET`）。元の内容がグラフィックスの状態を変えたままでも、文字の層は既定の座標で書ける。
- リソース: ページの `/Font` に `/TLH`（`Identity-H`）と `/TLV`（`Identity-V`）。2 つの Type0 フォントが 1 つの CIDFont と 1 つの `ToUnicode` を共有する。文書全体でフォントは 1 組。
- 本物のフォント: Noto Sans JP を `subsetter` 0.2.6 でサブセットにし、出てきた OTF から `CFF ` の表を取り出して `CIDFontType0` / `FontFile3`（`/Subtype /CIDFontType0C`）で埋め込む。CID はサブセットの字形の番号（`subsetter` は CID-keyed の CFF に直し、GID = CID にする）。`W` は元のフォントの送り幅（`skrifa` で読む）。`DW2 [880 −1000]`。
- 字形のないフォント（Tesseract のやり方）: 字形 2 つ（.notdef と四角の字形 1）の TrueType を手で組み（[`src/fonts.rs`](src/fonts.rs)）、`CIDFontType2` / `FontFile2`。CID は字の UCS-2 の値で、`CIDToGIDMap` のストリーム（128 KB、圧縮して数百バイト）で全部を字形 1 に写す。`DW 1000`、`DW2 [880 −1000]`。BMP の外の字と私用領域の字は、`U+F8FE` から下へ私用の CID を割り当て、`ToUnicode` でサロゲートの対に戻す。フォントは字の種類によらず同じで、`ToUnicode` だけが字の数に比例する。
- 縦書き: `Identity-V` で書き、元の点は em の上辺の中央（`DW2` の 880 で em の上端が元の点に来る）。行ごとに書くときは、列の長さに合わせて `Tc = 字の大きさ − 列の長さ / 字数` を置く（縦書きでは `Tz` は縦の送りに効かない。`Tc` は縦の送り `ty = w1·Tfs + Tc` に足されるので、広げるには負）。

9 ページの結果（字ごとに置く。全部の表は項目 5）:

| フォント | 全文 pdfium | 全文 pdftotext（縦書き t1 / v4 / m1） | 箱の中心が 4 点の中 | 検索 | 9 ページの大きさ |
| --- | ---: | ---: | ---: | ---: | ---: |
| 本物のサブセット | 全ページ 100 % | 100 % / 100 % / 100 % | 全ページ 100 % | 全ページ 100 % | 2,089,460 B |
| 字形のない（四角の字形） | 全ページ 100 % | 100 % / 100 % / 100 % | 全ページ 100 % | 全ページ 100 % | 1,876,539 B |
| 字形のない（輪郭の空の字形） | **全ページ 0 %** | 100 % / 100 % / 100 % | — | **0 %** | — |

- `Identity-V` で書くと、pdftotext も縦の列を正しい順で読む（krilla の字を立てるやり方では 5 % 前後）。pdftotext が使う poppler も縦書きのフォントを理解している。
- 縦書きの IoU が 0.76〜0.95 と横書きより低いのは、縦の字の 4 点（OCR の字の箱、列の幅）と em の正方形の差。箱の中心はすべて 4 点の中にある。

### 突き当たったこと

- **輪郭の空の字形は、`3 Tr` でも pdfium から消える。** 項目 3 と同じ。Tesseract の `pdf.ttf` のような、輪郭のない字形 1 つのフォントは、pdfium（つまり Chrome と Edge の内蔵ビューアー、アプリの worker）で検索も選択もできない。pdftotext は取れる。字形に em の四角を入れればよい（見えないので害がない）。**字形のないフォントを作るときの最大の落とし穴**。
- **`Identity-V` で列を 1 つの `Tj` にすると、pdfium は列を左から右へ並べる**（`t1_vertical` の全文 11 %）。箱の位置は正しい（列の中で字が em ずつ下へ並ぶ）が、テキストオブジェクトの順でなく位置で列を並べ替えるらしい。字ごとに `Tm` を置くと内容の順のまま正しく読む。項目 3 の 90° 回した列と同じ数字になった。**縦書きは字ごとに置く**。
- `W2` は書いていない。縦の送りはすべて 1 em（`DW2`）にして、位置は字ごとの `Tm` で決める。字ごとに置く限り `W2` は要らない。
- lopdf には `font_embedding` の機能（`skrifa` を使う）があるが、アプリは既定の機能なしで使っており、作りたいのは見えない CID フォントなので使わなかった。
- 依存（`textlayer-spike/Cargo.toml`）: `subsetter` 0.2.6（MIT OR Apache-2.0、krilla がすでに使っている）、`skrifa` 0.42（MIT OR Apache-2.0、krilla がすでに使っている）。アプリの `Cargo.lock` にどちらも入っているので、依存の木は増えない。

## 項目 5: 位置の合わせ方

### できたこと

[`src/layout.rs`](src/layout.rs) が OCR の 4 点から「ラン」（1 回の `Tj` で書く文字の並び）を作り、両方の経路が使う。比べた書き方:

| 名前 | 横書き | 縦書き |
| --- | --- | --- |
| `line` | 行ごとに 1 つの `Tj`。`Tm` で行の傾きと始まり、`Tz` で行の長さに合わせる | 列ごとに 1 つの `Tj`（`Identity-V`）。`Tc` で列の長さに合わせる |
| `tj` | `line` に加え、`TJ` の調整で各字を OCR の字の始まりに置く | `line` と同じ |
| `char` | 字ごとに `Tm`（字の 4 点の傾き）と `Tz`（字の幅） | 字ごとに `Tm`（`Identity-V`） |
| `char upright` | `char` と同じ | 字ごとに立てた横書きの字（krilla でもできる形） |
| `line rot90` | `line` と同じ | 列を 90° 回した横書きの 1 行 |

- 横書きの元の点: 4 点の左の辺の、下から高さの 12 %（em の 880 / −120 の分け方に合わせた基準線）。字の大きさ = 4 点の高さ。
- 縦書きの元の点: 4 点の上の辺の中点。字の大きさ = 列の幅。
- 傾き: 上の辺と下の辺の向きの平均。字ごとのときは字の 4 点ごとに。

lopdf の経路・本物のフォントで、9 ページを字の数で重みを付けて平均した（[`scripts/placement.sh`](scripts/placement.sh)、全部の表は `out/placement_tables.md`）:

| 書き方 | 全文 pdfium | 全文 pdftotext | 箱の中心 横書き | 箱の中心 縦書き・混在 | IoU 横 | IoU 縦・混在 | 検索 | 余計な空白（計） |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `line`（縦は `Identity-V`） | 40.7 % | 85.4 % | 95.1 % | 58.3 % | 0.79 | 0.48 | 100.0 % | 40 |
| `tj`（縦は `Identity-V`） | 40.7 % | 86.1 % | 99.6 % | 58.3 % | 0.93 | 0.49 | 99.8 % | 79 |
| **`char`（縦は `Identity-V`）** | **100.0 %** | **83.5 %** | **100.0 %** | **100.0 %** | **0.99** | **0.91** | **100.0 %** | **18** |
| `char upright` | 100.0 % | 27.0 % | 100.0 % | 100.0 % | 0.99 | 1.00 | 100.0 % | 18 |
| `line rot90` | 40.7 % | 84.5 % | 95.1 % | 58.3 % | 0.79 | 0.50 | 100.0 % | 41 |

- 余計な空白の 18 は、すべて `ja_ref`（同じ高さに別々の OCR の行が並ぶ写真）。字ごとに置いた横書きの生成ページでは 0。
- `line`・`tj`・`rot90` の縦書きの低い数字（全文 40 %、箱 58 %）は**位置の誤りではない**。pdfium が列を左から右へ並べ替えるので、読み順で並べた比べ方がずれ、並んだ字の数が減っている。絵（`overlays/p_line_v__t1_vertical.png`）では箱は列にきちんと重なる。ただしアプリは pdfium で文字を取り出すので、コピーした文字と読み上げの順が逆になるのは実害。
- `tj` は横書きの箱を良くする（95 → 99.6 %）が、OCR の字の区切り（CTC の時刻から見積もった区切り）がばらつくところで字の間が開き、pdfium が空白を足す（h1 で 17、`h1_horizontal_rot2` で 21）。コピーした文字に「一 人」のような空白が入る。`char` は字の幅も字ごとの `Tz` で合わせるので、隙間ができず空白が入らない。
- pdftotext の数字は、`Identity-V` を使う書き方だけが縦書きを正しい順で読む（`char upright` は 27 %）。pdfium だけを見るなら `char upright` でも同じだが、ほかのビューアーで読み順が崩れる心配が残る。

### 選択の帯を目で見る

[`src/overlay.rs`](src/overlay.rs)（`cargo run --release -- overlay <out の下のディレクトリ> <ページ> [倍率]`）。pdfium で描いたページに、pdfium の緩い箱（選択の帯）を半透明の青、OCR の字の 4 点を赤の枠で重ねる。`overlays/` に生成したページの絵だけを入れた（1.5 倍、`ja_ref` は入れていない）:

- `p_char_v__h1_horizontal.png` / `p_line_v__h1_horizontal.png`: 横書き。行ごとでも帯は行に重なるが、字の区切りは OCR の字とずれる。
- `p_char_v__t1_vertical.png` / `p_line_v__t1_vertical.png`: 縦書き。どちらも帯は列に重なる（行ごとの低い数字は順の問題）。
- `p_char_v__h1_horizontal_rot2.png` / `p_char_v__m1_mixed_rot2.png`: 2° の傾き。pdfium の緩い箱は傾かない四角（字ごとの外接の箱）だが、字ごとに置くので段々に傾きに沿う。
- `p_char_v__v4_dialogue.png`: 会話の括弧と句読点を含む縦書き。

### 突き当たったこと

- pdfium の緩い箱は、回したテキストでも軸に沿った四角しか返さない（`FPDFText_GetLooseCharBox` は `FS_RECTF`）。選択の帯が傾くかはビューアーしだい。
- 字ごとに置くと内容のストリームが大きくなる（字ごとに `Tf`・`Tz`・`Tm`）。大きさは項目 6。

## 項目 6: 大きさと速さ

### どう測ったか

`cargo run --release -- bench`（[`src/main.rs`](src/main.rs) の `bench`）。9 ページを順に繰り返して 1・10・100 ページにした（同じページの繰り返しだけだとサブセットの字が増えないので、9 ページを回した。字の種類は 9 ページで 431）。字ごとに置く書き方は項目 5 の `char`、行ごとは `line`、縦は `Identity-V`（krilla は字を立てる）。4 コアの入れ物、release ビルド、1 回だけの計測。

- 画像だけ（krilla）: 文字の層のない PDF。lopdf の経路の入力でもある。krilla は同じ画像を 1 つにまとめるので、100 ページでも 9 枚ぶんの大きさ（文字の層はページごとなので差の比べ方には効かない）。
- 時間: krilla は画像の読み込みと書き込みを含む全体（画像だけの時間と比べる）。lopdf は、画像だけの PDF を読み込み、文字の層を足し、保存するまで。

lopdf の内容のストリームは、変わらない演算子（`Tf`・`Tz`・`Tc`）を省き、同じ角度が続くときは `Tm` の代わりに相対の `Td` で動く（[`src/lopdf_write.rs`](src/lopdf_write.rs) の `State`）。これで字ごとの書き方の 100 ページの差が 522 KB → 236 KB になった（全文・箱・検索の数字は変わらない）。krilla の経路は字ごとに `q … cm … gs BT … ET Q` を書くので、この詰め方ができない。

### 数字

| ページ | 字 | 書き方 | 大きさ (B) | 画像だけとの差 (B) | 時間 |
| ---: | ---: | --- | ---: | ---: | ---: |
| 1 | 439 | 画像だけ（krilla） | 174906 | — | 61 ms |
| 1 | 439 | krilla 本物 char | 210831 | +35925 | 52 ms（画像込み） |
| 1 | 439 | krilla 字形なし char | 183976 | +9070 | 44 ms（画像込み） |
| 1 | 439 | lopdf 本物 char | 208192 | +33286 | 3 ms（読み込みと保存込み） |
| 1 | 439 | lopdf 字形なし char | 180383 | +5477 | 2 ms（読み込みと保存込み） |
| 1 | 439 | lopdf 本物 line | 206492 | +31586 | 2 ms（読み込みと保存込み） |
| 1 | 439 | lopdf 字形なし line | 178634 | +3728 | 1 ms（読み込みと保存込み） |
| 10 | 4323 | 画像だけ（krilla） | 1799585 | — | 407 ms |
| 10 | 4323 | krilla 本物 char | 1944296 | +144711 | 422 ms（画像込み） |
| 10 | 4323 | krilla 字形なし char | 1870025 | +70440 | 410 ms（画像込み） |
| 10 | 4323 | lopdf 本物 char | 1903737 | +104152 | 16 ms（読み込みと保存込み） |
| 10 | 4323 | lopdf 字形なし char | 1826983 | +27398 | 12 ms（読み込みと保存込み） |
| 10 | 4323 | lopdf 本物 line | 1892429 | +92844 | 9 ms（読み込みと保存込み） |
| 10 | 4323 | lopdf 字形なし line | 1815412 | +15827 | 3 ms（読み込みと保存込み） |
| 100 | 43163 | 画像だけ（krilla） | 1830796 | — | 4010 ms |
| 100 | 43163 | krilla 本物 char | 2572240 | +741444 | 4142 ms（画像込み） |
| 100 | 43163 | krilla 字形なし char | 2495749 | +664953 | 4080 ms（画像込み） |
| 100 | 43163 | lopdf 本物 char | 2135660 | +304864 | 96 ms（読み込みと保存込み） |
| 100 | 43163 | lopdf 字形なし char | 2066548 | +235752 | 88 ms（読み込みと保存込み） |
| 100 | 43163 | lopdf 本物 line | 2028662 | +197866 | 25 ms（読み込みと保存込み） |
| 100 | 43163 | lopdf 字形なし line | 1956807 | +126011 | 18 ms（読み込みと保存込み） |

- **フォントの固定の費用**: 本物のフォントのサブセットは、431 字で約 30 KB（1 字あたり約 70 B）。ふつうの本の 100 ページは字の種類が 2,000〜3,000 なので、150〜200 KB になる見込み。字形のないフォントは字の種類によらず約 1.5 KB（フォントの本体 0.3 KB、`CIDToGIDMap` が圧縮して約 0.3 KB、残りは `ToUnicode`。`ToUnicode` は 1 字あたり十数バイトで増える）。
- **ページごとの費用**（100 ページの差から固定の費用を引いて割る）: lopdf の字ごと 約 2.3 KB/ページ（1 ページ約 430 字、1 字あたり約 5 B）、行ごと 約 1.3 KB/ページ。krilla の字ごと 約 6.6 KB/ページ。
- 画像だけ 1 ページ 175 KB（200 dpi のグレースケールの PNG）に対して、lopdf・字形のない・字ごとの文字の層は 1 ページで +5.5 KB（+3 %）、100 ページで +236 KB。本物のフォントにすると、そこに +30〜200 KB。
- 速さ: lopdf で 100 ページに足すのに 90 ms 前後（読み込みと保存込み）。krilla で一緒に書くときの増えは 100 ページで 70〜130 ms（ばらつきの範囲に近い）。どちらも OCR（1 ページ数秒）に比べて無視できる。

## 項目 7: PDF/A-2b と暗号化

### どう作ったか

`cargo run --release -- pdfa`（[`src/main.rs`](src/main.rs) の `pdfa`）。3 ページ（`h1_horizontal`・`t1_vertical`・`m1_mixed_rot2`）の文書を、アプリの PDF/A-2b の書き方（`crates/core/src/pdf_write.rs`: `Archival::A2_B` の検証、作成日時を UTC で、補間なし）にならって作った。

- krilla の経路: PDF/A-2b の設定の `Document` に、画像と文字の層を一緒に書く（字ごと、透明な塗り、縦は字を立てる）。
- lopdf の経路: krilla で画像だけの PDF/A-2b を書き、lopdf で文字の層（字ごと、`3 Tr`、縦は `Identity-V`）を足して保存する。
- 暗号化: lopdf の経路の出力を、`crates/core/src/pdf_encrypt.rs` と同じ AES-256（V5・R6、`/Length` と `/Extensions` も同じ）で暗号化し、pdfium と pdftotext にパスワードを渡して読む。

### 数字

veraPDF 1.28.2、PDF/A-2b（[`scripts/verapdf.sh`](scripts/verapdf.sh)）:

| 文書 | krilla の検証 | veraPDF |
| --- | --- | --- |
| 画像だけ（krilla） | 通る | PASS |
| krilla・本物のフォント（`/ca 0` の透明な塗り） | 通る | PASS |
| krilla・字形のない（四角の字形、`/ca 0`） | 通る | PASS |
| lopdf・本物のフォント（`3 Tr`） | — | PASS |
| lopdf・字形のない（四角の字形、`3 Tr`） | — | PASS |
| lopdf・字形のない（輪郭の空の字形、`3 Tr`） | — | PASS |
| （対照）krilla の通常の PDF | — | FAIL |

- 字形のないフォントは、PDF/A-2b で問題にならなかった。`W`（`DW 1000`）とフォントの `hmtx`（すべて 1000）が合っている（6.2.11.5 の幅の一致）、フォントは埋め込んである（6.2.11.4）、`CIDToGIDMap` はストリームで、`CIDSet` は書いていない（PDF/A-2 では任意）。krilla は字形のないフォントにも `CIDSet` を書くが、それも通った。
- 透明な塗り（`/ca 0`）も PDF/A-2b で通る（PDF/A-2 は透明を許す。PDF/A-1 なら通らないはず）。
- lopdf で足した後も、krilla の XMP メタデータと出力インテントはそのまま残り、通った。XMP の `ModifyDate` などは更新していないが、veraPDF は問題にしなかった。アプリに入れるときは、krilla が XMP に書く作成ソフトと文書情報の食い違いが起きないように、lopdf では文書情報を触らない。

暗号化（AES-256）した後の取り出し（パスワード付き）:

| 文書 / ページ | 全文 pdfium | 全文 pdftotext | 箱の中心 | 検索 |
| --- | ---: | ---: | ---: | ---: |
| 本物 / h1・t1・m1_rot2 | 100 % | 100 % / 100 % / 41.0 % | 100 % | 100 % |
| 字形のない / h1・t1・m1_rot2 | 100 % | 100 % / 100 % / 41.0 % | 100 % | 100 % |

暗号化の前と数字は同じ（`m1_mixed_rot2` の pdftotext の 41 % は暗号化の前から。傾いた混在ページで poppler が横書きと縦書きを混ぜて並べる）。mutool でも読めた。

### 突き当たったこと

- **暗号化した PDF/A は veraPDF が「暗号化されているので処理できない」と FAIL にする。** PDF/A は暗号化を禁じており、アプリはすでに PDF/A とパスワードの組み合わせを拒んでいる（`docs/design.md` §4.3.1、§7.1）。文書情報を変えないので、文字の層はこの決まりに何も足さない。
- **暗号の辞書の `/Length` を省くと、poppler（pdftotext、Linux の多くのビューアー）が復号できない**（「Unknown compression method in flate stream」）。pdfium と qpdf は読める。アプリの `pdf_encrypt.rs` の `add_key_length` はこれを書いているので問題ない。試作で最初にこれを省いて気づいた。文字の層とは関係ないが、`add_key_length` を消さないように書き残す。
- 文字の層を足すのは暗号化の**前**（lopdf の `Document` のまま `add_text_layer` → `encrypt_document`）。整理（`organize.rs`）と同じ流れで、復号と再暗号化は要らない。

## 項目 8: 確信度の低い行、`ActualText` とタグ（一部だけ）

### 確信度の低い行

`layout::runs` は `min_confidence` を受け取り、それより低い行を層に入れない（試作では 0 で、すべて入れた）。OCR の結果（`fixtures/`）の行の確信度を見た:

| しきい値 | 正しく読めたページの行（167 行中） | 読み誤ったページの行（`ocr_misread/`、28 行中） |
| --- | ---: | ---: |
| 0.5 未満 | 1（ノンブルの「6」） | 0 |
| 0.7 未満 | 9 | 6 |
| 0.8 未満 | 15 | 12 |
| 0.9 未満 | 24 | 18 |

- 句読点の並びになった読み誤り（「。。、。」など）は確信度 0.63〜0.9 で、正しい行（見出し、ノンブル、1 字の列）と重なる。しきい値で分けると、ゴミを落とすより先に正しい行を落とす。
- 見えない層のゴミの害は、検索で句読点の並びがまれに当たることとコピーに混じることだけ。落とした行の害は、その行が検索できないこと。**行の確信度では落とさず全部入れる**のを勧める。落とすなら「句読点・記号だけの行」のように中身で判断するほうが良い（試していない）。

### `ActualText` とタグ（試していない）

時間を見て、作らずに調べるだけにした。

- アプリが使う pdfium の文字の取り出し（`FPDFText_*`）は構造ツリーを見ない。読み順は内容の順と位置で決まる（項目 5 の縦書きの並べ替えがその例）。タグを足しても、アプリの中の検索・コピーは変わらない。
- 効くのは Acrobat の読み上げ・リフロー・アクセシビリティの検査と、PDF/UA。OCR の層だけにタグを付けると、画像のページに `Figure` と `Span`（`ActualText`）を組むことになり、PDF/A-2b の範囲（タグは任意）を超えて PDF/A-2a・PDF/UA の検査を通すには代替テキストや言語の指定も要る。
- `ActualText` は、1 字ずつ置いた縦書きを 1 つの文字列として読ませたいビューアーに効く可能性がある。ただし krilla のソースのコメント（`content.rs` の `encode_consecutive_glyph_run` の上）にあるとおり、Acrobat の `ActualText` には行の終わりでコピーが崩れる不具合がある。入れるなら、Acrobat で確かめてから。

## まとめ

### 1. 勧める作り

**lopdf の経路で、字形のないフォント（四角の字形）を、字ごとに置く。縦書きは `Identity-V`。**

- 経路: **画像 → PDF もスキャンの PDF も、lopdf で足す 1 つの経路にする。** 画像 → PDF は今どおり krilla で画像のページを書き（PDF/A-2b も krilla の検証のまま）、その後 lopdf で文字の層を足す。理由:
  - krilla は `3 Tr` も `Identity-V` も書けず、作者も足す気がない（項目 3）。透明な塗りは `/ca` を無視する経路で文字（字形のないフォントなら四角）が黒く出る心配がある。縦書きを字を立てて置くと pdftotext が読み順を崩す（27 % 対 `Identity-V` で 84〜100 %）。
  - lopdf は既存の PDF にも同じ関数で足せる。暗号化（`encrypt_document`）と整理（`organize.rs`）がすでに lopdf の `Document` で動いているので、その手前に差し込むだけ。
  - 速さ: 100 ページで 90 ms（項目 6）。krilla で画像を書くのは今もしているので、増えるのは lopdf の読み込みと保存だけ。
- フォント: **字形のないフォント**。理由:
  - 検索・選択・コピー・箱の位置の数字は、本物のサブセットと同じ（全部 100 %、項目 4・5）。
  - 大きさ: フォントの固定の費用が約 1.5 KB（本物のサブセットは 431 字で 30 KB、本 1 冊で 150〜200 KB 見込み）。
  - どの字も書ける（本物のフォントにない字は .notdef になり、ToUnicode も壊れる）。OCR の辞書は簡体字も含む。
  - フォントの本体を同梱しなくてよい（本物なら Noto Sans JP の 4.5 MB をアプリに入れることになる）。
  - **字形には必ず輪郭（em の四角）を入れる**。輪郭が空だと pdfium（Chrome・Edge の内蔵ビューアーとアプリ自身）で文字が消える（項目 3・4）。
- 位置: **字ごとに置く**（字ごとに `Tm`／`Td`、横書きは字ごとの `Tz` で字の幅も合わせる）。理由:
  - 縦書きで pdfium が正しい順で読むのは字ごとのときだけ（行ごとは全文 41 %、項目 5）。
  - 横書きでも箱が 95 % → 100 %、IoU 0.79 → 0.98 になり、`TJ` で起きる余計な空白（コピーの「一 人」）も出ない。
  - 費用は 1 ページ約 2.3 KB（行ごとは 1.3 KB）。同じ角度が続くときは `Td` で詰めた（項目 6）。

### 2. 縦書きで足りないこと、ビューアーで違いそうなこと

- 試作で確かめたのは pdfium（アプリと Chrome・Edge と同じ系統）と poppler（pdftotext）と mutool だけ。**Acrobat Reader は確かめていない。**
- ビューアーで違いそうなこと:
  - 選択の帯の形: pdfium は回したテキストでも軸に沿った四角を返す。Acrobat は傾いた帯を描くはず。縦書きの帯が列に沿うか、字ごとに細切れになるか。
  - 字ごとに置いた文字の読み順: pdfium と poppler は列ごとにまとめたが、Acrobat が字ごとの `Tm` を 1 字ずつの「行」とみなし、コピーで字の間に改行を入れないか。
  - `Identity-V` の縦書きで、検索の当たりの帯が縦に描かれるか。
  - 縦書きの句読点（`、` `。`）・括弧・長音・小さいかな: 見えない層は横書きの字形の番号を使うので、どのビューアーでも中身は普通の字（`U+3001` など）になるはず。縦書き用の字形（`U+FE11` など）でコピーされないか。
  - 行の確信度で落とさないので、読み誤りのある行（`m1_mixed_rot2` の OCR の生の結果など）を選ぶと句読点の並びが入る。
- OCR の側の宿題: 2° 傾いた縦の列を横書きと取り違える（項目 1）。文字の層は正しい 4 点さえあれば傾きを正しく扱える。

**オーナーが Windows で手で確かめる手順**

見本は `textlayer-spike/samples/` にコミットした（自分で生成した 8 ページ、`ja_ref` は入れていない）。

| ファイル | 中身 |
| --- | --- |
| `A_lopdf_glyphless_char.pdf` | 勧める作り（lopdf・字形のない・字ごと・縦は `Identity-V`） |
| `B_krilla_real_char.pdf` | krilla・本物のフォント・字ごと・透明な塗り・縦は字を立てる |
| `C_lopdf_glyphless_line.pdf` | lopdf・字形のない・行ごと（縦書きの並べ替えを見るための対照） |

ページの順: 1 `h1_horizontal`（横書き）、2 `t1_vertical`（縦書き、走れメロス）、3 `v2_two_tiers`（縦 2 段＋横の柱）、4 `v4_dialogue`（会話の括弧）、5 `m1_mixed`（横の見出し＋縦）、6 `h1_horizontal_rot2`、7 `t1_vertical_rot2`、8 `m1_mixed_rot2`（6〜8 は 2° 傾き）。

Acrobat Reader、Edge、Chrome（PDF をタブにドラッグ）のそれぞれで、A・B・C を開いて:

1. **検索**（Ctrl+F）: 次を探し、当たりの数と帯の位置を見る。
   - 2 ページ（縦書き）: `激怒`、`邪智暴虐`、`メロスは、`（読点を含む）、`「王様は、人を殺します。」`（括弧と句点）、`きょう`（小さいかな）、`セリヌンティウス`（長音なし）、`けれども`
   - 4 ページ: `タンタアーン`（長音）、`「ぜんたい、`
   - 1 ページ（横書き）: `羅生門`、`雨やみを待っていた`（行をまたぐ。当たらなくてよいが、当たるかを記録）、`2026年11月5日`
   - 6〜8 ページ（傾き）: 同じ語が当たるか
2. **選択の帯**: 2 ページの右端の列（`メロスは激怒した。…`）をドラッグで選び、帯が列に沿うか、1 字ずつ細切れか、隣の列にはみ出すかを見る。1 ページで 1 行を選び、帯が字に重なるか。6・8 ページで帯が傾きに沿うか。Ctrl+A でページ全体を選んだときの帯も見る。
3. **コピーして貼る**: 2 ページの最初の 2 列を選んでコピーし、メモ帳に貼る。期待: `メロスは激怒した。必ず、かの邪智暴虐の王を除かなければならぬと決意した。…`。見ること: 字の間に改行や空白が入らないか、列の順（右から左）が保たれるか、句読点・括弧・長音・小さいかなが普通の字で入るか（縦書き用の字（`︑` `︒` など）になっていないか）。1 ページで 1 行をコピーし、字の間に空白が入らないか。
4. **C と比べる**: C の 2 ページで同じことをし、列の順が A と違うか（pdfium の系統の Edge・Chrome では左から右になるはず）。
5. **見た目**: A・B・C とも、ページに文字が見えない（画像だけに見える）こと。B を印刷プレビュー（Acrobat の「印刷」→ プレビュー）でも見て、黒い文字や四角が出ないか（透明な塗りの心配）。
6. 結果は、ビューアー × ファイル × 手順の表にして `textlayer-spike/REPORT.md` に足すか、イシューに書く。

### 3. アプリに入れるときの形

- **置く場所: `crates/core` の新しいモジュール `text_layer.rs`。** lopdf と krilla（`subsetter`・`skrifa` は使わない）はすでに core にある。OCR は worker（`ort`、pdfium で描く）が行い、`OcrPage` を作る。文字の層を書くのは PDF を書く側（core）なので、`OcrPage` の型は core に置き（worker が core に依存している）、worker の protocol と IPC はその型を運ぶ。
- 関数の形（案）:

```rust
/// `pages` は (ページの番号 0 始まり, そのページの OCR の結果)。
/// 座標は表示する向きのページの pt。/Rotate・CropBox・UserUnit はここで直す。
pub fn add_text_layer(doc: &mut lopdf::Document, pages: &[(usize, &OcrPage)]) -> Result<(), PdfWriteError>;
```

  中身は試作の 3 つ: `layout.rs` の字ごとのラン（`Fit::Char`、縦は `VMode::IdentityV`）、`fonts.rs` の 2 字形の字形のないフォント（四角の輪郭）、`lopdf_write.rs` の `add_fonts`（`LFont::Glyphless`）と `content`（`Td` で詰める）。フォントは文書に 1 組で、ページごとに `/Font` から参照する。
- 流れ:
  - 画像 → PDF: `PdfWriter::finish()`（krilla、PDF/A-2b ならその検証）→ `Document::load_mem` → `add_text_layer` → パスワードがあれば `encrypt_document` → 保存。今の `encrypt_pdf` と同じく、krilla の出力を lopdf で読み直す形。PDF/A とパスワードは今どおり一緒にしない。
  - スキャンの PDF: 整理（`organize.rs`）と同じく lopdf で読み、`add_text_layer`、必要なら暗号化、保存。
- **試作でしていないこと（入れるときに要る）**:
  - 既存の PDF の `/Rotate`（90°・180°・270°）、MediaBox・CropBox の原点のずれ、`UserUnit` での座標の直し。試作の入力は krilla が書いた回転のないページだけ。
  - すでに文字のあるページ（デジタルの PDF や、前に OCR したページ）の扱い（飛ばすか、上書きするか）。
  - 内容のストリームが配列のページ、`/Resources` が親（`Pages`）から継承されているページ、`/Font` が参照のページ（試作は `get_or_create_resources` に頼り、継承を見ていない）。
  - 1 ページだけの OCR のやり直し（層の差し替え）。
- 足す依存: **なし**。`lopdf`（MIT）と `krilla`（MIT OR Apache-2.0）はすでにある。字形のないフォントは自前で組む（数十行）。本物のフォントを選ぶなら `subsetter`・`skrifa`（どちらも MIT OR Apache-2.0、krilla 経由ですでに `Cargo.lock` にある）と、Noto Sans JP（SIL OFL 1.1、4.5 MB を同梱、`about.toml` と `deny.toml` に OFL を足す）が要る。

### 4. 実装の手間の見積もり（試作を土台に）

| 仕事 | 日 |
| --- | ---: |
| core に `text_layer.rs`（字形のないフォント、ラン、内容のストリーム、`OcrPage` の型）を lint に合わせて移す | 1 |
| 既存の PDF の座標の直し（`/Rotate`・CropBox・`UserUnit`）、リソースの継承、文字のあるページの扱い | 1 |
| 画像 → PDF・整理の流れへの組み込み（PDF/A、暗号化の順）、worker → core への `OcrPage` の受け渡し | 0.5 |
| テスト: pdfium での取り出し（全文・箱・検索、試作の `check.rs` を移す）、回転したページ、暗号化、PDF/A（krilla の検証＋手元の veraPDF） | 1 |
| Windows のビューアーでの手の確かめ（上の手順）と、それで見つかる直し | 0.5〜1 |
| **計** | **4〜4.5** |

当初の見積もり（4〜6 日）のうち、作り方が分からない部分（フォント、縦書き、ビューアーでの見え方の大部分）は試作で片付いた。残る幅は、既存の PDF の座標の直しと、Acrobat で何か見つかるかどうか。本物のフォントを選ぶなら、同梱とライセンスの手続きで +0.5 日。
