"""Tests for website/tools/docs.py. Needs website/requirements.txt installed.

Run: python3 website/tools/test_docs.py
"""

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("docs", HERE / "docs.py")
docs = importlib.util.module_from_spec(spec)
sys.modules["docs"] = docs
spec.loader.exec_module(docs)


def page(title, body="", front="blurb: A page.\n"):
    return f"---\ntitle: {title}\n{front}---\n\n# {title}\n\nThe lead.\n\n{body}"


class Fixture:
    """A throwaway repository with website/docs pages and a docs/ guide."""

    def __init__(self, test, files):
        self.tmp = tempfile.TemporaryDirectory()
        test.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        for rel, text in files.items():
            path = self.root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)

    def build(self, nav, repo_docs=None):
        return docs.build(self.root / "website/docs", self.root, docs.THEME, nav, repo_docs or {})


class SlugTests(unittest.TestCase):
    def test_matches_github(self):
        self.assertEqual(docs.github_slug("What a script cannot do"), "what-a-script-cannot-do")
        self.assertEqual(docs.github_slug("<code>mod.toml</code>"), "modtoml")
        self.assertEqual(docs.github_slug("Chats on SSH hosts"), "chats-on-ssh-hosts")
        self.assertEqual(docs.github_slug("Teleport, fan-out & dispatch"), "teleport-fan-out--dispatch")


class NormalizeTests(unittest.TestCase):
    def test_list_and_table_after_a_paragraph_get_a_blank_line(self):
        out = docs.normalize("Text:\n- one\n- two\n\nMore:\n| a |\n| - |\n")
        self.assertIn("Text:\n\n- one\n- two", out)
        self.assertIn("More:\n\n| a |", out)

    def test_two_space_nesting_becomes_four(self):
        self.assertIn("\n    - inner", docs.normalize("- outer\n  - inner\n"))

    def test_code_fences_are_left_alone(self):
        text = "```text\nText:\n- not a list\n  - nor this\n```\n"
        self.assertEqual(docs.normalize(text), text)


class FrontMatterTests(unittest.TestCase):
    def test_unknown_key_is_an_error(self):
        with self.assertRaises(docs.DocsError):
            docs.parse_front_matter("---\ncolour: red\n---\n# T\n", "x.md")

    def test_unclosed_is_an_error(self):
        with self.assertRaises(docs.DocsError):
            docs.parse_front_matter("---\ntitle: T\n# T\n", "x.md")


class BuildTests(unittest.TestCase):
    def test_pages_links_callouts_and_search(self):
        fx = Fixture(self, {
            "website/docs/introduction.md": page("Introduction", "See [setup](setup.md#first-run) and [the guide](../../docs/guide.md).\n"),
            "website/docs/setup.md": page("Setup", "## First run\n\n> **Note.** Read this.\n\n| a | b |\n| - | - |\n| 1 | `on\\|off` |\n", "blurb: Set up.\nstatus: optin\nplatforms: macOS\n"),
            "docs/guide.md": "# Guide\n\nAll about it. See [code](../src/main.rs).\n\n## Use it\n\nText.\n",
            "src/main.rs": "fn main() {}\n",
        })
        files, errors = fx.build([("Start", ["introduction", "setup"]), ("More", ["guide"])], {"guide": {"path": "docs/guide.md", "blurb": "The guide."}})
        self.assertEqual(errors, [])
        self.assertEqual(sorted(files), ["docs.css", "docs.js", "guide/index.html", "index.html", "search.js", "setup/index.html"])
        home = files["index.html"]
        self.assertIn('href="/docs/setup/#first-run"', home)
        self.assertIn('href="/docs/guide/"', home)
        self.assertIn("Browse the docs", home)
        setup = files["setup/index.html"]
        self.assertIn('<aside class="note"><b>Note</b>', setup)
        self.assertIn('<div class="table-wrap"><table>', setup)
        self.assertIn("<code>on|off</code>", setup)
        self.assertIn('<span class="st st-optin">Opt-in</span>', setup)
        self.assertIn('href="#first-run" data-toc="first-run"', setup)
        self.assertIn('<a href="/docs/setup/" class="on" aria-current="page">Setup</a>', setup)
        self.assertIn("https://github.com/sheprd-labs/sheprd/blob/main/src/main.rs", files["guide/index.html"])
        self.assertIn('"id":"use-it"', files["search.js"])
        self.assertNotIn("search.js", setup)  # loaded when search first opens

    def test_search_keeps_whole_sections(self):
        body = "## Long\n\n" + " ".join(f"word{i}" for i in range(400)) + " needle\n"
        fx = Fixture(self, {"website/docs/introduction.md": page("Introduction", body)})
        files, errors = fx.build([("Start", ["introduction"])])
        self.assertEqual(errors, [])
        self.assertIn("needle", files["search.js"])

    def test_broken_links_and_anchors_are_reported(self):
        fx = Fixture(self, {
            "website/docs/introduction.md": page("Introduction", "[a](missing.md) [b](setup.md#nowhere) [c](#here)\n"),
            "website/docs/setup.md": page("Setup"),
        })
        _, errors = fx.build([("Start", ["introduction", "setup"])])
        self.assertTrue(any("broken link missing.md" in e for e in errors), errors)
        self.assertTrue(any("no heading #nowhere" in e for e in errors), errors)
        self.assertTrue(any("no heading #here" in e for e in errors), errors)

    def test_unlisted_pages_bad_status_and_title_mismatch_are_reported(self):
        fx = Fixture(self, {
            "website/docs/introduction.md": page("Introduction", front="blurb: x\nstatus: beta\n"),
            "website/docs/orphan.md": page("Orphan"),
            "website/docs/setup.md": "---\ntitle: Set up\nblurb: x\n---\n\n# Setup\n\nLead.\n",
        })
        _, errors = fx.build([("Start", ["introduction", "setup"])])
        joined = "\n".join(errors)
        self.assertIn("orphan.md is not in NAV", joined)
        self.assertIn("status must be one of", joined)
        self.assertIn("differs from the # title", joined)

    def test_the_real_sources_build(self):
        _, errors = docs.build()
        self.assertEqual(errors, [])


if __name__ == "__main__":
    unittest.main()
