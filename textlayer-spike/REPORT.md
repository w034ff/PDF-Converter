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
