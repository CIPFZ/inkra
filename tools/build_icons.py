#!/usr/bin/env python3
"""Build the Inkra icon pack from the 1024x1024 master rendering.

Steps:
1. Locate the squircle in the master image and crop it tightly.
2. Rebuild a clean squircle alpha mask (Apple-style superellipse) so the icon has a
   transparent background and crisp edges.
3. Flatten the paper background to the brand paper color to remove shadow residue.
4. Emit: PNG set, Windows .ico, macOS .icns, Linux hicolor tree, monochrome
   symbolic/tray glyph, and a dark-mode variant.
"""
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw, ImageFilter
import icnsutil

ROOT = Path(__file__).resolve().parent.parent
# Layout matches SPEC appendix C (D.4): the archived master rendering lives in
# assets/brand/source/, every derived file is written to assets/brand/.
SRC = ROOT / "assets" / "brand" / "source" / "inkra-master-render.png"
OUT = ROOT / "assets" / "brand"
MONO = OUT / "mono"
OUT.mkdir(parents=True, exist_ok=True)
MONO.mkdir(parents=True, exist_ok=True)

INK = (46, 42, 90)        # #2E2A5A brand indigo
CORAL = (255, 107, 87)    # #FF6B57 accent
PAPER = (250, 248, 243)   # #FAF8F3 paper
PAPER_DARK = (30, 27, 51) # #1E1B33 dark variant background

MASTER = 1024


def squircle_mask(size: int, radius_ratio: float = 0.2237, n: float = 5.0) -> Image.Image:
    """Apple-style continuous-corner squircle via superellipse |x|^n + |y|^n = 1."""
    s = size * 4  # supersample
    yy, xx = np.mgrid[0:s, 0:s].astype(np.float64)
    cx = cy = (s - 1) / 2
    half = s / 2
    r = radius_ratio * s * 2.0  # corner radius in supersampled px
    # Distance to the inner rect, then superellipse falloff in corners
    dx = np.maximum(np.abs(xx - cx) - (half - r), 0)
    dy = np.maximum(np.abs(yy - cy) - (half - r), 0)
    d = (dx ** n + dy ** n) ** (1.0 / n)
    alpha = np.clip(r - d + 0.5, 0, 1)
    m = Image.fromarray((alpha * 255).astype(np.uint8), "L")
    return m.resize((size, size), Image.LANCZOS)


def find_squircle_bbox(img: Image.Image) -> tuple[int, int, int, int]:
    """Find the squircle body: warm paper, ink or coral pixels; exclude the neutral grey shadow."""
    a = np.asarray(img.convert("RGB")).astype(np.int16)
    paper = np.abs(a - np.array(PAPER)).sum(axis=2) < 30
    ink = np.abs(a - np.array(INK)).sum(axis=2) < 150
    coral = np.abs(a - np.array(CORAL)).sum(axis=2) < 150
    mask = paper | ink | coral
    rows = np.where(mask.mean(axis=1) > 0.6)[0]
    cols = np.where(mask.mean(axis=0) > 0.6)[0]
    return int(cols[0]), int(rows[0]), int(cols[-1]) + 1, int(rows[-1]) + 1


def build_master() -> Image.Image:
    src = Image.open(SRC).convert("RGB")
    x0, y0, x1, y1 = find_squircle_bbox(src)
    side = max(x1 - x0, y1 - y0)
    cx, cy = (x0 + x1) // 2, (y0 + y1) // 2
    crop = src.crop((cx - side // 2, cy - side // 2, cx - side // 2 + side, cy - side // 2 + side))
    crop = crop.resize((MASTER, MASTER), Image.LANCZOS)

    # Replace background with flat brand paper color while keeping the artwork.
    arr = np.asarray(crop).astype(np.int16)
    ink = np.abs(arr - np.array(INK)).sum(axis=2) < 200
    coral = np.abs(arr - np.array(CORAL)).sum(axis=2) < 200
    arr[~(ink | coral)] = PAPER
    flat = Image.fromarray(arr.astype(np.uint8), "RGB")

    icon = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))
    icon.paste(flat, (0, 0))
    icon.putalpha(squircle_mask(MASTER))
    return icon


def glyph_alpha(master: Image.Image) -> Image.Image:
    """Extract the ink 'i' + cursor as a single-channel alpha for monochrome variants."""
    arr = np.asarray(master.convert("RGB")).astype(np.int16)
    paper_dist = np.abs(arr - np.array(PAPER)).sum(axis=2)
    a = np.clip((paper_dist - 30) * 3, 0, 255).astype(np.uint8)
    return Image.fromarray(a, "L").filter(ImageFilter.GaussianBlur(0.6))


