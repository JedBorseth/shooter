# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Builds the body's movement sounds from real recordings.

What a jump, a landing, a mantle, a vault, and the weapon coming back up out
of a sprint sound like, the way tools/build_gunshots.py builds a gun: real
CC0 recordings, fetched and checked against pinned hashes, cut, layered and
processed in code (EQ, compression, loudness). Nothing is synthesized.

Modern Call of Duty (Modern Warfare 2019 onward) is the reference:

- a jump is a boot scuffing off the ground and the kit jostling, short;
- a landing is the boots' weight and the kit coming down after them;
- a mantle is two hands slapping onto the ledge, the body and boots
  scraping up over it, cloth, and kit;
- a vault is one hand planted on the cover and the body swinging over it
  (cloth and kit), quicker than a mantle;
- the end of a sprint is the weapon coming back up into the hands: a cloth
  settle and a handling clack.

Each sound is a few takes, each layered from cuts of the recordings below
(``SOUNDS``). Every take is mono (the game plays other players' copies in
3D), high-passed, lightly compressed so the hit reads, and set to the
sound's loudness (``Sound.lufs``) so the game's levels for each sound hold.

Sources are Freesound sounds released under CC0. The tool downloads each
one's high-quality preview (no account needed) and checks its hash. If the
original file has been downloaded from Freesound (it is named
``<id>__<user>__<title>.<ext>``), pass its folder with ``--originals`` and
the original is used instead: same timeline, full quality.

    uv run tools/build_movement_audio.py build [--originals ~/Downloads]
    uv run tools/build_movement_audio.py preview OUT.mp3 [--old DIR]

