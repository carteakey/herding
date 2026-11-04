"""Tests for scripts/herding-heartbeat.py."""

import importlib.util
import sys
import unittest
from pathlib import Path

# Loading the hyphenated script as a module
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "herding_heartbeat", Path(__file__).with_name("herding-heartbeat.py")
)
assert SPEC and SPEC.loader
herding_heartbeat = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(herding_heartbeat)


class HeartbeatClassificationTests(unittest.TestCase):
    def test_clean_output_is_healthy(self) -> None:
        text = "Compiling herdr-gpui v0.1.0\nFinished dev profile in 2m 41s\n"
        status, reason = herding_heartbeat.classify_pane_output(text)
        self.assertEqual(status, "HEALTHY")
        self.assertEqual(reason, "")

    def test_anthropic_usage_limit_detected(self) -> None:
        text = "Error: Usage limit reached. Your daily usage limit will reset at 12:00 UTC."
        status, reason = herding_heartbeat.classify_pane_output(text)
        self.assertEqual(status, "PTO")
        self.assertIn("usage limit", reason.lower())

    def test_anthropic_5_hour_limit_detected(self) -> None:
        text = "You've reached your 5-hour usage limit. Try again at 4:00 PM."
        status, reason = herding_heartbeat.classify_pane_output(text)
        self.assertEqual(status, "PTO")
        self.assertIn("usage quota", reason.lower())

    def test_openai_429_detected(self) -> None:
        text = "openai.RateLimitError: 429 Too Many Requests: Rate limit reached for default-gpt-4o"
        status, reason = herding_heartbeat.classify_pane_output(text)
        self.assertEqual(status, "PTO")
        self.assertIn("429", reason)

    def test_gemini_resource_exhausted_detected(self) -> None:
        text = "google.api_core.exceptions.ResourceExhausted: 429 Quota exceeded for quota metric 'GenerateContent'"
        status, reason = herding_heartbeat.classify_pane_output(text)
        self.assertEqual(status, "PTO")
        self.assertIn("quota", reason.lower())

    def test_zcode_out_of_usage_detected(self) -> None:
        text = "Worker process terminated: out of usage on start-plan."
        status, reason = herding_heartbeat.classify_pane_output(text)
        self.assertEqual(status, "PTO")
        self.assertIn("out of usage", reason.lower())

    def test_process_crashed_detected(self) -> None:
        text = "Session error: connection closed by remote host."
        status, reason = herding_heartbeat.classify_pane_output(text)
        self.assertEqual(status, "CRASHED")
        self.assertIn("connection closed", reason.lower())

    def test_empty_output_is_healthy(self) -> None:
        status, reason = herding_heartbeat.classify_pane_output("")
        self.assertEqual(status, "HEALTHY")


if __name__ == "__main__":
    unittest.main()
