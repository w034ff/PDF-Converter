"""Prepares the OCR model files that scripts/fetch-layout.ts fetches (design ocr §8).

The maintainer runs this once per model release; CI never does. The outputs are
uploaded to the GitHub Release `ocr-model-N` of this repository, and their
SHA-256 go into the `ocrModel` entry of scripts/layout-version.json.

Input: three files of PaddlePaddle's PP-OCRv6 small on Hugging Face, each
checked against EXPECTED_INPUT_SHA256 before anything reads it:

- `inference.onnx` of `PaddlePaddle/PP-OCRv6_small_det_onnx` at commit
  28fe5895c24fd108c19eb3e8479f4ab385fbfc62 (the detector);
- `inference.onnx` and `inference.yml` of `PaddlePaddle/PP-OCRv6_small_rec_onnx`
  at commit b8f84f0b80c529de40b4fbb3544b84fa7233a513 (the recogniser and its
  configuration, which holds the dictionary).

Output, in the output folder:

- `PP-OCRv6_small_det.onnx` and `PP-OCRv6_small_rec.onnx`: the two models,
  unchanged;
- `PP-OCRv6_small_rec_dict.txt`: the `PostProcess.character_dict` list of
  `inference.yml`, one entry per line in UTF-8 with LF line ends, so that the
  app does not read YAML. The CTC blank and the space that PaddleOCR adds
  around the list are not written; the app adds them.

The inputs are files from outside the repository, so run the script in a
container without a network. Build the image once (this step needs the
network), then run it from the repository root with the three inputs in it:

    docker build -t ocr-prepare - <<'EOF'
    FROM python:3.12-slim
    RUN pip install pyyaml==6.0.3
    EOF

    docker run --rm --network none --user "$(id -u):$(id -g)" \
      -v "$PWD:/work" -w /work ocr-prepare \
      python scripts/prepare-ocr-model.py \
        det/inference.onnx rec/inference.onnx rec/inference.yml ocr-model

Release steps:

1. Run the script and compare the printed SHA-256 with layout-version.json.
2. Create the release `ocr-model-N` with the three output files as its assets.
3. Write the new tag and the SHA-256 into scripts/layout-version.json.
"""

import hashlib
import json
import shutil
import sys
from pathlib import Path

EXPECTED_INPUT_SHA256 = {
    "det": "d73e0058b7a8086bbd57f3d10b8bcd4ff95363f67e06e2762b5e814fe9c9410e",
    "rec": "5435fd747c9e0efe15a96d0b378d5bd157e9492ed8fd80edf08f30d02fa24634",
    "rec_yml": "ab078671bb49f06228eadccd34f1bb501e157f7a047095ffb943ba81512c77d1",
}
DET_OUTPUT = "PP-OCRv6_small_det.onnx"
REC_OUTPUT = "PP-OCRv6_small_rec.onnx"
DICT_OUTPUT = "PP-OCRv6_small_rec_dict.txt"
VERSION_FILE = Path(__file__).resolve().parent / "layout-version.json"
READ_CHUNK_BYTES = 1024 * 1024


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        while chunk := file.read(READ_CHUNK_BYTES):
            digest.update(chunk)
    return digest.hexdigest()


def pinned_sha256() -> dict[str, str]:
    try:
        version = json.loads(VERSION_FILE.read_text(encoding="utf-8"))
        return {f["name"]: f["sha256"] for f in version["ocrModel"]["files"]}
    except (OSError, ValueError, KeyError, TypeError):
        return {}


def dictionary(yml: Path) -> list[str]:
    # Imported only after the input is accepted, so a wrong file never reaches
    # the YAML parser.
    import yaml

    config = yaml.safe_load(yml.read_text(encoding="utf-8"))
    entries = config["PostProcess"]["character_dict"]
    for entry in entries:
        # Every entry of this model is one character; a number or a boolean
        # would mean YAML read an entry as something else.
        if not isinstance(entry, str) or len(entry) != 1 or entry in "\r\n":
            raise ValueError(f"unexpected dictionary entry {entry!r}")
    return entries


def main() -> int:
    if len(sys.argv) != 5:
        print(
            "Usage: prepare-ocr-model.py <det inference.onnx> <rec inference.onnx> "
            "<rec inference.yml> <output folder>",
            file=sys.stderr,
        )
        return 2
    det, rec, rec_yml, output = (Path(a) for a in sys.argv[1:])

    for key, path in (("det", det), ("rec", rec), ("rec_yml", rec_yml)):
        actual = sha256_of(path)
        if actual != EXPECTED_INPUT_SHA256[key]:
            print(
                f"SHA-256 mismatch for {path}: expected {EXPECTED_INPUT_SHA256[key]}, "
                f"got {actual}. Nothing was written.",
                file=sys.stderr,
            )
            return 1

    entries = dictionary(rec_yml)
    output.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(det, output / DET_OUTPUT)
    shutil.copyfile(rec, output / REC_OUTPUT)
    with (output / DICT_OUTPUT).open("w", encoding="utf-8", newline="\n") as file:
        file.writelines(f"{entry}\n" for entry in entries)
    print(f"{len(entries)} dictionary entries")

    pinned = pinned_sha256()
    for name in (DET_OUTPUT, REC_OUTPUT, DICT_OUTPUT):
        produced = sha256_of(output / name)
        expected = pinned.get(name)
        if expected is None:
            note = "not in layout-version.json"
        elif expected == produced:
            note = "same as layout-version.json"
        else:
            note = f"differs from layout-version.json ({expected})"
        print(f"{name}: {produced} ({note})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
