"""Draws Recap's app icon: crates/app/assets/icon/recap.ico (16-256 px) and
recap.png (256 px).

The tray's mark (a white ring; the rec red dot while recording) on a dark
rounded tile, so it reads on light and dark Start menus. Colours are the
design tokens: rec #FF6363, the dark window background #1A1A1A.

    python tools/gen_icon.py      # needs Pillow
"""

from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent.parent / "crates" / "app" / "assets" / "icon"
TILE = (0x1A, 0x1A, 0x1A, 255)
RING = (0xFF, 0xFF, 0xFF, 255)
REC = (0xFF, 0x63, 0x63, 255)
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]
SCALE = 8  # supersampling, for clean edges


def draw(size: int) -> Image.Image:
    s = size * SCALE
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, s - 1, s - 1], radius=s * 0.22, fill=TILE)
    c = s / 2
    outer = s * 0.31
    width = max(s * 0.075, SCALE * 1.5)
    d.ellipse([c - outer, c - outer, c + outer, c + outer], outline=RING, width=round(width))
    dot = s * 0.13
    d.ellipse([c - dot, c - dot, c + dot, c + dot], fill=REC)
    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    images = [draw(size) for size in SIZES]
    images[-1].save(OUT / "recap.png")
    images[-1].save(OUT / "recap.ico", sizes=[(n, n) for n in SIZES], append_images=images[:-1])
    print(f"wrote {OUT / 'recap.ico'} and recap.png")


if __name__ == "__main__":
    main()
