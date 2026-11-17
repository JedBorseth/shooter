# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow>=11"]
# ///
"""Draw Slamcity's sign atlas: billboards, posters, station signs, street
signs, magazine covers, and the like, all original artwork, packed into one
2048 x 2048 image so every picture on the map shares one material.

    uv run tools/build_slamcity_signs.py

Writes godot/assets/textures/slamcity/signs.png, signs.json (each picture's
rectangle as fractions of the sheet, read by tools/build_slamcity_city.py),
chainlink.png (the fence's wire pattern), and leaves.png (the street trees'
leaf cards). Fonts: Barlow Condensed from the
repo, and Pacifico and Monoton (OFL, downloaded to ~/.cache/shooter-fonts).
"""

from __future__ import annotations

import json
import math
import random
import urllib.request
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "godot/assets/textures/slamcity"
FONT_CACHE = Path.home() / ".cache/shooter-fonts"
SHEET = 2048
FONTS = {
    "bold": REPO / "godot/assets/fonts/BarlowCondensed-Bold.ttf",
    "black": REPO / "godot/assets/fonts/BarlowCondensed-ExtraBold.ttf",
    "script": FONT_CACHE / "Pacifico-Regular.ttf",
    "deco": FONT_CACHE / "Monoton-Regular.ttf",
}
DOWNLOADS = {
    "Pacifico-Regular.ttf": "https://github.com/google/fonts/raw/main/ofl/pacifico/Pacifico-Regular.ttf",
    "Monoton-Regular.ttf": "https://github.com/google/fonts/raw/main/ofl/monoton/Monoton-Regular.ttf",
}


def fetch_fonts():
    FONT_CACHE.mkdir(parents=True, exist_ok=True)
    for name, url in DOWNLOADS.items():
        path = FONT_CACHE / name
        if not path.exists():
            print(f"  downloading {url}")
            request = urllib.request.Request(url, headers={"User-Agent": "shooter-asset-pipeline"})
            with urllib.request.urlopen(request, timeout=60) as response:
                path.write_bytes(response.read())


def font(kind: str, size: int) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(str(FONTS[kind]), size)


def centred(d: ImageDraw.ImageDraw, box, text, fnt, fill, stroke=0, stroke_fill=None):
    x0, y0, x1, y1 = box
    l, t, r, b = d.textbbox((0, 0), text, font=fnt, stroke_width=stroke)
    d.text(((x0 + x1 - (r - l)) / 2 - l, (y0 + y1 - (b - t)) / 2 - t), text, font=fnt, fill=fill,
           stroke_width=stroke, stroke_fill=stroke_fill)


def fit(d, text, kind, width, height):
    """The biggest font of `kind` that fits text in width x height."""
    size = int(height)
    while size > 8:
        fnt = font(kind, size)
        l, t, r, b = d.textbbox((0, 0), text, font=fnt)
        if r - l <= width and b - t <= height:
            return fnt
        size -= 2
    return font(kind, 8)


def grime(img: Image.Image, amount=0.25, seed=0):
    """Weather a picture: soft dark blotches and a faded edge."""
    rng = random.Random(seed)
    w, h = img.size
    layer = Image.new("L", (w, h), 0)
    d = ImageDraw.Draw(layer)
    for _ in range(int(w * h / 4000) + 3):
        x, y = rng.uniform(0, w), rng.uniform(0, h)
        r = rng.uniform(4, max(6, min(w, h) / 6))
        d.ellipse((x - r, y - r, x + r, y + r), fill=int(rng.uniform(20, 90)))
    layer = layer.filter(ImageFilter.GaussianBlur(max(2, min(w, h) / 30)))
    dark = Image.new("RGB", (w, h), (30, 26, 22))
    mask = layer.point(lambda v: int(v * amount))
    img.paste(dark, (0, 0), mask)
    return img


# --- The pictures ---------------------------------------------------------------------
PICTURES: dict[str, tuple[int, int, callable]] = {}


def picture(name, w, h):
    def wrap(fn):
        PICTURES[name] = (w, h, fn)
        return fn
    return wrap


def gradient(w, h, top, bottom):
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / max(1, h - 1)
        d.line([(0, y), (w, y)], fill=tuple(int(a + (b - a) * t) for a, b in zip(top, bottom)))
    return img


