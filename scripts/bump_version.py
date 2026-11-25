#!/usr/bin/env python3
"""Prepare one shared Echo release version before building either platform."""

import argparse
import os
from pathlib import Path
import plistlib
import re
import tempfile
import tomllib
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parent.parent
FILES = (
    "Cargo.toml", "Cargo.lock", "packaging/Info.plist",
    "packaging/windows/Echo.manifest", "README.md",
)
SEMVER = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"


class VersionError(ValueError):
    pass


def parts(version):
    if not re.fullmatch(SEMVER, version):
        raise VersionError(f"Expected a stable major.minor.patch version: {version}")
    values = tuple(map(int, version.split(".")))
    if any(value > 65535 for value in values):
        raise VersionError("Version components must fit Windows' 16-bit version fields")
    return values


def next_version(current, part):
    major, minor, patch = parts(current)
    if part == "major":
        result = f"{major + 1}.0.0"
    elif part == "minor":
        result = f"{major}.{minor + 1}.0"
    elif part == "patch":
        result = f"{major}.{minor}.{patch + 1}"
    else:
        raise VersionError(f"Unknown version part: {part}")
    parts(result)
    return result


def replace_one(text, pattern, value, label):
    result, count = re.subn(pattern, lambda m: m[1] + value + m[2], text)
    if count != 1:
        raise VersionError(f"Expected exactly one {label}, found {count}")
    return result


def prepare(root, part=None):
    """Validate all version references before returning any changes."""
    originals = {name: (root / name).read_bytes() for name in FILES}
    texts = {name: data.decode("utf-8") for name, data in originals.items()}
    manifest = tomllib.loads(texts["Cargo.toml"])
    if manifest["package"]["name"] != "echo":
        raise VersionError("Expected the Echo package")
    current = manifest["package"]["version"]
    parts(current)
    target = next_version(current, part) if part else current
    old = re.escape(current)

    # Limit replacements to Echo's package blocks; dependency versions stay intact.
    cargo = re.search(r"(?ms)^\[package\]\r?\n.*?(?=^\[|\Z)", texts["Cargo.toml"])
    if not cargo:
        raise VersionError("Missing Cargo [package] section")
    block = replace_one(cargo[0], rf'(?m)^(version\s*=\s*"){old}("[^\r\n]*\r?)$', target, "Cargo package version")
    texts["Cargo.toml"] = texts["Cargo.toml"][:cargo.start()] + block + texts["Cargo.toml"][cargo.end():]

    locked = [p for p in tomllib.loads(texts["Cargo.lock"])["package"] if p["name"] == "echo"]
    if len(locked) != 1 or locked[0]["version"] != current:
        raise VersionError("Cargo.lock does not match Echo's current version")
    lock = re.search(r'(?ms)^\[\[package\]\]\r?\nname = "echo"\r?\n.*?(?=^\[\[package\]\]|\Z)', texts["Cargo.lock"])
    if not lock:
        raise VersionError("Missing Echo lockfile entry")
    block = replace_one(lock[0], rf'(?m)^(version = "){old}("[^\r\n]*\r?)$', target, "Echo lockfile version")
    texts["Cargo.lock"] = texts["Cargo.lock"][:lock.start()] + block + texts["Cargo.lock"][lock.end():]

    plist = plistlib.loads(originals["packaging/Info.plist"])
    for key in ("CFBundleVersion", "CFBundleShortVersionString"):
        if plist.get(key) != current:
            raise VersionError(f"{key} does not match {current}")
        texts["packaging/Info.plist"] = replace_one(
            texts["packaging/Info.plist"],
            rf"(<key>{key}</key>\s*<string>){old}(</string>)", target, key,
        )

    identity = ET.fromstring(originals["packaging/windows/Echo.manifest"]).find("{urn:schemas-microsoft-com:asm.v1}assemblyIdentity")
    if identity is None or identity.get("version") != current + ".0":
        raise VersionError("Windows manifest does not match Echo's current version")
    texts["packaging/windows/Echo.manifest"] = replace_one(
        texts["packaging/windows/Echo.manifest"],
        rf'(<assemblyIdentity\s+version="){old}\.0(")', target + ".0", "Windows assembly version",
    )

    link = re.compile(r"(/releases/download/v)([0-9]+\.[0-9]+\.[0-9]+)(/Echo-)([0-9]+\.[0-9]+\.[0-9]+)(-(?:macos|windows)-)")
    links = list(link.finditer(texts["README.md"]))
    if not links or any(m[2] != current or m[4] != current for m in links):
        raise VersionError("README download links do not match Echo's current version")
    texts["README.md"] = link.sub(lambda m: m[1] + target + m[3] + target + m[5], texts["README.md"])
    return current, target, originals, {name: text.encode("utf-8") for name, text in texts.items()}


def atomic_write(path, data):
    mode = path.stat().st_mode
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".echo-version-", delete=False) as temp:
            temporary = Path(temp.name)
            temp.write(data)
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def apply(root, originals, updates):
    written = []
    try:
        for name, data in updates.items():
            if data != originals[name]:
                atomic_write(root / name, data)
                written.append(name)
    except OSError:
        for name in reversed(written):
            atomic_write(root / name, originals[name])
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--part", choices=("patch", "minor", "major"), default="patch")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="Validate version consistency without changing files")
    mode.add_argument("--dry-run", action="store_true", help="Print the next version without changing files")
    args = parser.parse_args()
    try:
        _, target, originals, updates = prepare(ROOT, None if args.check else args.part)
        if not args.check and not args.dry_run:
            apply(ROOT, originals, updates)
        print(target)
    except (VersionError, OSError, KeyError, tomllib.TOMLDecodeError, ET.ParseError, plistlib.InvalidFileException) as error:
        parser.exit(1, f"Version update failed: {error}\n")


if __name__ == "__main__":
    main()
