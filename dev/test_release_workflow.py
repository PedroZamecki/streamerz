"""Test release selection in disposable Git repositories with no GitHub connection."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


class ReleaseVersion(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="streamerz-version-")
        self.addCleanup(scratch.cleanup)
        self.repo = Path(scratch.name)
        self.output = self.repo / "output"
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.name", "Release Test")
        self.git("config", "user.email", "release@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "tag.gpgSign", "false")

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.repo, check=True,
                              capture_output=True, text=True).stdout.strip()

    def commit(self, subject):
        self.git("commit", "-q", "--allow-empty", "-m", subject)

    def run_script(self, script, *args, **environment):
        self.output.unlink(missing_ok=True)
        result = subprocess.run(
            ["bash", str(ROOT / "dev" / script), *args], cwd=self.repo,
            env=dict(os.environ, GITHUB_OUTPUT=str(self.output), **environment),
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return dict(line.split("=", 1) for line in self.output.read_text().splitlines()) if self.output.exists() else {}

    def version(self, event="push"):
        return self.run_script("release-version.sh", event)

    def test_first_release_uses_full_history(self):
        self.commit("fix: first change")
        self.assertEqual(self.version(), {"tag": "v0.0.1", "last": ""})

    def test_patch_minor_and_major_in_order(self):
        self.commit("docs: baseline")
        self.git("tag", "v2.3.4")
        self.commit("perf(packaging): reuse staged files")
        self.assertEqual(self.version()["tag"], "v2.3.5")
        self.commit("feat: install an extra package")
        self.assertEqual(self.version()["tag"], "v2.4.0")
        self.commit("refactor(packaging)!: change the layout")
        self.assertEqual(self.version()["tag"], "v3.0.0")

    def test_merge_subject_and_side_branch_tags_do_not_change_version(self):
        self.commit("docs: baseline")
        self.git("tag", "v1.2.3")
        self.git("switch", "-q", "-c", "topic")
        self.commit("fix: repair package")
        self.git("tag", "v99.0.0")
        self.git("switch", "-q", "main")
        self.git("merge", "-q", "--no-ff", "-m", "feat!: custom merge title", "topic")
        self.git("tag", "v1.3.0-rc.1")
        self.assertEqual(self.version(), {"tag": "v1.2.4", "last": "v1.2.3"})

    def test_same_tree_and_version_on_pr_and_main(self):
        self.commit("docs: baseline")
        self.git("tag", "v1.2.3")
        self.commit("feat: base release is still pending")
        self.git("switch", "-q", "-c", "topic")
        (self.repo / "package.txt").write_text("release fixture")
        self.git("add", "package.txt")
        self.git("commit", "-qm", "fix: repair package")
        self.git("switch", "-q", "main")
        self.git("merge", "-q", "--no-ff", "-m", "synthetic merge", "topic")
        pr_version = self.version("pull_request")
        pr_tree = self.git("rev-parse", "HEAD^{tree}")
        self.git("reset", "--hard", "HEAD^1")
        self.git("merge", "-q", "--no-ff", "-m", "Merge pull request #1", "topic")
        self.assertEqual(self.git("rev-parse", "HEAD^{tree}"), pr_tree)
        self.assertEqual(self.version(), pr_version)
        self.assertEqual(pr_version["tag"], "v1.3.0")

    def test_docs_only_pr_gets_preview_version_but_no_release(self):
        self.commit("docs: update help")
        sentinel = self.repo / "executed"
        self.assertEqual(self.run_script("release-version.sh", "pull_request",
                                         PR_TITLE=f"feat!: $(touch {sentinel})")["tag"], "v0.0.1")
        self.assertFalse(sentinel.exists())
        self.assertEqual(self.version(), {})

    def test_reuse_requires_same_repo_head_tree_version_and_live_pair(self):
        self.commit("docs: baseline")
        self.git("switch", "-q", "-c", "topic")
        self.commit("fix: repair package")
        head = self.git("rev-parse", "HEAD")
        self.git("switch", "-q", "main")
        self.git("merge", "-q", "--no-ff", "-m", "Merge pull request #1", "topic")
        key = f"v2.1.1-{self.git('rev-parse', 'HEAD^{tree}')}"
        windows, linux = (f"streamerz-{platform}-{key}" for platform in ("windows", "linux"))
        runs = self.repo / "runs.json"
        runs.write_text(json.dumps({
            "workflow_runs": [
                {"id": id, "head_repository": {"full_name": name}}
                for id, name in [(1, "fork/streamerz"), *((id, "owner/streamerz") for id in range(2, 6))]
            ],
        }))
        for id, entries in {
            2: [(windows, False), (linux, True)],
            3: [(windows, False), (linux.replace("v2.1.1", "v2.1.2"), False)],
            4: [(windows, False), (linux + "0", False)],
            5: [(windows, False), (linux, False)],
        }.items():
            (self.repo / f"artifacts-{id}.json").write_text(json.dumps({
                "artifacts": [{"name": name, "expired": expired} for name, expired in entries],
            }))
        # The API stand-in checks the request for the merged parent and evaluates the actual filter.
        gh = self.repo / "gh"
        gh.write_text(r"""#!/usr/bin/env bash
set -eu
[[ $1 == api && $2 == --paginate && $4 == --jq ]]
case $3 in
  *"/workflows/build.yml/runs?event=pull_request&status=success&head_sha=$TEST_HEAD&per_page=100")
    exec jq -r "$5" runs.json ;;
  */runs/*/artifacts\?per_page=100)
    id=${3%/artifacts*}
    exec jq -r "$5" "artifacts-${id##*/}.json" ;;
  *) exit 3 ;;
esac
""")
        gh.chmod(0o755)
        self.assertEqual(self.run_script("release-reuse.sh", key, TEST_HEAD=head,
                                         GH_REPO="owner/streamerz",
                                         PATH=f"{self.repo}{os.pathsep}{os.environ['PATH']}"),
                         {"run": "5"})
