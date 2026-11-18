"""Download Call of Duty multiplayer minimaps from the Call of Duty wiki (fandom)
for level-design reference, and lay them out as one contact sheet per game.

The images are Activision's. Keep them on your machine: never commit them or
publish them with the game. The default folder is outside the repository.

    uv run --with pillow tools/map_blockout/fetch_cod_minimaps.py [--dest DIR]

It skips maps already downloaded (DIR/index.json records each one, or a miss),
so it can be rerun to add maps to GAMES. Not every map has a minimap on the
wiki; misses are listed at the end.
"""

from __future__ import annotations

import argparse
import json
import re
import time
import urllib.parse
import urllib.request
from pathlib import Path

UA = {"User-Agent": "Mozilla/5.0 (map reference fetcher)"}
API = "https://callofduty.fandom.com/api.php?"
DEFAULT_DEST = Path.home() / ".local/share/shooter/map-references/cod-minimaps/by-game"

# Game folder: (the wiki's file-name suffix, map names with _ for spaces).
GAMES = {
 "CoD4": ("CoD4", "Ambush Backlot Bloc Bog Countdown Crash Crossfire District Downpour Overgrown Pipeline Shipment Showdown Strike Vacant Wet_Work Killhouse Broadcast Chinatown Creek"),
 "MW2": ("MW2", "Afghan Derail Estate Favela Highrise Invasion Karachi Quarry Rundown Rust Scrapyard Skidrow Sub_Base Terminal Underpass Wasteland Bailout Storm Fuel Strike Vacant Overgrown Crash Salvage Carnival Trailer_Park"),
 "BO1": ("BO", "Array Cracked Crisis Firing_Range Grid Hanoi Havana Jungle Launch Nuketown Radiation Summit Villa WMD Kowloon Stadium Discovery Berlin_Wall Convoy Stockpile Zoo Hotel Outskirts Silo Hazard"),
 "BO2": ("BOII", "Aftermath Cargo Carrier Drone Express Hijacked Meltdown Overflow Plaza Raid Slums Standoff Turbine Yemen Nuketown_2025 Downhill Mirage Hydro Grind Encore Magma Vertigo Studio Uplink Detour Cove Rush Dig Frost Pod Takeoff"),
 "MW3": ("MW3", "Arkaden Bakaara Bootleg Carbon Dome Downturn Fallen Hardhat Interchange Lockdown Mission Outpost Resistance Seatown Underground Village Piazza Liberation Overwatch Black_Box Sanctuary Foundation Oasis Terminal Aground Erosion Lookout Getaway Decommission Offshore Parish Gulch Boardwalk"),
 "Ghosts": ("Ghosts", "Chasm Freight Octane Prison_Break Stonehaven Strikezone Warhawk Whiteout Siege Sovereign Tremor Flooded Stormfront Overlord Free_Fall"),
 "BO3": ("BO3", "Aquarium Breach Combine Evac Exodus Fringe Havoc Hunted Infection Metro Redwood Stronghold Nuk3town Gauntlet Rise Skyjacked Splash Spire Rift Knockout Verge Berserk Cryogen Empire Rumble"),
 "BO4": ("BO4", "Arsenal Contraband Frequency Gridlock Hacienda Icebreaker Jungle Militia Morocco Payload Seaside Slums Summit Firing_Range Nuketown Madagascar Masquerade Remnant Lockup Casino Artifact"),
 "MW": ("MW", "Gun_Runner Hackney_Yard Piccadilly Shoot_House St._Petrograd Rammaza Grazna_Raid Azhir_Cave Euphrates_Bridge Arklov_Peak Aniyah_Palace Crash Vacant Hovec_Sawmill Atlas_Superstore Talsik_Backlot Cheshire_Park Khandor_Hideout Suldal_Harbor Shipment Hardhat Rust Scrapyard Backlot Killhouse Docks Aisle_9 Station Bazaar Zhokov_Boneyard"),
 "BOCW": ("BOCW", "Armada Cartel Checkmate Crossroads Garrison Miami Moscow Satellite Nuketown_'84 Raid Express Standoff Hijacked Yamantau Slums Collateral Mansion Apocalypse Diesel Zoo Duga Amsterdam Drive-In Echelon Hijacked Showroom Deprogram"),
 "MWII": ("MWII", "Breenbergh_Hotel Embassy Farm_18 Mercado_Las_Almas Santa_Sena_Border_Crossing Taraq Valderas_Museum Zarqwa_Hydroelectric El_Asilo Al_Bagra_Fortress Shoot_House Shipment Dome Crown_Raceway Himmelmatt_Expo Hydro Alboran_Hatchery Pelayo's_Lighthouse Guijarro Hotel Black_Gold Showroom Hardhat Mercado"),
 "MWIII": ("MWIII", "Afghan Derail Estate Favela Highrise Invasion Karachi Quarry Rundown Rust Scrapyard Skidrow Sub_Base Terminal Underpass Wasteland Meat Rio Departures Vista Greece Stash_House 6_Star Training_Facility Growhouse Vista Sub_Base Shipment Das_Haus"),
 "BO6": ("BO6", "Babylon Derelict Lowtown Payback Protocol Red_Card Rewind Skyline Subsonic Vault Vorkuta Warhead Stakeout Pit Heirloom Nuketown Hideout Extraction Bounty Dealership Firing_Range Scud Hacienda Grind Lifeline Signal Haven Racket Exposure Blackheart Fringe Hijacked Express Summit Eclipse Shutdown Tide Bullet Shutdown Stakeout Floor_Plan Kowloon"),
}


