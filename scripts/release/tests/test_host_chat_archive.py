"""The host builds' sheprd-acp archive in the release asset contract."""

import importlib.util
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "artifact-manifest.py"
SPEC = importlib.util.spec_from_file_location("artifact_manifest", SCRIPT)
MANIFEST = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MANIFEST)
VERSION = "20261108.3"
ARCHIVE = f"sheprd-acp-{VERSION}-linux-musl.tar.gz"


class HostChatArchive(unittest.TestCase):
    def checksums(self, names):
        directory = Path(self.enterContext(tempfile.TemporaryDirectory()))
        (directory / "SHA256SUMS").write_text("".join(f"{'0' * 64}  {name}\n" for name in names))
        return directory

    def test_a_release_publishes_it_with_its_sidecars_under_the_updater_limit(self):
        names = MANIFEST.asset_names(VERSION)
        self.assertIn(ARCHIVE, MANIFEST.base_names(VERSION))
        self.assertIn(ARCHIVE + ".sha256", names)
        self.assertLess(len(names) + 1, 128)
        self.assertNotIn(ARCHIVE, MANIFEST.base_names(VERSION, legacy=True))

    def test_a_release_without_it_still_promotes_but_does_not_verify(self):
        names = [name for name in MANIFEST.asset_names(VERSION) if not name.startswith(ARCHIVE)]
        directory = self.checksums(names)
        with self.assertRaises(ValueError):
            MANIFEST.verify_manifest(directory, VERSION)
        entries = MANIFEST.verify_manifest(directory, VERSION, optional=(ARCHIVE,))
        self.assertEqual(set(entries), set(names))

    def test_an_older_release_is_verified_by_its_own_contract(self):
        self.assertNotIn(ARCHIVE, MANIFEST.base_names(VERSION, host_chats=False))
        names = MANIFEST.asset_names(VERSION, host_chats=False)
        directory = self.checksums(names)
        entries = MANIFEST.verify_manifest(directory, VERSION, host_chats=False)
        self.assertEqual(set(entries), set(names))
        # The current contract still requires it.
        with self.assertRaises(ValueError):
            MANIFEST.verify_manifest(directory, VERSION)

    def test_an_unknown_asset_is_refused_even_when_one_may_be_missing(self):
        names = [*MANIFEST.asset_names(VERSION), "surprise.bin"]
        with self.assertRaises(ValueError):
            MANIFEST.verify_manifest(self.checksums(names), VERSION, optional=(ARCHIVE,))


if __name__ == "__main__":
    unittest.main()