@picture("billboard_cola", 1536, 512)
def billboard_cola(w, h):
    img = gradient(w, h, (200, 24, 30), (120, 8, 16))
    d = ImageDraw.Draw(img)
    rng = random.Random(1)
    for _ in range(70):
        x, y, r = rng.uniform(0, w), rng.uniform(0, h), rng.uniform(4, 22)
        d.ellipse((x - r, y - r, x + r, y + r), outline=(255, 140, 140), width=3)
    # A swoosh and a bottle.
    d.chord((-200, 300, 1300, 900), 200, 340, fill=(250, 245, 240))
    bx = 1250
    d.rounded_rectangle((bx, 120, bx + 150, 470), 40, fill=(70, 20, 15))
    d.rectangle((bx + 50, 40, bx + 100, 140), fill=(70, 20, 15))
    d.rectangle((bx + 45, 30, bx + 105, 52), fill=(210, 200, 190))
    d.rounded_rectangle((bx + 10, 240, bx + 140, 330), 10, fill=(250, 245, 240))
    centred(d, (bx + 10, 240, bx + 140, 330), "SLAM", font("black", 56), (200, 24, 30))
    d.line([(bx + 30, 150), (bx + 30, 450)], fill=(160, 70, 60), width=8)
    centred(d, (60, 40, 1180, 330), "Slam Cola", font("script", 210), (255, 255, 255), 6, (90, 0, 10))
    centred(d, (60, 360, 1180, 470), "TASTE THE CITY", font("black", 90), (120, 8, 16))
    return grime(img, 0.18, 1)


@picture("billboard_radio", 1536, 512)
def billboard_radio(w, h):
    img = gradient(w, h, (40, 16, 70), (12, 6, 30))
    d = ImageDraw.Draw(img)
    rng = random.Random(2)
    for i in range(40):
        x = 900 + i * 15
        bh = rng.uniform(40, 360)
        d.rectangle((x, 460 - bh, x + 10, 460), fill=(255, 80 + i * 4, 160))
    centred(d, (60, 30, 900, 330), "WSLM 98.6", font("black", 250), (255, 220, 80), 4, (120, 40, 0))
    centred(d, (60, 330, 900, 470), "THE SOUND OF SLAMCITY", font("bold", 92), (255, 255, 255))
    return grime(img, 0.18, 2)


@picture("poster_open", 200, 280)
def poster_open(w, h):
    img = Image.new("RGB", (w, h), (12, 12, 14))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((14, 60, w - 14, 200), 30, outline=(255, 60, 60), width=8)
    centred(d, (14, 60, w - 14, 200), "OPEN", font("black", 90), (255, 90, 80))
    centred(d, (0, 210, w, 270), "CASH FOR GOLD", font("bold", 30), (255, 220, 120))
    return img


@picture("sign_platform", 520, 200)
def sign_platform(w, h):
    img = Image.new("RGB", (w, h), (18, 18, 20))
    d = ImageDraw.Draw(img)
    d.rectangle((0, 22, w, 28), fill=(240, 240, 240))
    centred(d, (20, 40, w - 140, 170), "Slam Junction", fit(d, "Slam Junction", "bold", w - 180, 110), (245, 245, 245))
    for i, (letter, col) in enumerate((("S", (220, 40, 40)), ("J", (40, 150, 70)))):
        x = w - 120 + i * 58
        d.ellipse((x - 26, 80, x + 26, 132), fill=col)
        centred(d, (x - 26, 80, x + 26, 132), letter, font("black", 42), (255, 255, 255))
    return img


