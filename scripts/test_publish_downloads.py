"""Regression coverage for publication order, immutable retries and policy guards."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import publish_downloads as publish


class MemoryBucket:
    def __init__(self):
        self.data = {}
        self.writes = []

    def read(self, key):
        return self.data.get(key)

    def put(self, key, data, *, immutable=False, filename=None):
        if immutable and key in self.data and self.data[key] != data:
            raise ValueError("immutable object changed")
        self.data[key] = data
        self.writes.append(key)


class PublishTests(unittest.TestCase):
    def test_release_is_retriable_and_exposes_feed_last(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            dist = root / "dist"
            dist.mkdir()
            secret, pub = root / "secret", root / "pub"
            subprocess.run(["minisign", "-G", "-W", "-s", secret, "-p", pub], check=True, capture_output=True)
            for suffix in ("source.tar.gz", "macos-arm64.dmg", "macos-arm64.zip"):
                (dist / ("Echo-1.2.3-" + suffix)).write_bytes(suffix.encode())
            bucket = MemoryBucket()
            with patch.object(publish, "PUBLIC_KEY", pub), patch.dict("os.environ", {"ECHO_MINISIGN_KEYCHAIN_SERVICE": ""}):
                for i in range(2):
                    work = root / str(i)
                    work.mkdir()
                    publish.release(bucket, "1.2.3", ["macos-arm64"], dist, secret, work)
                    self.assertEqual(bucket.writes[-1], "latest/update-macos-arm64.json")
                expected = json.loads(bucket.data["latest/update-macos-arm64.json"])
                self.assertEqual(expected["url"], publish.ORIGIN + "/v1.2.3/Echo-1.2.3-macos-arm64.zip")
                previous = dict(bucket.data)
                (dist / "Echo-1.2.3-macos-arm64.zip").write_bytes(b"changed")
                work = root / "changed"
                work.mkdir()
                with self.assertRaises(subprocess.CalledProcessError):
                    publish.release(bucket, "1.2.3", ["macos-arm64"], dist, secret, work)
                self.assertEqual(bucket.data, previous)

    def test_policy_may_only_require_available_versions(self):
        import argparse
        bucket = MemoryBucket()
        args = argparse.Namespace(platform_min=["windows-x64=2.0.0"], min_version=None,
                                  message_zh="", message_en="")
        for feed in (None, b'{"version":"1.0.0"}'):
            if feed:
                bucket.data["latest/update-windows-x64.json"] = feed
            with self.assertRaises(ValueError):
                publish.policy(bucket, args, None, None)
            self.assertNotIn("latest/policy.json", bucket.data)
