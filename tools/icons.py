import re
import sys
from pathlib import Path

from fontTools.ttLib import TTFont

src = Path(sys.argv[1])
out = Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)

mat = TTFont(next((src / "assets").glob("material-symbols-rounded.*.woff2")))
mat.flavor = None
best = mat.getBestCmap()
cmap = {v: chr(k) for k, v in best.items()}
codes = {}
for k, v in best.items():
    if k >= 0xE000:
        codes.setdefault(v, k)
lines = []
for lookup in mat["GSUB"].table.LookupList.Lookup:
    for st in lookup.SubTable:
        if lookup.LookupType == 7:
            st = st.ExtSubTable
        if not hasattr(st, "ligatures"):
            continue
        for first, ligs in st.ligatures.items():
            for lig in ligs:
                name = "".join(cmap.get(g, "?") for g in [first] + lig.Component)
                if "?" not in name and lig.LigGlyph in codes:
                    lines.append(f"{name}	{codes[lig.LigGlyph]}")
from fontTools.varLib import instancer

if "fvar" in mat:
    axes = {a.axisTag: a.defaultValue for a in mat["fvar"].axes}
    mat = instancer.instantiateVariableFont(mat, axes)
mat.save(out / "material.ttf")
(out / "material.txt").write_text("\n".join(sorted(set(lines))), encoding="utf-8")
print("material:", len(set(lines)))

si = TTFont(next((src / "assets").glob("SimpleIcons.*.woff2")))
si.flavor = None
si.save(out / "simple.ttf")
js = (src / "menu_window" / "index.js").read_text(encoding="utf-8", errors="ignore")
simple = {}
for name, hexcode in re.findall(r"\.si-([a-z0-9_-]+)::before\s*\{\s*content:\s*\"\\+([0-9a-f]+)\"", js):
    simple.setdefault(name, int(hexcode, 16))
brand = {}
for name, color in re.findall(r"\.si-([a-z0-9_-]+)\.si--color::before\s*\{\s*color:\s*(#[0-9a-fA-F]{6})", js):
    brand.setdefault(name, color)
rows = [f"{n}\t{c}\t{brand[n]}" if n in brand else f"{n}\t{c}" for n, c in sorted(simple.items())]
(out / "simple.txt").write_text("\n".join(rows), encoding="utf-8")
print("simple icons:", len(simple))
