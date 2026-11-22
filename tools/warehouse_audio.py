"""Warehouse asymmetric outdoor soundscape and sparse off-map industrial layer.

Invoked via ``uv run tools/build_world_audio.py warehouse``.

Every sound is built from verified CC0 recordings from the USC Sound Effect
Archive (Craig Smith's SSE Library and Red Library on the Internet Archive,
https://creativecommons.org/publicdomain/zero/1.0/) and Wikimedia Commons CC0,
cached in ``~/.cache/shooter-world-audio/warehouse_sources/`` and checked
against pinned SHA-256 digests.

Outputs into ``godot/assets/audio/ambience/``:
- Beds (32 s seamless stereo loops, loudness-matched to day/city/interior):
  ``warehouse_field.ogg``, ``warehouse_docks.ogg``, ``warehouse_hall.ogg``,
  ``warehouse_breach_wind.ogg``
- Field outdoor one-shots (7–18 s outdoors in the Field):
  ``field_gust_1..3.ogg``, ``field_fence_rattle_1..3.ogg``,
  ``field_roof_bang_1..3.ogg``, ``field_crow_1..3.ogg``
- Docks outdoor one-shots (7–18 s outdoors at the Docks):
  ``docks_trailer_creak_1..3.ogg``, ``docks_pallet_drop_1..3.ogg``,
  ``docks_air_brakes_1..3.ogg``
- Off-map working-industrial one-shots (30–80 m out, muffled by distance,
  sparse 25–60 s cadence with per-category cooldowns):
  ``ind_hammer_1..3.ogg``, ``ind_shout_1..3.ogg``,
  ``ind_reverse_alarm_1..2.ogg``, ``ind_forklift_horn_1..3.ogg``,
  ``ind_metal_drop_1..3.ogg``, ``ind_impact_wrench_1..3.ogg``,
  ``ind_roller_door_1..2.ogg``, ``ind_pa_chime_1..2.ogg``
"""

from __future__ import annotations

import hashlib
import urllib.request
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy import signal

REPO = Path(__file__).resolve().parent.parent
AMBIENCE = REPO / "godot/assets/audio/ambience"
CACHE = Path.home() / ".cache/shooter-world-audio/warehouse_sources"
RATE = 48_000

