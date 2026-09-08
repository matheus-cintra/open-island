#!/usr/bin/env python3
"""Renders the app icon at every size this repo ships.

Usage: render-icon.py            write them
       render-icon.py --check    exit 1 if any is out of date
"""
import re
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
SPRITES = ROOT / "src" / "sprites.ts"
ICONS = Path(__file__).resolve().parent
RUNTIME = ROOT / "crates" / "open-islandd" / "assets"

TINT = (69, 151, 247, 255)
INK = (255, 255, 255, 255)
SPRITE_COLUMNS, SPRITE_ROWS = 18, 8
ALIEN_WIDTH, GLYPH_GAP = 11, 1
MARK_RATIO = 40 / 64
RADIUS_RATIO = 16 / 64

BUNDLE = {"32x32.png": 32, "128x128.png": 128, "128x128@2x.png": 256, "icon.png": 512}
RUNTIME_SIZES = (32, 48, 64, 128, 256)


def unknown_sprite() -> tuple[list[str], list[str]]:
    source = SPRITES.read_text()
    block = source[source.index("const UNKNOWN"):]
    block = block[: block.index("};")]
    rows = re.findall(r'"([.#]+)"', block)
    frame = rows[:SPRITE_ROWS]
    glyph = rows[2 * SPRITE_ROWS : 3 * SPRITE_ROWS]
    if len(frame) != SPRITE_ROWS or len(glyph) != SPRITE_ROWS:
        raise SystemExit("sprites.ts no longer holds UNKNOWN as two frames and a glyph")
    return frame, glyph


def render(size: int, frame: list[str], glyph: list[str]) -> Image.Image:
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=round(size * RADIUS_RATIO), fill=TINT)
    module = max(1, round(size * MARK_RATIO / SPRITE_COLUMNS))
    left = (size - module * SPRITE_COLUMNS) // 2
    top = (size - module * SPRITE_ROWS) // 2
    for rows, offset in ((frame, 0), (glyph, ALIEN_WIDTH + GLYPH_GAP)):
        for y, row in enumerate(rows):
            for x, cell in enumerate(row):
                if cell != "#":
                    continue
                px = left + (x + offset) * module
                py = top + y * module
                draw.rectangle((px, py, px + module - 1, py + module - 1), fill=INK)
    return image


def main() -> int:
    check = "--check" in sys.argv
    frame, glyph = unknown_sprite()
    stale = []
    RUNTIME.mkdir(parents=True, exist_ok=True)
    targets = [(ICONS / name, size) for name, size in BUNDLE.items()]
    targets += [(RUNTIME / f"open-island-{size}.png", size) for size in RUNTIME_SIZES]
    for path, size in targets:
        rendered = render(size, frame, glyph)
        if check:
            if not path.exists() or Image.open(path).convert("RGBA").tobytes() != rendered.tobytes():
                stale.append(path)
            continue
        rendered.save(path)
        print(f"wrote {path.relative_to(ROOT)}")
    if check:
        for path in stale:
            print(f"stale {path.relative_to(ROOT)}")
        return 1 if stale else 0
    for name, args in (("icon.ico", ["-define", "icon:auto-resize=16,32,48,64,128,256"]), ("icon.icns", [])):
        subprocess.run(["magick", str(ICONS / "icon.png"), *args, str(ICONS / name)], check=True)
        print(f"wrote {(ICONS / name).relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
