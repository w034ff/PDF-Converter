"""Writes {codepoint: vertical glyph id} from a font's GSUB 'vert' feature,
and {codepoint: glyph id} for the rest, so the Rust renderer can draw
vertical text without a shaping engine.  Needs fontTools (venv only)."""
import json, sys
from fontTools.ttLib import TTFont

font = TTFont(sys.argv[1])
cmap = font.getBestCmap()
order = font.getGlyphOrder()
gid = {name: i for i, name in enumerate(order)}
subst = {}
gsub = font["GSUB"].table
for fr in gsub.FeatureList.FeatureRecord:
    if fr.FeatureTag in ("vert", "vrt2"):
        for li in fr.Feature.LookupListIndex:
            for st in gsub.LookupList.Lookup[li].SubTable:
                st = getattr(st, "ExtSubTable", st)
                if hasattr(st, "mapping"):
                    subst.update(st.mapping)
out = {"vert": {}, "cmap": {}}
for cp, name in cmap.items():
    out["cmap"][cp] = gid[name]
    if name in subst:
        out["vert"][cp] = gid[subst[name]]
json.dump(out, open(sys.argv[2], "w"))
print(len(out["cmap"]), "glyphs,", len(out["vert"]), "vertical substitutions")
