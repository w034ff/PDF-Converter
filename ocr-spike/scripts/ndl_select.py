"""Picks the NDL validation pages without looking at any OCR output:
from each Showa folder (1930s, 1940s; bunkei and rikei), the pages whose
lines are at least 80 % vertical and that hold at least 200 characters,
ordered by the SHA-1 of the file name, first 5 of each folder.
Writes sets/ndl_pages.txt (FOLDER/NAME.json per line)."""
import glob, hashlib, json, os

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
FOLDERS = ["tosho_1930_bunkei", "tosho_1930_rikei", "tosho_1940_bunkei", "tosho_1940_rikei"]
PER_FOLDER = 5

out = []
for folder in FOLDERS:
    cand = []
    for p in glob.glob(os.path.join(ROOT, "data/ndl/json", folder, "*.json")):
        b = json.load(open(p, encoding="utf-8"))["boxes"]
        if not b:
            continue
        vertical = sum(x["orientation"] == "vertical" for x in b) / len(b)
        chars = sum(len(x["text"]) for x in b)
        if vertical >= 0.8 and chars >= 200:
            name = os.path.basename(p)
            cand.append((hashlib.sha1(name.encode()).hexdigest(), name))
    cand.sort()
    out += [f"{folder}/{n}" for _, n in cand[:PER_FOLDER]]
with open(os.path.join(ROOT, "sets/ndl_pages.txt"), "w", encoding="utf-8") as f:
    f.write("\n".join(out) + "\n")
print(len(out))
