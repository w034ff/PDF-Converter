"""Writes the OcrPage of a page turned by DEG degrees (as imageproc's
rotate_about_center turned the image: clockwise on screen for DEG > 0) from
the OcrPage of the upright page. Used where the OCR misreads the turned page
(fixtures/ocr_misread/), so that the text layer is tested on the true geometry.
usage: rotate_fixture.py SRC.json DEG DST.json"""
import json, math, sys

src, deg, dst = sys.argv[1], float(sys.argv[2]), sys.argv[3]
p = json.load(open(src))
cx, cy = p["widthPt"] / 2, p["heightPt"] / 2
c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))

def pt(q):
    x, y = q[0] - cx, q[1] - cy
    return [cx + x * c - y * s, cy + x * s + y * c]

def quad(q):
    return [pt(v) for v in q]

def bbox_of(q):
    xs, ys = [v[0] for v in q], [v[1] for v in q]
    return {"x0": min(xs), "y0": min(ys), "x1": max(xs), "y1": max(ys)}

def rect(r):
    return bbox_of(quad([[r["x0"], r["y0"]], [r["x1"], r["y0"]], [r["x1"], r["y1"]], [r["x0"], r["y1"]]]))

for b in p["blocks"]:
    b["bbox"] = rect(b["bbox"])
    for l in b["lines"]:
        l["quad"] = quad(l["quad"])
        l["bbox"] = bbox_of(l["quad"])
        for w in l["words"]:
            w["quad"] = quad(w["quad"])
            for ch in w["chars"]:
                ch["quad"] = quad(ch["quad"])
p["engine"] += f" (upright result turned by {deg:g} degrees)"
json.dump(p, open(dst, "w"), ensure_ascii=False, indent=2)
