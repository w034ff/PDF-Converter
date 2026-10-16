"""Builds the int8 layout model that scripts/fetch-layout.ts fetches (design §8.5).

The maintainer runs this once per model release; CI never does. The result is
uploaded to the GitHub Release `layout-model-N` of this repository, and its
SHA-256 goes into scripts/layout-version.json.

Input: `inference.onnx` of the Hugging Face repository
`PaddlePaddle/PP-DocLayout_plus-L_onnx` at commit
feb74619326f634e0e883218598096a3733ad9f7. The file is checked against
EXPECTED_INPUT_SHA256 before anything reads it.

Quantization: the `quant_pre_process` step (skip_symbolic_shape=True), then
dynamic quantization with UInt8 weights for every supported operator type.
The same onnxruntime version gives the same output file, so the Python package
must be onnxruntime 1.26.0 (the version pinned in scripts/layout-version.json):

    pip install onnxruntime==1.26.0 onnx

The input is a file from outside the repository, so run the script in a
container without a network. Build the image once (this step needs the
network), then run it from the repository root with `inference.onnx` in it:

    docker build -t layout-quantize - <<'EOF'
    FROM python:3.12-slim
    RUN pip install onnxruntime==1.26.0 onnx
    EOF

    docker run --rm --network none -v "$PWD:/work" -w /work layout-quantize \
      python scripts/quantize-layout-model.py \
        inference.onnx PP-DocLayout_plus-L-int8.onnx

Release steps:

1. Run the script and compare the printed SHA-256 with layout-version.json.
2. Create the release `layout-model-N` with the output file as its asset.
3. Write the new tag and the SHA-256 into scripts/layout-version.json.
"""

import hashlib
import json
import shutil
import sys
import tempfile
from pathlib import Path

EXPECTED_INPUT_SHA256 = (
    "77afb2caa74dd13240d087d2eced91d7fcd2caebd16006a0a66162fc8707ff0e"
)
VERSION_FILE = Path(__file__).resolve().parent / "layout-version.json"
READ_CHUNK_BYTES = 1024 * 1024


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        while chunk := file.read(READ_CHUNK_BYTES):
            digest.update(chunk)
    return digest.hexdigest()


def pinned_model_sha256() -> str | None:
    try:
        version = json.loads(VERSION_FILE.read_text(encoding="utf-8"))
        return version["model"]["sha256"]
    except (OSError, ValueError, KeyError, TypeError):
        return None


def main() -> int:
    if len(sys.argv) != 3:
        print(
            "Usage: quantize-layout-model.py <inference.onnx> <output.onnx>",
            file=sys.stderr,
        )
        return 2
    source, output = Path(sys.argv[1]), Path(sys.argv[2])

    actual = sha256_of(source)
    if actual != EXPECTED_INPUT_SHA256:
        print(
            f"SHA-256 mismatch for {source}: expected {EXPECTED_INPUT_SHA256}, "
            f"got {actual}. Nothing was written.",
            file=sys.stderr,
        )
        return 1

    # Imported only after the input is accepted, so a wrong file never reaches
    # onnxruntime.
    import onnxruntime
    from onnxruntime.quantization import QuantType, quantize_dynamic
    from onnxruntime.quantization.shape_inference import quant_pre_process

    print(f"onnxruntime {onnxruntime.__version__}")
    with tempfile.TemporaryDirectory() as work:
        pre = Path(work) / "pre.onnx"
        quantized = Path(work) / "quantized.onnx"
        quant_pre_process(str(source), str(pre), skip_symbolic_shape=True)
        quantize_dynamic(
            str(pre),
            str(quantized),
            weight_type=QuantType.QUInt8,
            op_types_to_quantize=None,
        )
        shutil.copyfile(quantized, output)

    produced = sha256_of(output)
    pinned = pinned_model_sha256()
    print(f"Wrote {output}")
    print(f"SHA-256 of the output: {produced}")
    if pinned is None:
        print(f"Could not read the model SHA-256 from {VERSION_FILE}")
    elif produced == pinned:
        print("Same as the SHA-256 in layout-version.json")
    else:
        # A different onnxruntime version gives a different file; the output
        # is kept so it can be looked at.
        print(f"Differs from the SHA-256 in layout-version.json ({pinned})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
