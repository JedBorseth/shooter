"""Marks every image in the project "Strip Visuals" for the dedicated server
export, so the server's pack carries placeholders (their sizes only) instead
of textures it never draws.

Godot's dedicated server export swaps a resource marked "strip" for its
placeholder (`create_placeholder`), and a directory's mode passes to the files
in it. Only image files are stripped: meshes stay, because the server builds
collision and trimesh shapes from some of them (Nuketown's props), and its
surfaces and hit tests read mesh data.

The list is made from the files present, so new images need nothing by hand.
It rewrites the "Linux Server" preset in godot/export_presets.cfg in place;
CI runs it before exporting, and so should anyone exporting a server by hand:

    python3 tools/server_export_filter.py
    godot --headless --path godot --export-release "Linux Server" ../builds/server/shooter-server.x86_64

Without it the export still works, only larger (693 MB instead of about
140 MB).
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path

PRESET_NAME = "Linux Server"
IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".webp", ".exr", ".hdr", ".tga", ".bmp", ".svg"}
SKIP_DIRS = {".godot", "tests"}


def image_paths(project: Path) -> list[str]:
    """Every imported image in the project, as res:// paths, sorted."""
    paths = []
    for file in project.rglob("*"):
        relative = file.relative_to(project)
        if relative.parts[0] in SKIP_DIRS or file.suffix.lower() not in IMAGE_SUFFIXES:
            continue
        # Only files Godot imports become resources.
        if not file.with_name(file.name + ".import").exists():
            continue
        paths.append("res://" + relative.as_posix())
    return sorted(paths)


def preset_section(text: str, name: str) -> tuple[int, int]:
    """The start and end of the [preset.N] section whose name is `name`."""
    for match in re.finditer(r"^\[preset\.(\d+)\]\n", text, re.M):
        start = match.end()
        end_match = re.compile(r"^\[", re.M).search(text, start)
        end = end_match.start() if end_match else len(text)
        if re.search(rf'^name="{re.escape(name)}"$', text[start:end], re.M):
            return start, end
    raise SystemExit(f"no preset named {name!r}")


def with_stripped_images(text: str, paths: list[str]) -> str:
    start, end = preset_section(text, PRESET_NAME)
    section = text[start:end]
    # Drop any earlier list, then set the filter and the new list.
    section = re.sub(r"^customized_files=\{.*?^\}\n", "", section, flags=re.M | re.S)
    entries = ['"res://": "keep"'] + [f'"{path}": "strip"' for path in paths]
    files = "customized_files={\n" + ",\n".join(entries) + "\n}\n"
    section, count = re.subn(
        r'^export_filter="[a-z_]+"\n', 'export_filter="customized"\n' + files, section, flags=re.M
    )
    if count != 1:
        raise SystemExit(f"the {PRESET_NAME} preset has no export_filter line")
    return text[:start] + section + text[end:]


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--project", type=Path, default=root / "godot")
    args = parser.parse_args()
    presets = args.project / "export_presets.cfg"
    paths = image_paths(args.project)
    presets.write_text(with_stripped_images(presets.read_text(), paths))
    print(f"{PRESET_NAME}: {len(paths)} images marked Strip Visuals in {presets}")


if __name__ == "__main__":
    main()
