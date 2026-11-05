# 縦書きの試作のページの組

`VERTICAL.md` の項目 1 で固定した組。正解の文字列と座標だけをここに置き、絵は `cargo run --release -- gen-sets` で `data/`（git に入れない）に作り直す。

| 組 | 中身 | 正解 | 絵 |
| --- | --- | --- | --- |
| `dev` | 項目 5 の 4 枚（IBM Plex Sans JP、`vertical::pages`）。**規則やしきい値はこの組だけを見て決める** | `sets/dev/*.json` | `data/vertical/*.png` |
| `val` | 確かめの組の生成 7 枚（明朝 5 枚: Noto Serif JP、ゴシック 2 枚: IBM Plex Sans JP、`valpages::pages`） | `sets/val/*.json` | `data/val/*.png` |
| `ndl` | 確かめの組の実際のスキャン 20 枚（国立国会図書館 `pdmocrdataset-part1`、Public Domain Mark 1.0） | 取得した JSON（`data/ndl/json/`） | `data/ndl/img/` |

## 生成の設定

- 1654 × 2339 px（A4 を 200 dpi）、白地に黒、グレースケールの PNG。
- フォント（`data/fonts/`、`scripts/vert_map.py` で `vert` の置き換えと em の位置を抜き出す）:
  - `IBMPlexSansJP-Regular.otf`: `node_modules/@ibm/plex-sans-jp/fonts/complete/woff/hinted/IBMPlexSansJP-Regular.woff`（@ibm/plex-sans-jp 3.x）を fontTools で OTF に戻したもの。SHA-256 `dda65e359ccf0de131c571fd4f42b9647fd912c00e5fb14ae431368473e0a5e3`。SIL OFL 1.1。
  - `NotoSerifJP-Regular.otf`: `https://raw.githubusercontent.com/notofonts/noto-cjk/main/Serif/SubsetOTF/JP/NotoSerifJP-Regular.otf`（2026-11-05 に取得）。SHA-256 `2c9a12dbd4f2408c4610c7ee84a108b62d7236c3775baed618c64d9cb44b2f04`。SIL OFL 1.1。
- `dev` の 4 枚は項目 5 と同じ描き方: `ab_glyph` の `PxScale` が名目の大きさ（フォントの高さ）なので、em は名目の 2/3（Plex）、字送りは名目の大きさ（1.5 em）、em の上端はペンの位置より名目の 0.12 下。`char_boxes` はこの実際の em の箱（`line_boxes` は項目 5 のまま）。
- `val` は em の上端をペンの位置に合わせて描いた（`em_align`）。明朝体の 5 枚は em を名目の大きさにしてベタ組み（`true_em`、字送り 1 em）、ゴシック体の 2 枚は開発の組と同じ em と字送り（対照）。
- `char_boxes` は字ごとの em の箱 `[x0, y0, x1, y1]`（ページの画素）。縦書きは em の正方形。

## NDL のページ

- `sets/ndl_pages.txt`: `scripts/ndl_select.py` が選んだ 20 枚（昭和の 4 つのフォルダ `tosho_1930_bunkei`・`tosho_1930_rikei`・`tosho_1940_bunkei`・`tosho_1940_rikei` から、行の 8 割以上が縦書きで 200 字以上あるページを、ファイル名の SHA-1 の順に 5 枚ずつ）。OCR の結果は見ずに選んだ。
- 取得: `scripts/ndl_fetch.py json <フォルダ>...` と `scripts/ndl_fetch.py img sets/ndl_pages.txt`（HTTP の範囲指定で zip の中央ディレクトリと選んだファイルだけを読む。`remotezip`、MIT）。
- 正解: `boxes` を `box_id` の順につないだもの（読み順）。旧字体のまま、`▣`（文字コードにない字）もそのまま。`dontcare` の 4 点の中は白で塗る。
