# OCR の Rust の試作 — 報告

捨てる前提の試作。コードは `ocr-spike/` だけにあり、マージしない。

## 入れ物

- CPU: Intel(R) Xeon(R) Processor @ 2.10GHz（クラウドの VM）、4 vCPU（1 コア 1 スレッド）、メモリ 15 GB。
- onnxruntime 1.26.0（`npm run layout:fetch`）、`ort =2.0.0-rc.15`（`load-dynamic`、`api-26`）。worker と同じ。
- スレッド数は worker の `load_session` と同じ決め方（`min(6, 論理 CPU − 1)`）で、この入れ物では **3**。spinning は切る、`Level3`。

## 項目 1: 骨組みと取得

できたこと:

- `ocr-spike/` に自分の `[workspace]` を持つクレートを作った（[Cargo.toml](Cargo.toml)）。取得は [scripts/fetch.sh](scripts/fetch.sh) にまとめた。
- 取れたもの: PP-OCRv6 small・medium の det と rec（`inference.onnx`・`inference.yml`）、pure-onnx-ocr（`59a6c2c`）の `tests/reference/`、照合の画像 `ja.jpg`、XFUND 日本語 val（50 枚）。
- 大きさ: small det 9.9 MB、small rec 21.2 MB（辞書入りの yml 150 KB）、medium det 62.0 MB、medium rec 76.6 MB。

突き当たったこと:

- 最初の試行では `huggingface.co` がネットの設定で拒否された（許可されたあとに再開した）。
- 照合の画像のうち `general_ocr_002.jpg`（中国語の搭乗券）は `paddle-model-ecology.bj.bcebos.com` にしかなく、この入れ物から取れない（403）。照合は `ja.jpg`（日本語）だけで行った。
- `npm ci` は、入れ物の Node が v22 でリポジトリの `engines`（`>=24.15.0`）に足りず、`EBADENGINE` で失敗した。`--engine-strict=false` を付けて入れた。
- 青空文庫（`www.aozora.gr.jp`）は拒否される（項目 5 で扱う）。
- モデル・データ・取得したリポジトリ・`.venv` は [.gitignore](.gitignore) で外した。

増やした依存（すべて `ocr-spike/` の中だけ）:

| クレート | 用途 | ライセンス |
| --- | --- | --- |
| `ort` =2.0.0-rc.15 | onnxruntime | MIT OR Apache-2.0（worker と同じ） |
| `image` 0.25 | 画像の読み込み | MIT OR Apache-2.0（worker と同じ） |
| `imageproc` 0.25 | 輪郭の抽出（Suzuki–Abe、`cv2.findContours` の代わり）、縦書きのページの描画 | MIT |
| `ab_glyph` 0.2 | フォントの読み込み（縦書きのページの描画） | Apache-2.0 |
| `yaml-rust2` 0.10 | rec の `inference.yml` から辞書を読む | MIT OR Apache-2.0 |
| `unicode-normalization` 0.1 | CER の NFKC | MIT OR Apache-2.0 |
| `serde`・`serde_json` | 照合の JSON | MIT OR Apache-2.0 |

すべて `deny.toml` が許すライセンスの範囲にある。本番に要るのは `ort`・`image`・輪郭の抽出（`imageproc` を使うか、自分で書くか）・YAML の読み込みの 4 つ（YAML は配るときに辞書を素のテキストにすれば要らない）。

## 項目 2: Det と Rec を Rust で通す

できたこと:

- PaddleOCR 3.x（PaddleX 3.7.2 のソースを読んで写した）の前処理・後処理を、`ort` の上で `Vec<f32>` / `&[f32]` のまま書いた。pure-onnx-ocr の型や tract は経由しない。
  - 画像は `cv2.imread` と同じく **BGR** で持つ。PaddleOCR は ImageNet の平均と分散を B, G, R の順に当てるので、RGB で持つと黙って入れ替わる。
  - 検出の縮小: `DetResizeForTest.resize_image_type0`（各辺を 32 の倍数に、Python の `round` なので 0.5 は偶数側へ）。`cv2.resize(INTER_LINEAR)` を固定小数点の丸めまで写した（[imgops.rs](src/imgops.rs) `resize_linear`）。
  - DB の後処理: `prob > 0.3` の二値画像 → 輪郭（穴も含む、`RETR_LIST` 相当）→ 最小面積矩形（短辺 3 未満は捨てる）→ 矩形の内側の平均確率が 0.6 未満は捨てる → 矩形を `面積 × 1.5 / 周長` だけ角を丸めて膨らませる（pyclipper の手順を写した）→ もう一度最小面積矩形（短辺 5 未満は捨てる）→ 軸ごとの倍率で元の画像の座標へ（[engine.rs](src/engine.rs) `db_boxes`、[geom.rs](src/geom.rs)）。
  - 並べ方: `SortQuadBoxes`（上から下、y の差が 10 px 未満なら左から右）。
  - 切り出し: 最小面積矩形の 4 点から透視変換で軸に平行な矩形に展開する（`cv2.warpPerspective` の `INTER_CUBIC`・`BORDER_REPLICATE`、小数の位置は 1/32 に量子化）。`h / w ≥ 1.5` なら左に 90° 回す（`np.rot90`）。
  - 認識: 縦横比の小さい順に並べ、6 行ずつ束ね、各行を高さ 48・幅 `ceil(48 × 縦横比)`（キャンバスの幅は `max(320, int(48 × 縦横比))`、上限 3200）に縮め、束の中で一番広い行に合わせて右を 0 で埋める（PaddleX の `ToBatch`）。正規化は `/255 − 0.5) / 0.5`。CTC は貪欲法で、確信度は出した文字の確率の平均。
- 照合の試験: [tests/parity.rs](tests/parity.rs)（`cargo test --release`）。PaddleOCR の既定（`limit_type=min`、`limit_side_len=64`、つまり等倍）で動かし、参照と同じ順で行の数・文字・4 隅を比べる。手で見るときは `cargo run --release -- parity --model small`。

数字（`ja.jpg`、PaddleOCR 3.7.0 の出力との比較）:

| モデル | 行の数（参照 / 自作） | 文字が完全に一致 | 並び順 | 4 隅が完全に一致 | 4 隅の最大のずれ |
| --- | --- | --- | --- | --- | --- |
| small | 56 / 56 | 56 / 56 | 同じ | 47 / 56 | 2 px |
| medium | 54 / 54 | 54 / 54 | 同じ | 47 / 54 | 2 px |

一致しない所と理由:

- 4 隅が 1〜2 px ずれる行が 7〜9 本ある。多くは 4 隅がそろって 1 px ずれている（大きさは同じ）。原因は OpenCV の `minAreaRect`・`boxPoints` が float32 で計算し、整数のはずの角が `334.99994` のようになり、pyclipper がそれを切り捨てて 334 にすること（入れ物の `opencv-python-headless` で `cv2.boxPoints` を呼んで確かめた）。誤差の向きは矩形ごとに違うので、固定のずれ（1e-4）を足すと一致は 47 本から 16 本に減った。完全に写すには OpenCV の float32 の計算の順序まで写す必要があり、文字には効かない（ずれた行も文字はすべて一致）ので、2 px を許容の幅にした。
- `general_ocr_002`（中国語の搭乗券）は画像が取れず、照合していない。

突き当たったこと:

- 照合の参照は PaddleOCR の既定（`min`・64、つまり等倍）で作られている。採用する設定（`max`・1600）とは違うので、試験は既定で動かし、ほかの項目は `max`・1600 で動かす。
