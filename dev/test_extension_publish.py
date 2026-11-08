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
                    patch.object(publisher, "publish_firefox", return_value=results[2]):
                self.assertEqual(publisher.main(), 0 if all(results) else 1)
                self.assertEqual(save.call_count, int(all(results)))

    def test_unchanged_successful_content_does_not_package_or_publish(self):
        with patch("sys.argv", ["publish-extensions.py"]), \
                patch.object(publisher, "compute_hash", return_value=("same", {})), \
                patch.object(publisher, "load_cache", return_value={"combined_hash": "same"}), \
                patch("subprocess.run") as package, patch.object(publisher, "publish_chrome") as publish:
            self.assertEqual(publisher.main(), 0)
            package.assert_not_called()
            publish.assert_not_called()
