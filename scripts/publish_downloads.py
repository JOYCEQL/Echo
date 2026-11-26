#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["boto3>=1.39,<2"]
# ///
"""Publish Echo packages to its Cloudflare R2 Standard bucket."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import time
import uuid

from bump_version import ROOT, parts, prepare

ORIGIN = "https://echo-downloads.magicv.art"
BUCKET = "echo-downloads"
ACCOUNT = "68b62d984f0498de86dce8630636ba01"
LONG = "public, max-age=31536000, immutable"
SHORT = "public, max-age=300"
PUBLIC_KEY = ROOT / "packaging/echo-update.pub"
PLATFORMS = {"macos-arm64": ".zip", "macos-x86_64": ".zip", "windows-x64": "-setup.exe"}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def encode(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()


def signed(path, comment, secret, work, existing=None):
    signature = work / (path.name + ".minisig")
    if existing is not None:
        signature.write_bytes(existing)
        lines = existing.decode().splitlines()
        if len(lines) != 4 or lines[2] != f"trusted comment: {comment}":
            raise ValueError(f"Unexpected signature name for {path.name}")
        subprocess.run(["minisign", "-V", "-q", "-p", PUBLIC_KEY, "-m", path, "-x", signature], check=True)
        return existing
    password = None
    service = os.environ.get("ECHO_MINISIGN_KEYCHAIN_SERVICE")
    if service:
        import getpass
        password = subprocess.check_output(["security", "find-generic-password", "-a", getpass.getuser(), "-s", service, "-w"])
    elif "MINISIGN_PASSWORD" in os.environ:
        password = (os.environ["MINISIGN_PASSWORD"] + "\n").encode()
    subprocess.run(["minisign", "-S", "-s", secret, "-m", path, "-x", signature, "-t", comment], input=password, check=True)
    subprocess.run(["minisign", "-V", "-q", "-p", PUBLIC_KEY, "-m", path, "-x", signature], check=True)
    return signature.read_bytes()


def mime(name):
    ext = Path(name).suffix
    return {".zip": "application/zip", ".dmg": "application/x-apple-diskimage",
            ".exe": "application/vnd.microsoft.portable-executable", ".gz": "application/gzip",
            ".json": "application/json", ".minisig": "text/plain", ".sha256": "text/plain"}.get(ext, "text/plain")


class Bucket:
    def __init__(self):
        import boto3
        from botocore.config import Config
        self.api = boto3.client("s3", endpoint_url=f"https://{ACCOUNT}.r2.cloudflarestorage.com",
                                region_name="auto", config=Config(signature_version="s3v4",
                                request_checksum_calculation="when_required", response_checksum_validation="when_required"))

    def read(self, key):
        from botocore.exceptions import ClientError
        try:
            return self.api.get_object(Bucket=BUCKET, Key=key)["Body"].read()
        except ClientError as error:
            if error.response["Error"]["Code"] == "NoSuchKey":
                return None
            raise

    @contextmanager
    def exclusive(self):
        key = "_publish.lock"
        lease = self.api.put_object(Bucket=BUCKET, Key=key, Body=uuid.uuid4().hex.encode(),
                                    IfNoneMatch="*", CacheControl="no-store")
        try:
            yield
        finally:
            self.api.delete_object(Bucket=BUCKET, Key=key, IfMatch=lease["ETag"])

    def put(self, key, data, *, immutable=False, filename=None):
        from botocore.exceptions import ClientError
        options = {"IfNoneMatch": "*"} if immutable else {}
        if filename:
            options["ContentDisposition"] = f'attachment; filename="{filename}"'
        try:
            self.api.put_object(Bucket=BUCKET, Key=key, Body=data, ContentType=mime(key),
                                CacheControl=LONG if immutable else SHORT, **options)
        except ClientError as error:
            if not immutable or error.response["ResponseMetadata"]["HTTPStatusCode"] != 412:
                raise
        if self.read(key) != data:
            raise ValueError(f"Published bytes differ for {key}; use a new version")


def release(bucket, version, platforms, directory, secret, work):
    prefix = f"v{version}/"
    files = [f"Echo-{version}-source.tar.gz"]
    manifests, aliases = {}, {}
    for platform in PLATFORMS:
        current = bucket.read(f"latest/update-{platform}.json")
        if current is not None and parts(json.loads(current)["version"]) > parts(version):
            raise ValueError(f"{platform} already offers a newer version")
    for platform in platforms:
        package = f"Echo-{version}-{platform}{PLATFORMS[platform]}"
        files.append(package)
        manual = package
        if platform.startswith("macos-"):
            manual = f"Echo-{version}-{platform}.dmg"
            files.append(manual)
        aliases[manual.replace(f"-{version}-", "-", 1)] = manual
        manifests[platform] = package

    packages = work / "packages"
    packages.mkdir()
    for name in files:
        shutil.copyfile(directory / name, packages / name)
        sidecar = directory / (name + ".sha256")
        if sidecar.is_file():
            checksum = f"{digest((packages / name).read_bytes())}  {name}"
            if sidecar.read_text(encoding="utf-8").strip() != checksum:
                raise ValueError(f"Build checksum mismatch: {name}")
    checks = {}
    sums = []
    for name in files:
        path = packages / name
        size = path.stat().st_size
        if not 0 < size <= 512 * 1024 * 1024:
            raise ValueError(f"Invalid file size for {name}")
        data = path.read_bytes()
        checksum = f"{digest(data)}  {name}\n"
        sums.append(checksum)
        checks[name] = digest(data)
        signature = signed(path, name, secret, work, bucket.read(prefix + name + ".minisig"))
        bucket.put(prefix + name, data, immutable=True, filename=name)
        bucket.put(prefix + name + ".sha256", checksum.encode(), immutable=True)
        bucket.put(prefix + name + ".minisig", signature, immutable=True)
        for platform, package in manifests.items():
            if package == name:
                manifest = dict(version=version, platform=platform, url=f"{ORIGIN}/{prefix}{name}",
                                size=size, signature=signature.decode())
                (work / f"update-{platform}.json").write_bytes(encode(manifest))

    checksums = work / "SHA256SUMS"
    checksums.write_text("".join(sorted(sums)), encoding="utf-8")
    # Each version has one complete release set, including all declared platforms.
    bucket.put(prefix + checksums.name, checksums.read_bytes(), immutable=True)
    bucket.put(prefix + "SHA256SUMS.minisig", signed(checksums, checksums.name, secret, work,
               bucket.read(prefix + "SHA256SUMS.minisig")), immutable=True)
    if {"macos-arm64", "macos-x86_64"} <= manifests.keys():
        cask = f'''cask "echo" do
  arch arm: "arm64", intel: "x86_64"
  version "{version}"
  sha256 arm: "{checks[manifests['macos-arm64']]}", intel: "{checks[manifests['macos-x86_64']]}"
  url "{ORIGIN}/v#{{version}}/Echo-#{{version}}-macos-#{{arch}}.zip"
  name "Echo"
  desc "Local-first document and canvas editor"
  homepage "https://github.com/JOYCEQL/Echo"
  auto_updates true
  depends_on macos: ">= :ventura"
  app "Echo.app"
end
'''.encode()
        (work / "echo.rb").write_bytes(cask)
        bucket.put(prefix + "echo.rb", cask, immutable=True)
        bucket.put("latest/echo.rb", cask)
    for alias, name in aliases.items():
        bucket.put("latest/" + alias, (packages / name).read_bytes(), filename=name)
    source = f"Echo-{version}-source.tar.gz"
    bucket.put("latest/Echo-source.tar.gz", (packages / source).read_bytes(), filename=source)
    for platform in manifests:
        name = f"update-{platform}.json"
        bucket.put("latest/" + name, (work / name).read_bytes())
    archive = directory / f"archive-v{version}"
    archive.mkdir(exist_ok=True)
    for name in files:
        shutil.copyfile(packages / name, archive / name)
        (archive / (name + ".sha256")).write_text(f"{checks[name]}  {name}\n", encoding="utf-8")
    for path in work.iterdir():
        if path.is_file():
            shutil.copyfile(path, archive / path.name)


def policy(bucket, args, secret, work):
    overrides = {}
    for pair in args.platform_min:
        name, value = pair.split("=", 1)
        if name not in PLATFORMS or name in overrides:
            raise ValueError(f"Invalid platform override: {name}")
        parts(value)
        overrides[name] = value
    if args.min_version:
        parts(args.min_version)
    available = 0
    for name in PLATFORMS:
        minimum = overrides.get(name, args.min_version)
        if minimum:
            current = bucket.read(f"latest/update-{name}.json")
            if current is None and name not in overrides:
                continue
            if current is None or parts(json.loads(current)["version"]) < parts(minimum):
                raise ValueError(f"Publish a suitable {name} release first")
            available += 1
    if args.min_version and not available:
        raise ValueError("Publish a release before setting the minimum version")
    previous = bucket.read("latest/policy.json")
    issued = time.time_ns() // 1_000_000
    if previous and issued <= json.loads(json.loads(previous)["policy"])["issued"]:
        raise ValueError("Policy timestamp must increase")
    policy_file = work / "policy"
    payload = encode(dict(issued=issued, min_version=args.min_version, platforms=overrides,
                          message_zh=args.message_zh, message_en=args.message_en))
    policy_file.write_bytes(payload)
    signature = signed(policy_file, "echo-policy sha256:" + digest(payload), secret, work)
    bucket.put("latest/policy.json", encode(dict(policy=payload.decode(), signature=signature.decode())))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    package = commands.add_parser("release")
    package.add_argument("--dist", type=Path, default=ROOT / "dist")
    package.add_argument("platforms", nargs="+", choices=PLATFORMS)
    forced = commands.add_parser("policy")
    forced.add_argument("--min-version")
    forced.add_argument("--platform-min", action="append", default=[])
    forced.add_argument("--message-zh", default="")
    forced.add_argument("--message-en", default="")
    args = parser.parse_args()
    version = prepare(ROOT)[0]
    secret = Path(os.environ["MINISIGN_SECRET_KEY"]).resolve(strict=True)
    bucket = Bucket()
    with tempfile.TemporaryDirectory() as folder, bucket.exclusive():
        if args.command == "policy":
            policy(bucket, args, secret, Path(folder))
        else:
            if len(args.platforms) != len(set(args.platforms)):
                parser.error("duplicate platform")
            release(bucket, version, args.platforms, args.dist, secret, Path(folder))


if __name__ == "__main__":
    main()
