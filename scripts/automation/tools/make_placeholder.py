#!/usr/bin/env python3
"""Draw a "screenshot pending" card for an image nobody has shot yet.

A page that references a missing file breaks the Docusaurus build, and a page
that silently drops the reference loses the fact that an illustration is owed.
A placeholder keeps both: the build passes, and a reader who lands on the card
learns which screen is missing and why.

The card is deliberately not a screenshot. It carries the eyebrow, the screen's
title, one sentence on what it will show, the reason it is not there, and the
file name it must be saved under, so the person who finally shoots it knows
exactly what they are replacing.

    python3 scripts/automation/tools/make_placeholder.py \\
        --name agents-installer-un-agent-2ter.png \\
        --title "Python dependencies confirmation" \\
        --shows "The deps-confirm step listing the pip packages the package declares." \\
        --why "Not taken yet. See scripts/automation/SHOOT-BY-HAND.md, session C."
"""

import argparse
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

REPO_ROOT = Path(__file__).resolve().parents[3]
IMG_DIR = REPO_ROOT / "docs" / "site" / "static" / "img" / "operator-help"

WIDTH, HEIGHT = 1280, 800
PAPER = (246, 243, 235)
CARD = (242, 239, 230)
BORDER = (198, 192, 174)
HATCH = (238, 234, 223)
INK = (43, 41, 38)
MUTED = (118, 112, 100)
ACCENT = (79, 70, 229)

HELVETICA = "/System/Library/Fonts/Helvetica.ttc"


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(HELVETICA, size, index=1 if bold else 0)


def wrap(draw: ImageDraw.ImageDraw, text: str, f: ImageFont.FreeTypeFont, width: int) -> list[str]:
    lines: list[str] = []
    line = ""
    for word in text.split():
        candidate = f"{line} {word}".strip()
        if draw.textlength(candidate, font=f) <= width:
            line = candidate
        else:
            if line:
                lines.append(line)
            line = word
    if line:
        lines.append(line)
    return lines


def draw_card(name: str, title: str, shows: str, why: str) -> Image.Image:
    image = Image.new("RGB", (WIDTH, HEIGHT), PAPER)
    draw = ImageDraw.Draw(image)

    draw.rectangle([40, 40, WIDTH - 40, HEIGHT - 40], fill=CARD, outline=BORDER)
    for x in range(-HEIGHT, WIDTH, 22):
        draw.line([(x, HEIGHT - 40), (x + HEIGHT, 40)], fill=HATCH, width=1)
    draw.rectangle([40, 40, WIDTH - 40, HEIGHT - 40], outline=BORDER)

    left = 90
    right = WIDTH - 150
    y = 300

    eyebrow = font(13, bold=True)
    draw.text((left, y), "SCREENSHOT PENDING", font=eyebrow, fill=ACCENT)
    y += 34

    heading = font(38, bold=True)
    for line in wrap(draw, title, heading, right - left):
        draw.text((left, y), line, font=heading, fill=INK)
        y += 50
    y += 4

    body = font(17)
    for line in wrap(draw, shows, body, right - left):
        draw.text((left, y), line, font=body, fill=(74, 70, 64))
        y += 26

    y += 22
    draw.line([(left, y), (right - 108, y)], fill=BORDER, width=1)
    y += 28

    small = font(15)
    for line in wrap(draw, why, small, 470):
        draw.text((left, y), line, font=small, fill=MUTED)
        y += 24

    draw.text((left, HEIGHT - 130), name, font=small, fill=(160, 152, 136))
    return image


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--name", required=True, help="file name, saved verbatim into the image directory")
    parser.add_argument("--title", required=True, help="the screen this card stands in for")
    parser.add_argument("--shows", required=True, help="one sentence on what the real capture will show")
    parser.add_argument("--why", required=True, help="why it is not there yet, and where the recipe lives")
    args = parser.parse_args()

    if not args.name.endswith(".png"):
        print(f"name must end in .png, got {args.name}")
        return 1

    target = IMG_DIR / args.name
    draw_card(args.name, args.title, args.shows, args.why).save(target)
    print(f"wrote {target.relative_to(REPO_ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
