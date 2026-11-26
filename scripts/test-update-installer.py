#!/usr/bin/env python3
"""macOS helper integration test; disposable key, app bundles and isolated data only."""
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


with tempfile.TemporaryDirectory(prefix="echo-update-audit-") as temporary:
    base = Path(temporary).resolve()
    secret, public = base / "test.key", base / "test.pub"
    run("minisign", "-G", "-W", "-s", str(secret), "-p", str(public), stdout=subprocess.DEVNULL)
    env = os.environ.copy()
    # Exercise the same embedded-key path as a real build in a disposable source tree.
    source = base / "source"
    shutil.copytree(ROOT, source, ignore=shutil.ignore_patterns(".git", "target", "dist", "__pycache__"))
    shutil.copyfile(public, source / "packaging/echo-update.pub")
    env["CARGO_TARGET_DIR"] = str(ROOT / "target/qa")
    run("cargo", "build", "--locked", cwd=source, env=env)
    current = next(p["version"] for p in json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT))["packages"] if p["name"] == "echo")
    parts = [int(p) for p in current.split(".")]
    parts[2] += 1
    version = ".".join(map(str, parts))
    platform = {"arm64": "macos-arm64", "x86_64": "macos-x86_64"}[os.uname().machine]
    data = base / "isolated-data"
    data.mkdir()
    (data / "note.md").write_text("This note must remain untouched.\n")
    env["ECHO_DATA_DIR"] = str(data)

    def bundle(path, executable, ver):
        (path / "Contents/MacOS").mkdir(parents=True)
        shutil.copy2(executable, path / "Contents/MacOS/Echo")
        with (path / "Contents/Info.plist").open("wb") as f:
            plistlib.dump(dict(CFBundleIdentifier="local.echo.macos", CFBundleExecutable="Echo",
                              CFBundleName="Echo", CFBundlePackageType="APPL", LSBackgroundOnly=True,
                              CFBundleShortVersionString=ver, CFBundleVersion=ver), f)
        run("codesign", "--force", "--sign", "-", str(path), stderr=subprocess.DEVNULL)

    old = base / "Echo.app"
    bundle(old, ROOT / "target/qa/debug/echo", current)
    source = base / "probe.c"
    source.write_text('#include <stdio.h>\n#include <stdlib.h>\n'
                      'int main(void) { char p[4096]; const char *d=getenv("ECHO_DATA_DIR"); '
                      'if(!d)return 2; snprintf(p,sizeof(p),"%s/restarted.txt",d); '
                      'FILE *f=fopen(p,"w"); if(!f)return 3; fputs("new version started",f); '
                      'return fclose(f); }\n')
    probe = base / "probe"
    run("clang", str(source), "-o", str(probe))
    stage = Path(tempfile.mkdtemp(prefix=".echo-update-", dir=base))
    new = stage / "unpacked/Echo.app"
    bundle(new, probe, version)
    package = stage / "package.zip"
    run("ditto", "-c", "-k", "--keepParent", str(new), str(package))
    name = f"Echo-{version}-{platform}.zip"
    run("minisign", "-S", "-s", str(secret), "-m", str(package), "-t", name)
    manifest = dict(version=version, platform=platform, url=f"https://echo-downloads.magicv.art/v{version}/{name}", size=package.stat().st_size,
                    signature=Path(str(package)+".minisig").read_text())
    metadata = old.stat()
    plan = dict(manifest=manifest, app=str(old), device=metadata.st_dev, inode=metadata.st_ino)
    (stage / "plan.json").write_text(json.dumps(plan))
    parent = subprocess.Popen(["/bin/sleep", "90"])
    helper = None
    try:
        # A corrupt package cannot even reach the READY handshake.
        payload = package.read_bytes()
        package.write_bytes(bytes([payload[0] ^ 1])+payload[1:])
        rejected = subprocess.run([str(old / "Contents/MacOS/Echo"), "--apply-update", str(stage), str(parent.pid)],
                                  env=env, input="INSTALL\n", capture_output=True, text=True, timeout=30)
        assert rejected.returncode != 0 and not rejected.stdout
        assert old.stat().st_ino == metadata.st_ino
        package.write_bytes(payload)
        helper = subprocess.Popen([str(old / "Contents/MacOS/Echo"), "--apply-update", str(stage), str(parent.pid)],
                                  env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        import select
        assert select.select([helper.stdout], [], [], 45)[0], "helper handshake timed out"
        handshake = helper.stdout.readline()
        assert handshake == "READY\n", helper.stderr.read()
        assert old.stat().st_ino == metadata.st_ino, "must not replace before commit"
        helper.stdin.write("INSTALL\n")
        helper.stdin.close()
        helper.stdin = None
        time.sleep(0.3)
        assert old.stat().st_ino == metadata.st_ino, "must wait for running application"
        parent.terminate()
        parent.wait(timeout=5)
        stdout, stderr = helper.communicate(timeout=30)
        assert helper.returncode == 0, stderr
        deadline = time.monotonic()+15
        marker = data / "restarted.txt"
        while time.monotonic() < deadline:
            if marker.exists() and marker.read_text() == "new version started":
                break
            time.sleep(0.1)
        assert marker.exists() and marker.read_text() == "new version started", "updated process did not finish its startup probe"
        assert (data / "note.md").read_text() == "This note must remain untouched.\n"
        assert not stage.exists(), "successful install must clean its staging directory"
        print("PASS: corrupt package rejected; commit/exit gates; atomic swap; new process started; data unchanged; stage cleaned")
    finally:
        if helper is not None and helper.poll() is None:
            helper.kill()
            helper.wait()
        if parent.poll() is None:
            parent.terminate()
            parent.wait()