@picture("mosaic_junction", 1000, 200)
def mosaic_junction(w, h):
    img = Image.new("RGB", (w, h), (232, 222, 196))
    d = ImageDraw.Draw(img)
    rng = random.Random(3)
    t = 10
    for y in range(0, h, t):
        for x in range(0, w, t):
            v = rng.randint(-12, 12)
            border = y < 26 or y >= h - 26 or x < 26 or x >= w - 26
            base = (40, 110, 80) if border and ((x // t + y // t) % 3) else (232, 222, 196)
            if border and not ((x // t + y // t) % 3):
                base = (180, 70, 40)
            d.rectangle((x, y, x + t - 2, y + t - 2), fill=tuple(max(0, min(255, c + v)) for c in base))
    text = Image.new("L", (w, h), 0)
    centred(ImageDraw.Draw(text), (40, 30, w - 40, h - 30), "SLAM JUNCTION", font("black", 128), 255)
    tiles = Image.new("RGB", (w, h), (30, 80, 60))
    td = ImageDraw.Draw(tiles)
    for y in range(0, h, t):
        for x in range(0, w, t):
            v = rng.randint(-10, 10)
            td.rectangle((x, y, x + t - 2, y + t - 2), fill=(30 + v, 90 + v, 66 + v))
    mask = text.point(lambda v: 255 if v > 100 else 0)
    img.paste(tiles, (0, 0), mask)
    return grime(img, 0.15, 3)


@picture("route_map", 512, 352)
def route_map(w, h):
    img = Image.new("RGB", (w, h), (238, 236, 228))
    d = ImageDraw.Draw(img)
    rng = random.Random(4)
    lines = [((220, 40, 40), 0.25), ((40, 150, 70), 0.45), ((40, 90, 200), 0.65), ((240, 170, 20), 0.85)]
    for col, f in lines:
        pts = [(30, int(h * f))]
        x = 30
        while x < w - 40:
            x += rng.randint(40, 80)
            pts.append((min(x, w - 30), pts[-1][1] + rng.choice((-30, 0, 0, 30))))
        d.line(pts, fill=col, width=9, joint="curve")
        for p in pts:
            d.ellipse((p[0] - 7, p[1] - 7, p[0] + 7, p[1] + 7), fill=(255, 255, 255), outline=(20, 20, 20),
                      width=2)
    d.rectangle((0, 0, w, 30), fill=(20, 20, 24))
    centred(d, (0, 0, w, 30), "SLAMCITY TRANSIT  ·  SUBWAY MAP", font("bold", 24), (255, 255, 255))
    return img


@picture("poster_concert", 512, 352)
def poster_concert(w, h):
    img = gradient(w, h, (250, 200, 40), (240, 120, 30))
    d = ImageDraw.Draw(img)
    for y in range(0, h, 14):
        for x in range(0, w, 14):
            r = 2 + 4 * (x / w)
            d.ellipse((x - r, y - r, x + r, y + r), fill=(200, 60, 30))
    centred(d, (20, 30, w - 20, 170), "THE RIOT BELLS", fit(d, "THE RIOT BELLS", "black", w - 60, 96), (20, 18, 30), 3, (255, 240, 200))
    centred(d, (20, 180, w - 20, 250), "LIVE AT THE SLAM ROOM", font("bold", 48), (255, 255, 255))
    centred(d, (20, 260, w - 20, 320), "FRI 9PM  ·  NO COVER", font("bold", 40), (20, 18, 30))
    return grime(img, 0.25, 5)


def _sign(text, fg, bg, w, h, kind="black", border=None):
    img = Image.new("RGB", (w, h), bg)
    d = ImageDraw.Draw(img)
    if border:
        d.rectangle((6, 6, w - 7, h - 7), outline=border, width=6)
    lines = text.split("\n")
    lh = (h - 24) / len(lines)
    for i, line in enumerate(lines):
        box = (16, 12 + i * lh, w - 16, 12 + (i + 1) * lh)
        centred(d, box, line, fit(d, line, kind, w - 40, lh * 0.82), fg)
    return img


for _name, _w, _h, _text, _fg, _bg, _border in (
    ("sign_authorized", 420, 120, "AUTHORIZED\nPERSONNEL ONLY", (200, 20, 20), (245, 245, 240), (200, 20, 20)),
    ("sign_dispatch", 660, 96, "DISPATCH", (255, 255, 255), (30, 70, 140), None),
    ("sign_depot", 780, 180, "SLAMCITY TRANSIT\nDEPOT", (255, 255, 255), (20, 80, 55), (255, 255, 255)),
    ("sign_aisle", 360, 90, "AISLE 2 · SNACKS", (255, 255, 255), (200, 40, 40), None),
    ("sign_caution", 300, 210, "CAUTION\nSTEEP STAIRS", (20, 20, 20), (250, 200, 20), (20, 20, 20)),
    ("sign_cold", 560, 60, "ICE COLD DRINKS", (255, 255, 255), (30, 90, 180), None),
    ("sign_dumpster", 440, 140, "NO DUMPING\nVIOLATORS FINED", (255, 255, 255), (30, 30, 34), None),
    ("sign_fares", 240, 60, "FARES", (255, 255, 255), (20, 20, 24), None),
    ("sign_high_voltage", 180, 120, "DANGER\nHIGH VOLTAGE", (20, 20, 20), (250, 200, 20), (200, 20, 20)),
    ("sign_news", 660, 84, "NEWS · CANDY · COFFEE", (255, 230, 120), (20, 60, 40), None),
    ("sign_no_entry", 480, 240, "NO ENTRY\nTRACK AREA", (255, 255, 255), (200, 20, 20), (255, 255, 255)),
    ("sign_tokens", 600, 84, "TOKENS · INFORMATION", (255, 255, 255), (20, 20, 24), None),
    ("street_junction", 360, 80, "JUNCTION PL", (255, 255, 255), (20, 100, 60), (255, 255, 255)),
    ("street_slam_ave", 360, 80, "SLAM AVE", (255, 255, 255), (20, 100, 60), (255, 255, 255)),
    ("hang_entrance", 680, 120, "TO TRAINS · UPTOWN & DOWNTOWN", (255, 255, 255), (20, 20, 24), None),
    ("hang_junction", 680, 120, "SLAM JUNCTION · EXIT TO STREET", (255, 255, 255), (20, 20, 24), None),
):
    PICTURES[_name] = (_w, _h, (lambda t, fg, bg, b: lambda w, h: _sign(t, fg, bg, w, h, border=b))(
        _text, _fg, _bg, _border))


@picture("gantry_depot", 1280, 200)
def gantry_depot(w, h):
    img = Image.new("RGB", (w, h), (20, 100, 60))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((8, 8, w - 9, h - 9), 18, outline=(255, 255, 255), width=6)
    centred(d, (30, 20, 920, 180), "SLAMCITY TRANSIT DEPOT", fit(d, "SLAMCITY TRANSIT DEPOT", "black", 860, 110), (255, 255, 255))
    d.rounded_rectangle((950, 50, 1230, 150), 14, fill=(255, 255, 255))
    centred(d, (950, 50, 1230, 150), "BUSES ONLY", font("black", 64), (20, 100, 60))
    return grime(img, 0.12, 6)


@picture("sign_bus_stop", 180, 180)
def sign_bus_stop(w, h):
    img = Image.new("RGB", (w, h), (255, 255, 255))
    d = ImageDraw.Draw(img)
    d.ellipse((6, 6, w - 6, h - 6), fill=(200, 30, 30))
    d.rounded_rectangle((45, 40, 135, 110), 10, fill=(255, 255, 255))
    d.rectangle((52, 50, 128, 75), fill=(200, 30, 30))
    d.ellipse((55, 102, 75, 122), fill=(255, 255, 255))
    d.ellipse((105, 102, 125, 122), fill=(255, 255, 255))
    centred(d, (0, 125, w, 165), "BUS STOP", font("black", 30), (255, 255, 255))
    return img


@picture("clock_face", 192, 192)
def clock_face(w, h):
    img = Image.new("RGB", (w, h), (20, 20, 22))
    d = ImageDraw.Draw(img)
    d.ellipse((6, 6, w - 6, h - 6), fill=(245, 242, 232))
    c = w / 2
    for i in range(60):
        a = i * math.pi / 30
        r0 = 78 if i % 5 == 0 else 84
        d.line([(c + math.sin(a) * r0, c - math.cos(a) * r0), (c + math.sin(a) * 88, c - math.cos(a) * 88)],
               fill=(20, 20, 20), width=5 if i % 5 == 0 else 2)
    for a, r, wd in ((math.radians(10 * 30 + 15), 50, 8), (math.radians(8 * 6), 72, 5)):
        d.line([(c, c), (c + math.sin(a) * r, c - math.cos(a) * r)], fill=(20, 20, 20), width=wd)
    d.line([(c, c), (c + math.sin(2.1) * 80, c - math.cos(2.1) * 80)], fill=(200, 30, 30), width=2)
    return img


@picture("vending_front", 192, 400)
def vending_front(w, h):
    img = Image.new("RGB", (w, h), (30, 30, 36))
    d = ImageDraw.Draw(img)
    cols = [(220, 30, 30), (30, 120, 220), (40, 170, 70), (240, 180, 30), (240, 240, 240), (120, 40, 160)]
    for row in range(6):
        y = 12 + row * 64
        d.rectangle((6, y + 52, w - 6, y + 58), fill=(120, 120, 130))
        for k in range(5):
            x = 12 + k * 36
            col = cols[(row + k) % len(cols)]
            d.rounded_rectangle((x, y + 4, x + 26, y + 52), 6, fill=col)
            d.rectangle((x + 4, y + 20, x + 22, y + 34), fill=(250, 250, 250))
    return img


@picture("menu_board", 1024, 256)
def menu_board(w, h):
    img = Image.new("RGB", (w, h), (20, 22, 20))
    d = ImageDraw.Draw(img)
    centred(d, (0, 4, w, 60), "SLAM DINER · ALL DAY BREAKFAST", font("black", 48), (255, 210, 80))
    items = [("2 EGGS ANY STYLE", "4.50"), ("SHORT STACK", "5.25"), ("CHEESEBURGER", "7.95"),
             ("PATTY MELT", "8.25"), ("CHERRY PIE", "3.75"), ("BOTTOMLESS COFFEE", "1.95")]
    for i, (name, price) in enumerate(items):
        col, row = i % 2, i // 2
        x = 40 + col * 500
        y = 72 + row * 58
        d.text((x, y), name, font=font("bold", 40), fill=(245, 245, 235))
        d.text((x + 380, y), price, font=font("bold", 40), fill=(255, 210, 80))
    return img


@picture("cigarette_wall", 768, 276)
def cigarette_wall(w, h):
    img = Image.new("RGB", (w, h), (60, 50, 44))
    d = ImageDraw.Draw(img)
    rng = random.Random(8)
    cols = [(230, 230, 230), (200, 30, 30), (30, 60, 150), (240, 180, 30), (40, 120, 60), (20, 20, 20)]
    for row in range(5):
        y = 8 + row * 54
        d.rectangle((0, y + 46, w, y + 52), fill=(140, 130, 120))
        x = 6
        while x < w - 30:
            bw = rng.randint(22, 36)
            col = rng.choice(cols)
            d.rectangle((x, y + 6, x + bw, y + 46), fill=col)
            d.rectangle((x + 3, y + 26, x + bw - 3, y + 34), fill=(250, 250, 250) if col != (230, 230, 230) else (200, 30, 30))
            x += bw + 3
    return grime(img, 0.12, 8)


def _magazine(i):
    def draw(w, h):
        rng = random.Random(20 + i)
        bg = [(220, 40, 60), (30, 30, 34), (40, 120, 200), (250, 200, 40)][i]
        img = Image.new("RGB", (w, h), bg)
        d = ImageDraw.Draw(img)
        title = ["SLAM", "BEAT", "WHEELS", "CITY"][i]
        centred(d, (4, 4, w - 4, 52), title, fit(d, title, "black", w - 12, 46), (255, 255, 255))
        d.ellipse((20, 70, w - 20, h - 40), fill=tuple(min(255, c + 60) for c in bg))
        for k in range(3):
            y = h - 36 + k * 10
            d.rectangle((10, y, 10 + rng.randint(40, w - 20), y + 5), fill=(255, 255, 255))
        return img
    return draw


for _i in range(4):
    PICTURES[f"magazine_{_i}"] = (96, 136, _magazine(_i))


def _newspaper(i):
    def draw(w, h):
        img = Image.new("RGB", (w, h), (232, 228, 214))
        d = ImageDraw.Draw(img)
        centred(d, (4, 4, w - 4, 30), "THE SLAMCITY POST", fit(d, "THE SLAMCITY POST", "black", w - 12, 24), (20, 20, 20))
        d.line([(6, 32), (w - 6, 32)], fill=(20, 20, 20), width=2)
        head = ["TRAIN STALLS AGAIN", "PIT SWALLOWS AVENUE"][i]
        centred(d, (6, 36, w - 6, 70), head, fit(d, head, "black", w - 12, 30), (20, 20, 20))
        d.rectangle((8, 76, w // 2 - 4, h - 8), fill=(120, 120, 116))
        for k in range(8):
            y = 78 + k * 7
            d.line([(w // 2 + 4, y), (w - 8, y)], fill=(90, 90, 90), width=2)
        return img
    return draw


for _i in range(2):
    PICTURES[f"newspaper_{_i}"] = (152, 104, _newspaper(_i))


def _poster(i):
    def draw(w, h):
        rng = random.Random(40 + i)
        palettes = [((250, 230, 60), (20, 20, 20)), ((240, 240, 235), (20, 20, 20)), ((30, 60, 140), (255, 255, 255)),
                    ((220, 40, 120), (255, 255, 255)), ((250, 140, 30), (30, 10, 10)), ((40, 40, 44), (250, 210, 60))]
        bg, fg = palettes[i]
        img = Image.new("RGB", (w, h), bg)
        d = ImageDraw.Draw(img)
        if i == 0:
            centred(d, (8, 10, w - 8, 120), "RIOT", font("black", 110), fg)
            centred(d, (8, 120, w - 8, 210), "BELLS", font("black", 90), fg)
            d.ellipse((60, 220, w - 60, 340), outline=fg, width=10)
            centred(d, (8, 350, w - 8, 410), "SAT · SLAM ROOM", font("bold", 40), fg)
        elif i == 1:
            centred(d, (8, 10, w - 8, 90), "LOST CAT", font("black", 84), (200, 20, 20))
            # A sitting cat, drawn from circles.
            d.ellipse((90, 150, 230, 330), fill=fg)
            d.ellipse((115, 100, 205, 185), fill=fg)
            d.polygon([(120, 120), (130, 80), (150, 112)], fill=fg)
            d.polygon([(200, 120), (190, 80), (170, 112)], fill=fg)
            d.arc((200, 230, 290, 330), 180, 300, fill=fg, width=12)
            centred(d, (8, 340, w - 8, 380), "ANSWERS TO 'BISCUIT'", font("bold", 32), fg)
            centred(d, (8, 380, w - 8, 420), "REWARD · CALL 555-0144", font("bold", 30), fg)
        elif i == 2:
            centred(d, (8, 20, w - 8, 110), "VOTE", font("black", 100), fg)
            centred(d, (8, 110, w - 8, 230), "DELGADO", font("black", 92), (250, 210, 60))
            d.rectangle((20, 250, w - 20, 262), fill=fg)
            centred(d, (8, 280, w - 8, 400), "FIX THE TRAINS", fit(d, "FIX THE TRAINS", "bold", w - 30, 56), fg)
        elif i == 3:
            for k in range(14):
                r = 20 + k * 16
                d.ellipse((w / 2 - r, 200 - r, w / 2 + r, 200 + r), outline=(255, 200, 240) if k % 2 else bg, width=6)
            centred(d, (8, 10, w - 8, 90), "NIGHTSHIFT", fit(d, "NIGHTSHIFT", "black", w - 30, 80), fg)
            centred(d, (8, 340, w - 8, 410), "WAREHOUSE 9 · LATE", font("bold", 40), fg)
        elif i == 4:
            centred(d, (8, 20, w - 8, 140), "CIRCUS", fit(d, "CIRCUS", "deco", w - 30, 70), fg)
            for k in range(8):
                a = k * math.pi / 4
                d.polygon([(w / 2, 260), (w / 2 + math.cos(a) * 120, 260 + math.sin(a) * 120),
                           (w / 2 + math.cos(a + 0.3) * 120, 260 + math.sin(a + 0.3) * 120)], fill=(250, 240, 220))
            centred(d, (8, 380, w - 8, 420), "PIER 4 · ONE WEEK ONLY", font("bold", 32), fg)
        else:
            centred(d, (8, 20, w - 8, 140), "SALE", font("black", 130), fg)
            centred(d, (8, 160, w - 8, 300), "50%", font("black", 150), (240, 60, 60))
            centred(d, (8, 320, w - 8, 400), "EVERYTHING MUST GO", fit(d, "EVERYTHING MUST GO", "bold", w - 30, 40), fg)
        img = grime(img, 0.3, 40 + i)
        # Wrinkles from the paste.
        d = ImageDraw.Draw(img)
        for _ in range(6):
            y = rng.uniform(0, h)
            d.line([(0, y), (w, y + rng.uniform(-20, 20))], fill=tuple(int(c * 0.82) for c in bg), width=2)
        return img
    return draw


for _i in range(6):
    PICTURES[f"poster_{_i}"] = (320, 440, _poster(_i))


def _ad_shelter(i):
    def draw(w, h):
        if i == 0:
            img = gradient(w, h, (10, 10, 14), (60, 20, 50))
            d = ImageDraw.Draw(img)
            d.rounded_rectangle((120, 120, 220, 300), 20, fill=(200, 160, 220))
            d.rectangle((150, 80, 190, 124), fill=(30, 30, 30))
            centred(d, (10, 300, w - 10, 380), "NOIR", font("deco", 70), (240, 220, 255))
            return img
        img = gradient(w, h, (210, 20, 30), (120, 8, 16))
        d = ImageDraw.Draw(img)
        centred(d, (10, 30, w - 10, 160), "Slam", font("script", 110), (255, 255, 255))
        centred(d, (10, 150, w - 10, 260), "Cola", font("script", 110), (255, 255, 255))
        centred(d, (10, 300, w - 10, 370), "ICE COLD · ALL CITY", font("bold", 40), (255, 230, 230))
        return img
    return draw


for _i in range(2):
    PICTURES[f"ad_shelter_{_i}"] = (340, 400, _ad_shelter(_i))


def _ad_strip(i):
    def draw(w, h):
        texts = [("LEARN TO DRIVE A BUS", "THE CITY NEEDS YOU · 555-0199", (30, 90, 180)),
                 ("DR. ADEYEMI DENTAL", "SMILE, SLAMCITY · 555-0123", (40, 150, 120)),
                 ("SLAM COLA", "TASTE THE CITY", (200, 24, 30))]
        head, sub, bg = texts[i]
        img = Image.new("RGB", (w, h), bg)
        d = ImageDraw.Draw(img)
        centred(d, (10, 4, w - 10, h * 0.62), head, fit(d, head, "black", w - 20, h * 0.5), (255, 255, 255))
        centred(d, (10, h * 0.6, w - 10, h - 4), sub, fit(d, sub, "bold", w - 20, h * 0.3), (255, 240, 200))
        return img
    return draw


for _i in range(3):
    PICTURES[f"ad_strip_{_i}"] = (448, 100, _ad_strip(_i))


def _ghost_sign(i):
    """A faded sign painted on brick long ago: pale letters on worn paint."""
    def draw(w, h):
        rng = random.Random(70 + i)
        base = [(96, 44, 34), (58, 62, 70)][i]
        img = Image.new("RGB", (w, h), base)
        d = ImageDraw.Draw(img)
        # Mortar lines showing through the paint.
        for y in range(0, h, 18):
            d.line([(0, y), (w, y)], fill=tuple(int(c * 0.82) for c in base), width=2)
            off = 0 if (y // 18) % 2 else 22
            for x in range(off, w, 44):
                d.line([(x, y), (x, y + 18)], fill=tuple(int(c * 0.85) for c in base), width=2)
        ink = (222, 206, 170) if i == 0 else (210, 190, 120)
        lines = [("SLAM COLA", "script"), ("DELICIOUS · REFRESHING · 5¢", "bold")] if i == 0 else \
            [("HARTWELL'S", "black"), ("FURNITURE & STORAGE", "bold")]
        centred(d, (30, 20, w - 30, h * 0.66), lines[0][0], fit(d, lines[0][0], lines[0][1], w - 80, h * 0.55), ink)
        centred(d, (30, h * 0.66, w - 30, h - 20), lines[1][0], fit(d, lines[1][0], lines[1][1], w - 120, h * 0.24),
                ink)
        d.rectangle((14, 14, w - 15, h - 15), outline=ink, width=8)
        # Weather it heavily: blotches of bare brick where the paint is gone.
        layer = Image.new("L", (w, h), 0)
        ld = ImageDraw.Draw(layer)
        for _ in range(140):
            x, y, r = rng.uniform(0, w), rng.uniform(0, h), rng.uniform(8, 60)
            ld.ellipse((x - r, y - r, x + r, y + r), fill=int(rng.uniform(80, 200)))
        layer = layer.filter(ImageFilter.GaussianBlur(10))
        bare = Image.new("RGB", (w, h), tuple(int(c * 0.9) for c in base))
        img.paste(bare, (0, 0), layer)
        return grime(img, 0.25, 70 + i)
    return draw


for _i in range(2):
    PICTURES[f"ghost_sign_{_i}"] = (896, 280, _ghost_sign(_i))


@picture("food_truck_side", 640, 190)
def food_truck_side(w, h):
    img = gradient(w, h, (250, 190, 40), (230, 120, 30))
    d = ImageDraw.Draw(img)
    for k in range(0, w, 40):
        d.polygon([(k, h), (k + 20, h - 26), (k + 40, h)], fill=(200, 40, 40))
    centred(d, (20, 6, w - 20, 120), "Slam Tacos", fit(d, "Slam Tacos", "script", w - 60, 110), (255, 255, 255), 4,
            (140, 30, 20))
    centred(d, (20, 118, w - 20, 160), "STREET TACOS · BURRITOS · HORCHATA", font("bold", 30), (90, 20, 10))
    return grime(img, 0.12, 9)


def _truck_side(i):
    def draw(w, h):
        if i == 0:
            img = Image.new("RGB", (w, h), (238, 236, 230))
            d = ImageDraw.Draw(img)
            d.rectangle((0, h - 40, w, h - 26), fill=(200, 30, 30))
            centred(d, (20, 10, w - 20, 120), "SLAM MOVERS", fit(d, "SLAM MOVERS", "black", w - 80, 110),
                    (200, 30, 30))
            centred(d, (20, 120, w - 20, h - 44), "WE MOVE THE CITY · 555-0178", font("bold", 30), (40, 40, 44))
        else:
            img = Image.new("RGB", (w, h), (236, 232, 222))
            d = ImageDraw.Draw(img)
            for k in range(5):
                d.ellipse((30 + k * 60, 50, 110 + k * 60, 130), fill=(200, 150, 80), outline=(140, 90, 40), width=4)
            centred(d, (330, 20, w - 20, 120), "Fresh Bread", fit(d, "Fresh Bread", "script", w - 360, 100),
                    (30, 70, 140))
            centred(d, (330, 120, w - 20, 170), "DAILY · SINCE 1952", font("bold", 30), (40, 40, 44))
        return grime(img, 0.2, 80 + i)
    return draw


for _i in range(2):
    PICTURES[f"truck_side_{_i}"] = (640, 214, _truck_side(_i))


def _goods(i):
    """Package fronts for the shop shelves: invented brands, bold shapes."""
    def draw(w, h):
        palettes = [((220, 40, 40), (255, 220, 60)), ((30, 90, 180), (255, 255, 255)), ((40, 150, 70), (250, 240, 200)),
                    ((250, 170, 30), (120, 40, 10)), ((240, 240, 235), (200, 30, 30)), ((120, 50, 150), (250, 220, 90)),
                    ((30, 30, 34), (240, 120, 40)), ((250, 220, 70), (40, 90, 170))]
        names = ["CRUNCH", "BLU", "OATY", "SNAX", "FIZZ", "GRAPE", "JAVA", "CHIPS"]
        bg, fg = palettes[i]
        img = Image.new("RGB", (w, h), bg)
        d = ImageDraw.Draw(img)
        d.ellipse((w * 0.15, h * 0.4, w * 0.85, h * 0.9), fill=fg)
        centred(d, (2, 2, w - 2, h * 0.38), names[i], fit(d, names[i], "black", w - 6, h * 0.3), fg)
        d.rectangle((0, h - 8, w, h), fill=tuple(int(c * 0.6) for c in bg))
        return img
    return draw


for _i in range(8):
    PICTURES[f"goods_{_i}"] = (64, 88, _goods(_i))


def leaves(size=256):
    """A cluster of leaves with alpha, for the street trees' leaf cards."""
    rng = random.Random(12)
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    for _ in range(170):
        x, y = rng.gauss(size / 2, size / 5), rng.gauss(size / 2, size / 5)
        if not (10 < x < size - 10 and 10 < y < size - 10):
            continue
        a = rng.uniform(0, 2 * math.pi)
        r = rng.uniform(9, 16)
        g = rng.randint(70, 140)
        col = (int(g * 0.45), g, int(g * 0.3), 255)
        pts = [(x + math.cos(a) * r, y + math.sin(a) * r), (x + math.cos(a + 1.9) * r * 0.45, y + math.sin(a + 1.9) * r * 0.45),
               (x - math.cos(a) * r * 0.8, y - math.sin(a) * r * 0.8), (x + math.cos(a - 1.9) * r * 0.45, y + math.sin(a - 1.9) * r * 0.45)]
        d.polygon(pts, fill=col)
    return img


def chainlink(size=128):
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    for k in range(-1, 2):
        o = k * size
        d.line([(o, 0), (o + size, size)], fill=(255, 255, 255, 255), width=5)
        d.line([(o + size, 0), (o, size)], fill=(255, 255, 255, 255), width=5)
    return img


def main():
    fetch_fonts()
    OUT.mkdir(parents=True, exist_ok=True)
    sheet = Image.new("RGB", (SHEET, SHEET), (128, 128, 128))
    rects = {}
    # Shelf packing, tallest first: each picture goes on the first shelf with
    # room for it, or opens a new one.
    # Big pictures are drawn at their full size and packed smaller.
    def packed(w, h):
        f = 0.6 if w * h > 300_000 else 0.7 if w * h > 100_000 else 0.85 if w * h > 50_000 else 1.0
        return round(w * f), round(h * f)

    items = sorted(PICTURES.items(), key=lambda kv: (-packed(*kv[1][:2])[1], -packed(*kv[1][:2])[0]))
    pad = 6
    shelves: list[list[int]] = []  # [y, height, used width]
    for name, (dw, dh, draw) in items:
        w, h = packed(dw, dh)
        for shelf in shelves:
            if h + pad <= shelf[1] and shelf[2] + w + pad <= SHEET:
                break
        else:
            y = shelves[-1][0] + shelves[-1][1] if shelves else 0
            if y + h + pad > SHEET:
                raise SystemExit(f"the sign atlas is full at {name}")
            shelf = [y, h + 2 * pad, 0]
            shelves.append(shelf)
        x, y = shelf[2] + pad, shelf[0] + pad
        shelf[2] += w + 2 * pad
        img = draw(dw, dh).convert("RGB").resize((w, h), Image.LANCZOS)
        # Bleed the edges so mipmaps do not mix in neighbours.
        sheet.paste(img.resize((w + 2 * pad, h + 2 * pad)), (x - pad, y - pad))
        sheet.paste(img, (x, y))
        rects[name] = [(x + 0.5) / SHEET, (y + 0.5) / SHEET, (x + w - 0.5) / SHEET, (y + h - 0.5) / SHEET]
    sheet.save(OUT / "signs.png", optimize=True)
    (OUT / "signs.json").write_text(json.dumps(rects, indent=1, sort_keys=True) + "\n")
    chainlink().save(OUT / "chainlink.png")
    leaves().save(OUT / "leaves.png")
    print(f"wrote {len(rects)} pictures to {(OUT / 'signs.png').relative_to(REPO)}")


if __name__ == "__main__":
    main()
