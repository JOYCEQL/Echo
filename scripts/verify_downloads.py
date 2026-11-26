#!/usr/bin/env python3
"""Verify the anonymous public release, including caches, aliases, signatures and feeds."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import urllib.request
from publish_downloads import ORIGIN, PLATFORMS, PUBLIC_KEY
from bump_version import parts


def get(path, versioned):
    url = f"{ORIGIN}/{path}"
    request = urllib.request.Request(url, headers={"User-Agent": "Echo-Release-Verification/1.0"})
    with urllib.request.urlopen(request, timeout=120) as response:
        if response.status != 200 or response.geturl() != url:
            raise ValueError(f"Unexpected response for {path}")
        cache = {part.strip() for part in response.headers["Cache-Control"].split(",")}
        expected = {"public", "max-age=31536000", "immutable"} if versioned else {"public", "max-age=300"}
        if not expected <= cache:
            raise ValueError(f"Incorrect Cache-Control for {path}: {cache}")
        return response.read()


def verify_signature(work, payload, signature, name):
    data, sig = work / "data", work / "signature"
    if signature.decode().splitlines()[2] != f"trusted comment: {name}":
        raise ValueError(f"Unexpected signature identity for {name}")
    data.write_bytes(payload)
    sig.write_bytes(signature)
    subprocess.run(["minisign", "-V", "-q", "-p", PUBLIC_KEY, "-m", data, "-x", sig], check=True)


def verify(version, platforms, work):
    prefix = f"v{version}/"
    sums = get(prefix + "SHA256SUMS", True)
    verify_signature(work, sums, get(prefix + "SHA256SUMS.minisig", True), "SHA256SUMS")
    files = {}
    for line in sums.decode().splitlines():
        sha, name = line.split("  ", 1)
        if Path(name).name != name or name in files:
            raise ValueError(f"Invalid release filename: {name}")
        payload = get(prefix + name, True)
        if hashlib.sha256(payload).hexdigest() != sha:
            raise ValueError(f"Incorrect SHA-256 for {name}")
        if get(prefix + name + ".sha256", True).decode().strip() != line:
            raise ValueError(f"Incorrect SHA-256 sidecar for {name}")
        signature = get(prefix + name + ".minisig", True)
        verify_signature(work, payload, signature, name)
        files[name] = (sha, len(payload), signature.decode())
        print(f"Verified {name} ({len(payload)} bytes)", flush=True)
    for platform in platforms:
        name = f"Echo-{version}-{platform}{PLATFORMS[platform]}"
        manifest = json.loads(get(f"latest/update-{platform}.json", False))
        if manifest != dict(version=version, platform=platform, url=f"{ORIGIN}/{prefix}{name}",
                            size=files[name][1], signature=files[name][2]):
            raise ValueError(f"Unexpected manifest for {platform}")
    aliases = [name for name in files if name.endswith((".dmg", ".exe", ".tar.gz"))]
    for name in aliases:
        alias = name.replace(f"-{version}-", "-", 1)
        if hashlib.sha256(get("latest/" + alias, False)).hexdigest() != files[name][0]:
            raise ValueError(f"Stale or invalid latest link for {name}")
    policy = json.loads(get("latest/policy.json", False))
    payload = policy["policy"].encode()
    verify_signature(work, payload, policy["signature"].encode(), "echo-policy sha256:" + hashlib.sha256(payload).hexdigest())
    if {"macos-arm64", "macos-x86_64"} <= set(platforms):
        cask = get("latest/echo.rb", False)
        if cask != get(prefix + "echo.rb", True):
            raise ValueError("Homebrew cask differs from the versioned cask")
        for platform in ("macos-arm64", "macos-x86_64"):
            if files[f"Echo-{version}-{platform}.zip"][0].encode() not in cask:
                raise ValueError("Homebrew checksum mismatch")
    print("Verified all requested feeds, stable links and policy", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument("platforms", nargs="+", choices=PLATFORMS)
    args = parser.parse_args()
    parts(args.version)
    with tempfile.TemporaryDirectory() as folder:
        verify(args.version, args.platforms, Path(folder))
