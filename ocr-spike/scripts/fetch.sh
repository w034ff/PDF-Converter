#!/usr/bin/env bash
# Fetches everything the spike needs into git-ignored directories.
# onnxruntime comes from the app's own `npm run layout:fetch`.
set -euo pipefail
cd "$(dirname "$0")/.."
HF=https://huggingface.co/PaddlePaddle
for tier in small medium; do
  for kind in det rec; do
    mkdir -p "models/${tier}_${kind}"
    for f in inference.onnx inference.yml; do
      [[ -s "models/${tier}_${kind}/$f" ]] || curl -sSfL -o "models/${tier}_${kind}/$f" "$HF/PP-OCRv6_${tier}_${kind}_onnx/resolve/main/$f"
    done
  done
done
if [[ ! -d third_party/pure-onnx-ocr ]]; then
  git clone -q https://github.com/siska-tech/pure-onnx-ocr third_party/pure-onnx-ocr
  git -C third_party/pure-onnx-ocr checkout -q 59a6c2c
fi
mkdir -p data/ref data/xfund
[[ -s data/ref/ja.jpg ]] || curl -sSfL -o data/ref/ja.jpg https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/release/2.7/doc/imgs/japan_2.jpg
# general_ocr_002 is only on paddle-model-ecology.bj.bcebos.com, which the cloud sandbox denies.
curl -sSfL -o data/ref/general_ocr_002.jpg https://paddle-model-ecology.bj.bcebos.com/paddlex/imgs/demo_image/general_ocr_002.png || rm -f data/ref/general_ocr_002.jpg
X=https://github.com/doc-analysis/XFUND/releases/download/v1.0
[[ -s data/xfund/ja.val.json ]] || curl -sSfL -o data/xfund/ja.val.json $X/ja.val.json
[[ -d data/xfund/val ]] || { curl -sSfL -o data/xfund/ja.val.zip $X/ja.val.zip && mkdir -p data/xfund/val && (cd data/xfund/val && unzip -q ../ja.val.zip); }
echo done
