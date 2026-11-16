#!/usr/bin/env python3
"""Synthesize the Prism Laser's sounds (standard library only).

Writes the gunshot layers the weapon view loads from
`godot/assets/audio/weapons/laser/` (`snap_N`, `body_N`, `tail_N`) plus the
secret wall's phase sound to `godot/assets/audio/player/phase.wav`.
Every file is generated, so the output is original and CC0.

    python3 tools/make_laser_sounds.py
"""
import math
from pathlib import Path
import random
import struct
import wave

RATE = 44100
ROOT = Path(__file__).resolve().parent.parent / 'godot/assets/audio'


def write(path, samples):
    path.parent.mkdir(parents=True, exist_ok=True)
    peak = max(1e-9, max(abs(s) for s in samples))
    scale = 0.89 / peak
    with wave.open(str(path), 'wb') as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(RATE)
        out.writeframes(b''.join(struct.pack('<h', int(s * scale * 32767)) for s in samples))


def sweep(duration, start_hz, end_hz, curve, decay, harmonics=(1.0,), vibrato=0.0, seed=0):
    """A pitch sweep from start_hz to end_hz with an exponential decay."""
    rng = random.Random(seed)
    samples, phase = [], 0.0
    count = int(duration * RATE)
    for i in range(count):
        t = i / RATE
        progress = (i / count) ** curve
        hz = start_hz * (end_hz / start_hz) ** progress
        hz *= 1.0 + vibrato * math.sin(2 * math.pi * 38.0 * t)
        phase += 2 * math.pi * hz / RATE
        value = sum(a * math.sin(phase * (n + 1)) for n, a in enumerate(harmonics))
        attack = min(1.0, t / 0.002)
        value *= attack * math.exp(-t * decay)
        value += rng.uniform(-1, 1) * 0.04 * math.exp(-t * decay * 3)
        samples.append(value)
    return samples


def shimmer(duration, base_hz, seed):
    """A decaying cluster of detuned high partials: the beam's ring-out."""
    rng = random.Random(seed)
    partials = [(base_hz * rng.uniform(0.98, 1.02) * m, rng.uniform(0, 6.28)) for m in (1, 1.5, 2.01, 3.03)]
    samples = []
    for i in range(int(duration * RATE)):
        t = i / RATE
        value = sum(math.sin(2 * math.pi * hz * t + p) / (k + 1) for k, (hz, p) in enumerate(partials))
        tremolo = 0.6 + 0.4 * math.sin(2 * math.pi * 23.0 * t)
        samples.append(value * tremolo * math.exp(-t * 9.0) * min(1.0, t / 0.01))
    return samples


def main():
    weapon = ROOT / 'weapons/laser'
    for n, (start, end) in enumerate([(5200, 900), (4800, 820), (5600, 980)], 1):
        # Snap: the bright "pew" zap.
        write(weapon / f'snap_{n}.wav',
              sweep(0.11, start, end, 0.55, 28.0, harmonics=(1.0, 0.35, 0.15), vibrato=0.02, seed=n))
        # Body: a low electric thump under it.
        write(weapon / f'body_{n}.wav',
              sweep(0.16, 420 + 40 * n, 70, 0.4, 22.0, harmonics=(1.0, 0.5, 0.25, 0.12), seed=10 + n))
        # Tail: a short shimmering ring-out.
        write(weapon / f'tail_{n}.wav', shimmer(0.42, 1300 + 90 * n, seed=20 + n))
    # Phasing through the secret wall: a rising, warbling whoosh.
    rise = sweep(0.9, 90, 1800, 1.6, 1.2, harmonics=(1.0, 0.6, 0.3, 0.2), vibrato=0.06, seed=99)
    write(ROOT / 'player/phase.wav', rise)


if __name__ == '__main__':
    main()