def build_dark(master: Image.Image) -> Image.Image:
    """Dark variant: dark squircle, paper-colored 'i', coral cursor kept."""
    arr = np.asarray(master.convert("RGB")).astype(np.int16)
    out = np.empty_like(arr)
    out[:] = PAPER_DARK
    ink_dist = np.abs(arr - np.array(INK)).sum(axis=2)
    coral_dist = np.abs(arr - np.array(CORAL)).sum(axis=2)
    out[ink_dist < 120] = PAPER
    out[coral_dist < 120] = CORAL
    img = Image.fromarray(out.astype(np.uint8), "RGB").convert("RGBA")
    img.putalpha(squircle_mask(MASTER))
    return img


def build_mono(master: Image.Image, color=(0, 0, 0)) -> Image.Image:
    """Transparent glyph-only icon for tray / symbolic use."""
    a = glyph_alpha(master)
    img = Image.new("RGBA", master.size, color + (0,))
    img.putalpha(a)
    # tighten bbox and re-center into a square canvas with 10% padding
    bbox = a.getbbox()
    g = img.crop(bbox)
    side = int(max(g.size) * 1.2)
    canvas = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    canvas.paste(g, ((side - g.width) // 2, (side - g.height) // 2), g)
    return canvas.resize((MASTER, MASTER), Image.LANCZOS)


def save_png_set(img: Image.Image, stem: str, sizes, out: Path = OUT):
    for s in sizes:
        img.resize((s, s), Image.LANCZOS).save(out / f"{stem}-{s}.png", optimize=True)


def save_ico(img: Image.Image, path: Path):
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    img.save(path, format="ICO", sizes=[(s, s) for s in sizes])


def save_icns(img: Image.Image, path: Path):
    icns = icnsutil.IcnsFile()
    tmp = OUT / "_icns_tmp"
    tmp.mkdir(exist_ok=True)
    # icnsutil infers OSType from filename suffix like 16x16@2x
    for s, scale in [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)]:
        px = s * scale
        f = tmp / f"icon_{s}x{s}{'@2x' if scale == 2 else ''}.png"
        img.resize((px, px), Image.LANCZOS).save(f)
        icns.add_media(file=str(f))
    icns.write(str(path))


def save_hicolor(img: Image.Image, name: str):
    base = OUT / "hicolor"
    for s in [16, 22, 24, 32, 48, 64, 128, 256, 512]:
        d = base / f"{s}x{s}" / "apps"
        d.mkdir(parents=True, exist_ok=True)
        img.resize((s, s), Image.LANCZOS).save(d / f"{name}.png", optimize=True)


def main():
    master = build_master()
    master.save(OUT / "inkra-1024.png", optimize=True)
    save_png_set(master, "inkra", [512, 256, 128, 64, 48, 32, 16])
    save_ico(master, OUT / "inkra.ico")
    save_icns(master, OUT / "inkra.icns")
    save_hicolor(master, "io.inkra.Inkra")

    dark = build_dark(master)
    dark.save(OUT / "inkra-dark-1024.png", optimize=True)
    save_png_set(dark, "inkra-dark", [512, 256, 128])

    mono = build_mono(master, (0, 0, 0))
    mono.save(MONO / "inkra-mono-black-1024.png", optimize=True)
    save_png_set(mono, "inkra-mono-black", [64, 32, 22, 16], MONO)
    mono_w = build_mono(master, (255, 255, 255))
    mono_w.save(MONO / "inkra-mono-white-1024.png", optimize=True)
    save_png_set(mono_w, "inkra-mono-white", [64, 32, 22, 16], MONO)
    sym = OUT / "hicolor" / "symbolic" / "apps"
    sym.mkdir(parents=True, exist_ok=True)
    mono.resize((16, 16), Image.LANCZOS).save(sym / "io.inkra.Inkra-symbolic.png")

    # Contact sheet for review
    sheet = Image.new("RGBA", (1024, 460), (240, 240, 240, 255))
    x = 20
    for s in [256, 128, 64, 48, 32, 16]:
        r = master.resize((s, s), Image.LANCZOS)
        sheet.paste(r, (x, 20), r)
        x += s + 20
    d = dark.resize((128, 128), Image.LANCZOS)
    sheet.paste(d, (20, 300), d)
    m = mono.resize((64, 64), Image.LANCZOS)
    sheet.paste(m, (180, 332), m)
    dark_bg = Image.new("RGBA", (84, 84), (30, 27, 51, 255))
    mw = mono_w.resize((64, 64), Image.LANCZOS)
    dark_bg.paste(mw, (10, 10), mw)
    sheet.paste(dark_bg, (270, 322))
    sheet.save(OUT / "source" / "contact-sheet.png")
    for p in (OUT / "_icns_tmp").glob("*"):
        p.unlink()
    (OUT / "_icns_tmp").rmdir()
    print("done:", sorted(p.name for p in OUT.iterdir()))


if __name__ == "__main__":
    main()
