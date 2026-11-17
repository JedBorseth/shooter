"""Download the pinned CC0 source packs the weapon build starts from.

    uv run tools/fetch_weapon_sources.py            # fetch what is missing
    uv run tools/fetch_weapon_sources.py --verify   # check hashes only

Every archive is CC0 and is pinned by size and SHA-256 (fill a hash in with
`--print-hashes` after a first download). Downloads are cached in
~/.cache/shooter-weapons/sources and extracted next to them, so the Blender
build (tools/build_weapons.py) never touches the network.

itch.io serves free packs without an account, but not by plain GET: the page
carries a CSRF token, `POST <game>/download_url` returns a download page, and
`POST <game>/file/<upload>` on that page returns a signed URL that lives for
about a minute. This script does that dance with a cookie jar.
"""

from __future__ import annotations

import argparse
import hashlib
import http.cookiejar
import json
import re
import subprocess
import sys
import urllib.parse
import urllib.request
from pathlib import Path

CACHE = Path.home() / ".cache/shooter-weapons/sources"
UA = "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0"

# name: (kind, where, upload id or url, archive file name, size, sha256)
SOURCES = {
    # stein-indie, "Free Classic Weapons Pack" v1.1 (CC0 1.0):
    # https://stein-indie.itch.io/classic-weapons-pack
    "classic": {
        "kind": "itch",
        "game": "https://stein-indie.itch.io/classic-weapons-pack",
        "upload": "15953723",
        "file": "classic_weapons_pack_v1.1.zip",
        "size": 130441022,
        "sha256": "e91fd8954226cf99f72bd27065e0ef1042675a96cd9b600d77c623c2f50e90c0",
    },
    # 3dmodelscc0, "Free CC0 Guns & Explosives Pack" (CC0):
    # https://3dmodelscc0.itch.io/free-cc0-guns-explosives-pack
    "guns_explosives": {
        "kind": "itch",
        "game": "https://3dmodelscc0.itch.io/free-cc0-guns-explosives-pack",
        "upload": "9000802",
        "file": "guns_explosives.rar",
        "size": 162200198,
        "sha256": "275679f1f8c7d22aa413c04012571a92e6c7b266ed7174125b64b9d8533f7a1a",
    },
}


class Session:
    def __init__(self):
        self.jar = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))

    def request(self, url: str, data: dict | None = None, referer: str | None = None) -> bytes:
        body = urllib.parse.urlencode(data).encode() if data is not None else None
        req = urllib.request.Request(url, data=body, headers={"User-Agent": UA})
        if referer:
            req.add_header("Referer", referer)
        if body is not None:
            req.add_header("Content-Type", "application/x-www-form-urlencoded")
            req.add_header("X-Requested-With", "XMLHttpRequest")
        with self.opener.open(req, timeout=60) as response:
            return response.read()


def csrf(html: str) -> str:
    match = re.search(r'name="csrf_token"\s+value="([^"]+)"', html) or re.search(
        r'"csrf_token"\s*:\s*"([^"]+)"', html
    ) or re.search(r'<meta name="csrf_token" value="([^"]+)"', html)
    if not match:
        raise SystemExit("itch.io: no csrf_token on the page")
    return match.group(1)


def itch_url(game: str, upload: str) -> str:
    s = Session()
    page = s.request(game).decode()
    token = csrf(page)
    answer = json.loads(s.request(f"{game}/download_url", {"csrf_token": token}, referer=game))
    download_page = answer["url"]
    page = s.request(download_page, referer=game).decode()
    token = csrf(page)
    answer = json.loads(
        s.request(
            f"{game}/file/{upload}?source=view_game&as_props=1&after_download_lightbox=true",
            {"csrf_token": token},
            referer=download_page,
        )
    )
    return answer["url"]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def fetch(name: str, spec: dict) -> Path:
    CACHE.mkdir(parents=True, exist_ok=True)
    path = CACHE / spec["file"]
    if path.exists() and path.stat().st_size == spec["size"]:
        return path
    url = itch_url(spec["game"], spec["upload"]) if spec["kind"] == "itch" else spec["url"]
    print(f"[fetch] {name}: downloading {spec['file']}", flush=True)
    part = path.with_suffix(path.suffix + ".part")
    subprocess.run(["curl", "-fsSL", "-A", UA, "-o", str(part), url], check=True)
    if part.stat().st_size != spec["size"]:
        raise SystemExit(f"{name}: size {part.stat().st_size}, expected {spec['size']}")
    part.rename(path)
    return path


def verify(name: str, spec: dict, path: Path):
    if not spec["sha256"]:
        print(f"[fetch] {name}: no pinned hash ({sha256(path)})")
        return
    got = sha256(path)
    if got != spec["sha256"]:
        raise SystemExit(f"{name}: sha256 {got}, expected {spec['sha256']}")


def extract(path: Path) -> Path:
    out = path.with_suffix("")
    stamp = out / ".extracted"
    if stamp.exists():
        return out
    out.mkdir(exist_ok=True)
    # bsdtar (libarchive) reads both zip and RAR5; Debian: libarchive-tools.
    subprocess.run(["bsdtar", "-xf", str(path), "-C", str(out)], check=True)
    stamp.write_text(path.name)
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--print-hashes", action="store_true")
    args = parser.parse_args()
    for name, spec in SOURCES.items():
        path = CACHE / spec["file"]
        if not args.verify:
            path = fetch(name, spec)
        if args.print_hashes:
            print(name, sha256(path))
            continue
        verify(name, spec, path)
        if not args.verify:
            print(f"[fetch] {name}: {extract(path)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
