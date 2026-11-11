"""The shipped Linux variants embed the Herding circle tile, self-contained."""
from pathlib import Path
import base64
import re
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[3]
ASSETS = ROOT / "assets/icons"
SVG = "{http://www.w3.org/2000/svg}"


def strip_data(text):
    return re.sub(r"data:image/png;base64,[A-Za-z0-9+/=]+", "DATA", text)


class LinuxIconTests(unittest.TestCase):
    def test_committed_icons_embed_the_herding_circle_tile(self):
        tile = (ASSETS / "Herding-linux.png").read_bytes()
        for name in ("sheprd-linux.svg", "sheprd-linux-worktree.svg"):
            with self.subTest(icon=name):
                committed = (ASSETS / name).read_text()
                root = ET.fromstring(committed)
                self.assertEqual(root.get("viewBox"), "0 0 1024 1024")
                self.assertEqual(root.find(f"{SVG}title").text, "Herding")
                # One self-contained raster: the shell resolves nothing
                # external, so a desktop shell renders it anywhere.
                hrefs = re.findall(r'href="([^"]+)"', committed)
                self.assertEqual(len(hrefs), 1)
                self.assertTrue(hrefs[0].startswith("data:image/png;base64,"))
                embedded = base64.b64decode(hrefs[0].removeprefix("data:image/png;base64,"))
                self.assertEqual(embedded, tile, "the embedded raster is the circle tile")
                image = root.find(f"{SVG}image")
                self.assertEqual((image.get("width"), image.get("height")), ("1024", "1024"))
                clip = root.find(f"{SVG}defs/{SVG}clipPath/{SVG}circle")
                self.assertEqual([clip.get(key) for key in ("cx", "cy", "r")],
                                 ["512", "512", "512"])
                self.assertEqual(image.get("clip-path"), "url(#circle)")

    def test_the_variants_share_one_shell_around_the_tile(self):
        # The stable and worktree variants differ only in the embedded raster,
        # so artwork updates rewrap one tile; the badge lives in the .icns.
        self.assertEqual(strip_data((ASSETS / "sheprd-linux.svg").read_text()),
                         strip_data((ASSETS / "sheprd-linux-worktree.svg").read_text()))
