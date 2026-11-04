"""Minimal PP-OCRv6 pipeline in Python (onnxruntime + numpy + opencv only).

Copies the PaddleOCR 3.x / PaddleX 3.7 steps that the Rust spike implements,
to compare speed and memory in the same container (item 3).

    python -I ocr_min.py --model small --threads 3 IMAGE... [--json out.json]
"""

import argparse
import json
import math
import statistics
import sys
import time
from pathlib import Path

import cv2
import numpy as np
import onnxruntime as ort

ROOT = Path(__file__).resolve().parent.parent
MEAN = np.array([0.485, 0.456, 0.406], np.float32)
STD = np.array([0.229, 0.224, 0.225], np.float32)


def session(path, threads):
    so = ort.SessionOptions()
    so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    so.intra_op_num_threads = threads
    so.inter_op_num_threads = 1
    so.add_session_config_entry("session.intra_op.allow_spinning", "0")
    return ort.InferenceSession(str(path), so, providers=["CPUExecutionProvider"])


def load_dict(yml):
    # The yml's character_dict is a plain YAML list; read it without PyYAML.
    chars, on = [], False
    import ast
    for line in Path(yml).read_text(encoding="utf-8").splitlines():
        if line.startswith("  character_dict:"):
            on = True
            continue
        if on:
            if not line.startswith("  - "):
                break
            v = line[4:]
            if v.startswith("'"):
                v = v[1:-1].replace("''", "'")
            elif v.startswith('"'):
                v = ast.literal_eval(v)
            chars.append(v)
    return [""] + chars + [" "]


def det_resize(img, limit, limit_type="max", max_side=4000):
    h, w = img.shape[:2]
    if limit_type == "max":
        ratio = limit / max(h, w) if max(h, w) > limit else 1.0
    else:
        ratio = limit / min(h, w) if min(h, w) < limit else 1.0
    rh, rw = int(h * ratio), int(w * ratio)
    if max(rh, rw) > max_side:
        r = max_side / max(rh, rw)
        rh, rw = int(rh * r), int(rw * r)
    rh = max(int(round(rh / 32) * 32), 32)
    rw = max(int(round(rw / 32) * 32), 32)
    if (rh, rw) != (h, w):
        img = cv2.resize(img, (rw, rh))
    return img


def mini_box(contour):
    rect = cv2.minAreaRect(contour)
    pts = sorted(list(cv2.boxPoints(rect)), key=lambda x: x[0])
    a, d = (0, 1) if pts[1][1] > pts[0][1] else (1, 0)
    b, c = (2, 3) if pts[3][1] > pts[2][1] else (3, 2)
    return np.array([pts[a], pts[b], pts[c], pts[d]]), min(rect[1])


def box_score(pred, box):
    h, w = pred.shape
    xmin = int(np.clip(np.floor(box[:, 0].min()), 0, w - 1))
    xmax = int(np.clip(np.ceil(box[:, 0].max()), 0, w - 1))
    ymin = int(np.clip(np.floor(box[:, 1].min()), 0, h - 1))
    ymax = int(np.clip(np.ceil(box[:, 1].max()), 0, h - 1))
    mask = np.zeros((ymax - ymin + 1, xmax - xmin + 1), np.uint8)
    b = box.copy()
    b[:, 0] -= xmin
    b[:, 1] -= ymin
    cv2.fillPoly(mask, b.reshape(1, -1, 2).astype(np.int32), 1)
    return cv2.mean(pred[ymin : ymax + 1, xmin : xmax + 1], mask)[0]


def unclip(box, ratio):
    # Offsetting a rectangle with round joins and taking the min-area rect of
    # the result is the rectangle grown by `distance` on every side.
    area = cv2.contourArea(box.astype(np.float32))
    length = cv2.arcLength(box.astype(np.float32), True)
    d = area * ratio / length
    (cx, cy), (w, h), ang = cv2.minAreaRect(box.astype(np.float32))
    return cv2.boxPoints(((cx, cy), (w + 2 * d, h + 2 * d), ang))


def db_post(pred, dw, dh, thresh=0.3, box_thresh=0.6, ratio=1.5, max_cand=3000):
    ph, pw = pred.shape
    bitmap = (pred > thresh).astype(np.uint8) * 255
    contours, _ = cv2.findContours(bitmap, cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE)
    out = []
    for c in contours[:max_cand]:
        pts, s = mini_box(c)
        if s < 3:
            continue
        if box_thresh > box_score(pred, pts.reshape(-1, 2)):
            continue
        box, s = mini_box(unclip(pts, ratio).reshape(-1, 1, 2))
        if s < 5:
            continue
        box[:, 0] = np.clip(np.round(box[:, 0] * dw / pw), 0, dw)
        box[:, 1] = np.clip(np.round(box[:, 1] * dh / ph), 0, dh)
        out.append(box.astype(np.int16))
    return out


def sort_boxes(boxes):
    b = sorted(boxes, key=lambda x: (x[0][1], x[0][0]))
    for i in range(len(b) - 1):
        for j in range(i, -1, -1):
            if abs(b[j + 1][0][1] - b[j][0][1]) < 10 and b[j + 1][0][0] < b[j][0][0]:
                b[j], b[j + 1] = b[j + 1], b[j]
            else:
                break
    return b


def crop(img, box):
    q, _ = mini_box(np.array(box).astype(np.int32))
    w = int(max(np.linalg.norm(q[0] - q[1]), np.linalg.norm(q[2] - q[3])))
    h = int(max(np.linalg.norm(q[0] - q[3]), np.linalg.norm(q[1] - q[2])))
    m = cv2.getPerspectiveTransform(q.astype(np.float32), np.float32([[0, 0], [w, 0], [w, h], [0, h]]))
    dst = cv2.warpPerspective(img, m, (w, h), borderMode=cv2.BORDER_REPLICATE, flags=cv2.INTER_CUBIC)
    if dst.shape[0] / max(dst.shape[1], 1) >= 1.5:
        dst = np.rot90(dst)
    return dst


