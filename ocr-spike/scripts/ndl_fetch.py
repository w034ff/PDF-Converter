"""Reads files out of NDL's pdmocrdataset-part1 line zip with HTTP range
requests (remotezip), without downloading the whole 4.8 GB archive.

  ndl_fetch.py json FOLDER...     every page JSON of these folders -> data/ndl/json/
  ndl_fetch.py img LIST           the images named in LIST (one json name per line) -> data/ndl/img/

Data: National Diet Library, pdmocrdataset-part1, Public Domain Mark 1.0.
"""
import os, sys
from remotezip import RemoteZip

URL = "https://lab.ndl.go.jp/dataset/pdm_ocr_dataset/line/tosho_all_linejson.zip"
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "data", "ndl")


def main():
    mode = sys.argv[1]
    with RemoteZip(URL) as z:
        names = z.namelist()
        if mode == "json":
            for folder in sys.argv[2:]:
                want = [n for n in names if n.endswith(".json") and n.split("/")[2] == folder]
                out = os.path.join(ROOT, "json", folder)
                os.makedirs(out, exist_ok=True)
                for n in want:
                    dst = os.path.join(out, os.path.basename(n))
                    if not os.path.exists(dst):
                        with open(dst, "wb") as f:
                            f.write(z.read(n))
                print(folder, len(want))
        elif mode == "img":
            for line in open(sys.argv[2], encoding="utf-8"):
                rel = line.strip()  # FOLDER/NAME.json
                if not rel:
                    continue
                folder, base = rel.split("/")
                n = f"tosho_all_linejson/img/{folder}/{base[:-5]}.jpg"
                out = os.path.join(ROOT, "img", folder)
                os.makedirs(out, exist_ok=True)
                dst = os.path.join(out, os.path.basename(n))
                if not os.path.exists(dst):
                    with open(dst, "wb") as f:
                        f.write(z.read(n))
                print(n)


main()
