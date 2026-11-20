"""The prop catalog: one searchable list of every prop a map can place.

    python3 tools/prop_catalog.py --write          # rebuild catalog.json
    python3 tools/prop_catalog.py                  # check it (CI does this)
    python3 tools/prop_catalog.py --find street crate   # props with all these tags

Reads the table in tools/prop_library.py and the built glTFs in
godot/assets/props, and writes godot/assets/props/catalog.json: for each
prop its scene path, size in metres (Godot axes: x wide, y tall, z deep;
the origin is the bottom centre), triangle count, tags, kit, and its Poly
Haven source and authors. Size and triangles come from the glTF's own JSON
(accessor bounds and index counts), so it needs neither Blender nor the
LFS .bin files. `--write` asks the Poly Haven API for the authors of new
sources (generic user agent, nothing else sent) and keeps the ones it knows.

The check fails when a prop in the table has no glTF, the catalog is out of
date with the table or the files, or a source is not credited in
godot/assets/CREDITS.md.
"""

from __future__ import annotations

import json
import sys
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from prop_library import PROPS, folder, res_path

REPO = Path(__file__).resolve().parent.parent
PROPS_DIR = REPO / "godot/assets/props"
CATALOG = PROPS_DIR / "catalog.json"
CREDITS = REPO / "godot/assets/CREDITS.md"
USER_AGENT = "shooter-asset-pipeline"


def gltf_stats(path: Path) -> tuple[list[float], int]:
    """The model's size (x, y, z in metres) and triangle count, from its JSON."""
    data = json.loads(path.read_text())
    accessors = data["accessors"]
    lo = [float("inf")] * 3
    hi = [float("-inf")] * 3
    tris = 0
    for mesh in data.get("meshes", []):
        for prim in mesh["primitives"]:
            position = accessors[prim["attributes"]["POSITION"]]
            for i in range(3):
                lo[i] = min(lo[i], position["min"][i])
                hi[i] = max(hi[i], position["max"][i])
            if "indices" in prim:
                tris += accessors[prim["indices"]]["count"] // 3
            else:
                tris += position["count"] // 3
    return [round(hi[i] - lo[i], 3) for i in range(3)], tris


def authors(asset: str, known: dict[str, list[str]]) -> list[str]:
    if asset in known:
        return known[asset]
    request = urllib.request.Request(
        f"https://api.polyhaven.com/info/{asset}", headers={"User-Agent": USER_AGENT}
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        info = json.load(response)
    known[asset] = sorted(info.get("authors", {}))
    return known[asset]


def entry(name: str, by: list[str]) -> dict:
    prop = PROPS[name]
    size, tris = gltf_stats(PROPS_DIR / folder(name) / f"{name}.gltf")
    return {
        "path": res_path(name),
        "size": size,
        "triangles": tris,
        "tags": list(prop.tags),
        "kit": prop.kit or None,
        "source": f"https://polyhaven.com/a/{prop.asset}",
        "asset": prop.asset,
        "authors": by,
        "license": "CC0",
    }


def load() -> dict:
    return json.loads(CATALOG.read_text()) if CATALOG.exists() else {"props": {}}


def write() -> None:
    old = load()["props"]
    known = {e["asset"]: e["authors"] for e in old.values()}
    props = {name: entry(name, authors(PROPS[name].asset, known)) for name in sorted(PROPS)}
    catalog = {
        "about": "Every prop in godot/assets/props. Written by tools/prop_catalog.py --write "
        "from tools/prop_library.py; size is metres (x wide, y tall, z deep) about a "
        "bottom-centre origin.",
        "props": props,
    }
    CATALOG.write_text(json.dumps(catalog, indent=1, sort_keys=False) + "\n")
    print(f"wrote {CATALOG.relative_to(REPO)} ({len(props)} props)")


def problems() -> list[str]:
    found = []
    missing = [n for n in PROPS if not (PROPS_DIR / folder(n) / f"{n}.gltf").exists()]
    found += [f"{n}: no {res_path(n)}; run tools/build_props.py -- {n}" for n in missing]
    catalog = load()["props"]
    if sorted(catalog) != sorted(PROPS):
        extra = sorted(set(catalog) - set(PROPS))
        absent = sorted(set(PROPS) - set(catalog))
        found.append(f"catalog.json out of date (not listed: {absent}, not in the table: {extra})")
    for name in sorted(set(catalog) & set(PROPS) - set(missing)):
        if catalog[name] != entry(name, catalog[name]["authors"]):
            found.append(f"{name}: catalog.json entry out of date")
    credits = CREDITS.read_text()
    for asset in sorted({p.asset for p in PROPS.values()}):
        if f"`{asset}`" not in credits:
            found.append(f"{asset}: not credited in godot/assets/CREDITS.md")
    if found:
        found.append("fix: python3 tools/prop_catalog.py --write (and credit new sources)")
    return found


def find(tags: list[str]) -> None:
    for name, e in sorted(load()["props"].items()):
        if all(t in e["tags"] or t == e["kit"] for t in tags):
            x, y, z = e["size"]
            print(f"{name:26} {x:5.2f} x {y:5.2f} x {z:5.2f} m  {e['triangles']:5} tris  "
                  f"{','.join(e['tags'])}  {e['path']}")


def main() -> int:
    args = sys.argv[1:]
    if args[:1] == ["--write"]:
        write()
        return 0
    if args[:1] == ["--find"]:
        find(args[1:])
        return 0
    found = problems()
    for line in found:
        print(line)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
