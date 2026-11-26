"""Publish the same signed files as R2, resuming a partially uploaded GitHub draft."""
import argparse
import hashlib
import os
import urllib.request
import json
from pathlib import Path
import subprocess

from bump_version import ROOT, prepare
from publish_downloads import PLATFORMS


def gh(*args):
    return subprocess.check_output(["gh", *args], text=True)


def release_for_tag(tag, version):
    endpoint = "repos/{owner}/{repo}/releases"
    existing = gh("api", "--paginate", endpoint + "?per_page=100", "--jq",
                  f'.[] | select(.tag_name == "{tag}") | .id').strip()
    if existing:
        return json.loads(gh("api", f"{endpoint}/{int(existing)}"))
    # GitHub's list can lag behind creation. Use the identity returned by the write.
    return json.loads(gh("api", "--method", "POST", endpoint, "-f", f"tag_name={tag}",
                         "-f", f"name=Echo {version}", "-F", "draft=true", "-F", "generate_release_notes=true"))


def asset_digests(release):
    return {item["name"]: item["digest"] for item in release["assets"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    tag = parser.parse_args().tag
    version = prepare(ROOT)[0]
    if tag != "v" + version:
        parser.error("tag must match the checked-out release")
    archive = ROOT / "dist" / ("archive-" + tag)
    sums = (archive / "SHA256SUMS").read_text().splitlines()
    packages = {f"Echo-{version}-{platform}{ext}" for platform, ext in PLATFORMS.items()}
    packages.update(f"Echo-{version}-macos-{arch}.dmg" for arch in ("arm64", "x86_64"))
    packages.add(f"Echo-{version}-source.tar.gz")
    if {line.split("  ", 1)[1] for line in sums} != packages:
        raise ValueError("incomplete package checksums")
    names = {name + ext for name in packages for ext in ("", ".sha256", ".minisig")}
    names.update({"SHA256SUMS", "SHA256SUMS.minisig", "echo.rb"})
    names.update(f"update-{platform}.json" for platform in PLATFORMS)
    expected = {name: "sha256:" + hashlib.sha256((archive / name).read_bytes()).hexdigest() for name in names}
    release = release_for_tag(tag, version)
    draft = release["draft"]
    uploaded = asset_digests(release)
    endpoint = f"repos/{{owner}}/{{repo}}/releases/{release['id']}"
    if not uploaded.items() <= expected.items():
        raise ValueError("existing release contains different assets")
    for name in sorted(expected.keys() - uploaded.keys()):
        if not draft:
            raise ValueError("published release is incomplete")
        upload = release["upload_url"].split("{", 1)[0]
        # The upload endpoint accepts bytes directly and addresses the draft by ID.
        request = urllib.request.Request(upload + "?name=" + name, data=(archive / name).read_bytes(),
                                         headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
                                                  "Content-Type": "application/octet-stream"}, method="POST")
        with urllib.request.urlopen(request, timeout=180) as response:
            asset = json.load(response)
        if asset["digest"] != expected[name]:
            raise ValueError(f"GitHub upload digest mismatch for {name}")
    if asset_digests(json.loads(gh("api", endpoint))) != expected:
        raise ValueError("GitHub asset digests do not match the signed archive")
    if draft:
        release = json.loads(gh("api", "--method", "PATCH", endpoint, "-F", "draft=false", "-f", "make_latest=true"))
    if release["draft"]:
        raise ValueError("GitHub release is still a draft")
    print(release["html_url"])


if __name__ == "__main__":
    main()