def rec_norm(img):
    h, w = img.shape[:2]
    max_ratio = max(320 / 48, w / h)
    cw = int(48 * max_ratio)
    if cw > 3200:
        r = cv2.resize(img, (3200, 48))
        rw = cw = 3200
    else:
        rw = cw if math.ceil(48 * w / h) > cw else int(math.ceil(48 * w / h))
        r = cv2.resize(img, (rw, 48))
    r = (r.astype(np.float32).transpose(2, 0, 1) / 255 - 0.5) / 0.5
    out = np.zeros((3, 48, cw), np.float32)
    out[:, :, :rw] = r
    return out


class Ocr:
    def __init__(self, model, threads, limit=1600, limit_type="max", batch=6):
        d = ROOT / "models"
        self.det = session(d / f"{model}_det" / "inference.onnx", threads)
        self.rec = session(d / f"{model}_rec" / "inference.onnx", threads)
        self.chars = load_dict(d / f"{model}_rec" / "inference.yml")
        self.limit, self.limit_type, self.batch = limit, limit_type, batch

    def detect(self, img, t):
        s = time.perf_counter()
        r = det_resize(img, self.limit, self.limit_type)
        x = ((r.astype(np.float32) / 255 - MEAN) / STD).transpose(2, 0, 1)[None]
        x = np.ascontiguousarray(x)
        t["det_pre"] += time.perf_counter() - s
        s = time.perf_counter()
        pred = self.det.run(None, {self.det.get_inputs()[0].name: x})[0][0, 0]
        t["det_run"] += time.perf_counter() - s
        s = time.perf_counter()
        boxes = sort_boxes(db_post(pred, img.shape[1], img.shape[0]))
        t["det_post"] += time.perf_counter() - s
        return boxes

    def run(self, img, t):
        boxes = self.detect(img, t)
        s = time.perf_counter()
        crops = [crop(img, b) for b in boxes]
        t["crop"] += time.perf_counter() - s
        order = sorted(range(len(crops)), key=lambda i: crops[i].shape[1] / crops[i].shape[0])
        res = [None] * len(crops)
        for k in range(0, len(order), self.batch):
            idx = order[k : k + self.batch]
            s = time.perf_counter()
            normed = [rec_norm(crops[i]) for i in idx]
            mw = max(n.shape[2] for n in normed)
            x = np.zeros((len(idx), 3, 48, mw), np.float32)
            for j, n in enumerate(normed):
                x[j, :, :, : n.shape[2]] = n
            t["rec_pre"] += time.perf_counter() - s
            s = time.perf_counter()
            probs = self.rec.run(None, {self.rec.get_inputs()[0].name: x})[0]
            t["rec_run"] += time.perf_counter() - s
            s = time.perf_counter()
            ids, ps = probs.argmax(2), probs.max(2)
            for j, i in enumerate(idx):
                sel = np.ones(ids.shape[1], bool)
                sel[1:] = ids[j][1:] != ids[j][:-1]
                sel &= ids[j] != 0
                text = "".join(self.chars[c] for c in ids[j][sel])
                conf = float(ps[j][sel].mean()) if sel.any() else 0.0
                res[i] = (text, conf)
            t["rec_post"] += time.perf_counter() - s
        return [(b.tolist(), *r) for b, r in zip(boxes, res)]


def peak_rss_mib():
    for line in Path("/proc/self/status").read_text().splitlines():
        if line.startswith("VmHWM:"):
            return int(line.split()[1]) / 1024
    return 0.0


KEYS = ["det_pre", "det_run", "det_post", "crop", "rec_pre", "rec_run", "rec_post"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("images", nargs="+")
    ap.add_argument("--model", default="small")
    ap.add_argument("--threads", type=int, default=3)
    ap.add_argument("--limit", type=int, default=1600)
    ap.add_argument("--batch", type=int, default=6)
    ap.add_argument("--warmup", type=int, default=3)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--json")
    a = ap.parse_args()
    s = time.perf_counter()
    ocr = Ocr(a.model, a.threads, a.limit, batch=a.batch)
    load_s = time.perf_counter() - s
    report = {"load_s": load_s, "pages": []}
    for path in a.images:
        img = cv2.imread(path, cv2.IMREAD_COLOR)
        runs = []
        for i in range(a.warmup + a.runs):
            t = dict.fromkeys(KEYS, 0.0)
            lines = ocr.run(img, t)
            if i >= a.warmup:
                runs.append(t)
        med = {k: statistics.median(r[k] for r in runs) for k in KEYS}
        med["det"] = statistics.median(r["det_pre"] + r["det_run"] + r["det_post"] for r in runs)
        med["rec"] = statistics.median(sum(r[k] for k in KEYS[3:]) for r in runs)
        report["pages"].append({"image": Path(path).name, "lines": len(lines), "median_s": med,
                                "texts": [l[1] for l in lines]})
        print(f"{Path(path).name}: lines {len(lines)} det {med['det']*1000:.0f} ms rec {med['rec']*1000:.0f} ms "
              + " ".join(f"{k} {med[k]*1000:.1f}" for k in KEYS), file=sys.stderr)
    report["peak_rss_mib"] = peak_rss_mib()
    print(f"load {load_s:.2f} s, peak RSS {report['peak_rss_mib']:.0f} MiB", file=sys.stderr)
    if a.json:
        Path(a.json).write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding="utf-8")


if __name__ == "__main__":
    main()
