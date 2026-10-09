#!/usr/bin/env python3
"""Render Hlas for Windows brand assets from the macOS design sources.

Outputs (committed, so the Windows build needs no Python):
  assets/onboarding/step-{0..3}.jpg   left panel of the welcome tour, 2x scale
  assets/tray-idle.rgba               32x32 RGBA tray icon
  assets/tray-recording.rgba          32x32 RGBA tray icon while recording
  assets/hlas.ico                     app icon, 16 to 256 px

Usage: python3 tools/render_assets.py <path-to-hlas-macos-checkout>
"""
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
MAC = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT.parent / "hlas"
RES = MAC / "Resources"
OUT = ROOT / "assets"

BRAND = (0, 230, 126)
INK = (15, 15, 15)

# The two-pulse Hlas mark, from Sources/Hlas/HlasMark.swift (source units).
MARK = [
    ("M", (478, 328)),
    ("C", (435, 328), (401, 362), (401, 405)),
    ("L", (401, 850)),
    ("C", (401, 893), (435, 927), (478, 927)),
    ("C", (521, 927), (556, 893), (556, 850)),
    ("L", (556, 711)),
    ("C", (556, 673), (580, 643), (616, 643)),
    ("L", (660, 643)),
    ("C", (692, 643), (715, 667), (715, 699)),
    ("L", (715, 738)),
    ("C", (715, 776), (745, 806), (783, 806)),
    ("C", (821, 806), (851, 776), (851, 738)),
    ("L", (851, 523)),
    ("C", (851, 485), (821, 455), (783, 455)),
    ("C", (745, 455), (715, 485), (715, 523)),
    ("L", (715, 557)),
    ("C", (715, 588), (692, 611), (661, 611)),
    ("L", (612, 611)),
    ("C", (580, 611), (556, 586), (556, 554)),
    ("L", (556, 405)),
    ("C", (556, 362), (521, 328), (478, 328)),
]
SRC = (401, 328, 450, 599)  # x, y, w, h


def mark_polygon(x, y, w, h):
    sx, sy, swid, shei = SRC
    s = min(w / swid, h / shei)
    ox = x + (w - swid * s) / 2
    oy = y + (h - shei * s) / 2
    pt = lambda p: (ox + (p[0] - sx) * s, oy + (p[1] - sy) * s)
    poly, cur = [], None
    for seg in MARK:
        if seg[0] in ("M", "L"):
            cur = seg[1]
            poly.append(pt(cur))
        else:
            c1, c2, end = seg[1], seg[2], seg[3]
            for i in range(1, 25):
                t = i / 24
                bx = (1 - t) ** 3 * cur[0] + 3 * (1 - t) ** 2 * t * c1[0] + 3 * (1 - t) * t ** 2 * c2[0] + t ** 3 * end[0]
                by = (1 - t) ** 3 * cur[1] + 3 * (1 - t) ** 2 * t * c1[1] + 3 * (1 - t) * t ** 2 * c2[1] + t ** 3 * end[1]
                poly.append(pt((bx, by)))
            cur = end
    return poly


def draw_mark(img, box, color, supersample=8):
    """Antialiased mark: draw large, downsample into place."""
    x, y, w, h = box
    big = Image.new("L", (int(w * supersample), int(h * supersample)), 0)
    ImageDraw.Draw(big).polygon(mark_polygon(0, 0, w * supersample, h * supersample), fill=255)
    mask = big.resize((int(w), int(h)), Image.LANCZOS)
    layer = Image.new("RGBA", mask.size, color + (255,))
    img.paste(layer, (int(x), int(y)), mask)


def font(name, size):
    return ImageFont.truetype(str(RES / "Fonts" / name), size)


def tracked(draw, xy, text, fnt, fill, tracking):
    x, y = xy
    for ch in text:
        draw.text((x, y), ch, font=fnt, fill=fill)
        x += draw.textlength(ch, font=fnt) + tracking
    return x


def cover(img, size):
    w, h = size
    s = max(w / img.width, h / img.height)
    img = img.resize((int(img.width * s) + 1, int(img.height * s) + 1), Image.LANCZOS)
    left = (img.width - w) // 2
    top = (img.height - h) // 2
    return img.crop((left, top, left + w, top + h))