# Verified CC0 (https://creativecommons.org/publicdomain/zero/1.0/) sources
# with pinned SHA-256 hashes for reproducible rebuilds.
SOURCES: dict[str, tuple[str, str]] = {
    "wind_prairie.wav": (
        "https://archive.org/download/SSE_Library_WIND/GENERAL/WIND_Jim%20Cook%3Fs%20wind%3B%20bluster%3B%20prairie%20wind_CS_USC.wav",
        "492cd00f5f99a4d22d53760321a8c236cde5569e660f0fe733dab64de361e58d",
    ),
    "wind_steady.wav": (
        "https://archive.org/download/SSE_Library_WIND/GENERAL/WIND_Steady%20wind_CS_USC.wav",
        "a73f04ae4b3646b710556c28a3453235610417998144d3a25fc6b6783f26a1bb",
    ),
    "wind_gust_desert.wav": (
        "https://archive.org/download/SSE_Library_WIND/GUST/WIND_Gusty%20desert%20wind_CS_USC.wav",
        "48f2dcdf19a693621f45a060909f735b0d2aa3ab68a704a690b44a6e83185e31",
    ),
    "wind_blustery.wav": (
        "https://archive.org/download/Red_Library_Nature_Wind/R22-11-Blustery%20Wind%20Loop.wav",
        "f11f16bbe8bf1d66603a9b4fda5ee5073fc79c8a3b3c0fe9943ab2976b39c793",
    ),
    "birds_outside.wav": (
        "https://archive.org/download/Red_Library_Animals_Birds/R01-24-Birds%20Chirping%20Outside.wav",
        "5dfaccd580132003e7817dd25e821297ac7441bac1d1e2576819abead5bfe18d",
    ),
    "meadow_lark.wav": (
        "https://archive.org/download/Red_Library_Animals_Birds/R30-40-Western%20Meadow%20Lark%20Calls.wav",
        "b8983fa8fa1819974fa13b54f378c938a2706fe35f4f89fb7d704494ace4ed57",
    ),
    "cicada.ogg": (
        "https://upload.wikimedia.org/wikipedia/commons/b/b1/Chorus_Cicada_singing.ogg",
        "b58bb6545ec080fd75eb68e9f580a96b4a1217c6294e80233f56b49fc6d8b250",
    ),
    "raven_scold.wav": (
        "https://archive.org/download/SSE_Library_BIRDS/MISC/BIRDMisc_Raven%20scolding_CS_USC.wav",
        "44acaf191f77b99df2b7081008808dc50037f7a2e36015406d2d03b7f0ec25b9",
    ),
    "traffic_rumble.wav": (
        "https://archive.org/download/SSE_Library_AMBIENCE/TRAFFIC/AMBTraf_Distant%20traffic%20rumble_CS_USC.wav",
        "22bff83cdee33da25bcaa034753631e528b8a434cd8e6721b70776bfb18e1e3e",
    ),
    "highway_sounds.wav": (
        "https://archive.org/download/Red_Library_Ambience_1/R05-27-Highway%20Sounds.wav",
        "1ec874f9e3f4cff01145d19aaeeec0e4db399ec5bb646078470d44727d86e58c",
    ),
    "distant_highway.wav": (
        "https://archive.org/download/Red_Library_Ambience_2/R20-09-Distant%20Highway.wav",
        "736758ba817d4250efcc8a941d3fca6a0d2e6124cdcb29b56f4a5273fe893462",
    ),
    "truck_bus_idle.wav": (
        "https://archive.org/download/SSE_Library_VEHICLES/BUS/VEHBus_Partridge%20Family%20bus%20%28school%20bus%29%20idle_CS_USC.wav",
        "cc477f834902579d227bac6207737e238179039e2fb55f31ddf0bb4dff86d6b4",
    ),
    "diesel_crane_idle.wav": (
        "https://archive.org/download/SSE_Library_MACHINES/CONSTRUCTION/MACHCnst_Crane%3B%20or%20other%20construction%20equipment%3B%20idle_CS_USC.wav",
        "b68d20df744240c06641502abc3f0c3b1294d4eb86efe8579dfba25219aca497",
    ),
    "factory_distant_machine.wav": (
        "https://archive.org/download/Red_Library_Machines_2/R18-32-Distant%20Machine%20in%20Factory.wav",
        "921e815bfb8752b412ab4adfb503bcf70b54ebf824620c7e9074f10b12f9c2a3",
    ),
    "factory_exhaust_fan.wav": (
        "https://archive.org/download/SSE_Library_AMBIENCE/INDUSTRIAL/AMBInd_Factory%20interior%20with%20large%20exhaust%20fan%3B_CS_USC.wav",
        "b0ca57f3b4538d2692daeeb4d8daefc9b7b80380eac93958f71d9608e6146e15",
    ),
    "generator_building.wav": (
        "https://archive.org/download/SSE_Library_MACHINES/INDUSTRIAL/MACHInd_Generator%20building_CS_USC.wav",
        "9833bfbd107f4daa5e25173c05fd088d63ec003d450f1809a29460bc6231d9ba",
    ),
    "metal_rattle.wav": (
        "https://archive.org/download/Red_Library_Metal/R16-43-Metal%20Rattling%20and%20Dragging.wav",
        "816146e836ed9f919376cd48fe316c4b2c3360faa34338d7015404078c092fa3",
    ),
    "sheet_metal_flap.wav": (
        "https://archive.org/download/SSE_Library_METAL/MOVEMENT/METLMvmt_Fender%20flapping%20on%20damaged%20car%3B%20bouncing_CS_USC.wav",
        "6025be6d829c3a03f995255116cb884e07f456fff52a457c6741ae02f5323f11",
    ),
    "metal_clangs.wav": (
        "https://archive.org/download/Red_Library_Metal/R15-07-Metal%20Clangs%20and%20Hollow%20Hits.wav",
        "5b4898c9c762e55dbba0b9134eecb83d948287b65c53e562b57ba31bc2565b9e",
    ),
    "steel_creak.wav": (
        "https://archive.org/download/SSE_Library_METAL/MOVEMENT/METLMvmt-GIRDERS_Steel%20girders%20wrench%20and%20tear%3B%20creaking%20metal_CS_USC.wav",
        "1068d8af03371a3d59ac554428e59a0703d9e73892fa2a4299eb3e71e57a36f3",
    ),
    "metal_squeak.wav": (
        "https://archive.org/download/Red_Library_Metal/R16-30-Large%20Metal%20Squeak.wav",
        "2c656924120294e5a364d36484e905b095aede17c6fd2ee178374935743379e9",
    ),
    "wood_drop.wav": (
        "https://archive.org/download/SSE_Library_WOOD/IMPACT/WOODImpt_Wooden%20barrel%20drop%3B%20several_CS_USC.wav",
        "5da8f08e36213edbd042332321bc19eae03f24642a74c17a1cda5e50df603f37",
    ),
    "wood_clatter.wav": (
        "https://archive.org/download/Red_Library_Creaks/R17-38-Wood%20Clattering.wav",
        "80be9a73d563db5a65c3ec364fec516b1a89c95a1a0a878bf576558508dc75a1",
    ),
    "air_blast.wav": (
        "https://archive.org/download/Red_Library_Air_Industry/R17-35-Steam%20or%20Compressed%20Air.wav",
        "75deab41a331442a20880c1e462b4940936857111a6c4e3b8eddb9317d12aa48",
    ),
    "air_blasts_big.wav": (
        "https://archive.org/download/Red_Library_Air_Industry/R28-07-Big%20Blasts%20of%20Air.wav",
        "08a3424384c77c13b5b499e7603dcd3641688a467dd7d4902854819750438863",
    ),
    "hammer_anvil.wav": (
        "https://archive.org/download/SSE_Library_METAL/IMPACT/METLImpt_Blacksmith%20hitting%20anvil%20with%20hammer%3B%20two_CS_USC.wav",
        "e2542dc8142fb0c5a3fd989f6d6868ff338427e9a057e1663edad8d8280d98b7",
    ),
    "factory_pounding.wav": (
        "https://archive.org/download/SSE_Library_MACHINES/INDUSTRIAL/MACHInd_Metallic%20pounding%20in%20factory_CS_USC.wav",
        "ad2cfbb24f12d32c0682620dd7dcee4eb8bb94b712e6902df218f82413ae168a",
    ),
    "unloading_voices.wav": (
        "https://archive.org/download/Red_Library_Crowds_Outdoor/R24-04-Unloading%20Ammunition%20Truck.wav",
        "587737d004135af1151dc5e21dfb15ebdc925b127ee85f9575f2b6538e3429c3",
    ),
    "men_shouting.wav": (
        "https://archive.org/download/Red_Library_Crowds_Men/R15-70-Males%20Shouting.wav",
        "5b8bb132c208eca9317794549577fe6dca75d6928c6b25006826974752a34761",
    ),
    "beep_1khz.wav": (
        "https://archive.org/download/SSE_Library_BEEPS/MEDICAL/BEEPMed_Cardiac%20arrest%20alarm%20%281%20kHz%20square%20wave%29_CS_USC.wav",
        "20cebd25be46941849b34ef1eda221bbc9b4e399d252971f70b0fb24ab53b184",
    ),
    "horn_beeps.wav": (
        "https://archive.org/download/Red_Library_Bells_Horns_Whistles/R12-45-Car%20Horn%20Beeps.wav",
        "5ad7f93e86b6dad111b595581c4e2a305b9daea033bac8e4adb4b55a35abd5ed",
    ),
    "horn_steady.wav": (
        "https://archive.org/download/Red_Library_Bells_Horns_Whistles/R04-23-Steady%20Horn%20Beep.wav",
        "8ecf29789e40ebb4c723ff5aac09d8a8d6db0d69bf5054fb61a25de0d373f09e",
    ),
    "metal_crash_drop.wav": (
        "https://archive.org/download/SSE_Library_METAL/CRASH%20%26%20DEBRIS/METLCrsh_Heavy%20metal%20objets%20crashing%20on%20stone%20floor%3B_CS_USC.wav",
        "705140d5e13da9330788edd0b5cd33fb8c84701fa6bfcbc67a5aa741e1537754",
    ),
    "metal_moving.wav": (
        "https://archive.org/download/SSE_Library_METAL/MOVEMENT/METLMvmt_Moving%20large%20metal%20objects%20around%20in%20building%3B_CS_USC.wav",
        "958032332e90f8f9d324a36774601f233825341201ba187ca554c7434f9762ad",
    ),
    "rivet_gun.wav": (
        "https://archive.org/download/SSE_Library_TOOLS/PNEUMATIC/TOOLPneu_Riveting%20gun_CS_USC.wav",
        "fb72b15a3994d2630454c43b306131ca299d39e7cf9cefa95e26f79c55baed80",
    ),
    "garage_door.wav": (
        "https://archive.org/download/SSE_Library_DOORS/ELECTRIC/DOORElec_Electric%20garage%20door%20motor%20open%20%26%20close_CHS_USC.wav",
        "be479b0645107f956b2117d21c9424f7edc89b8fdcc22a09fca761911a92f979",
    ),
    "boxcar_door.wav": (
        "https://archive.org/download/SSE_Library_DOORS/SLIDING/DOORSlid_Sliding%20boxcar%20door%3B%20could%20be%20any%20sliding_CHS_USC.wav",
        "aa2bb796446a96fd31c4a4a90324b8a5dd8ae2b671b50a38322cbe3f48235bce",
    ),
    "store_chimes.wav": (
        "https://archive.org/download/Red_Library_Bells_Horns_Whistles/R04-40-Department%20Store%20Chimes.wav",
        "7cd8de4279690a9c82203cf2ca6d1e067e0741def84cd715cde5afe4279eac89",
    ),
    "door_chime.wav": (
        "https://archive.org/download/SSE_Library_BELLS/DOORBELL/BELLDoor_Door%20bell%3B%20door%20chime%20%28Columbia%20Pictures%29_CHS_USC.wav",
        "83a254bdef2c2e434f97c83c8df966e2d5f15c3d1bafb2529166270b3b745ea9",
    ),
}


