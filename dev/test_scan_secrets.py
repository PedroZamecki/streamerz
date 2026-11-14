"""Keep the historical path review narrow without allowing new paths or credentials."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("secret_scanner", Path(__file__).with_name("scan_secrets.py"))
scanner = importlib.util.module_from_spec(SPEC)
saved_path = sys.path[:]
try:
    SPEC.loader.exec_module(scanner)
finally:
    sys.path[:] = saved_path


class HistoryScanTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.commit = "a" * 40
        self.path = "docs/benchmark.json"
        self.line = '  "binary": "/home/' + "developer" + '/repo/target/Streamerz",'
        self.rule = "A person's home folder"
        self.entry = {"commit": self.commit, "path": self.path, "rule": self.rule,
                      "sha256": hashlib.sha256(self.line.encode()).hexdigest(), "reason": "Current file is redacted."}
        (self.root / "docs").mkdir()
        (self.root / self.path).write_text('{"binary":"<temporary benchmark binary>"}\n')
        (self.root / "dev").mkdir()
        self.root_patch = patch.object(scanner, "ROOT", self.root)
        self.root_patch.start()
        self.addCleanup(self.root_patch.stop)

    def diff(self, line=None, commit=None, path=None):
        return f"commit {commit or self.commit}\n+++ b/{path or self.path}\n+{self.line if line is None else line}\n"

    def baseline(self, entry=None):
        (self.root / "dev/secrets-history.json").write_text(json.dumps({"version": 1, "findings": [entry or self.entry]}))
        return scanner.history_baseline()

    def test_current_and_newly_committed_home_paths_are_blocked(self):
        self.assertEqual(len(scanner.scan(self.diff(), [])), 1)
        baseline = self.baseline()
        self.assertEqual(len(scanner.scan(self.diff(commit="b" * 40), [], baseline)), 1)

    def test_only_the_exact_legacy_occurrence_is_reviewed(self):
        reviewed = []
        self.assertEqual(scanner.scan(self.diff(), [], self.baseline(), reviewed), [])
        self.assertEqual(len(reviewed), 1)

    def test_changed_line_or_file_is_not_exempt(self):
        baseline = self.baseline()
        self.assertEqual(len(scanner.scan(self.diff(line=self.line + " changed"), [], baseline)), 1)
        self.assertEqual(len(scanner.scan(self.diff(path="docs/other.json"), [], baseline)), 1)

    def test_reintroduced_current_path_invalidates_legacy_review(self):
        (self.root / self.path).write_text(self.line)
        with self.assertRaisesRegex(ValueError, "must be redacted"):
            self.baseline()

    def test_credentials_are_always_blocked_even_in_a_reviewed_commit(self):
        token_line = 'token = "' + "ghp_" + "a" * 36 + '"'
        self.assertEqual(scanner.scan(self.diff(line=token_line), [], self.baseline())[0][1], "GitHub token")
        mixed_line = self.line + " " + token_line
        mixed_baseline = self.baseline(dict(self.entry, sha256=hashlib.sha256(mixed_line.encode()).hexdigest()))
        self.assertEqual([finding[1] for finding in scanner.scan(self.diff(line=mixed_line), [], mixed_baseline)], ["GitHub token"])
        with self.assertRaisesRegex(ValueError, "Credentials cannot be exempted"):
            self.baseline(dict(self.entry, rule="GitHub token", sha256=hashlib.sha256(token_line.encode()).hexdigest()))

    def test_baseline_cannot_escape_repository_or_use_partial_commits(self):
        for change in [{"path": "../outside"}, {"path": str(self.root / "outside")}, {"commit": "a" * 8}, {"sha256": ""}, {"reason": ""}]:
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.baseline(dict(self.entry, **change))

    def test_missing_baseline_preserves_strict_history_scan(self):
        self.assertEqual(scanner.history_baseline(), set())
        self.assertEqual(len(scanner.scan(self.diff(), [], scanner.history_baseline())), 1)

    def test_git_errors_fail_closed(self):
        with patch.object(scanner.subprocess, "run", side_effect=subprocess.CalledProcessError(128, "git")):
            with self.assertRaises(subprocess.CalledProcessError):
                scanner.git("log", "--all")


if __name__ == "__main__":
    unittest.main()
