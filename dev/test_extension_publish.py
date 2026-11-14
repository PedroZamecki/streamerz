"""Publishing failures must remain retryable. Never contacts a store."""
import importlib.util
from pathlib import Path
import unittest
import sys
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("extension_publish", Path(__file__).with_name("publish-extensions.py"))
publisher = importlib.util.module_from_spec(spec)
with patch("sys.path", list(sys.path)):
    spec.loader.exec_module(publisher)


class PublishCache(unittest.TestCase):
    def test_only_successful_publishing_updates_the_cache(self):
        for results in [(True, True, True), (False, True, True), (True, False, True), (True, True, False)]:
            with self.subTest(results=results), patch("sys.argv", ["publish-extensions.py"]), \
                    patch.object(publisher, "compute_hash", return_value=("new", {"bridge.js": "digest"})), \
                    patch.object(publisher, "load_cache", return_value={}), \
                    patch.object(publisher, "save_cache") as save, \
                    patch("subprocess.run"), \
                    patch.object(publisher, "publish_chrome", return_value=results[0]), \
                    patch.object(publisher, "publish_edge", return_value=results[1]), \
                    patch.object(publisher, "publish_firefox", return_value=results[2]), \
                    patch.object(publisher, "release_firefox", return_value=True):
                self.assertEqual(publisher.main(), 0 if all(results) else 1)
                self.assertEqual(save.call_count, int(all(results)))

    def test_unchanged_successful_content_does_not_package_or_publish(self):
        with patch("sys.argv", ["publish-extensions.py"]), \
                patch.object(publisher, "compute_hash", return_value=("same", {})), \
                patch.object(publisher, "load_cache", return_value={"combined_hash": "same"}), \
                patch("subprocess.run") as package, patch.object(publisher, "publish_chrome") as publish, \
                patch.object(publisher, "release_firefox", return_value=True) as release:
            self.assertEqual(publisher.main(), 0)
            package.assert_not_called()
            publish.assert_not_called()
            # A signed Firefox file missing from its release is still attached.
            release.assert_called_once()


class FirefoxRelease(unittest.TestCase):
    def xpi(self, version="1.4.0", signed=True, addon="tiktok-bridge@streamerz.app"):
        import io, json, zipfile
        out = io.BytesIO()
        with zipfile.ZipFile(out, "w") as z:
            z.writestr("manifest.json", json.dumps({"version": version, "browser_specific_settings": {"gecko": {"id": addon}}}))
            if signed:
                z.writestr("META-INF/mozilla.rsa", b"sig")
        return out.getvalue()

    def test_only_the_signed_file_of_this_version_is_released(self):
        ok = publisher.signed_xpi_ok
        self.assertTrue(ok(self.xpi(), "tiktok-bridge@streamerz.app", "1.4.0"))
        self.assertFalse(ok(self.xpi(signed=False), "tiktok-bridge@streamerz.app", "1.4.0"))
        self.assertFalse(ok(self.xpi(version="1.3.0"), "tiktok-bridge@streamerz.app", "1.4.0"))
        self.assertFalse(ok(self.xpi(addon="other@x"), "tiktok-bridge@streamerz.app", "1.4.0"))
        self.assertFalse(ok(b"<html>not found</html>", "tiktok-bridge@streamerz.app", "1.4.0"))

    def test_the_release_never_becomes_the_apps_latest(self):
        calls = []
        def gh(*args, check=True):
            calls.append(args)
            from types import SimpleNamespace
            return SimpleNamespace(returncode=1 if args[:2] == ("release", "view") else 0, stdout="{}")
        with patch.dict("os.environ", {"AMO_JWT_ISSUER": "i", "AMO_JWT_SECRET": "s", "GH_TOKEN": "t"}), \
                patch.object(publisher, "gh", side_effect=gh), \
                patch.object(publisher, "signed_xpi", return_value=self.xpi(version=publisher.firefox_version()[1])), \
                patch.object(publisher, "DIST_DIR", Path(__import__("tempfile").mkdtemp())):
            self.assertTrue(publisher.release_firefox())
        create = next(c for c in calls if c[:2] == ("release", "create"))
        self.assertEqual(create[2], "tiktok-extension-v" + publisher.firefox_version()[1])
        self.assertIn("--prerelease", create)
        self.assertIn("--latest=false", create)