def fetch_source(name: str) -> Path:
    url, expected = SOURCES[name]
    CACHE.mkdir(parents=True, exist_ok=True)
    path = CACHE / name
    if not path.exists():
        req = urllib.request.Request(url, headers={"User-Agent": "shooter-asset-pipeline/1.0"})
        with urllib.request.urlopen(req, timeout=90) as response:
            path.write_bytes(response.read())
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    assert digest == expected, f"SHA-256 mismatch for {name}: got {digest}, expected {expected}"
    return path


def load_mono(name: str) -> np.ndarray:
    x, sr = sf.read(fetch_source(name), always_2d=True, dtype="float64")
    mono = x.mean(axis=1)
    if sr != RATE:
        mono = signal.resample_poly(mono, RATE, sr)
    return mono - mono.mean()


def clip(x: np.ndarray, start_s: float, end_s: float) -> np.ndarray:
    a = max(0, int(start_s * RATE))
    b = min(len(x), int(end_s * RATE))
    return x[a:b].copy()


def fade_ends(x: np.ndarray, in_s: float = 0.04, out_s: float = 0.08) -> np.ndarray:
    y = x.copy()
    fi = min(len(y) // 3, int(in_s * RATE))
    fo = min(len(y) // 3, int(out_s * RATE))
    if fi > 0:
        y[:fi] *= np.sin(np.linspace(0, np.pi / 2, fi)) ** 2
    if fo > 0:
        y[-fo:] *= np.cos(np.linspace(0, np.pi / 2, fo)) ** 2
    return y


def bp(x: np.ndarray, lo: float, hi: float, order: int = 2) -> np.ndarray:
    return signal.sosfilt(signal.butter(order, (lo, hi), "band", fs=RATE, output="sos"), x)


def lp(x: np.ndarray, hz: float, order: int = 2) -> np.ndarray:
    return signal.sosfilt(signal.butter(order, hz, "low", fs=RATE, output="sos"), x)


def hp(x: np.ndarray, hz: float, order: int = 2) -> np.ndarray:
    return signal.sosfilt(signal.butter(order, hz, "high", fs=RATE, output="sos"), x)


def pitch_shift(x: np.ndarray, ratio: float) -> np.ndarray:
    up, down = int(round(1000 / ratio)), 1000
    return signal.resample_poly(x, up, down)


def match_rms(x: np.ndarray, target_dbfs: float, ceiling_dbfs: float = -4.0) -> np.ndarray:
    rms = np.sqrt(np.mean(x**2)) + 1e-12
    y = x * (10 ** (target_dbfs / 20.0) / rms)
    peak = np.abs(y).max()
    ceiling = 10 ** (ceiling_dbfs / 20.0)
    if peak > ceiling:
        y = y * (ceiling / peak)
    return y


def seamless_mono(x: np.ndarray, seconds: float, xfade_s: float = 3.0) -> np.ndarray:
    """Makes an exact `seconds` loop from continuous audio `x` with an
    equal-power crossfade so the wrap point has no seam."""
    n = int(seconds * RATE)
    xf = int(xfade_s * RATE)
    need = n + xf
    if len(x) < need:
        # Tile with equal-power overlap if the source recording is shorter.
        step = max(RATE, len(x) - xf)
        tiled = np.zeros(need + len(x))
        pos = 0
        while pos < need:
            seg = x.copy()
            if pos > 0:
                seg[:xf] *= np.sin(np.linspace(0, np.pi / 2, xf))
                tiled[pos : pos + xf] *= np.cos(np.linspace(0, np.pi / 2, xf))
            tiled[pos : pos + len(seg)] += seg
            pos += step
        x = tiled
    body = x[:n].copy()
    tail = x[n : n + xf]
    fade = np.linspace(0.0, np.pi / 2, xf)
    body[:xf] = body[:xf] * np.sin(fade) + tail * np.cos(fade)
    return body


def stereo_bed(x: np.ndarray, seconds: float, offset_s: float = 7.0) -> np.ndarray:
    """Decorrelated stereo loop from two offset spans of a continuous recording."""
    n = int(seconds * RATE)
    left = seamless_mono(x, seconds)
    shift = int(offset_s * RATE) % max(1, len(x) - RATE)
    rolled = np.roll(x, -shift)
    right = seamless_mono(rolled, seconds)
    # Keep a modest centre coherence so beds feel wide without phase hollowing.
    return np.stack([0.85 * left + 0.15 * right, 0.15 * left + 0.85 * right], axis=1)[:n]


def exterior_distance(
    x: np.ndarray,
    rng: np.random.Generator,
    cutoff_hz: float,
    tail_s: float,
    wet: float = 0.28,
    hp_hz: float = 70.0,
) -> np.ndarray:
    """Air absorption + diffuse exterior reflection tail so a recording reads
    as coming from 15–80 m away rather than inside the play space."""
    y = hp(lp(x, cutoff_hz, order=2), hp_hz, order=2)
    t = np.arange(int(tail_s * RATE)) / RATE
    ir = rng.normal(size=len(t)) * np.exp(-t / (tail_s / 4.5))
    ir = bp(ir, 180.0, min(cutoff_hz * 0.85, 3_500.0), order=2)
    ir = ir / (np.sqrt(np.mean(ir**2)) + 1e-12) * 0.04
    rev = signal.fftconvolve(y, ir)[: len(y) + len(t)]
    out = np.zeros(len(rev))
    out[: len(y)] += y
    out += wet * rev
    return fade_ends(out, 0.015, min(0.18, tail_s * 0.35))


def place_stereo(out: np.ndarray, mono: np.ndarray, at_s: float, pan: float, gain: float = 1.0) -> None:
    idx = (int(at_s * RATE) + np.arange(len(mono))) % len(out)
    angle = (pan + 1.0) * np.pi / 4.0
    np.add.at(out[:, 0], idx, mono * gain * np.cos(angle))
    np.add.at(out[:, 1], idx, mono * gain * np.sin(angle))


def write_ambience(name: str, x: np.ndarray) -> None:
    AMBIENCE.mkdir(parents=True, exist_ok=True)
    path = AMBIENCE / f"{name}.ogg"
    y = np.clip(x, -0.98, 0.98).astype(np.float32)
    sf.write(path, y, RATE, format="OGG", subtype="VORBIS")
    rms_db = 20.0 * np.log10(np.sqrt(np.mean(y**2)) + 1e-12)
    pk_db = 20.0 * np.log10(np.abs(y).max() + 1e-12)
    ch = "stereo" if y.ndim == 2 else "mono"
    print(f"  {path.relative_to(REPO)}: {len(y) / RATE:.2f} s ({ch}), RMS {rms_db:.1f} dBFS, peak {pk_db:.1f} dBFS")


# --- Beds -------------------------------------------------------------------


def build_field_bed(rng: np.random.Generator) -> np.ndarray:
    """THE FIELD: wind through long grass, sparse daytime birds & insects,
    faint distant road traffic."""
    seconds = 32.0
    n = int(seconds * RATE)
    prairie = hp(lp(load_mono("wind_prairie.wav"), 3_600.0), 65.0)
    steady = hp(lp(load_mono("wind_steady.wav"), 4_200.0), 120.0)
    # Grass rustle lives in the airy 700-4000 Hz shimmer riding on the breeze.
    grass = bp(prairie, 650.0, 4_200.0)
    wind_bed = (
        0.65 * match_rms(stereo_bed(prairie, seconds, 11.0), -33.0)
        + 0.35 * match_rms(stereo_bed(steady, seconds, 5.5), -34.0)
        + 0.28 * match_rms(stereo_bed(grass, seconds, 17.0), -36.0)
    )
    # Gentle slow wind breathing over the 32 s loop.
    t = np.arange(n) / n
    swell = 0.80 + 0.20 * np.sin(2 * np.pi * t + 0.6) * np.cos(2 * np.pi * 2 * t)
    out = wind_bed * swell[:, None]

    # Distant road traffic (edge of town, very quiet low wash).
    traffic = lp(hp(load_mono("traffic_rumble.wav"), 35.0), 420.0)
    out += match_rms(stereo_bed(traffic, seconds, 19.0), -42.5)

    # Sparse daytime insects (faint dry-grass cicadas tucked low).
    cicada = bp(load_mono("cicada.ogg"), 2_600.0, 7_500.0)
    insect_bed = match_rms(stereo_bed(cicada, seconds, 9.0), -47.5)
    insect_gate = (0.55 + 0.45 * np.sin(2 * np.pi * t + 1.7))[:, None]
    out += insect_bed * insect_gate

    # Sparingly placed daytime birds (3 brief distant chirps over 32 s).
    birds = load_mono("birds_outside.wav")
    lark = load_mono("meadow_lark.wav")
    bird_clips = [
        (clip(birds, 2.2, 4.3), 5.0, -0.55),
        (clip(lark, 2.8, 4.4), 16.5, 0.60),
        (clip(birds, 10.3, 12.2), 26.5, -0.25),
    ]
    for seg, at_s, pan in bird_clips:
        seg = exterior_distance(fade_ends(seg, 0.08, 0.15), rng, 4_200.0, 0.35, wet=0.25, hp_hz=900.0)
        seg = match_rms(seg, -44.0)
        place_stereo(out, seg, at_s, pan)

    return match_rms(out, -32.8, ceiling_dbfs=-14.0)


def build_docks_bed() -> np.ndarray:
    """THE DOCKS: harder industrial concrete apron — distant highway, far truck
    idling, and hall machinery bleeding out of the dock doors."""
    seconds = 32.0
    # Distant highway wash.
    hwy1 = hp(lp(load_mono("highway_sounds.wav"), 2_000.0), 40.0)
    hwy2 = hp(lp(load_mono("traffic_rumble.wav"), 1_200.0), 35.0)
    highway = (
        0.55 * match_rms(stereo_bed(hwy1, seconds, 8.0), -32.5)
        + 0.45 * match_rms(stereo_bed(hwy2, seconds, 15.0), -32.0)
    )
    # Far diesel truck idling (warm chug panned slightly off-centre).
    truck = hp(lp(load_mono("truck_bus_idle.wav"), 680.0), 35.0)
    crane = hp(lp(load_mono("diesel_crane_idle.wav"), 550.0), 35.0)
    idle_l = seamless_mono(truck, seconds)
    idle_r = seamless_mono(np.roll(crane, -int(25 * RATE)), seconds)
    idle = np.stack([0.72 * idle_l + 0.28 * idle_r, 0.42 * idle_l + 0.58 * idle_r], axis=1)
    idle = match_rms(idle, -34.0)

    # Hall machinery bleeding out of the open dock bays.
    gen = hp(lp(load_mono("generator_building.wav"), 520.0), 45.0)
    fan = hp(lp(load_mono("factory_exhaust_fan.wav"), 850.0), 60.0)
    bleed = (
        0.6 * match_rms(stereo_bed(gen, seconds, 6.0), -36.5)
        + 0.4 * match_rms(stereo_bed(fan, seconds, 9.0), -37.0)
    )

    out = highway + idle + bleed
    return match_rms(out, -29.8, ceiling_dbfs=-11.0)


def build_hall_bed() -> np.ndarray:
    """THE HALL: existing indoor room tone with a very quiet continuous plant
    machinery hum/thrum from elsewhere in the plant tucked underneath."""
    seconds = 32.0
    n = int(seconds * RATE)
    # Reuse the project's existing indoor room tone so the hall keeps its
    # established character.
    room_path = AMBIENCE / "interior.ogg"
    if room_path.exists():
        room_raw, sr = sf.read(room_path, always_2d=True, dtype="float64")
        if sr != RATE:
            room_raw = signal.resample_poly(room_raw, RATE, sr, axis=0)
        room = np.stack(
            [seamless_mono(room_raw[:, 0], seconds, 1.5), seamless_mono(room_raw[:, 1], seconds, 1.5)],
            axis=1,
        )
    else:
        room = np.zeros((n, 2))
    room = match_rms(room, -37.2)

    # Low continuous plant machinery hum/thrum from adjacent rooms, very quiet.
    machine = hp(lp(load_mono("factory_distant_machine.wav"), 290.0), 35.0)
    fan = hp(lp(load_mono("factory_exhaust_fan.wav"), 360.0), 40.0)
    plant = (
        0.55 * match_rms(stereo_bed(machine, seconds, 10.0), -40.5)
        + 0.45 * match_rms(stereo_bed(fan, seconds, 7.0), -41.5)
    )
    out = room + plant
    return match_rms(out, -35.5, ceiling_dbfs=-18.0)


def build_breach_wind_bed() -> np.ndarray:
    """Faint sky wind bleeding down through the 12x12 m roof breach."""
    seconds = 24.0
    steady = bp(load_mono("wind_steady.wav"), 110.0, 2_400.0)
    blustery = bp(load_mono("wind_blustery.wav"), 140.0, 2_800.0)
    out = (
        0.65 * match_rms(stereo_bed(steady, seconds, 5.0), -34.5)
        + 0.35 * match_rms(stereo_bed(blustery, seconds, 13.0), -36.0)
    )
    t = np.arange(len(out)) / len(out)
    swell = 0.75 + 0.25 * np.sin(2 * np.pi * t + 0.4)
    return match_rms(out * swell[:, None], -34.0, ceiling_dbfs=-16.0)


# --- One-shots --------------------------------------------------------------


def build_field_one_shots(rng: np.random.Generator) -> None:
    # 1. Wind gusts across the grass lot (stereo, non-positional on outdoor bus)
    desert = load_mono("wind_gust_desert.wav")
    blustery = load_mono("wind_blustery.wav")
    gust_spans = [
        (clip(desert, 3.0, 7.2), -0.45, 0.45),
        (clip(desert, 13.8, 18.4), 0.50, -0.40),
        (clip(blustery, 8.0, 12.6), -0.35, 0.55),
    ]
    for i, (seg, pan0, pan1) in enumerate(gust_spans, 1):
        seg = bp(seg, 85.0, 2_900.0)
        t = np.linspace(0.0, 1.0, len(seg))
        env = np.sin(np.pi * t**0.85) ** 1.6
        shaped = seg * env
        # Slight stereo movement + decorrelated tail
        delayed = np.roll(shaped, int(0.011 * RATE))
        pan = np.linspace(pan0, pan1, len(seg))
        angle = (pan + 1.0) * np.pi / 4.0
        st = np.stack([shaped * np.cos(angle) + 0.2 * delayed, delayed * np.sin(angle) + 0.2 * shaped], axis=1)
        write_ambience(f"field_gust_{i}", match_rms(st, -23.5, ceiling_dbfs=-5.0))

    # 2. Chain-link fence rattling in a gust (mono, positional)
    rattle = load_mono("metal_rattle.wav")
    fence_spans = [(0.15, 2.3), (11.5, 13.7), (24.4, 26.8)]
    for i, (a, b) in enumerate(fence_spans, 1):
        seg = fade_ends(clip(rattle, a, b), 0.06, 0.14)
        # Light gust breath under the fence rattle
        breath = bp(clip(desert, 3.5 + i, 3.5 + i + (b - a)), 250.0, 2_200.0)
        m = min(len(seg), len(breath))
        seg = seg[:m] + 0.18 * match_rms(breath[:m], 20 * np.log10(np.sqrt(np.mean(seg[:m] ** 2)) + 1e-12))
        y = exterior_distance(seg, rng, 4_800.0, 0.35, wet=0.22, hp_hz=280.0)
        write_ambience(f"field_fence_rattle_{i}", match_rms(y, -21.0, ceiling_dbfs=-4.5))

    # 3. Loose sheet of corrugated roofing banging (mono, positional)
    flap = load_mono("sheet_metal_flap.wav")
    clangs = load_mono("metal_clangs.wav")
    roof_segs = [
        clip(flap, 0.55, 2.55),
        clip(flap, 5.95, 7.65),
        clip(clangs, 6.5, 8.1),
    ]
    for i, seg in enumerate(roof_segs, 1):
        y = exterior_distance(fade_ends(seg, 0.02, 0.12), rng, 3_800.0, 0.45, wet=0.26, hp_hz=110.0)
        write_ambience(f"field_roof_bang_{i}", match_rms(y, -21.0, ceiling_dbfs=-4.5))

    # 4. Distant crow / raven calls (mono, positional)
    raven = load_mono("raven_scold.wav")
    crow_spans = [(3.75, 4.75), (5.35, 6.55), (8.75, 10.15)]
    for i, (a, b) in enumerate(crow_spans, 1):
        seg = fade_ends(clip(raven, a, b), 0.03, 0.12)
        y = exterior_distance(seg, rng, 3_600.0, 0.45, wet=0.30, hp_hz=350.0)
        write_ambience(f"field_crow_{i}", match_rms(y, -21.5, ceiling_dbfs=-4.5))


def build_docks_one_shots(rng: np.random.Generator) -> None:
    # 1. Semi-trailer / container structural creak
    creak = load_mono("steel_creak.wav")
    squeak = load_mono("metal_squeak.wav")
    creak_segs = [
        clip(creak, 2.1, 4.6),
        clip(creak, 13.3, 15.8),
        clip(squeak, 3.6, 5.9),
    ]
    for i, seg in enumerate(creak_segs, 1):
        y = exterior_distance(fade_ends(seg, 0.08, 0.18), rng, 3_200.0, 0.40, wet=0.25, hp_hz=110.0)
        write_ambience(f"docks_trailer_creak_{i}", match_rms(y, -21.5, ceiling_dbfs=-4.5))

    # 2. Wooden pallet dropped far off
    wdrop = load_mono("wood_drop.wav")
    wclat = load_mono("wood_clatter.wav")
    pallet_segs = [
        clip(wdrop, 1.0, 1.9),
        clip(wdrop, 18.2, 19.2),
        clip(wclat, 0.8, 2.1),
    ]
    for i, seg in enumerate(pallet_segs, 1):
        y = exterior_distance(fade_ends(seg, 0.01, 0.12), rng, 2_800.0, 0.45, wet=0.28, hp_hz=90.0)
        write_ambience(f"docks_pallet_drop_{i}", match_rms(y, -20.5, ceiling_dbfs=-4.5))

    # 3. Truck pneumatic air-brake hiss release
    air_big = load_mono("air_blasts_big.wav")
    air_one = load_mono("air_blast.wav")
    brake_segs = [
        clip(air_big, 0.4, 1.75),
        clip(air_big, 6.4, 7.85),
        clip(air_one, 0.5, 1.7),
    ]
    for i, seg in enumerate(brake_segs, 1):
        # Crisp pneumatic release envelope: fast puff open, smooth decay.
        t = np.linspace(0.0, 1.0, len(seg))
        env = np.minimum(t / 0.04, 1.0) * np.exp(-np.maximum(t - 0.12, 0.0) / 0.35)
        y = exterior_distance(seg * env, rng, 5_200.0, 0.35, wet=0.22, hp_hz=450.0)
        write_ambience(f"docks_air_brakes_{i}", match_rms(y, -21.0, ceiling_dbfs=-4.5))


def build_industrial_one_shots(rng: np.random.Generator) -> None:
    """Off-map working-industrial one-shots (30–80 m beyond the perimeter walls),
    filtered by distance and exterior/building reflections so they never read as
    in-map combat cues."""
    # 1. Hammering on steel (short bursts of 4–6 blows, never a continuous loop)
    anvil = load_mono("hammer_anvil.wav")
    pounding = load_mono("factory_pounding.wav")
    hammer_segs = [
        clip(anvil, 7.6, 11.4),     # 5 blows
        clip(anvil, 11.6, 14.8),    # 4 blows
        clip(pounding, 20.1, 23.6), # 5–6 distant blows
    ]
    for i, seg in enumerate(hammer_segs, 1):
        y = exterior_distance(fade_ends(seg, 0.02, 0.15), rng, 2_400.0, 0.65, wet=0.38, hp_hz=150.0)
        write_ambience(f"ind_hammer_{i}", match_rms(y, -21.5, ceiling_dbfs=-4.5))

    # 2. Distant muffled worker shouting / calling out indistinctly
    # Heavily band-limited (300–1300 Hz) + diffuse exterior tail so no words
    # are intelligible and it can never be mistaken for a player callout.
    unload = load_mono("unloading_voices.wav")
    shouts = load_mono("men_shouting.wav")
    shout_segs = [
        clip(shouts, 2.2, 3.6),
        clip(shouts, 11.0, 12.7),
        clip(unload, 16.5, 18.3),
    ]
    for i, seg in enumerate(shout_segs, 1):
        # Slight pitch/formant smear + narrow distant bandpass keeps it indistinct.
        seg = pitch_shift(fade_ends(seg, 0.06, 0.15), 0.94 + 0.05 * i)
        seg = bp(seg, 320.0, 1_300.0, order=3)
        y = exterior_distance(seg, rng, 1_250.0, 0.70, wet=0.48, hp_hz=280.0)
        write_ambience(f"ind_shout_{i}", match_rms(y, -22.5, ceiling_dbfs=-6.0))

    # 3. Truck reversing alarm with engine (short 5.2–6.0 s, dock-side mostly)
    beep_raw = load_mono("beep_1khz.wav")
    idle_raw = load_mono("truck_bus_idle.wav")
    for i, (pulses, period_s, pitch) in enumerate(((6, 0.84, 1.08), (7, 0.82, 1.02)), 1):
        dur = pulses * period_s + 0.4
        n = int(dur * RATE)
        # Steady 1 kHz tone starts after ~2.7 s in beep_1khz.wav.
        tone = pitch_shift(clip(beep_raw, 3.5, 3.5 + dur + 1.5), pitch)[:n]
        tone = bp(tone, 700.0, 2_800.0, order=2)
        gate = np.zeros(n)
        on_n = int(0.38 * RATE)
        edge = int(0.012 * RATE)
        for k in range(pulses):
            a = int((0.15 + k * period_s) * RATE)
            b = min(n, a + on_n)
            win = np.ones(b - a)
            if len(win) > 2 * edge:
                win[:edge] = np.linspace(0, 1, edge)
                win[-edge:] = np.linspace(1, 0, edge)
            gate[a:b] = win
        alarm = match_rms(tone * gate, -22.0)
        engine = hp(lp(clip(idle_raw, 2.0 * i, 2.0 * i + dur), 520.0), 40.0)[:n]
        engine = match_rms(fade_ends(engine, 0.35, 0.50), -25.5)
        y = exterior_distance(alarm + engine, rng, 2_300.0, 0.55, wet=0.32, hp_hz=55.0)
        write_ambience(f"ind_reverse_alarm_{i}", match_rms(y, -22.0, ceiling_dbfs=-5.0))

    # 4. Forklift horn (short distant double-toot at a blind corner)
    horn_b = load_mono("horn_beeps.wav")
    horn_s = load_mono("horn_steady.wav")
    for i, ratio in enumerate((1.16, 1.24, 1.12), 1):
        if i == 1:
            base = pitch_shift(fade_ends(clip(horn_b, 0.18, 1.38), 0.02, 0.08), ratio)
        else:
            # Two short 0.17 s toots separated by 0.14 s from the steady horn recording.
            h = pitch_shift(clip(horn_s, 1.5 * i, 1.5 * i + 1.5), ratio)
            toot_n = int(0.17 * RATE)
            gap_n = int(0.14 * RATE)
            t1 = fade_ends(h[:toot_n], 0.015, 0.03)
            t2 = fade_ends(h[toot_n : 2 * toot_n], 0.015, 0.04)
            base = np.concatenate([t1, np.zeros(gap_n), t2])
        y = exterior_distance(base, rng, 2_400.0, 0.55, wet=0.35, hp_hz=280.0)
        write_ambience(f"ind_forklift_horn_{i}", match_rms(y, -21.5, ceiling_dbfs=-5.0))

    # 5. Pallet or steel sheet dropped off-map
    crash = load_mono("metal_crash_drop.wav")
    moving = load_mono("metal_moving.wav")
    drop_segs = [
        clip(crash, 2.0, 3.1),
        clip(crash, 3.7, 4.9),
        clip(moving, 4.9, 6.2),
    ]
    for i, seg in enumerate(drop_segs, 1):
        y = exterior_distance(fade_ends(seg, 0.01, 0.15), rng, 2_400.0, 0.60, wet=0.38, hp_hz=90.0)
        write_ambience(f"ind_metal_drop_{i}", match_rms(y, -20.5, ceiling_dbfs=-4.5))

    # 6. Air-impact wrench burst (short 0.9–1.5 s pneumatic bursts)
    rivet = load_mono("rivet_gun.wav")
    wrench_spans = [(4.0, 5.15), (9.2, 10.6), (14.5, 15.65)]
    for i, (a, b) in enumerate(wrench_spans, 1):
        seg = fade_ends(clip(rivet, a, b), 0.025, 0.09)
        y = exterior_distance(seg, rng, 2_500.0, 0.50, wet=0.34, hp_hz=220.0)
        write_ambience(f"ind_impact_wrench_{i}", match_rms(y, -21.5, ceiling_dbfs=-4.5))

    # 7. Roller door rattling up / down
    garage = load_mono("garage_door.wav")
    boxcar = load_mono("boxcar_door.wav")
    door_segs = [
        clip(garage, 2.0, 6.4),
        clip(garage, 14.8, 19.4),
    ]
    for i, seg in enumerate(door_segs, 1):
        # Layer a hint of heavy metal track rattle from the boxcar door.
        track = clip(boxcar, 0.2, min(1.6, len(seg) / RATE))
        seg = fade_ends(seg, 0.15, 0.25)
        seg[: len(track)] += 0.16 * match_rms(fade_ends(track, 0.05, 0.12), -26.0)
        y = exterior_distance(seg, rng, 2_200.0, 0.55, wet=0.34, hp_hz=95.0)
        write_ambience(f"ind_roller_door_{i}", match_rms(y, -22.0, ceiling_dbfs=-5.0))

    # 8. Far PA / intercom chime without speech
    chimes = load_mono("store_chimes.wav")
    dchime = load_mono("door_chime.wav")
    chime_segs = [
        clip(chimes, 0.0, 1.9),
        clip(dchime, 0.15, 1.65),
    ]
    for i, seg in enumerate(chime_segs, 1):
        # Horn-speaker bandpass + distant hall/yard echo, zero speech.
        seg = bp(fade_ends(seg, 0.015, 0.15), 420.0, 2_600.0, order=2)
        y = exterior_distance(seg, rng, 2_400.0, 0.85, wet=0.45, hp_hz=350.0)
        write_ambience(f"ind_pa_chime_{i}", match_rms(y, -22.0, ceiling_dbfs=-5.5))


def build_warehouse(preview_dir: Path | None = None) -> None:
    """Downloads/verifies CC0 sources and builds every Warehouse ambience asset."""
    print("Building Warehouse ambience from verified CC0 recordings...")
    rng = np.random.default_rng(2026_11)
    write_ambience("warehouse_field", build_field_bed(rng))
    write_ambience("warehouse_docks", build_docks_bed())
    write_ambience("warehouse_hall", build_hall_bed())
    write_ambience("warehouse_breach_wind", build_breach_wind_bed())
    build_field_one_shots(rng)
    build_docks_one_shots(rng)
    build_industrial_one_shots(rng)
    if preview_dir is not None:
        render_listening_previews(preview_dir)


# --- 3-minute listening previews (matching in-game levels & cadence) ---------


def _load_ambience_stereo(stem: str) -> np.ndarray:
    x, sr = sf.read(AMBIENCE / f"{stem}.ogg", always_2d=True, dtype="float64")
    if sr != RATE:
        x = signal.resample_poly(x, RATE, sr, axis=0)
    if x.shape[1] == 1:
        x = np.repeat(x, 2, axis=1)
    return x


def _load_takes(stem: str) -> list[np.ndarray]:
    takes = []
    for k in range(1, 9):
        p = AMBIENCE / f"{stem}_{k}.ogg"
        if not p.exists():
            break
        x, sr = sf.read(p, always_2d=True, dtype="float64")
        if sr != RATE:
            x = signal.resample_poly(x, RATE, sr, axis=0)
        takes.append(x)
    return takes


def _tile(bed: np.ndarray, n: int) -> np.ndarray:
    reps = int(np.ceil(n / len(bed)))
    return np.tile(bed, (reps, 1))[:n].copy()


def _mix_event(out: np.ndarray, clip_data: np.ndarray, at_s: float, gain_db: float, pan: float | None = None) -> None:
    a = int(at_s * RATE)
    if a >= len(out):
        return
    m = min(len(clip_data), len(out) - a)
    g = 10.0 ** (gain_db / 20.0)
    if clip_data.shape[1] == 1 or pan is not None:
        mono = clip_data[:m].mean(axis=1)
        p = 0.0 if pan is None else pan
        angle = (p + 1.0) * np.pi / 4.0
        out[a : a + m, 0] += mono * g * np.cos(angle)
        out[a : a + m, 1] += mono * g * np.sin(angle)
    else:
        out[a : a + m] += clip_data[:m] * g


def render_listening_previews(out_dir: Path, seconds: float = 180.0) -> None:
    """Renders 3-minute stereo listening clips for the Field, the Docks, and the
    Hall (with a mid-clip pass under the roof breach) using the exact in-game
    bed levels, low-pass muffling, and sparse industrial scheduler rules."""
    out_dir.mkdir(parents=True, exist_ok=True)
    n = int(seconds * RATE)
    rng = np.random.default_rng(4242)

    ind_specs = [
        ("ind_hammer", -21.0, 90.0),
        ("ind_shout", -23.0, 150.0),
        ("ind_reverse_alarm", -22.0, 120.0),
        ("ind_forklift_horn", -22.0, 90.0),
        ("ind_metal_drop", -20.5, 90.0),
        ("ind_impact_wrench", -22.0, 90.0),
        ("ind_roller_door", -21.0, 90.0),
        ("ind_pa_chime", -23.0, 90.0),
    ]
    ind_takes = {stem: _load_takes(stem) for stem, _, _ in ind_specs}

    def schedule_industrial(buf: np.ndarray, indoors: bool, dock_side: bool, log: list[str]) -> None:
        t = float(rng.uniform(14.0, 22.0))
        last_stem: str | None = None
        last_at: dict[str, float] = {}
        while t < seconds - 6.0:
            elig = [
                (stem, db, cd)
                for stem, db, cd in ind_specs
                if stem != last_stem and (t - last_at.get(stem, -1e9)) >= cd
            ]
            if not elig:
                t += 5.0
                continue
            weights = np.array(
                [
                    (1.8 if dock_side else 0.55)
                    if s == "ind_reverse_alarm"
                    else (1.4 if dock_side else 0.85)
                    if s == "ind_forklift_horn"
                    else 0.7
                    if s == "ind_shout"
                    else 1.0
                    for s, _, _ in elig
                ]
            )
            weights /= weights.sum()
            idx = int(rng.choice(len(elig), p=weights))
            stem, db, _ = elig[idx]
            take = ind_takes[stem][int(rng.integers(len(ind_takes[stem])))]
            if indoors:
                # Match Soundscape's indoor outdoor-bus 650 Hz LPF + 5.5 dB attenuation.
                take = lp(take.mean(axis=1), 650.0, order=2)[:, None]
                db -= 5.5
            pan = float(rng.uniform(-0.85, 0.85))
            _mix_event(buf, take, t, db, pan=pan)
            log.append(f"  {t:5.1f}s  {stem} ({db:.1f} dB, pan {pan:+.2f})")
            last_stem = stem
            last_at[stem] = t
            t += float(rng.uniform(25.0, 60.0))

    # 1. THE FIELD (3 min)
    field_buf = _tile(_load_ambience_stereo("warehouse_field"), n) * 10 ** (-15.5 / 20.0)
    field_log = ["Field 3-min mix events:"]
    gusts = _load_takes("field_gust")
    field_pos = [
        ("field_fence_rattle", -21.0, _load_takes("field_fence_rattle")),
        ("field_roof_bang", -21.5, _load_takes("field_roof_bang")),
        ("field_crow", -22.0, _load_takes("field_crow")),
    ]
    t = float(rng.uniform(6.0, 10.0))
    last_pos = -1
    while t < seconds - 4.0:
        if rng.random() < 0.45:
            take = gusts[int(rng.integers(len(gusts)))]
            _mix_event(field_buf, take, t, -13.5)
            field_log.append(f"  {t:5.1f}s  field_gust (-13.5 dB)")
        else:
            choices = [i for i in range(len(field_pos)) if i != last_pos]
            pick = int(rng.choice(choices))
            last_pos = pick
            stem, db, takes = field_pos[pick]
            pan = float(rng.uniform(-0.8, 0.8))
            _mix_event(field_buf, takes[int(rng.integers(len(takes)))], t, db, pan=pan)
            field_log.append(f"  {t:5.1f}s  {stem} ({db:.1f} dB, pan {pan:+.2f})")
        t += float(rng.uniform(7.0, 18.0))
    schedule_industrial(field_buf, indoors=False, dock_side=False, log=field_log)

    # 2. THE DOCKS (3 min)
    docks_buf = _tile(_load_ambience_stereo("warehouse_docks"), n) * 10 ** (-15.0 / 20.0)
    docks_log = ["Docks 3-min mix events:"]
    docks_pos = [
        ("docks_trailer_creak", -21.0, _load_takes("docks_trailer_creak")),
        ("docks_pallet_drop", -21.5, _load_takes("docks_pallet_drop")),
        ("docks_air_brakes", -21.0, _load_takes("docks_air_brakes")),
    ]
    t = float(rng.uniform(6.0, 10.0))
    last_pos = -1
    while t < seconds - 4.0:
        choices = [i for i in range(len(docks_pos)) if i != last_pos]
        pick = int(rng.choice(choices))
        last_pos = pick
        stem, db, takes = docks_pos[pick]
        pan = float(rng.uniform(-0.8, 0.8))
        _mix_event(docks_buf, takes[int(rng.integers(len(takes)))], t, db, pan=pan)
        docks_log.append(f"  {t:5.1f}s  {stem} ({db:.1f} dB, pan {pan:+.2f})")
        t += float(rng.uniform(7.0, 18.0))
    schedule_industrial(docks_buf, indoors=False, dock_side=True, log=docks_log)

    # 3. THE HALL (3 min; passes under the roof breach between 60 s and 115 s)
    hall_buf = _tile(_load_ambience_stereo("warehouse_hall"), n) * 10 ** (-14.0 / 20.0)
    ext_bleed = _tile(_load_ambience_stereo("warehouse_docks"), n) * 10 ** ((-15.0 - 9.0) / 20.0)
    ext_bleed = np.stack([lp(ext_bleed[:, 0], 650.0), lp(ext_bleed[:, 1], 650.0)], axis=1)
    hall_buf += ext_bleed
    # Roof breach wind bleed gently rises 60..115 s as if walking through Mid Stockpile.
    breach = _tile(_load_ambience_stereo("warehouse_breach_wind"), n) * 10 ** (-20.5 / 20.0)
    tt = np.arange(n) / RATE
    breach_env = np.clip((tt - 60.0) / 5.0, 0.0, 1.0) * np.clip((115.0 - tt) / 5.0, 0.0, 1.0)
    hall_buf += breach * breach_env[:, None]
    hall_log = ["Hall 3-min mix events (under roof breach 60–115 s):"]
    schedule_industrial(hall_buf, indoors=True, dock_side=False, log=hall_log)

    # Write listening WAVs + a +12 dB audition boost copy or note? Keep exact in-game
    # levels so loudness matches the live Ambience bus cleanly.
    for name, buf, log in (
        ("warehouse_field_3min.wav", field_buf, field_log),
        ("warehouse_docks_3min.wav", docks_buf, docks_log),
        ("warehouse_hall_3min.wav", hall_buf, hall_log),
    ):
        p = out_dir / name
        sf.write(p, np.clip(buf, -0.99, 0.99).astype(np.float32), RATE, subtype="PCM_16")
        rms_db = 20.0 * np.log10(np.sqrt(np.mean(buf**2)) + 1e-12)
        pk_db = 20.0 * np.log10(np.abs(buf).max() + 1e-12)
        print(f"  preview {p}: {seconds:.0f} s, RMS {rms_db:.1f} dBFS, peak {pk_db:.1f} dBFS")
        (out_dir / f"{p.stem}.txt").write_text("\n".join(log) + "\n")
