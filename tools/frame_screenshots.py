"""Put the app screenshots on a desktop-style wallpaper for the README.

Reads docs/screenshots/raw/*.jpg and writes docs/screenshots/*.jpg:
one framed image per window, plus desktop.jpg with Home, Notes and the
caption overlay together. Needs Pillow only.

    python tools/frame_screenshots.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
RAW = ROOT / "docs" / "screenshots" / "raw"
OUT = ROOT / "docs" / "screenshots"

RADIUS = 10  # Windows 11 window corners
SHADOW_BLUR = 28
SHADOW_OFFSET = (0, 14)
SHADOW_ALPHA = 120


def wallpaper(size):
    """A soft dark-blue gradient with a few blurred colour blobs."""
    w, h = size
    small = Image.new("RGB", (4, 3))
    colours = [
        (18, 28, 64), (32, 44, 96), (22, 60, 104), (14, 22, 52),
        (40, 52, 120), (30, 86, 132), (12, 18, 44), (26, 40, 92),
        (20, 70, 110), (16, 24, 58), (28, 36, 88), (14, 30, 66),
    ]
    small.putdata(colours)
    img = small.resize((w, h), Image.BICUBIC)

    blobs = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(blobs)
    for cx, cy, r, colour in [
        (0.78, 0.22, 0.30, (90, 120, 230, 90)),
        (0.18, 0.80, 0.34, (40, 150, 190, 80)),
        (0.55, 0.95, 0.25, (120, 90, 210, 60)),
    ]:
        x, y, rad = cx * w, cy * h, r * max(w, h)
        d.ellipse((x - rad, y - rad, x + rad, y + rad), fill=colour)
    blobs = blobs.filter(ImageFilter.GaussianBlur(max(w, h) // 8))
    img = Image.alpha_composite(img.convert("RGBA"), blobs)
    return img


def rounded(window, radius=RADIUS):
    mask = Image.new("L", window.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, *window.size), radius, fill=255)
    out = window.convert("RGBA")
    out.putalpha(mask)
    return out


def paste_window(canvas, window, xy):
    """Paste a rounded window with a soft drop shadow at xy (top-left)."""
    win = rounded(window)
    x, y = xy
    pad = SHADOW_BLUR * 3
    shadow = Image.new("RGBA", (win.width + pad * 2, win.height + pad * 2), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        (pad, pad, pad + win.width, pad + win.height), RADIUS, fill=(0, 0, 0, SHADOW_ALPHA)
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(SHADOW_BLUR))
    canvas.alpha_composite(shadow, (x - pad + SHADOW_OFFSET[0], y - pad + SHADOW_OFFSET[1]))
    canvas.alpha_composite(win, (x, y))


def scaled(img, factor):
    return img.resize((round(img.width * factor), round(img.height * factor)), Image.LANCZOS)


def save(canvas, name):
    canvas.convert("RGB").save(OUT / name, quality=88, optimize=True)
    print(OUT / name)


def framed(name, margin=110):
    window = Image.open(RAW / name)
    canvas = wallpaper((window.width + margin * 2, window.height + margin * 2))
    paste_window(canvas, window, (margin, margin))
    save(canvas, name)


def desktop():
    canvas = wallpaper((2400, 1500))
    home = Image.open(RAW / "home.jpg")
    notes = scaled(Image.open(RAW / "notes-live.jpg"), 0.92)
    overlay = Image.open(RAW / "overlay.jpg")

    paste_window(canvas, home, (150, 150))
    paste_window(canvas, notes, (2400 - notes.width - 110, 90))
    # The overlay is always on top, low in the middle of the screen.
    paste_window(canvas, overlay, ((2400 - overlay.width) // 2 - 180, 1500 - overlay.height - 110))
    save(canvas, "desktop.jpg")


def main():
    for name in [
        "home.jpg", "overlay.jpg", "notes-live.jpg", "summary.jpg",
        "transcript.jpg", "name-speakers.jpg", "providers.jpg",
    ]:
        framed(name, margin=70 if name == "overlay.jpg" else 110)
    desktop()


if __name__ == "__main__":
    main()