``preview`` writes a listening clip: each sound's takes one after another,
old then new, loudness-matched (``--old`` is an audio folder with the old
files, such as a checkout of main's ``godot/assets/audio``). Then run ``godot --headless --path godot
--import`` to import the new files.
"""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import tempfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import pyloudnorm
import soundfile as sf
from scipy import signal

RATE = 48_000
REPO = Path(__file__).resolve().parent.parent
AUDIO = REPO / "godot/assets/audio"
CACHE = Path.home() / ".cache/shooter-movement-audio"


@dataclass(frozen=True)
class Source:
    user: str
    title: str
    preview: str
    sha256: str

    def page(self, sid: int) -> str:
        return f"https://freesound.org/s/{sid}/"


# Every recording used, by Freesound id. All CC0 (checked on each page).
SOURCES: dict[int, Source] = {
    558477: Source(
        "Nox_Sound",
        "Footsteps_Mountain_Boots_Rock_Jump_Sequence_Mono.wav",
        "https://cdn.freesound.org/previews/558/558477_9250976-hq.ogg",
        "47da9ead3f88cb09aff6b104fac994fc27050e8671e5ad80a5460ce5c380a322",
    ),
    614064: Source(
        "magnuswaker",
        "Bodyfall",
        "https://cdn.freesound.org/previews/614/614064_11537497-hq.ogg",
        "042b4622ef462471a9d8ba7f1f73f46ed4cd5b0e5808029f28ce6c3fc88861c9",
    ),
    430599: Source(
        "Juandamb",
        "Hands on a wall",
        "https://cdn.freesound.org/previews/430/430599_7876490-hq.ogg",
        "391d4506bda35ce0e3cccd8c14f554386d9ae0e6f5b652f6fdb92ac09f032741",
    ),
    812599: Source(
        "modusmogulus",
        "Recording of a Ledge Climb (climbing to a concrete roof from a wall)",
        "https://cdn.freesound.org/previews/812/812599_15956618-hq.ogg",
        "28a885456577c920b2885611d815a7077b7af12aadd70213fd95bb5527cfce5d",
    ),
    658410: Source(
        "IENBA",
        "Nylon Jacket or Raincoat",
        "https://cdn.freesound.org/previews/658/658410_9616576-hq.ogg",
        "e020d697f0491f7f29fc242b188ca590d9e593749a9c9d983f4d410cc492b6c0",
    ),
    719241: Source(
        "serøutōnin--deprivəd",
        "A rifle being moved around and rattled constantly",
        "https://cdn.freesound.org/previews/719/719241_7157894-hq.ogg",
        "12f09d93f09b00e2c58519e6489e06c0b6bdc1f1f72d2190cff40467bd6037b5",
    ),
    393732: Source(
        "jackthemurray",
        "AR15 M4 Gun Hardware Magazine Movement Sounds",
        "https://cdn.freesound.org/previews/393/393732_5618682-hq.ogg",
        "ed97e7f463b2d91c20ecaaf674d13554ac6e6966c6eac1c8134679d3105eb8a1",
    ),
}


@dataclass(frozen=True)
class Cut:
    """A slice of a recording, placed in a take."""

    src: int
    start: float
    end: float
    # Where it starts in the take, seconds, and its level, dB.
    at: float = 0.0
    db: float = 0.0
    # Band limits (0: none), Hz.
    hp: float = 0.0
    lp: float = 0.0
    fade_out: float = 0.04


@dataclass(frozen=True)
class Sound:
    path: str
    takes: tuple[tuple[Cut, ...], ...]
    # Integrated loudness of each take (padded to 0.5 s), LUFS.
    lufs: float
    # Shelves: below 150 Hz and above 4 kHz, dB.
    low_db: float = 0.0
    high_db: float = 0.0
    # Compressor threshold under each take's peak, dB, and ratio.
    squash_db: float = -14.0
    ratio: float = 3.0


NOX = 558477  # boots: jump and land on rock, six takes
DROP = 614064  # heavy boots dropped
HANDS = 430599  # hands slapping a wall
LEDGE = 812599  # climbing onto a concrete roof
NYLON = 658410  # nylon jacket movement
RATTLE = 719241  # a rifle moved and rattled
HARDWARE = 393732  # AR-15 hardware and magazine handled

# The push-off peaks and landings in the Nox_Sound sequence.
PUSH = (0.181, 1.698, 3.170, 4.689, 6.259, 7.601)
LAND = (0.469, 2.002, 3.433, 4.961, 6.514, 7.883)
# Clean single hand slaps.
SLAPS = (0.638, 1.137, 5.204, 1.649, 0.104, 5.685)


def jump(i: int, rattle: float, cloth: float) -> tuple[Cut, ...]:
    p = PUSH[i]
    return (
        Cut(NOX, p - 0.06, p + 0.2, 0.0, 0.0, hp=150),
        Cut(RATTLE, rattle - 0.005, rattle + 0.16, 0.05, -5.0, hp=300),
        Cut(NYLON, cloth, cloth + 0.24, 0.0, -9.0, hp=200, fade_out=0.12),
    )


def landing(i: int, weight: float, clank: float, rattle: float) -> tuple[Cut, ...]:
    land = LAND[i]
    return (
        Cut(NOX, land - 0.005, land + 0.38, 0.0, 0.0, hp=50),
        Cut(DROP, weight - 0.003, weight + 0.25, 0.0, -7.0, hp=45, lp=1_500),
        Cut(HARDWARE, clank - 0.003, clank + 0.12, 0.012, -3.0, hp=400),
        Cut(RATTLE, rattle - 0.005, rattle + 0.2, 0.05, -8.0, hp=300),
    )


def mantle(a: int, b: int, gap: float, rattle: float, cloth: float) -> tuple[Cut, ...]:
    return (
        Cut(HANDS, SLAPS[a] - 0.004, SLAPS[a] + 0.22, 0.0, 0.0, hp=100),
        Cut(HANDS, SLAPS[b] - 0.004, SLAPS[b] + 0.22, gap, -2.0, hp=100),
        Cut(LEDGE, 0.6, 1.02, 0.14, -5.0, hp=120, fade_out=0.12),
        Cut(NYLON, cloth, cloth + 0.45, 0.04, -9.0, hp=200, fade_out=0.2),
        Cut(RATTLE, rattle - 0.005, rattle + 0.2, 0.2, -9.0, hp=300),
    )


def vault(a: int, scuff: float, rattle: float, cloth: float) -> tuple[Cut, ...]:
    return (
        Cut(HANDS, SLAPS[a] - 0.004, SLAPS[a] + 0.2, 0.0, 0.0, hp=100),
        Cut(NYLON, cloth, cloth + 0.3, 0.03, -6.0, hp=200, fade_out=0.15),
        Cut(LEDGE, scuff, scuff + 0.12, 0.09, -9.0, hp=150),
        Cut(RATTLE, rattle - 0.005, rattle + 0.18, 0.1, -7.0, hp=300),
    )


def sprint_out(clack: float, cloth: float, rattle: float) -> tuple[Cut, ...]:
    return (
        Cut(NYLON, cloth, cloth + 0.26, 0.0, -8.0, hp=200, fade_out=0.14),
        Cut(RATTLE, rattle - 0.005, rattle + 0.12, 0.03, -9.0, hp=300),
        Cut(HARDWARE, clack - 0.003, clack + 0.14, 0.09, 0.0, hp=250),
    )


SOUNDS: dict[str, Sound] = {
    "jump": Sound(
        "player/jump",
        (
            jump(1, 4.010, 3.378),
            jump(2, 6.649, 6.537),
            jump(4, 0.196, 0.465),
            jump(5, 7.372, 10.044),
        ),
        lufs=-19.0,
    ),
    "land": Sound(
        "player/land",
        (
            landing(0, 0.023, 1.043, 4.010),
            landing(1, 0.133, 5.008, 6.649),
            landing(3, 0.023, 9.213, 12.280),
            landing(5, 0.133, 16.683, 0.196),
        ),
        lufs=-18.0,
        squash_db=-20.0,
        ratio=4.0,
    ),
    "mantle": Sound(
        "player/mantle",
        (
            mantle(0, 3, 0.08, 4.010, 1.160),
            mantle(1, 5, 0.1, 6.649, 6.133),
            mantle(2, 4, 0.065, 12.098, 3.378),
        ),
        lufs=-18.0,
        squash_db=-20.0,
        ratio=4.0,
    ),
    "vault": Sound(
        "player/vault",
        (
            vault(0, 0.001, 0.196, 10.044),
            vault(2, 0.322, 6.649, 6.537),
            vault(1, 1.133, 4.010, 0.465),
        ),
        lufs=-18.0,
        squash_db=-20.0,
        ratio=4.0,
    ),
    "sprint_out": Sound(
        "foley/sprint_out",
        (
            sprint_out(14.029, 4.151, 4.723),
            sprint_out(16.203, 6.537, 7.372),
            sprint_out(6.966, 9.484, 0.196),
            sprint_out(18.148, 0.465, 11.326),
        ),
        lufs=-19.0,
    ),
}

# Files a sound replaces (the old single landing).
REPLACED = {"land": ("player/land.ogg",)}


# --- Recordings ---------------------------------------------------------------


def fetch(sid: int, cache: Path, originals: Path | None) -> np.ndarray:
    """The recording, mono at ``RATE``: the original if downloaded, else the
    pinned preview."""
    if originals is not None:
        found = sorted(originals.glob(f"{sid}__*"))
        if found:
            print(f"  {sid}: original {found[0].name}")
            return mono48(found[0])
    src = SOURCES[sid]
    path = cache / f"{sid}.ogg"
    if not path.exists():
        cache.mkdir(parents=True, exist_ok=True)
        request = urllib.request.Request(
            src.preview, headers={"User-Agent": "Mozilla/5.0"}
        )
        with urllib.request.urlopen(request, timeout=120) as response:
            path.write_bytes(response.read())
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != src.sha256:
        raise SystemExit(f"{sid}: hash {digest} is not the pinned {src.sha256}")
    return mono48(path)


def mono48(path: Path) -> np.ndarray:
    x, rate = sf.read(path, always_2d=True, dtype="float64")
    x = x.mean(1)
    if rate != RATE:
        g = np.gcd(rate, RATE)
        x = signal.resample_poly(x, RATE // g, rate // g)
    return x


# --- Processing ---------------------------------------------------------------


def band(x: np.ndarray, hp: float, lp: float) -> np.ndarray:
    if hp > 0:
        x = signal.sosfiltfilt(signal.butter(2, hp, "high", fs=RATE, output="sos"), x)
    if lp > 0:
        x = signal.sosfiltfilt(signal.butter(2, lp, "low", fs=RATE, output="sos"), x)
    return x


def shelf(x: np.ndarray, hz: float, gain_db: float, high: bool) -> np.ndarray:
    """RBJ shelving filter."""
    if gain_db == 0:
        return x
    a = 10 ** (gain_db / 40)
    w = 2 * np.pi * hz / RATE
    al = np.sin(w) / 2 * np.sqrt(2)
    c, s = np.cos(w), 2 * np.sqrt(a) * al
    if high:
        b = [
            a * ((a + 1) + (a - 1) * c + s),
            -2 * a * ((a - 1) + (a + 1) * c),
            a * ((a + 1) + (a - 1) * c - s),
        ]
        d = [
            (a + 1) - (a - 1) * c + s,
            2 * ((a - 1) - (a + 1) * c),
            (a + 1) - (a - 1) * c - s,
        ]
    else:
        b = [
            a * ((a + 1) - (a - 1) * c + s),
            2 * a * ((a - 1) - (a + 1) * c),
            a * ((a + 1) - (a - 1) * c - s),
        ]
        d = [
            (a + 1) + (a - 1) * c + s,
            -2 * ((a - 1) + (a + 1) * c),
            (a + 1) + (a - 1) * c - s,
        ]
    return signal.lfilter(np.array(b) / d[0], np.array(d) / d[0], x)


def squash(
    x: np.ndarray,
    threshold_db: float,
    ratio: float,
    attack: float = 0.002,
    release: float = 0.08,
) -> np.ndarray:
    """Peak compressor relative to the take's own peak (no makeup gain)."""
    level = np.abs(x) / max(np.abs(x).max(), 1e-9)
    a, r = np.exp(-1 / (attack * RATE)), np.exp(-1 / (release * RATE))
    env = np.empty_like(level)
    c = 0.0
    for i, v in enumerate(level):
        k = a if v > c else r
        c = k * c + (1 - k) * v
        env[i] = c
    over = np.maximum(20 * np.log10(np.maximum(env, 1e-9)) - threshold_db, 0)
    return x * 10 ** (-over * (1 - 1 / ratio) / 20)


def faded(x: np.ndarray, fade_out: float) -> np.ndarray:
    x = x.copy()
    n_in = min(int(0.003 * RATE), len(x) // 4)
    x[:n_in] *= np.linspace(0, 1, n_in)
    n_out = min(int(fade_out * RATE), len(x) // 2)
    x[-n_out:] *= 0.5 + 0.5 * np.cos(np.linspace(0, np.pi, n_out))
    return x


def loudness(x: np.ndarray) -> float:
    pad = np.concatenate([x, np.zeros(max(0, int(0.5 * RATE) - len(x)))])
    return pyloudnorm.Meter(RATE, block_size=0.1).integrated_loudness(pad)


def take(
    sound: Sound, cuts: tuple[Cut, ...], recordings: dict[int, np.ndarray]
) -> np.ndarray:
    parts = []
    for cut in cuts:
        x = recordings[cut.src]
        # Filter with margin, then cut, so the filters do not ring at the edges.
        a, b = int(cut.start * RATE), int(cut.end * RATE)
        m = int(0.05 * RATE)
        seg = band(x[max(a - m, 0) : b + m], cut.hp, cut.lp)[a - max(a - m, 0) :][
            : b - a
        ]
        seg = faded(seg / max(np.abs(seg).max(), 1e-9), cut.fade_out) * 10 ** (
            cut.db / 20
        )
        parts.append((int(cut.at * RATE), seg))
    out = np.zeros(max(at + len(seg) for at, seg in parts))
    for at, seg in parts:
        out[at : at + len(seg)] += seg
    out = shelf(out, 150, sound.low_db, high=False)
    out = shelf(out, 4_000, sound.high_db, high=True)
    out = squash(out, sound.squash_db, sound.ratio)
    out = faded(out, 0.03)
    # Level to the target, limiting the transients (a hand slap, a boot on
    # rock) that would clip; repeat until the limited take is on target.
    for _ in range(4):
        out = limit(out * 10 ** ((sound.lufs - loudness(out)) / 20))
    return out


def limit(
    x: np.ndarray,
    ceiling_db: float = -1.0,
    lookahead: float = 0.003,
    release: float = 0.06,
) -> np.ndarray:
    """Look-ahead peak limiter: the gain falls before a peak arrives and
    recovers over ``release``."""
    ceiling = 10 ** (ceiling_db / 20)
    need = np.minimum(1.0, ceiling / np.maximum(np.abs(x), 1e-9))
    k = int(lookahead * RATE)
    # The lowest gain needed in the next ``lookahead`` seconds.
    ahead = np.lib.stride_tricks.sliding_window_view(
        np.concatenate([need, np.ones(k)]), k + 1
    ).min(1)
    r = np.exp(-1 / (release * RATE))
    a = np.exp(-1 / (lookahead / 3 * RATE))
    gain = np.empty_like(ahead)
    g = 1.0
    for i, v in enumerate(ahead):
        g = a * g + (1 - a) * v if v < g else r * g + (1 - r) * v
        gain[i] = min(g, v)
    return x * gain


def build_all(cache: Path, originals: Path | None) -> dict[str, list[np.ndarray]]:
    recordings = {sid: fetch(sid, cache, originals) for sid in SOURCES}
    return {
        name: [take(s, cuts, recordings) for cuts in s.takes]
        for name, s in SOUNDS.items()
    }


def write_ogg(path: Path, x: np.ndarray) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    sf.write(
        path, np.clip(x, -1, 1).astype(np.float32), RATE, format="OGG", subtype="VORBIS"
    )


def build(cache: Path, originals: Path | None) -> None:
    for name, takes in build_all(cache, originals).items():
        sound = SOUNDS[name]
        folder = AUDIO / Path(sound.path).parent
        stem = Path(sound.path).name
        # Takes beyond the new count go; the rest are rewritten in place, so
        # their imports (and uids) stay.
        for old in folder.glob(f"{stem}_*.ogg"):
            number = old.stem.removeprefix(f"{stem}_")
            if number.isdigit() and int(number) > len(takes):
                old.unlink()
                old.with_suffix(".ogg.import").unlink(missing_ok=True)
        for rel in REPLACED.get(name, ()):
            (AUDIO / rel).unlink(missing_ok=True)
            (AUDIO / f"{rel}.import").unlink(missing_ok=True)
        for i, x in enumerate(takes, 1):
            write_ogg(folder / f"{stem}_{i}.ogg", x)
        print(f"  {sound.path}: {len(takes)} takes, {SOUNDS[name].lufs} LUFS")


# --- Listening clips ------------------------------------------------------------


def old_takes(folder: Path, path: str) -> list[np.ndarray]:
    stem = folder / path
    files = [f for f in [stem.with_suffix(".ogg")] if f.exists()]
    files += sorted(stem.parent.glob(f"{stem.name}_[0-9].ogg"))
    return [mono48(f) for f in files]


# What the game played before a sound existed: a vault played the mantle.
OLD_STANDIN = {"vault": "player/mantle"}
# A jump's air time before its landing in the jump-and-land section, seconds.
AIR = 0.55


def preview(
    out: Path,
    old: Path | None,
    cache: Path,
    originals: Path | None,
    only: list[str] | None = None,
) -> None:
    """For each sound, the old takes, a pause, then the new takes; every take
    at the new sound's loudness so only the character differs. With both the
    jump and the landing, then a jump and its landing together, old then new."""
    new = build_all(cache, originals)
    names = only or list(new)
    gap = np.zeros(int(0.45 * RATE))
    pause = np.zeros(int(1.4 * RATE))
    clip: list[np.ndarray] = []

    def at(x: np.ndarray, name: str) -> np.ndarray:
        return x * 10 ** ((SOUNDS[name].lufs - loudness(x)) / 20)

    def before(name: str) -> list[np.ndarray]:
        return old_takes(old, OLD_STANDIN.get(name, SOUNDS[name].path)) if old else []

    for name in names:
        olds = before(name)
        for group in (olds, new[name]):
            for x in group:
                clip += [at(x, name), gap]
            if group:
                clip.append(pause)
        print(f"  {name}: {len(olds)} old, {len(new[name])} new")
    if "jump" in names and "land" in names:
        for jumps, lands in (
            (before("jump"), before("land")),
            (new["jump"], new["land"]),
        ):
            for i in range(min(len(jumps), 3)):
                j, land = at(jumps[i], "jump"), at(lands[i % len(lands)], "land")
                pair = np.zeros(int(AIR * RATE) + len(land))
                pair[: len(j)] += j
                pair[int(AIR * RATE) :] += land
                clip += [pair, gap]
            clip.append(pause)
        print("  jump and land: old, new")
    mix = np.concatenate(clip) * 10 ** (-3 / 20)
    with tempfile.TemporaryDirectory() as tmp:
        wav = Path(tmp) / "clip.wav"
        sf.write(wav, mix.astype(np.float32), RATE)
        out.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["ffmpeg", "-y", "-loglevel", "error", "-i", wav, "-b:a", "192k", out],
            check=True,
        )
    print(f"wrote {out}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("build", "preview"):
        p = sub.add_parser(name)
        p.add_argument("--cache", type=Path, default=CACHE)
        p.add_argument("--originals", type=Path, default=None)
        if name == "preview":
            p.add_argument("out", type=Path)
            p.add_argument(
                "--old",
                type=Path,
                default=None,
                help="an audio folder with the old sounds",
            )
            p.add_argument(
                "--only",
                action="append",
                choices=list(SOUNDS),
                help="one sound (repeatable)",
            )
    args = parser.parse_args()
    if args.command == "build":
        build(args.cache, args.originals)
    else:
        preview(args.out, args.old, args.cache, args.originals, args.only)


if __name__ == "__main__":
    main()
