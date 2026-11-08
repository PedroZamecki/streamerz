"""Run version selection with a hostile PR title, without GitHub or publishing."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


class ReleaseVersion(unittest.TestCase):
    def test_fallback_title_is_data_not_shell_code(self):
        workflow = (ROOT / ".github/workflows/build.yml").read_text()
        step = workflow.split("      - id: next\n", 1)[1].split("      # On main:", 1)[0]
        script = "\n".join(line[10:] for line in step.split("        run: |\n", 1)[1].splitlines())
        script = script.replace("${{ github.event_name }}", "pull_request")
        with tempfile.TemporaryDirectory(prefix="streamerz-version-") as scratch:
            sentinel = Path(scratch) / "executed"
            output = Path(scratch) / "output"
            env = dict(os.environ, BASE_REF="missing-test-ref", GITHUB_OUTPUT=str(output),
                       PR_TITLE=f"feat: $(touch {sentinel})")
            result = subprocess.run(["bash", "-e", "-c", script], cwd=ROOT, env=env,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(sentinel.exists(), "the PR title executed shell code")
            self.assertTrue(output.read_text().startswith("tag=v"))
