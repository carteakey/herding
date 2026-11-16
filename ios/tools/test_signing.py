#!/usr/bin/env python3
"""Tests signing.sh with a stand-in for `security`, so they need no
keychain, certificate, or Apple account. Uses only Python's standard
library."""

import base64
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent

# Keeps the search list in a file, prints it as `security` does (indented
# and quoted), and records every call's arguments as one JSON line.
SECURITY = r"""#!/usr/bin/env python3
import json, os, sys
state = os.environ["STUB_STATE"]
args = sys.argv[1:]
with open(os.path.join(state, "calls"), "a") as calls:
    calls.write(json.dumps(args) + "\n")
search = os.path.join(state, "search")
command = args[0]
if command == "list-keychains":
    if "-s" in args:
        with open(search, "w") as out:
            out.write("".join(path + "\n" for path in args[args.index("-s") + 1:]))
    else:
        with open(search) as listed:
            for path in listed.read().splitlines():
                print(f'    "{path}"')
elif command == "import" and os.environ.get("STUB_IMPORT_FAILS"):
    sys.exit("security: SecKeychainItemImport: MAC verification failed")
elif command == "find-identity" and not os.environ.get("STUB_NO_IDENTITY"):
    print('  1) ABCDEF "Apple Development: Someone (TEAM)"')
    print("     1 valid identities found")
"""

ORIGINAL = [
    "/Users/someone/Library/Keychains/login.keychain-db",
    "/Users/someone/Library/Keychains/My Work Keys.keychain-db",
]


class SigningTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        root = Path(self.temp.name)
        self.state = root / "state"
        self.work = root / "work dir"
        stubs = root / "bin"
        for directory in (self.state, self.work, stubs):
            directory.mkdir()
        (self.state / "search").write_text("".join(path + "\n" for path in ORIGINAL))
        security = stubs / "security"
        security.write_text(SECURITY)
        security.chmod(0o755)
        self.env = {
            "PATH": f"{stubs}:/usr/bin:/bin",
            "STUB_STATE": str(self.state),
            "IOS_SIGNING_P12_BASE64": base64.b64encode(b"not really a p12").decode(),
            "IOS_SIGNING_P12_PASSWORD": "secret",
        }

    def tearDown(self):
        self.temp.cleanup()

    def run_setup(self, **env):
        """Sources signing.sh as testflight.sh does: set -eu, cleanup on exit."""
        script = (
            'set -eu; . "$1"; trap signing_cleanup EXIT; signing_setup "$2"; '
            'signing_list > "$STUB_STATE/during"'
        )
        merged = {**self.env, **env}
        merged = {key: value for key, value in merged.items() if value is not None}
        return subprocess.run(
            ["/bin/sh", "-c", script, "sh", str(HERE / "signing.sh"), str(self.work)],
            env=merged,
            capture_output=True,
            text=True,
        )

    def calls(self):
        path = self.state / "calls"
        if not path.exists():
            return []
        return [json.loads(line) for line in path.read_text().splitlines()]

    def search(self):
        return (self.state / "search").read_text().splitlines()

    def keychain(self):
        return str(self.work / "signing.keychain-db")

    def test_the_keychain_is_searched_first_with_every_path_whole(self):
        result = self.run_setup()
        self.assertEqual(result.returncode, 0, result.stderr)
        during = (self.state / "during").read_text().splitlines()
        self.assertEqual(during, [self.keychain(), *ORIGINAL])

    def test_cleanup_restores_the_search_list_and_deletes_the_keychain(self):
        result = self.run_setup()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.search(), ORIGINAL)
        self.assertIn(["delete-keychain", self.keychain()], self.calls())

    def test_the_p12_is_not_left_on_disk(self):
        self.run_setup()
        self.assertFalse((self.work / "signing.p12").exists())

    def test_a_failed_import_stops_and_cleans_up(self):
        result = self.run_setup(STUB_IMPORT_FAILS="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("could not be imported", result.stderr)
        self.assertFalse((self.work / "signing.p12").exists())
        self.assertEqual(self.search(), ORIGINAL)
        self.assertIn(["delete-keychain", self.keychain()], self.calls())

    def test_a_p12_without_a_development_identity_is_refused(self):
        result = self.run_setup(STUB_NO_IDENTITY="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no Apple Development identity", result.stderr)
        self.assertEqual(self.search(), ORIGINAL)
        self.assertIn(["delete-keychain", self.keychain()], self.calls())

    def test_ci_refuses_without_the_certificate(self):
        result = self.run_setup(CI="true", IOS_SIGNING_P12_BASE64=None)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("TestFlight upload refused", result.stderr)
        self.assertEqual(self.calls(), [])

    def test_a_local_run_without_the_certificate_uses_the_login_keychain(self):
        result = self.run_setup(IOS_SIGNING_P12_BASE64=None)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call for call in self.calls() if call[0] != "list-keychains"], [])
        self.assertEqual(self.search(), ORIGINAL)

    def test_a_certificate_without_its_password_is_refused(self):
        result = self.run_setup(IOS_SIGNING_P12_PASSWORD=None)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("without IOS_SIGNING_P12_PASSWORD", result.stderr)
        self.assertEqual(self.calls(), [])


if __name__ == "__main__":
    os.chdir(HERE)
    unittest.main()
