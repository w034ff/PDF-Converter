"""Median over rounds of the per-page medians written by scripts/bench.sh."""
import glob, json, statistics, sys
model = sys.argv[1] if len(sys.argv) > 1 else "small"
tag = sys.argv[2] if len(sys.argv) > 2 else ""
keys = ["det", "rec", "det_pre", "det_run", "det_post", "crop", "rec_pre", "rec_run", "rec_post"]
def load(lang):
    rows, peak, load_s = {}, [], []
    for f in sorted(glob.glob(f"out/bench_{lang}_{model}{tag}_r*.json")):
        d = json.load(open(f))
        peak.append(d["peak_rss_mib"]); load_s.append(d["load_s"])
        for p in d["pages"]:
            if lang == "py":
                m = {k: v * 1000 for k, v in p["median_s"].items()}
            else:
                m = {k: p[k] for k in keys[2:]}
                m["det"], m["rec"] = p["det_ms"], p["rec_ms"]
            m["lines"] = p["lines"]
            rows.setdefault(p["image"], []).append(m)
    return {k: {kk: statistics.median(r[kk] for r in v) for kk in keys + ["lines"]} for k, v in rows.items()}, max(peak), statistics.median(load_s), len(peak)
rs, rs_peak, rs_load, n = load("rs")
py, py_peak, py_load, _ = load("py")
print(f"rounds {n}; peak RSS Rust {rs_peak:.0f} MiB, Python {py_peak:.0f} MiB; load Rust {rs_load:.2f} s, Python {py_load:.2f} s")
print("| page | lines (Rust/Py) | Det Rust | Det Py | Rec Rust | Rec Py | Rec Rust/Py |")
print("| --- | --- | ---: | ---: | ---: | ---: | ---: |")
tot = {k: 0.0 for k in ["dr", "dp", "rr", "rp"]}
for img in rs:
    r, p = rs[img], py[img]
    tot["dr"] += r["det"]; tot["dp"] += p["det"]; tot["rr"] += r["rec"]; tot["rp"] += p["rec"]
    print(f"| {img} | {r['lines']:.0f} / {p['lines']:.0f} | {r['det']:.0f} | {p['det']:.0f} | {r['rec']:.0f} | {p['rec']:.0f} | {r['rec']/p['rec']:.2f} |")
print(f"| total | | {tot['dr']:.0f} | {tot['dp']:.0f} | {tot['rr']:.0f} | {tot['rp']:.0f} | {tot['rr']/tot['rp']:.2f} |")
print()
print("| stage (sum of pages, ms) | Rust | Python |")
print("| --- | ---: | ---: |")
for k in keys[2:]:
    print(f"| {k} | {sum(rs[i][k] for i in rs):.0f} | {sum(py[i][k] for i in py):.0f} |")
