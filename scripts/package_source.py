#!/usr/bin/env python3
"""Archive the source tree used to build this release, including named new files."""
import argparse
import gzip
import os
from pathlib import Path
import subprocess
import tarfile
from bump_version import ROOT, prepare


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--include", action="append", default=[], metavar="NEW_FILE")
    args = parser.parse_args()
    version = prepare(ROOT)[0]
    def paths(*options):
        raw = subprocess.check_output(["git", "ls-files", "-z", *options], cwd=ROOT)
        return set(map(os.fsdecode, filter(None, raw.split(b"\0"))))
    tracked = paths("--cached") - paths("--deleted")
    new = paths("--others", "--exclude-standard")
    if set(args.include) != new:
        parser.error(f"Review and name every new source file with --include: {sorted(new)}")
    target = ROOT / "dist" / f"Echo-{version}-source.tar.gz"
    target.parent.mkdir(exist_ok=True)
    with target.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as gz:
        with tarfile.open(fileobj=gz, mode="w", dereference=False) as tar:
            def normalize(info):
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ""
                return info
            for name in sorted(tracked | new):
                tar.add(ROOT / name, arcname=f"Echo-{version}/{name}", recursive=False, filter=normalize)
    print(target)


if __name__ == "__main__":
    main()
