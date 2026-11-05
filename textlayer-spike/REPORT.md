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