def onboarding():
    W, H = 560, 1000  # 280 x 500 logical, rendered at 2x
    steps = ["Welcome", "Microphone", "Engine & language", "Try it"]
    photos = ["gradient-dark", "gradient-purple", "gradient-blue", "gradient-dark"]
    (OUT / "onboarding").mkdir(parents=True, exist_ok=True)
    for idx in range(4):
        base = cover(Image.open(RES / "Gradients" / f"{photos[idx]}.jpg").convert("RGB"), (W, H)).convert("RGBA")
        scrim = Image.new("RGBA", (W, H), INK + (int(255 * 0.38),))
        base = Image.alpha_composite(base, scrim)
        grad = Image.new("L", (1, H), 0)
        for yy in range(H):
            t = max(0.0, (yy - H * 0.35) / (H * 0.65))
            grad.putpixel((0, yy), int(255 * 0.88 * t))
        bottom = Image.new("RGBA", (W, H), INK + (0,))
        bottom.putalpha(grad.resize((W, H)))
        base = Image.alpha_composite(base, bottom)
        # Translucent shapes and text go on their own layer so alpha blends
        # instead of replacing the photo underneath.
        layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        d = ImageDraw.Draw(layer)

        pad = 48
        chip_h, gap = 76, 14
        chips_top = H - pad - 4 * chip_h - 3 * gap
        # Brand block above the step list.
        y = chips_top - 250
        draw_mark(layer, (pad, y + 6, 48, 64), BRAND)
        tracked(d, (pad + 72, y), "HLAS", font("Konect-Regular.otf", 72), (237, 239, 238), 16)
        tracked(d, (pad, y + 104), "DICTATION. NOT TYPING.", font("Satoshi-Bold.otf", 20), BRAND, 7)
        body = font("Satoshi-Medium.otf", 25)
        d.text((pad, y + 152), "Hold a key, speak, release.", font=body, fill=(255, 255, 255, 200))
        d.text((pad, y + 186), "Your words land at your cursor.", font=body, fill=(255, 255, 255, 200))

        label = font("Satoshi-Bold.otf", 25)
        small = font("Satoshi-Bold.otf", 20)
        for i, name in enumerate(steps):
            top = chips_top + i * (chip_h + gap)
            active, done = i == idx, i < idx
            fill = (240, 241, 240, 236) if active else INK + (100,)
            d.rounded_rectangle((pad, top, W - pad, top + chip_h), radius=20, fill=fill)
            cx, cy, r = pad + 44, top + chip_h // 2, 20
            circle = INK + (255,) if active else (255, 255, 255, 46)
            d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=circle)
            mark = "✓" if done else str(i + 1)
            mfont = font("Satoshi-Bold.otf", 20) if not done else ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial Unicode.ttf", 22)
            tw = d.textlength(mark, font=mfont)
            d.text((cx - tw / 2, cy - 13), mark, font=mfont, fill=(255, 255, 255, 235) if not active else (255, 255, 255))
            d.text((pad + 84, cy - 16), name, font=label, fill=INK if active else (255, 255, 255, 220))
        base = Image.alpha_composite(base, layer)
        base.convert("RGB").save(OUT / "onboarding" / f"step-{idx}.jpg", quality=86, optimize=True)


def tray(recording):
    size = 32
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    if recording:
        ImageDraw.Draw(img).ellipse((0, 0, size - 1, size - 1), fill=BRAND + (255,))
        draw_mark(img, (8, 5, 16, 22), INK, supersample=16)
    else:
        draw_mark(img, (6, 2, 20, 28), BRAND, supersample=16)
    return img


def icon():
    big = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    d = ImageDraw.Draw(big)
    d.rounded_rectangle((26, 16, 998, 998), radius=234, fill=INK + (255,))
    draw_mark(big, (327, 268, 368, 489), BRAND, supersample=4)
    big.save(OUT / "hlas.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])


def main():
    onboarding()
    for name, rec in (("tray-idle", False), ("tray-recording", True)):
        (OUT / f"{name}.rgba").write_bytes(tray(rec).tobytes())
        tray(rec).resize((128, 128), Image.NEAREST).save(f"/tmp/{name}-preview.png")
    icon()
    print("assets rendered")


if __name__ == "__main__":
    main()
