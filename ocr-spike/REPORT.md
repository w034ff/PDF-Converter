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

## 項目 3: 速さとメモリ

できたこと:

- Python の最小のスクリプト [scripts/ocr_min.py](scripts/ocr_min.py)（`onnxruntime` 1.26.0、`numpy`、`opencv-python-headless` だけ）で、同じ手順（縮小 → DB の後処理 → 並べ替え → 透視変換で切り出し → 6 行ずつ束ねて認識 → CTC）を書いた。Rust は `cargo run --release -- bench`（[main.rs](src/main.rs) `bench`）。
- 測り方: ページごとに 3 回慣らして 5 回の中央値。Det は縮小・正規化 → 推論 → 後処理、Rec は切り出し → 正規化 → 推論 → CTC。どちらもセッションの設定は同じ（`Level3`／`ORT_ENABLE_ALL`、intra 3 スレッド、inter 1、spinning 切り）。
- クラウドの CPU は実行ごとの揺れが大きい（同じ処理が ±30 % 変わる）ので、Rust と Python を交互に 3 回ずつ走らせ、ページごとに 3 回の中央値を取った（[scripts/bench.sh](scripts/bench.sh)、[scripts/bench_summary.py](scripts/bench_summary.py)）。
- ページ: `ja.jpg`（1536 × 839）と XFUND の val の 4 枚（300 dpi の A4、2480 × 3508 など）。検出は長辺 1600。

数字（small、ms、3 回の中央値）:

| ページ | 行（Rust / Py） | Det Rust | Det Py | Rec Rust | Rec Py | Rec Rust / Py |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| ja.jpg | 56 / 56 | 261 | 262 | 622 | 596 | 1.04 |
| ja_val_0 | 93 / 92 | 390 | 385 | 1831 | 1367 | 1.34 |
| ja_val_1 | 108 / 108 | 347 | 471 | 1584 | 2073 | 0.76 |
| ja_val_2 | 45 / 45 | 340 | 459 | 1219 | 1893 | 0.64 |
| ja_val_3 | 33 / 33 | 366 | 483 | 594 | 839 | 0.71 |
| 合計 | | 1704 | 2060 | 5851 | 6767 | **0.86** |

| 段階（5 ページの合計、ms） | Rust | Python |
| --- | ---: | ---: |
| det の前処理（縮小・正規化） | 157 | 200 |
| det の推論 | 1483 | 1826 |
| det の後処理 | 48 | 39 |
| 切り出し（透視変換） | 155 | 55 |
| rec の前処理 | 83 | 73 |
| rec の推論 | 5416 | 6336 |
| rec の CTC | 202 | 292 |

- **Rust の Rec は Python より遅くない**（合計で 0.86 倍、Det も 0.83 倍）。検証で見た「Rec が 2〜3 割遅い」は、自分で書いた前処理・後処理では起きなかった。ページごとの差（0.64〜1.34）はほとんどが推論の時間の揺れで、推論は同じ onnxruntime なので、言語の差ではない。検証の遅さは pure-onnx-ocr の tract の型からの変換や、テンソルの作り直しにあったと考えるのが自然（ここでは `Vec<f32>` を作って `Tensor::from_array` に渡し、出力は `try_extract_tensor` でコピーせずに読む）。
- 時間の 9 割は rec の推論。300 dpi の帳票は 1 ページ 2〜2.5 秒（small、3 スレッド）。検証の PC（P コア 6 スレッド）で約 0.9 秒だったのと比べ、この VM は 2〜3 倍遅い。
- Python と Rust の文字は、`ja.jpg` は完全に同じで、XFUND では 4〜7 行が違う。Python のスクリプトは unclip を「矩形を各辺に同じだけ広げる」と簡単にしていて（pyclipper を使わないため）、枠が 1 px 違う行で読みが変わる。Rust は項目 2 で PaddleOCR の出力と一致を確かめた方。

Rec を詰めた所:

| 試したこと | 結果 |
| --- | --- |
| CTC の argmax を `iter().max_by(total_cmp)` から 16 レーンの最大値探し（ベクトル化される）に | CTC の時間がおよそ半分（5 ページで約 400 → 202 ms） |
| 切り出しで、透視変換が整数の平行移動だけのとき（傾いていない行）は画素をそのまま写す | 結果は同じ（3 次補間の重みが [0, 1, 0, 0] になるため）。`ja.jpg` で 35 → 13 ms。スキャンは少し傾いていて一般の経路を通るので、XFUND では Python の 3 倍（cv2 は SIMD）が残る。全体の 2〜3 % |
| 束ねる行の数（5 ページの Rec の合計、2 回の中央値） | 1 行ずつ: 6137 ms・ピーク RSS 749 MiB／**6 行: 5325 ms・1115 MiB**／12 行: 6056 ms・1689 MiB |

