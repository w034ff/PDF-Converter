"""Reads a two-column horizontal page by columns.

ocr-spike's reading order (order.rs rule 4) reads a band of horizontal blocks
row by row across the band, which suits forms but interleaves the two columns
of body text. For the fixture, blocks between `first` and `last` (inclusive,
0-based) are split at the page's middle into a left block and a right block.

    python regroup.py <in.json> <out.json> <first> <last>
"""
import json, sys

src, dst, first, last = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
page = json.load(open(src))
mid = page["widthPt"] / 2
band = [l for b in page["blocks"][first : last + 1] for l in b["lines"]]


def block(lines):
    return {
        "bbox": {
            "x0": min(l["bbox"]["x0"] for l in lines),
            "y0": min(l["bbox"]["y0"] for l in lines),
            "x1": max(l["bbox"]["x1"] for l in lines),
            "y1": max(l["bbox"]["y1"] for l in lines),
        },
        "orientation": "horizontal",
        "lines": sorted(lines, key=lambda l: l["bbox"]["y0"]),
    }


left = [l for l in band if l["bbox"]["x0"] < mid]
right = [l for l in band if l["bbox"]["x0"] >= mid]
page["blocks"][first : last + 1] = [block(left), block(right)]
json.dump(page, open(dst, "w"), ensure_ascii=False, indent=1)
