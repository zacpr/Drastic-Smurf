#!/usr/bin/env python3
"""Generate app and installer icons from the existing character artwork."""

from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageOps


ROOT = Path(__file__).resolve().parents[1]


def main():
    with Image.open(ROOT / "drastic.png") as source:
        head = source.convert("RGBA").crop((20, 0, 210, 180))

    mask = Image.new("L", head.size)
    ImageDraw.Draw(mask).polygon(
        [
            (0, 0), (160, 0), (160, 90), (153, 105), (148, 128),
            (128, 145), (112, 159), (77, 163), (51, 153), (32, 163),
            (5, 162), (0, 135),
        ],
        fill=255,
    )
    head.putalpha(ImageChops.multiply(head.getchannel("A"), mask))
    icon = Image.new("RGBA", (256, 256))
    head = ImageOps.contain(head.crop(head.getbbox()), (240, 240), Image.Resampling.LANCZOS)
    icon.alpha_composite(head, ((256 - head.width) // 2, (256 - head.height) // 2))
    icon.save(ROOT / "assets" / "drastic-smurf.png")
    icon.save(
        ROOT / "wix" / "Product.ico",
        sizes=[(size, size) for size in (16, 24, 32, 48, 64, 128, 256)],
    )


if __name__ == "__main__":
    main()
