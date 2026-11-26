"""Check release package verification with offline stand-ins for package managers."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

DEV = Path(__file__).resolve().parent


class CheckPackages(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="streamerz packages ")
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        self.tools = self.root / "fake tools"
        self.tools.mkdir()
        self.dist = self.root / "dist"
        self.dist.mkdir()
        self.program = self.root / "Streamerz test binary"
        self.program.write_bytes(b"fixture")
        for ext in ("", ".AppImage", ".AppImage.zsync", ".deb", ".rpm", ".pkg.tar.zst"):
            (self.dist / f"Streamerz-linux-x86_64{ext}").write_bytes(b"fixture")
        self.stub("dpkg-deb", "printf '%s\\n' ./usr/bin/Streamerz ./etc/apt/sources.list.d/streamerz.sources")
        self.stub("tar", "echo usr/bin/Streamerz")
        self.stub("rpm", "echo /etc/yum.repos.d/streamerz.repo")
        self.stub("flatpak", "exit 0")

    def stub(self, name, script):
        path = self.tools / name
        path.write_text(f"#!/bin/sh\n{script}\n")
        path.chmod(0o755)

    def run_script(self, script):
        return subprocess.run(
            ["bash", str(DEV / script)], cwd=self.root,
            env=dict(os.environ, PATH=f"{self.tools}{os.pathsep}{os.environ['PATH']}",
                     STREAMERZ_BIN=str(self.program), TOOLS=str(self.tools)),
            capture_output=True, text=True,
        )

    def test_complete_packages_pass(self):
        result = self.run_script("check-packages.sh")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_partial_arch_listing_cannot_hide_failure(self):
        self.stub("tar", "echo usr/bin/Streamerz; exit 2")
        self.assertEqual(self.run_script("check-packages.sh").returncode, 2)

    def test_missing_update_metadata_fails(self):
        (self.dist / "Streamerz-linux-x86_64.AppImage.zsync").unlink()
        self.assertNotEqual(self.run_script("check-packages.sh").returncode, 0)

    def test_failed_download_does_not_leave_a_cached_nfpm(self):
        if any((Path(folder) / "nfpm").exists() for folder in os.get_exec_path()):
            self.skipTest("download is skipped when nfpm is installed")
        self.stub("curl", "exit 22")
        self.stub("tar", 'printf "#!/bin/sh\\nexit 0\\n" > "$3/nfpm"; chmod +x "$3/nfpm"')
        result = self.run_script("packages.sh")
        self.assertEqual(result.returncode, 22, result.stderr)
        self.assertFalse((self.tools / "nfpm").exists())
        self.assertEqual(list(self.tools.glob(".nfpm.*")), [])
