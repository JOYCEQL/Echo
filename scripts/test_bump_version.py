"""Run with: python3 -m unittest discover -s scripts -p 'test_bump_version.py'."""

from pathlib import Path
import plistlib
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET

import bump_version as version


class ReleaseVersionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for name in version.FILES:
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes((version.ROOT / name).read_bytes())
        # Exercise Windows' legacy default even when tests run on a UTF-8 host.
        original_read_text = Path.read_text

        def windows_read_text(path, encoding=None, errors=None, **kwargs):
            return original_read_text(path, encoding=encoding or "cp1252", errors=errors, **kwargs)

        self.windows_encoding = patch.object(Path, "read_text", windows_read_text)
        self.windows_encoding.start()
        self.addCleanup(self.windows_encoding.stop)

    def snapshot(self):
        return {name: (self.root / name).read_bytes() for name in version.FILES}

    def test_semantic_increments(self):
        for current, part, expected in (
            ("0.0.1", "patch", "0.0.2"), ("0.0.9", "patch", "0.0.10"),
            ("0.0.9", "minor", "0.1.0"), ("0.4.9", "major", "1.0.0"),
        ):
            with self.subTest(current=current, part=part):
                self.assertEqual(version.next_version(current, part), expected)

    def test_bump_synchronizes_all_platforms_and_preserves_dependencies(self):
        current, target, originals, updates = version.prepare(self.root, "patch")
        before = tomllib.loads(originals["Cargo.lock"].decode())["package"]
        version.apply(self.root, originals, updates)
        self.assertEqual(tomllib.loads((self.root / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"], target)
        after = tomllib.loads((self.root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
        self.assertEqual([p for p in before if p["name"] != "echo"], [p for p in after if p["name"] != "echo"])
        self.assertEqual(next(p for p in after if p["name"] == "echo")["version"], target)
        plist = plistlib.loads((self.root / "packaging/Info.plist").read_bytes())
        self.assertEqual(plist["CFBundleVersion"], target)
        self.assertEqual(plist["CFBundleShortVersionString"], target)
        identity = ET.parse(self.root / "packaging/windows/Echo.manifest").getroot()[0]
        self.assertEqual(identity.get("version"), target + ".0")
        self.assertNotIn(f"/v{current}/Echo-", (self.root / "README.md").read_text(encoding="utf-8"))
        self.assertIn(f"/v{target}/Echo-{target}-", (self.root / "README.md").read_text(encoding="utf-8"))
        self.assertEqual(version.prepare(self.root)[0], target)

    def test_preview_and_consistency_check_do_not_write_files(self):
        before = self.snapshot()
        self.assertEqual(version.prepare(self.root)[0], version.prepare(self.root)[1])
        self.assertNotEqual(*version.prepare(self.root, "patch")[:2])
        self.assertEqual(before, self.snapshot())

    def test_windows_crlf_checkout_is_supported_and_preserved(self):
        for name in version.FILES:
            path = self.root / name
            path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
        _, target, originals, updates = version.prepare(self.root, "patch")
        version.apply(self.root, originals, updates)
        self.assertEqual(version.prepare(self.root)[0], target)
        for data in self.snapshot().values():
            self.assertNotIn(b"\n", data.replace(b"\r\n", b""))

    def test_inconsistent_references_abort_before_writing(self):
        for name in ("Cargo.lock", "packaging/Info.plist", "packaging/windows/Echo.manifest", "README.md"):
            with self.subTest(name=name):
                original = (self.root / name).read_bytes()
                current = version.prepare(self.root)[0]
                (self.root / name).write_bytes(original.replace(current.encode(), b"99.99.99", 1))
                before = self.snapshot()
                with self.assertRaises(version.VersionError):
                    version.prepare(self.root, "patch")
                self.assertEqual(before, self.snapshot())
                (self.root / name).write_bytes(original)

    def test_partial_write_failure_restores_original_files(self):
        _, _, originals, updates = version.prepare(self.root, "patch")
        real_write = version.atomic_write
        calls = 0

        def fail_second_write(path, data):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise OSError("Simulated disk error")
            real_write(path, data)

        with patch.object(version, "atomic_write", side_effect=fail_second_write):
            with self.assertRaises(OSError):
                version.apply(self.root, originals, updates)
        self.assertEqual(originals, self.snapshot())

    def test_windows_version_limits_and_invalid_semver(self):
        for current in ("01.0.0", "0.0.1-beta", "1.0", "1.0.65536"):
            with self.subTest(current=current):
                with self.assertRaises(version.VersionError):
                    version.next_version(current, "patch")
        with self.assertRaises(version.VersionError):
            version.next_version("1.0.65535", "patch")


if __name__ == "__main__":
    unittest.main()
