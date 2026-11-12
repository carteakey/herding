"""CI eligibility on the fork: the minimum set, everything else skipped.

The fork runs only the commit-message job (which also computes the
code-change path filter) and the Linux Format, lint, and test job; the
Windows, macOS, website, release, and aggregate jobs stay upstream and
skip with `if: false` so their bodies remain for merges.
"""

import ast
import itertools
import os
from pathlib import Path
import re
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = (ROOT / ".github/workflows/ci.yml").read_text()
SECTIONS = re.split(r"^  ([a-z-]+):\n", WORKFLOW.split("\njobs:\n", 1)[1], flags=re.M)
JOBS = dict(zip(SECTIONS[1::2], SECTIONS[2::2]))
# The jobs the fork runs; every other job is `if: false` on the fork.
ON_JOBS = ("commits", "checks")
OFF_JOBS = ("zizmor", "windows", "windows-native", "windows-package", "website",
            "checks-passed", "build", "build-macos")
REPO = "carteakey/herding"
OWNER = "carteakey"


def condition_matches(job, context, jobs=JOBS):
    # Interpret only the boolean/equality subset used by these job conditions.
    # Unsupported syntax fails rather than executing workflow text as Python.
    if re.search(r"^    if: false$", jobs[job], re.M):
        return False
    expression = re.search(r"^    if: >-\n((?:      .+\n)+)", jobs[job], re.M)[1]
    expression = " ".join(expression.split()).replace("&&", "and").replace("||", "or")

    def evaluate(node):
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            return node.value
        if isinstance(node, ast.Attribute):
            return context[ast.unparse(node)]
        if isinstance(node, ast.BoolOp):
            values = [evaluate(value) for value in node.values]
            if isinstance(node.op, ast.And):
                return all(values)
            if isinstance(node.op, ast.Or):
                return any(values)
        if isinstance(node, ast.Compare) and len(node.ops) == 1 and isinstance(node.ops[0], ast.Eq):
            return evaluate(node.left) == evaluate(node.comparators[0])
        if (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                and node.func.id == "always" and not node.args and not node.keywords):
            return True
        raise AssertionError(f"Unsupported CI expression: {ast.dump(node)}")

    return evaluate(ast.parse(expression, mode="eval").body)