def get(params):
    req = urllib.request.Request(API + urllib.parse.urlencode(params), headers=UA)
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.load(response)


def norm(text):
    return re.sub(r"[^a-z0-9]", "", text.lower())


def fetch(dest: Path):
    index_path = dest / "index.json"
    index = json.loads(index_path.read_text()) if index_path.exists() else {}
    for game, (code, names) in GAMES.items():
        (dest / game).mkdir(parents=True, exist_ok=True)
        for name in dict.fromkeys(names.split()):
            key = f"{game}/{name}"
            if key in index:
                continue
            pretty = name.replace("_", " ")
            try:
                found = get(dict(action="query", list="search", srnamespace=6, srlimit=20, format="json",
                                 srsearch=f"{pretty} minimap {code}"))
            except OSError as error:
                print("error", key, error)
                continue
            titles = [hit["title"] for hit in found["query"]["search"]]
            want = norm(pretty)
            pick = next((t for t in titles
                         if want in norm(t) and "minimap" in norm(t) and norm(t).endswith(norm(code) + "png")), None)
            if not pick:
                print("miss", key)
                index[key] = None
                continue
            info = get(dict(action="query", titles=pick, prop="imageinfo", iiprop="url", format="json"))
            url = next(iter(info["query"]["pages"].values()))["imageinfo"][0]["url"]
            with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=60) as response:
                (dest / game / f"{name}.png").write_bytes(response.read())
            index[key] = {"file": f"{game}/{name}.png", "wiki_title": pick, "url": url}
            index_path.write_text(json.dumps(index, indent=1))
            print("ok", key, pick)
            time.sleep(0.3)
    index_path.write_text(json.dumps(index, indent=1))
    return index


def sheets(dest: Path, index):
    """One contact sheet per game, 6 maps across, each labelled."""
    from PIL import Image, ImageDraw, ImageFont
    try:
        font = ImageFont.truetype("DejaVuSans-Bold.ttf", 20)
    except OSError:
        font = ImageFont.load_default()
    size, cols = 360, 6
    for game in GAMES:
        names = [k.split("/", 1)[1] for k, v in index.items() if v and k.startswith(game + "/")]
        if not names:
            continue
        rows = (len(names) + cols - 1) // cols
        sheet = Image.new("RGB", (cols * size, rows * (size + 28)), "white")
        draw = ImageDraw.Draw(sheet)
        for k, name in enumerate(names):
            image = Image.open(dest / game / f"{name}.png").convert("RGBA")
            backing = Image.new("RGBA", image.size, (40, 40, 40, 255))
            backing.alpha_composite(image)
            image = backing.convert("RGB")
            image.thumbnail((size, size))
            x, y = (k % cols) * size, (k // cols) * (size + 28)
            sheet.paste(image, (x + (size - image.width) // 2, y + 28 + (size - image.height) // 2))
            draw.text((x + 6, y + 3), name.replace("_", " "), fill="black", font=font)
        sheet.save(dest / f"sheet_{game}.jpg", quality=86)
        print("sheet", dest / f"sheet_{game}.jpg")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dest", type=Path, default=DEFAULT_DEST)
    parser.add_argument("--no-sheets", action="store_true")
    args = parser.parse_args()
    args.dest.mkdir(parents=True, exist_ok=True)
    index = fetch(args.dest)
    got = sum(1 for v in index.values() if v)
    print(f"{got} minimaps; missing: {', '.join(k for k, v in index.items() if not v) or 'none'}")
    if not args.no_sheets:
        sheets(args.dest, index)


if __name__ == "__main__":
    main()
