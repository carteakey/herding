"""Independent geometry checks: embedding a tile alone cannot prove a
circular silhouette the desktop shell can mask."""
from pathlib import Path
import base64
import re
import struct
import unittest
import xml.etree.ElementTree as ET

ASSETS = Path(__file__).resolve().parents[3] / "assets/icons"
SVG = "{http://www.w3.org/2000/svg}"


def embedded_png(svg_text):
    """Read the one embedded raster, rejecting stray or foreign references."""
    hrefs = re.findall(r'href="([^"]+)"', svg_text)
    if len(hrefs) != 1 or not hrefs[0].startswith("data:image/png;base64,"):
        raise ValueError(f"Expected one embedded PNG data URI, got {hrefs}")
    return base64.b64decode(hrefs[0].removeprefix("data:image/png;base64,"))


def png_size(data):
    """Width and height from the IHDR, rejecting anything else loudly."""
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        raise ValueError("Not a PNG with a leading IHDR")
    return struct.unpack(">II", data[16:24])


class LinuxIconGeometryTests(unittest.TestCase):
    def check_geometry(self, root, svg_text):
        self.assertEqual(root.get("viewBox"), "0 0 1024 1024")
        self.assertEqual((root.get("width"), root.get("height")), ("1024", "1024"))
        clip = root.find(f"{SVG}defs/{SVG}clipPath/{SVG}circle")
        self.assertIsNotNone(clip)
        cx, cy, radius = (float(clip.get(key)) for key in ("cx", "cy", "r"))
        # Independent design contract: a full-bleed circular silhouette on the
        # 1024 canvas, so the desktop shell needs no shape mask of its own.
        self.assertEqual((cx, cy), (512.0, 512.0))
        self.assertEqual(radius, 512.0)
        image = root.find(f"{SVG}image")
        self.assertIsNotNone(image)
        self.assertEqual((image.get("width"), image.get("height")), ("1024", "1024"))
        self.assertEqual(image.get("clip-path"), "url(#circle)")
        # The raster fills the canvas, so the clip alone shapes the silhouette.
        self.assertEqual(png_size(embedded_png(svg_text)), (1024, 1024))

    def test_both_shipped_icons_clip_the_full_tile_to_a_full_bleed_circle(self):
        for name in ("sheprd-linux.svg", "sheprd-linux-worktree.svg"):
            with self.subTest(icon=name):
                text = (ASSETS / name).read_text()
                self.check_geometry(ET.fromstring(text), text)

    def test_geometry_rejects_regenerated_but_broken_layouts(self):
        text = (ASSETS / "sheprd-linux.svg").read_text()
        broken = (
            text.replace('viewBox="0 0 1024 1024"', 'viewBox="0 0 2048 2048"', 1),
            text.replace('width="1024" height="1024" clip-path',
                         'width="512" height="512" clip-path', 1),
            text.replace('<circle cx="512" cy="512" r="512"/>',
                         '<circle cx="540" cy="512" r="512"/>', 1),
            text.replace('<circle cx="512" cy="512" r="512"/>',
                         '<circle cx="512" cy="512" r="448"/>', 1),
            text.replace(' clip-path="url(#circle)"', "", 1),
        )
        for layout in broken:
            with self.subTest(), self.assertRaises(AssertionError):
                self.check_geometry(ET.fromstring(layout), layout)
        # A reference the SVG spec renders is still rejected: only a PNG data
        # URI is an acceptable source for the silhouette.
        foreign = text.replace("data:image/png;base64,", "data:image/jpeg;base64,", 1)
        with self.assertRaises(ValueError):
            self.check_geometry(ET.fromstring(foreign), foreign)