class CiPolicy(unittest.TestCase):
    def context(self, **overrides):
        return {
            "github.repository": REPO,
            "github.event_name": "pull_request",
            "github.ref": "refs/pull/456/merge",
            "github.actor": OWNER,
            "github.triggering_actor": OWNER,
            "github.event.pull_request.user.login": OWNER,
            "github.event.pull_request.head.repo.full_name": f"{OWNER}/herding",
            "needs.commits.outputs.code": "true",
            "needs.commits.outputs.windows_package": "true",
            "needs.commits.outputs.website": "true",
            **overrides,
        }

    def test_fork_pull_requests_run_the_kept_jobs_for_any_actor(self):
        # Fork pull requests carry no secrets and read the repository only, so
        # any actor's PR may run them; the skipped jobs never run.
        for actor in (OWNER, "penso", "a-contributor"):
            context = self.context(**{"github.actor": actor,
                                      "github.triggering_actor": actor,
                                      "github.event.pull_request.user.login": actor})
            for job in ON_JOBS:
                with self.subTest(actor=actor, job=job):
                    self.assertTrue(condition_matches(job, context))
            for job in OFF_JOBS:
                self.assertFalse(condition_matches(job, context), job)

    def test_non_pr_events_require_owner_main_and_expected_event(self):
        for event, ref, actor, triggering_actor in itertools.product(
                ("push", "workflow_dispatch", "pull_request_target", "workflow_run"),
                ("refs/heads/main", "refs/heads/topic", "refs/tags/v1"),
                (OWNER, "someone-else"), (OWNER, "someone-else")):
            context = self.context(**{
                "github.event_name": event, "github.ref": ref,
                "github.actor": actor, "github.triggering_actor": triggering_actor,
            })
            allowed = (event in ("push", "workflow_dispatch") and ref == "refs/heads/main"
                       and actor == OWNER and triggering_actor == OWNER)
            for job in JOBS:
                with self.subTest(job=job, context=context):
                    self.assertEqual(condition_matches(job, context),
                                     allowed and job in ON_JOBS)

    def test_other_repositories_cannot_run_jobs(self):
        for event in ("pull_request", "push", "workflow_dispatch"):
            context = self.context(**{
                "github.repository": "someone-else/herding", "github.event_name": event,
                "github.ref": "refs/heads/main",
            })
            for job in JOBS:
                self.assertFalse(condition_matches(job, context), (job, event))

    def test_upstream_repositories_do_not_run_the_forks_jobs(self):
        # The upstream workflows keep their own gates; this fork's ci.yml
        # runs nowhere else.
        for repository in ("penso/herdr-gpui", "sheprd-labs/sheprd"):
            context = self.context(**{"github.repository": repository})
            for job in JOBS:
                self.assertFalse(condition_matches(job, context), (job, repository))

    def test_owner_only_workflows_allow_both_repository_locations_but_not_forks(self):
        for workflow in ("nix.yml", "supply-chain.yml"):
            text = (ROOT / ".github/workflows" / workflow).read_text()
            sections = re.split(r"^  ([a-z-]+):\n", text.split("\njobs:\n", 1)[1], flags=re.M)
            jobs = dict(zip(sections[1::2], sections[2::2]))
            for repository, head in itertools.product(
                    ("penso/herdr-gpui", "sheprd-labs/sheprd", "other/sheprd"),
                    ("penso/herdr-gpui", "sheprd-labs/sheprd", "other/sheprd")):
                context = self.context(**{
                    "github.repository": repository, "github.actor": "penso",
                    "github.triggering_actor": "penso",
                    "github.event.pull_request.user.login": "penso",
                    "github.event.pull_request.head.repo.full_name": head,
                })
                expected = repository != "other/sheprd" and head == repository
                for job in jobs:
                    with self.subTest(workflow=workflow, job=job, repository=repository, head=head):
                        self.assertEqual(condition_matches(job, context, jobs), expected)

    def test_path_selection_only_skips_the_checks_job(self):
        for code in ("false", ""):
            context = self.context(**{"needs.commits.outputs.code": code})
            for job in JOBS:
                expected = job == "commits"
                self.assertEqual(condition_matches(job, context), expected, (job, code))
        for website in ("false", ""):
            # Only the disabled website job read that output; the kept jobs
            # match on unchanged conditions.
            context = self.context(**{"needs.commits.outputs.website": website})
            for job in JOBS:
                expected = job in ON_JOBS
                self.assertEqual(condition_matches(job, context), expected, (job, website))

    def test_the_disabled_aggregate_gate_still_rejects_failures_and_skips(self):
        # The checks-passed body stays for upstream merges; its gate script
        # keeps requiring every platform it names.
        script = JOBS["checks-passed"].split("        run: |\n", 1)[1]
        script = "\n".join(line[10:] for line in script.splitlines() if line.startswith("          "))

        def gate(code, checks, windows, native, commits, package_inputs="false", package="skipped",
                 event="push", macos=None, site_inputs="false", site="skipped"):
            if macos is None:
                macos = "success" if code == "true" and event == "pull_request" else "skipped"
            return subprocess.run(
                ["bash", "-e", "-c", script], capture_output=True,
                env={**os.environ, "CODE": code, "RESULT": checks, "WINDOWS": windows,
                     "NATIVE": native, "COMMITS": commits,
                     "PACKAGE_INPUTS": package_inputs, "PACKAGE": package,
                     "EVENT": event, "MACOS_RELEASE": macos,
                     "SITE_INPUTS": site_inputs, "SITE": site},
            ).returncode == 0

        self.assertTrue(gate("true", "success", "success", "success", "success"))
        self.assertFalse(gate("true", "failure", "success", "success", "success"))
        self.assertFalse(gate("true", "success", "skipped", "success", "success"))

    def test_ci_has_no_privileged_trigger_or_credentials(self):
        triggers = WORKFLOW.split("\npermissions:", 1)[0]
        self.assertEqual(re.findall(r"^  (\w+):", triggers, re.M),
                         ["push", "pull_request", "workflow_dispatch"])
        self.assertIn("\npermissions: {}\n", WORKFLOW)
        for name, job in JOBS.items():
            with self.subTest(job=name):
                permissions = re.search(r"^    permissions:([^\n]*\n(?:      [^\n]*\n)*)", job, re.M)[1]
                self.assertIn(permissions.strip(), ("{}", "contents: read"))
                for forbidden in ("secrets.", "secrets[", "secrets:", "environment:", "id-token:"):
                    self.assertNotIn(forbidden, job)
                if "actions/checkout@" in job:
                    self.assertIn("persist-credentials: false", job)
                if "Swatinem/rust-cache@" in job:
                    self.assertIn("save-if: ${{ github.ref == 'refs/heads/main' }}", job)
        # Keep runners a fixed hosted set; never accept a PR-controlled label.
        self.assertEqual(re.findall(r"^    runs-on: (.+)$", WORKFLOW, re.M), [
            "ubuntu-latest", "ubuntu-24.04", "${{ matrix.runner }}",
            "${{ matrix.runner }}", "${{ matrix.runner }}", "windows-2025",
            "ubuntu-24.04", "ubuntu-24.04", "${{ matrix.runner }}", "macos-15",
        ])
        self.assertEqual(re.findall(r"^        runner: (.+)$", WORKFLOW, re.M), [
            "[ubuntu-24.04]",
            "[windows-2025, windows-11-arm]",
            "[windows-2025, windows-11-arm]",
        ])
        self.assertEqual(re.findall(r"^          - runner: (.+)$", WORKFLOW, re.M),
                         ["ubuntu-24.04", "ubuntu-24.04-arm"])
