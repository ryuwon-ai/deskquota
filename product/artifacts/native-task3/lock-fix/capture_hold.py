#!/usr/bin/env python3
import datetime
import gzip
import hashlib
import io
import json
import pathlib
import tarfile

OUT = pathlib.Path(__file__).resolve().parent
PRODUCT = OUT.parents[2]
RESEARCH = PRODUCT.parent
INITIAL = PRODUCT / 'artifacts/native-task3'
BASE = PRODUCT / 'artifacts/native-task2/quality-fix'

def digest_bytes(data):
    return hashlib.sha256(data).hexdigest()

def digest(path):
    return digest_bytes(path.read_bytes())

source_paths = (INITIAL / 'source-files.txt').read_text().splitlines()
if len(source_paths) != 79 or len(set(source_paths)) != 79:
    raise SystemExit(f'expected 79 unique source files, found {len(source_paths)}')
files = []
for relative in source_paths:
    path = RESEARCH / relative
    data = path.read_bytes()
    files.append({'path': relative, 'bytes': len(data), 'sha256': digest_bytes(data)})
manifest = {
    'phase': 'native-task3-lock-fix-final-hold',
    'captured_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'files': files,
}
(OUT / 'source-files.txt').write_text('\n'.join(source_paths) + '\n')
(OUT / 'source-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')

def changes_against(manifest_path):
    before_items = json.loads(manifest_path.read_text())['files']
    before = {item['path']: item for item in before_items}
    after = {item['path']: item for item in files}
    changed = []
    for relative in sorted(before.keys() | after.keys()):
        old = before.get(relative)
        new = after.get(relative)
        if old is not None and new is not None and old['sha256'] == new['sha256']:
            continue
        changed.append({
            'path': relative,
            'change': 'added' if old is None else 'removed' if new is None else 'modified',
            'before_sha256': None if old is None else old['sha256'],
            'after_sha256': None if new is None else new['sha256'],
        })
    return changed

base_changed = changes_against(BASE / 'source-manifest.json')
if len(base_changed) != 12:
    raise SystemExit(f'expected 12 changes from Task2 baseline, found {len(base_changed)}')
initial_changed = changes_against(INITIAL / 'source-manifest.json')
expected_fix_paths = {
    'product/src/config_patch/apply.rs',
    'product/src/config_patch/restore.rs',
    'product/src/config_patch/storage.rs',
    'product/tests/patch_contract.rs',
}
if {item['path'] for item in initial_changed} != expected_fix_paths:
    raise SystemExit(f'unexpected lock-fix source paths: {initial_changed}')
for name, payload in [
    ('changed-files.json', {
        'baseline': 'native-task2 quality-fix FINAL_HOLD',
        'baseline_manifest_sha256': digest(BASE / 'source-manifest.json'),
        'current_source_files': len(files),
        'changed_files': base_changed,
    }),
    ('lock-fix-changed-files.json', {
        'baseline': 'native-task3 initial FINAL_HOLD',
        'baseline_manifest_sha256': digest(INITIAL / 'source-manifest.json'),
        'current_source_files': len(files),
        'changed_files': initial_changed,
    }),
]:
    (OUT / name).write_text(json.dumps(payload, indent=2) + '\n')

archive = OUT / 'source-hold.tar.gz'
with archive.open('wb') as raw:
    with gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as zipped:
        with tarfile.open(fileobj=zipped, mode='w') as tar:
            for relative in source_paths:
                path = RESEARCH / relative
                data = path.read_bytes()
                stat = path.stat()
                info = tarfile.TarInfo(relative)
                info.size = len(data)
                info.mode = stat.st_mode & 0o777
                info.mtime = 0
                info.uid = 0
                info.gid = 0
                info.uname = ''
                info.gname = ''
                tar.addfile(info, io.BytesIO(data))
summary = {
    'source_files': len(files),
    'changed_files_from_task2': len(base_changed),
    'changed_files_from_initial_task3': len(initial_changed),
    'source_manifest_sha256': digest(OUT / 'source-manifest.json'),
    'changed_files_manifest_sha256': digest(OUT / 'changed-files.json'),
    'lock_fix_changed_files_manifest_sha256': digest(OUT / 'lock-fix-changed-files.json'),
    'source_archive_sha256': digest(archive),
    'source_archive_bytes': archive.stat().st_size,
}
(OUT / 'capture-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
