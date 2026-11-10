"""The host build manifest stays separate from the updater's families."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("update_manifest", Path(__file__).with_name("update-manifest.py"))
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)


class HostManifest(unittest.TestCase):
    def test_both_host_builds_are_required_and_hashed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = []
            for target in updater.HOST_TARGETS:
                with self.assertRaises(ValueError):
                    updater.create(root, "20261017.1", hosts=True)
                self.assertFalse((root / "host-manifest.json").exists())
                name = f"sheprd-remote-control-20261017.1-{target}"
                data = name.encode()
                (root / name).write_bytes(data)
                expected.append(dict(target=target, name=name, size=len(data), sha256=hashlib.sha256(data).hexdigest()))
            payload = updater.create(root, "20261017.1", hosts=True)
            self.assertEqual(json.loads(payload), dict(schema=4, version="20261017.1", assets=expected))
            self.assertEqual((root / "host-manifest.json").read_bytes(), payload)
            for other in ("update-manifest.json", "package-manifest.json", "windows-manifest.json"):
                self.assertFalse((root / other).exists())

    def test_host_builds_share_a_manifest_with_no_other_family(self):
        with tempfile.TemporaryDirectory() as directory:
            for family in (dict(packages=True), dict(windows=True)):
                with self.assertRaises(ValueError):
                    updater.create(directory, "20261017.1", hosts=True, **family)

    def test_host_builds_refuse_empty_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for target in updater.HOST_TARGETS:
                (root / f"sheprd-remote-control-20261017.1-{target}").touch()
            with self.assertRaises(ValueError):
                updater.create(root, "20261017.1", hosts=True)


class HostChats(unittest.TestCase):
    def write_builds(self, root):
        for target in updater.HOST_TARGETS:
            name = updater.host_chat_name("20261017.1", target)
            (root / name).write_bytes(name.encode())

    def test_both_builds_are_listed_in_their_own_schema(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(ValueError):
                updater.create(root, "20261017.1", host_chats=True)
            self.write_builds(root)
            listed = json.loads(updater.create(root, "20261017.1", host_chats=True))
            self.assertEqual(listed["schema"], 5)
            self.assertEqual([asset["target"] for asset in listed["assets"]], list(updater.HOST_TARGETS))
            self.assertFalse((root / "host-manifest.json").exists())
            with self.assertRaises(ValueError):
                updater.create(root, "20261017.1", hosts=True, host_chats=True)

    def test_the_archive_leads_with_the_signed_manifest_and_holds_each_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_builds(root)
            manifest = updater.create(root, "20261017.1", host_chats=True)
            (root / "host-chat-manifest.sig").write_bytes(bytes(64))
            output = updater.pack_host_chats(root, "20261017.1")
            self.assertEqual(output.name, "sheprd-acp-20261017.1-linux-musl.tar.gz")
            with tarfile.open(output) as archive:
                names = archive.getnames()
                self.assertEqual(names[:2], ["host-chat-manifest.json", "host-chat-manifest.sig"])
                self.assertEqual(sorted(names[2:]), sorted(f"{target}/sheprd-acp" for target in updater.HOST_TARGETS))
                self.assertEqual(archive.extractfile(names[0]).read(), manifest)
                self.assertEqual(archive.getmember(names[2]).mode, 0o755)
            # Only the archive is left to publish.
            self.assertEqual([path.name for path in root.iterdir()], [output.name])

    def test_a_build_changed_to_the_same_length_is_refused_and_kept(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_builds(root)
            updater.create(root, "20261017.1", host_chats=True)
            (root / "host-chat-manifest.sig").write_bytes(bytes(64))
            build = root / updater.host_chat_name("20261017.1", updater.HOST_TARGETS[0])
            original = build.read_bytes()
            build.write_bytes(bytes(len(original)))
            with self.assertRaises(ValueError):
                updater.pack_host_chats(root, "20261017.1")
            self.assertTrue(build.exists(), "nothing is deleted")
            self.assertFalse((root / "sheprd-acp-20261017.1-linux-musl.tar.gz").exists())

    def test_a_build_that_differs_from_its_manifest_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_builds(root)
            updater.create(root, "20261017.1", host_chats=True)
            (root / "host-chat-manifest.sig").write_bytes(bytes(64))
            (root / updater.host_chat_name("20261017.1", updater.HOST_TARGETS[0])).write_bytes(b"other bytes")
            with self.assertRaises(ValueError):
                updater.pack_host_chats(root, "20261017.1")
            self.assertFalse((root / "sheprd-acp-20261017.1-linux-musl.tar.gz").exists())


if __name__ == "__main__":
    unittest.main()
