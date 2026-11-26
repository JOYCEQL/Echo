"""Publish the same signed files as R2, resuming a partially uploaded GitHub draft."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from bump_version import ROOT, prepare
from publish_downloads import PLATFORMS


def gh(*args):
    return subprocess.check_output(["gh", *args], text=True)


def assets(tag):
    release_id = gh("api", "--paginate", "repos/{owner}/{repo}/releases?per_page=100",
                    "--jq", f'.[] | select(.tag_name == "{tag}") | .id').strip()
    release = json.loads(gh("api", f"repos/{{owner}}/{{repo}}/releases/{int(release_id)}"))
    return release["draft"], {item["name"]: item["digest"] for item in release["assets"]}


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
    # Listing includes unpublished drafts and distinguishes API failures from an absent release.
    tags = gh("api", "--paginate", "repos/{owner}/{repo}/releases?per_page=100", "--jq", ".[].tag_name").splitlines()
    if tag not in tags:
        gh("release", "create", tag, "--verify-tag", "--draft", "--title", f"Echo {version}", "--generate-notes")
    draft, uploaded = assets(tag)
    if not uploaded.items() <= expected.items():
        raise ValueError("existing release contains different assets")
    for name in sorted(expected.keys() - uploaded.keys()):
        if not draft:
            raise ValueError("published release is incomplete")
        gh("release", "upload", tag, str(archive / name))
    if assets(tag)[1] != expected:
        raise ValueError("GitHub asset digests do not match the signed archive")
    if draft:
        gh("release", "edit", tag, "--draft=false", "--latest")
    print(gh("release", "view", tag, "--json", "url", "--jq", ".url").strip())


if __name__ == "__main__":
    main()
