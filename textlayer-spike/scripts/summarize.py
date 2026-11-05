"""One row per '### ' section of a table file: character-weighted means over
the pages (horizontal-only pages for the horizontal columns)."""
import re, sys
rows, cur = {}, None
for line in open(sys.argv[1]):
    if line.startswith("### "):
        cur = line[4:].strip(); rows[cur] = []
    elif cur and line.startswith("| ") and not line.startswith("| ページ") and not line.startswith("| ---"):
        c = [x.strip() for x in line.strip().strip("|").split("|")]
        num = lambda s: float(s.split()[0]) if s[0].isdigit() else None
        rows[cur].append(dict(name=c[0], n=int(c[1]), text=num(c[2]), pt=num(c[3]), centre=num(c[4]),
                              iou=float(c[5]) if c[5] != "NaN" else None, found=num(c[6]), spaces=int(c[9])))
print("| 書き方 | 全文 pdfium | 全文 pdftotext | 箱の中心 横書き | 箱の中心 縦書き・混在 | IoU 横 | IoU 縦・混在 | 検索 | 余計な空白（計） |")
print("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |")
def wmean(rs, k):
    rs = [r for r in rs if r[k] is not None]
    return sum(r[k] * r["n"] for r in rs) / max(1, sum(r["n"] for r in rs))
for k, rs in rows.items():
    h = [r for r in rs if r["name"].startswith(("h1", "ja"))]
    v = [r for r in rs if not r["name"].startswith(("h1", "ja"))]
    print(f"| {k} | {wmean(rs,'text'):.1f} % | {wmean(rs,'pt'):.1f} % | {wmean(h,'centre'):.1f} % | {wmean(v,'centre'):.1f} % | "
          f"{wmean(h,'iou'):.2f} | {wmean(v,'iou'):.2f} | {wmean(rs,'found'):.1f} % | {sum(r['spaces'] for r in rs)} |")