- 束ねる数は PaddleOCR と同じ 6 が速い。12 にすると、幅の違う行が同じ束に入り、右の 0 埋めが増えて遅くなる。
- 幅の揃え方: 縦横比の順に並べてから束ねるので、0 埋めは小さい。32 の倍数に揃える工夫（pure-onnx-ocr にある）は、onnxruntime では計画のキャッシュがないので要らない。

メモリ:

- ピーク RSS（5 ページ）: Rust 1115 MiB、Python 1280 MiB。仮想メモリのピーク（Linux の worker は `RLIMIT_AS` で 2 GiB に制限している）は、6 行ずつで 1493 MiB、1 行ずつで 1093 MiB。`prlimit --as=2GiB` の下で 5 ページを通せた。
- メモリを食うのは rec の出力（束 × 時間 × 18,710 クラスの float）と途中の活性化。幅が 3200 の行が 6 本そろうと、出力だけで 6 × 400 × 18,710 × 4 B ≈ 180 MB になる。本番では束を「行の数」でなく「幅の合計」（例: 幅の合計 ≤ 6 × 1280）で切ると、幅の広い行の多い帳票でも上限を保てる。
- セッションの読み込み（det + rec、small）は 0.15 秒（ファイルがページキャッシュにある状態）。

## 項目 4: 精度（XFUND 日本語 val 50 枚）

できたこと:

- 正規化（NFKC、空白と改行の除去、`·` → `・`、`〜`・`～`・`~` を同じ字に）と編集距離を [cer.rs](src/cer.rs) に書いた。`cargo run --release -- xfund --model small` で 50 枚を数える（標準出力には数字だけを出す）。
- 正解の単位は XFUND の「塊」（`document[]` の各要素。キー・値・見出しなど。`box` と `text` を持ち、`words` に 1 文字ずつの箱がある）。

数字（small、長辺 1600）:

| 数え方 | CER |
| --- | --- |
| **文字ごとに対応付け**（予測の各文字の位置（項目 6 の CTC からの推定）の中心が入る正解の塊に配る。どこにも入らない文字は挿入） | **4.27 %**（1494 / 34956） |
| 行ごとに対応付け（予測の行を、その面積の半分以上が入る正解の塊に配る。どこにも入らない行は挿入） | 13.13 %（4590 / 34956） |

- 文字ごとの対応付けで **4.27 %** になり、検証の 4.31 %（small、1600、val 50 枚）とほぼ同じ。
- 1 ページの時間（この VM、3 スレッド、50 枚の中央値）: Det 377 ms、Rec 1330 ms。ピーク RSS 1278 MiB。

外れた理由を探したこと:

- 最初に書いた「行ごとに対応付け」では 13.13 % と大きく外れた。誤りの内訳を [scripts/xfund_breakdown.py](scripts/xfund_breakdown.py) で分けると、どの塊にも入らない行による挿入が 4.50 %、行の付かない塊の脱落が 5.08 %、付いた塊の中の誤りが 5.07 %。
- 「行の付かない塊」の文字 1777 字のうち 1622 字は、ほかの塊に配られた行に 30 % 以上覆われていた。帳票では「氏名　山田太郎」のようにキーと値が 1 行として検出されることが多く（2 つ以上の塊にまたがる行が 222 本）、行ごとに配ると片方の塊は脱落、もう片方は余分な文字として 2 回数えてしまう。本当に検出されなかったのは 155 字（0.44 %）だけだった。
- 検証の「対応付け CER」の細かい定義は分からないが、文字の位置で配る数え方で 4.31 % に近い値が出たので、前処理・後処理・モデルの読みは検証と同じと見てよい。
- アプリとしての含意: 帳票で「キーと値が 1 行になる」のは検出器の性質で、画面で文字を選ぶ・検索するときは困らない（文字ごとの位置があるため）。塊（項目 5）を作るときに、1 行を 2 つの塊に分けたいなら、文字の位置と行の中の大きな空白で切る必要がある。
