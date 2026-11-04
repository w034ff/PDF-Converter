# OCR の Rust の試作 — 報告

状態: **項目 1 で中断**。PP-OCRv6 のモデルを Hugging Face から取れなかった（ネットの設定で拒否）。指示どおり、ここで止めた。

## 入れ物

- CPU: Intel(R) Xeon(R) Processor @ 2.10GHz、4 コア（1 コア 1 スレッド）、メモリ 15 GB。
- Node v22.23.4 / npm 10.9.9（リポジトリの `engines` は `node >= 24.15.0`）。

## 項目 1: 骨組みと取得

| 取得するもの | 結果 |
| --- | --- |
| `npm ci` | 素のままでは `EBADENGINE`（Node が 22 で、`>=24.15.0` に足りない）で失敗。`npm ci --engine-strict=false` で入った（`node_modules/@ibm/plex-sans-jp` あり） |
| `npm run layout:fetch`（onnxruntime 1.26.0） | 成功。`src-tauri/onnxruntime/linux/libonnxruntime.so` |
| `PaddlePaddle/PP-OCRv6_small_det_onnx`・`_rec_onnx`（`inference.onnx`・`inference.yml`） | **失敗**。`huggingface.co:443` への接続を入れ物の出口のプロキシが 403 で拒否（ポリシーによる拒否）。4 ファイルとも同じ |
| `siska-tech/pure-onnx-ocr` を `59a6c2c` で | 成功。`tests/reference/` に `v6-small__ja.json`・`v6-small__general_ocr_002.json` など 8 件（PaddleOCR 3.7.0、engine=onnxruntime） |
| 照合の画像（`general_ocr_002`・`ja.jpg`） | 未取得。pure-onnx-ocr の `scripts/fetch_fixtures.sh` は `paddle-model-ecology.bj.bcebos.com` と `raw.githubusercontent.com` から取る。モデルがないので取りに行っていない |
| XFUND（`github.com/doc-analysis/XFUND/releases/...`） | 到達できる（302 で S3 に転送）。未取得 |
| 青空文庫（`www.aozora.gr.jp`） | **拒否**（同じく 403）。項目 5 では GitHub の `aozorabunko/aozorabunko` のような写しを使う必要がある |

突き当たったこと:

- `huggingface.co` が許されていない。モデルは Hugging Face にしかない（pure-onnx-ocr の取得スクリプトも `huggingface.co/PaddlePaddle` から取る）ため、項目 2 以降に進めない。ほかの写し（ModelScope など）から取るのはネットの制限を迂回することになるので、していない。
- 照合の参照の `det_params` は `limit_type=min`、`limit_side_len=64`（PaddleOCR の既定）で作られている。項目 2 の照合は、この設定で前処理を動かし、試験の後で `max`・1600 に切り替える必要がある（検証の設定と参照の設定が違う点に注意）。

## 再開するには

1. 環境の Network access で `huggingface.co`（と、Hugging Face が大きいファイルを転送する先の `cdn-lfs.huggingface.co`、`cdn-lfs-us-1.hf.co`、`*.hf.co` など）を許す。青空文庫を使うなら `www.aozora.gr.jp` も。手順: https://code.claude.com/docs/en/cloud-environments#network-access
2. あるいは、モデルの 4 ファイル（medium も使うなら 8 ファイル）を `layout:fetch` と同じく GitHub のリリースに置けば、今の設定のまま取れる（GitHub のリリースは届くことを確かめた）。

## まとめ

項目 2 以降を行っていないので、本番の作り・残る弱点・手間の見積もりはまだ書けない。
