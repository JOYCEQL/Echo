#!/usr/bin/env python3
"""One-time SQLite -> Markdown transfer. SQLite is never used by the new app.
Run with Echo closed for --apply. Without it, only prepare and verify a snapshot.
The original database and attachments are backed up before installation.
"""
import argparse
import datetime
import json
import hashlib
import pathlib
import shutil
import sqlite3
import subprocess
import tempfile

p = argparse.ArgumentParser()
p.add_argument('--root', type=pathlib.Path, required=True)
p.add_argument('--binary', type=pathlib.Path, required=True)
p.add_argument('--apply', action='store_true')
a = p.parse_args()
root = a.root.resolve()
if (root / '.echo/index.json').exists():
    raise SystemExit('This library already uses Markdown; migration refused.')
db = root / 'echo.sqlite3'
if not db.is_file():
    raise SystemExit('No legacy echo.sqlite3 found.')
if a.apply:
    running = subprocess.run(['pgrep', '-f', '/Echo.app/Contents/MacOS/Echo'], capture_output=True)
    if running.returncode == 0:
        raise SystemExit('Quit Echo before applying the migration.')
    for name in ('Documents', 'Canvas', '.echo'):
        if (root / name).exists():
            raise SystemExit(f'{name} already exists; inspect it before migration.')
stamp = datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=8))).strftime('%Y%m%d-%H%M%S')
work = pathlib.Path(tempfile.mkdtemp(prefix='echo-md-migration-', dir=root.parent))
snapshot = work / 'original.sqlite3'
# A closed WAL-mode database has no shared-memory files. Open that immutable
# snapshot explicitly; a live library must use SQLite's online backup protocol.
wal = root / 'echo.sqlite3-wal'
if a.apply and wal.exists():
    raise SystemExit('The legacy WAL is still present; close/checkpoint the old app before migrating.')
offline = not wal.exists()
if offline:
    running = subprocess.run(['pgrep', '-f', '/Echo.app/Contents/MacOS/Echo'], capture_output=True)
    if running.returncode == 0:
        raise SystemExit('The old app is running without a WAL; close it before taking an offline snapshot.')
    source_digest = hashlib.sha256(db.read_bytes()).hexdigest()
source = sqlite3.connect(db.as_uri() + ('?mode=ro&immutable=1' if offline else '?mode=ro'), uri=True)
with sqlite3.connect(snapshot) as dest:
    source.backup(dest)
source.close()
with sqlite3.connect(snapshot) as source:
    columns = {r[1] for r in source.execute('PRAGMA table_info(notes)')}
    order = 'sort_order, rowid' if 'sort_order' in columns else 'rowid'
    notes = [json.loads(r[0]) for r in source.execute('SELECT canvas FROM notes ORDER BY ' + order)]
    prefs = dict(source.execute('SELECT key,value FROM preferences'))
# The pre-v3 canvas used an array for tasks. Modern versions render the
# Markdown body; remove obsolete shadow task arrays from the transfer.
for note in notes:
    for block in note.get('blocks', []):
        tasks = block.pop('tasks', [])
        if note.get('editor_version', 0) < 3 and block.get('kind') == 'Tasks':
            block['body'] = '\n'.join('- [%s] %s' % ('x' if t['done'] else ' ', t['text']) for t in tasks)
transfer = work / 'transfer.json'
transfer.write_text(json.dumps({'notes': notes, 'preferences': prefs}, ensure_ascii=False), encoding='utf-8')
stage = work / 'library'
stage.mkdir()
if (root / 'images').exists():
    shutil.copytree(root / 'images', stage / 'images')
subprocess.run([str(a.binary.resolve()), '--import-library', str(transfer), str(stage)], check=True)
report = {'notes': len(notes), 'documents': sum(n.get('format') == 'Document' for n in notes),
          'canvas': sum(n.get('format', 'Canvas') == 'Canvas' for n in notes),
          'stage': str(stage), 'applied': a.apply}
if a.apply:
    if wal.exists() or hashlib.sha256(db.read_bytes()).hexdigest() != source_digest:
        raise SystemExit('The source library changed during verification; migration was not installed.')
    backup = root / 'Backups' / ('before-native-markdown-' + stamp)
    backup.mkdir(parents=True, exist_ok=False)
    shutil.copy2(snapshot, backup / 'echo.sqlite3')
    shutil.copy2(transfer, backup / 'verified-transfer.json')
    if (root / 'images').exists():
        shutil.copytree(root / 'images', backup / 'images')
    for name in ('Documents', 'Canvas'):
        if (stage / name).exists():
            shutil.move(str(stage / name), root / name)
    # Attachments retain their original location and bytes. Add only new assets.
    for asset in (stage / 'images').rglob('*'):
        if asset.is_file():
            dest = root / 'images' / asset.relative_to(stage / 'images')
            dest.parent.mkdir(parents=True, exist_ok=True)
            if dest.exists():
                if dest.read_bytes() != asset.read_bytes():
                    raise SystemExit(f'Attachment changed during migration: {dest}')
            else:
                shutil.copy2(asset, dest)
    # Commit marker last. Until this rename, the new app refuses the legacy root.
    shutil.move(str(stage / '.echo'), root / '.echo')
    for name in ('echo.sqlite3', 'echo.sqlite3-wal', 'echo.sqlite3-shm'):
        src = root / name
        if src.exists():
            shutil.move(str(src), backup / ('legacy-' + name))
    # Old Electron builds left browser databases and caches here. They are not
    # used by the native app; archive them rather than leave live-looking DBs.
    legacy_names = ('blob_storage', 'DawnGraphiteCache', 'Session Storage', 'GPUCache',
                    'Preferences', 'DIPS', 'Cache', 'settings.json', 'SharedStorage',
                    'Shared Dictionary', 'Network Persistent State', 'Trust Tokens-journal',
                    'Trust Tokens', 'DawnWebGPUCache', 'Code Cache', 'Local Storage',
                    'Cookies', 'Cookies-journal')
    for name in legacy_names:
        if (root / name).exists():
            archived = backup / 'legacy-app-data'
            archived.mkdir(exist_ok=True)
            shutil.move(str(root / name), archived / name)
    report['backup'] = str(backup)
    (root / '.echo/migration.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
print(json.dumps(report, ensure_ascii=False, indent=2))
