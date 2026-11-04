"""Splits the XFUND matched-CER errors by cause (prints numbers only)."""
import json, sys, unicodedata
gt = json.load(open("data/xfund/ja.val.json"))["documents"]
pred = {p["fname"]: p["lines"] for p in json.load(open(sys.argv[1]))}

def norm(s):
    s = unicodedata.normalize("NFKC", s)
    s = "".join(c for c in s if not c.isspace())
    return s.replace("·", "・").replace("〜", "~").replace("～", "~")

def lev(a, b):
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a):
        cur = [i + 1]
        for j, cb in enumerate(b):
            cur.append(min(prev[j] + (ca != cb), prev[j + 1] + 1, cur[j] + 1))
        prev = cur
    return prev[-1]

def bounds(q):
    xs = [p[0] for p in q]; ys = [p[1] for p in q]
    return [min(xs), min(ys), max(xs), max(ys)]

def inter(a, b):
    return max(0, min(a[2], b[2]) - max(a[0], b[0])) * max(0, min(a[3], b[3]) - max(a[1], b[1]))

tot = dict(chars=0, unassigned_ins=0, missed_del=0, matched_edits=0, multi_gt=0, multi_gt_edits=0, empty_gt_chars=0)
alt = dict(edits=0, chars=0)
for doc in gt:
    ents = doc["document"]; lines = pred[doc["img"]["fname"]]
    eb = [e["box"] for e in ents]
    assigned = {i: [] for i in range(len(ents))}
    for l in lines:
        b = bounds(l["quad"]); area = max((b[2]-b[0])*(b[3]-b[1]), 1e-9)
        covers = [inter(e, b) / area for e in eb]
        best = max(range(len(ents)), key=lambda i: covers[i]) if ents else None
        # A line spread over several boxes (key and value read as one line).
        if sum(1 for c in covers if c > 0.15) > 1:
            tot["multi_gt"] += 1
        if best is not None and covers[best] >= 0.5:
            assigned[best].append(l)
        else:
            tot["unassigned_ins"] += len(norm(l["text"]))
    for i, e in enumerate(ents):
        g = norm(e["text"]); tot["chars"] += len(g)
        ls = sorted(assigned[i], key=lambda l: (bounds(l["quad"])[1], bounds(l["quad"])[0]))
        p = norm("".join(l["text"] for l in ls))
        if not ls:
            tot["missed_del"] += len(g)
        else:
            tot["matched_edits"] += lev(g, p)
    # Alternative: each GT box takes the characters of every line whose
    # centre lies in it, so a merged line is not double-counted.
    for i, e in enumerate(ents):
        g = norm(e["text"]); b = e["box"]
        ls = [l for l in lines if b[0] <= (bounds(l["quad"])[0]+bounds(l["quad"])[2])/2 <= b[2] and b[1] <= (bounds(l["quad"])[1]+bounds(l["quad"])[3])/2 <= b[3]]
        ls.sort(key=lambda l: (bounds(l["quad"])[1], bounds(l["quad"])[0]))
        alt["edits"] += lev(g, norm("".join(l["text"] for l in ls))); alt["chars"] += len(g)
c = tot["chars"]
print(f"GT chars {c}")
print(f"insertions from lines outside any box: {tot['unassigned_ins']} ({tot['unassigned_ins']/c*100:.2f} % of GT chars)")
print(f"deletions of boxes with no line: {tot['missed_del']} ({tot['missed_del']/c*100:.2f} %)")
print(f"edits in matched boxes: {tot['matched_edits']} ({tot['matched_edits']/c*100:.2f} %)")
print(f"lines touching more than one box: {tot['multi_gt']}")
print(f"variant: centre-in-box, unmatched lines ignored: CER {alt['edits']/alt['chars']*100:.2f} %")

# Of the boxes with no assigned line: is some line overlapping them anyway?
over, none = 0, 0
for doc in gt:
    ents = doc["document"]; lines = pred[doc["img"]["fname"]]
    eb = [e["box"] for e in ents]
    assigned = set()
    for l in lines:
        b = bounds(l["quad"]); area = max((b[2]-b[0])*(b[3]-b[1]), 1e-9)
        covers = [inter(e, b) / area for e in eb]
        if ents:
            best = max(range(len(ents)), key=lambda i: covers[i])
            if covers[best] >= 0.5: assigned.add(best)
    for i, e in enumerate(ents):
        if i in assigned: continue
        g = len(norm(e["text"]))
        if any(inter(e["box"], bounds(l["quad"])) > 0.3 * max(1, (e["box"][2]-e["box"][0])*(e["box"][3]-e["box"][1])) for l in lines): over += g
        else: none += g
print(f"chars of unassigned boxes overlapped >30 % by some line: {over}; not overlapped: {none}")
