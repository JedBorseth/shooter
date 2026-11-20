"""Write, search, or validate the decal catalog with no third-party dependencies."""

import argparse
import json
from pathlib import Path

from decal_library import DECALS

REPO = Path(__file__).resolve().parent.parent
DECAL_DIR = REPO / "godot/assets/textures/decals"
CATALOG = DECAL_DIR / "catalog.json"
CREDITS = REPO / "godot/assets/CREDITS.md"


def catalog_data() -> dict:
    entries = {}
    for name, decal in sorted(DECALS.items()):
        maps = ("albedo", "normal", "orm") if decal.orm else ("albedo", "normal")
        entries[name] = {
            "textures": {channel: f"res://assets/textures/decals/{name}_{channel}.png" for channel in maps},
            "size": list(decal.size),
            "tags": list(decal.tags),
            "source": (f"https://ambientcg.com/a/{decal.asset}" if decal.asset else
                       "https://github.com/JedBorseth/shooter/blob/main/tools/build_decals.py"),
            "asset": decal.asset,
            "authors": ["Lennart Demes" if decal.asset else "This project"],
            "license": "CC0",
        }
    return {
        "about": "Written by tools/decal_catalog.py --write from tools/decal_library.py. "
                 "Sizes are suggested projection footprints in metres (image width by height).",
        "decals": entries,
    }


def load() -> dict:
    return json.loads(CATALOG.read_text())


def write() -> None:
    CATALOG.write_text(json.dumps(catalog_data(), indent=1) + "\n")
    print(f"wrote {CATALOG.relative_to(REPO)} ({len(DECALS)} decals)")


def problems() -> list[str]:
    expected = catalog_data()
    found = []
    try:
        if load() != expected:
            found.append("catalog.json is stale; run python3 tools/decal_catalog.py --write")
    except (FileNotFoundError, ValueError) as error:
        found.append(f"catalog.json could not be read: {error}")
    credits = CREDITS.read_text()
    expected_files = set()
    for name, entry in expected["decals"].items():
        for path in entry["textures"].values():
            filename = path.rsplit("/", 1)[-1]
            expected_files.add(filename)
            for required in (filename, f"{filename}.import"):
                file = DECAL_DIR / required
                if not file.is_file() or file.stat().st_size == 0:
                    found.append(f"{name}: missing {required}")
        credit = entry["asset"] or name
        if f"`{credit}`" not in credits:
            found.append(f"{credit}: missing from godot/assets/CREDITS.md")
    extra = {file.name for file in DECAL_DIR.glob("*.png")} - expected_files
    found.extend(f"{filename}: texture missing from the decal table" for filename in sorted(extra))
    return found


def find(tags: list[str]) -> None:
    for name, entry in sorted(load()["decals"].items()):
        if all(tag in entry["tags"] for tag in tags):
            width, height = entry["size"]
            print(f"{name:26} {width:5.2f} x {height:5.2f} m  "
                  f"{','.join(entry['tags'])}  {entry['textures']['albedo']}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--write", action="store_true", help="regenerate catalog.json")
    action.add_argument("--find", nargs="*", metavar="tag", help="require all given tags")
    args = parser.parse_args()
    if args.write:
        write()
        return 0
    if args.find is not None:
        find(args.find)
        return 0
    found = problems()
    for problem in found:
        print(problem)
    return int(bool(found))


if __name__ == "__main__":
    raise SystemExit(main())
