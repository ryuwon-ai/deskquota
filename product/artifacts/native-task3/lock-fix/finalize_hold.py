#!/usr/bin/env python3
import datetime
import hashlib
import json
import pathlib
import platform
import re
import subprocess
import tarfile
import tempfile

OUT = pathlib.Path(__file__).resolve().parent
PRODUCT = OUT.parents[2]
RESEARCH = PRODUCT.parent
INITIAL = PRODUCT / 'artifacts/native-task3'
PROTECTED_BASE = RESEARCH / 'evidence/native-task2-quality-review/rereview/after.json'
BENCHMARK_RAW_BASE = RESEARCH / 'evidence/native-task1-quality-review/pilot-preservation.json'
ACCOUNTING_RAW_BASE = RESEARCH / 'evidence/accounting-task2-quality-review/hashes-after.json'

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def identity(path, shown=None):
    return {'path': shown or str(path.relative_to(PRODUCT)), 'bytes': path.stat().st_size, 'sha256': digest(path)}

def require_identity(path, expected_sha, expected_bytes=None):
    if not path.is_file() or digest(path) != expected_sha or (expected_bytes is not None and path.stat().st_size != expected_bytes):
        raise SystemExit(f'protected identity changed: {path}')

capture = json.loads((OUT / 'capture-summary.json').read_text())
manifest = json.loads((OUT / 'source-manifest.json').read_text())
source_drift = []
for item in manifest['files']:
    path = RESEARCH / item['path']
    if not path.is_file():
        source_drift.append({'path': item['path'], 'reason': 'missing'})
    elif path.stat().st_size != item['bytes'] or digest(path) != item['sha256']:
        source_drift.append({'path': item['path'], 'reason': 'content changed'})
if source_drift:
    raise SystemExit(f'source drift after capture: {source_drift}')
with tarfile.open(OUT / 'source-hold.tar.gz', 'r:gz') as archive:
    members = archive.getmembers()
    if [member.name for member in members] != [item['path'] for item in manifest['files']]:
        raise SystemExit('source archive member list differs from manifest')
    for member, item in zip(members, manifest['files']):
        archived = archive.extractfile(member).read()
        if len(archived) != item['bytes'] or hashlib.sha256(archived).hexdigest() != item['sha256']:
            raise SystemExit(f'source archive mismatch: {member.name}')

initial_expected = {
    INITIAL / 'source-manifest.json': ('6cdfbec0b41839689364345e4b973a2f6af9409234fc5e2189f77f4144873e9a', None),
    INITIAL / 'source-hold.tar.gz': ('17c82d6aec084de464f4ae4b1eca850772c46f195d55997dcd534b0b15b4451d', 234307),
    INITIAL / 'final-hold.json': ('601a465a47c60418ef756edccaabc484e5afce9c1c8323e41fea754e03a22973', None),
    PRODUCT / 'target/native-task3/debug/llmgw': ('66142c7dd7cd21a500b4075e77bf840283c4741ef178b275a09a5f19d3ba504c', 28254424),
    PRODUCT / 'target/native-task3/release/llmgw': ('215511357446c5583ebd5d4a3d9aee6b8ea5df4d83387d31d0ce23ae2a97eb6d', 9314736),
}
for path, (sha, size) in initial_expected.items():
    require_identity(path, sha, size)

protected = json.loads(PROTECTED_BASE.read_text())['protected']
protected_drift = []
for item in protected:
    path = RESEARCH / item['path']
    if not path.is_file():
        protected_drift.append({'path': item['path'], 'reason': 'missing'})
    elif path.stat().st_size != item['bytes'] or digest(path) != item['sha256']:
        protected_drift.append({'path': item['path'], 'reason': 'content changed'})
if protected_drift:
    raise SystemExit(f'protected artifact drift: {protected_drift}')
benchmark_raws = sorted((PRODUCT / 'artifacts/pilot-runs').glob('*.json'))
accounting_raws = sorted((PRODUCT / 'artifacts/accounting-ablation/pilot-runs').glob('*.json'))
if len(benchmark_raws) != 40 or len(accounting_raws) != 10:
    raise SystemExit(f'raw fixture count changed: benchmark={len(benchmark_raws)}, accounting={len(accounting_raws)}')
benchmark_hashes = {item['path']: item['sha256'] for item in json.loads(BENCHMARK_RAW_BASE.read_text())}
accounting_hashes = json.loads(ACCOUNTING_RAW_BASE.read_text())
for path, hashes in [
    *((path, benchmark_hashes) for path in benchmark_raws),
    *((path, accounting_hashes) for path in accounting_raws),
]:
    relative = str(path.relative_to(RESEARCH))
    if relative not in hashes or digest(path) != hashes[relative]:
        raise SystemExit(f'raw fixture drift: {relative}')

protected_result = {
    'baseline': str(PROTECTED_BASE.relative_to(RESEARCH)),
    'checked': len(protected),
    'drift': protected_drift,
    'benchmark_raws': len(benchmark_raws),
    'accounting_raws': len(accounting_raws),
    'initial_task3': [identity(path) for path in initial_expected],
    'superseded_pre_acl_current_identities': [
        identity(path) for path in sorted((INITIAL / 'superseded/pre-acl').iterdir()) if path.is_file()
    ],
}
(OUT / 'protected-check.json').write_text(json.dumps(protected_result, indent=2) + '\n')

def result_rows(name):
    text = (OUT / name).read_text()
    return [int(match.group(1)) for match in re.finditer(r'test result: ok\. (\d+) passed; 0 failed;', text)]

full_rows = result_rows('cargo-test-full-final.log')
if len(full_rows) != 15 or sum(full_rows) != 289:
    raise SystemExit(f'unexpected full test totals: {full_rows}')
if result_rows('patch-contract-final.log') != [25] or result_rows('patch-contract-release-final.log') != [25]:
    raise SystemExit('unexpected focused test totals')
for name in (
    'cargo-check-all-targets-final.log',
    'cargo-clippy-all-targets-final.log',
    'cargo-build-debug-final.log',
    'cargo-build-release-final.log',
):
    if 'Finished' not in (OUT / name).read_text():
        raise SystemExit(f'incomplete command log: {name}')
if (OUT / 'cargo-fmt-final.log').read_text():
    raise SystemExit('final rustfmt check produced output')

sentinels = (
    ''.join(('upstream-', 'synthetic-', '8f2f5a47')),
    ''.join(('local-data-', 'synthetic-', '3dcb99a1')),
    ''.join(('control-', 'synthetic-', '6a01c442')),
)
sentinel_hits = []
scan_paths = [RESEARCH / item['path'] for item in manifest['files']]
scan_paths.extend(OUT.glob('*.log'))
for path in scan_paths:
    text = path.read_text(errors='replace')
    for sentinel in sentinels:
        if sentinel in text:
            sentinel_hits.append({'path': str(path.relative_to(RESEARCH)), 'sentinel_kind': sentinel.split('-', 1)[0]})
if sentinel_hits:
    raise SystemExit(f'synthetic credential leaked into source/logs: {sentinel_hits}')

process_text = subprocess.run(['ps', '-axo', 'pid=,ppid=,command='], capture_output=True, text=True, check=True).stdout
owned_processes = [
    line.strip() for line in process_text.splitlines()
    if 'target/native-task3-lock-fix/' in line
    and any(name in line for name in ('/llmgw', 'patch_contract-', '/cargo', '/rustc'))
]
temp_root = pathlib.Path(tempfile.gettempdir())
temp_paths = sorted({
    str(path) for pattern in ('llmgw-patch-*', 'llmgw-windows-*', 'llmgw-config-patch-locks-*')
    for path in temp_root.glob(pattern)
})
cleanup = {
    'captured_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'owned_processes': owned_processes,
    'matching_temp_paths': temp_paths,
    'execution_finished': not owned_processes and not temp_paths,
}
(OUT / 'cleanup-final.json').write_text(json.dumps(cleanup, indent=2) + '\n')
(OUT / 'owned-processes-final.txt').write_text('none\n' if not owned_processes else '\n'.join(owned_processes) + '\n')
(OUT / 'owned-temp-paths-final.txt').write_text('none\n' if not temp_paths else '\n'.join(temp_paths) + '\n')
if not cleanup['execution_finished']:
    raise SystemExit(f'owned cleanup incomplete: {cleanup}')

release = PRODUCT / 'target/native-task3-lock-fix/release/llmgw'
debug = PRODUCT / 'target/native-task3-lock-fix/debug/llmgw'
rustc = subprocess.run(['rustc', '--version', '--verbose'], capture_output=True, text=True, check=True).stdout.strip()
cargo = subprocess.run(['cargo', '--version', '--verbose'], capture_output=True, text=True, check=True).stdout.strip()
final = {
    'phase': 'FINAL_HOLD',
    'fix': 'native-task3 cooperative resource lock identity',
    'execution_finished': True,
    'writer_runtime_ownership_released': True,
    'source_stable_after_capture': True,
    'source_drift': source_drift,
    **capture,
    'binaries': {'release': identity(release), 'debug': identity(debug)},
    'lockfile': identity(PRODUCT / 'Cargo.lock'),
    'toolchain_file': identity(PRODUCT / 'rust-toolchain.toml'),
    'verification': {
        'full_debug': {'passed': 289, 'failed': 0, 'rows': full_rows, 'log': 'artifacts/native-task3/lock-fix/cargo-test-full-final.log'},
        'patch_debug': {'passed': 25, 'failed': 0},
        'patch_release': {'passed': 25, 'failed': 0},
        'static': {'rustfmt': 'passed', 'check_all_targets': 'passed', 'clippy_all_targets_deny_warnings': 'passed', 'debug_build': 'passed', 'release_build': 'passed'},
    },
    'red_green': {
        'red': 'artifacts/native-task3/lock-fix/patch-contract-lock-red.log (18 passed, 5 failed)',
        'failed_intermediate': 'artifacts/native-task3/lock-fix/patch-contract-lock-green-attempt-1.log (21 passed, 2 failed)',
        'green': 'artifacts/native-task3/lock-fix/patch-contract-final.log (25 passed, 0 failed)',
        'release_green': 'artifacts/native-task3/lock-fix/patch-contract-release-final.log (25 passed, 0 failed)',
    },
    'directly_verified': [
        'dot-dot aliases for one existing client file resolve to one adjacent lock and a held lock blocks alias apply',
        'lock identity is identical across child processes with different TMPDIR, TMP, and TEMP values',
        'apply and restore use the same normalized resource lock identity',
        'repeated connect through an equivalent path spelling retains one journal resource and the first preownership value',
        'case spelling aliases lock together when this host filesystem resolves them to the same directory entry',
        'unresolved aliases through a missing parent are refused before preview or write',
        'existing ordinary parent directory mode is preserved',
    ],
    'platform_evidence': {
        'macos_runtime': 'full and focused suites, including actual case-insensitive alias, mode, ACL, partial, crash, and restore tests',
        'windows': 'static cfg parse only for the new normalization and UTF-16 lock digest path; no Windows runtime claim',
        'linux': 'static cfg parse only; no Linux runtime claim',
    },
    'protected': {
        'baseline': str(PROTECTED_BASE.relative_to(RESEARCH)),
        'checked': len(protected),
        'drift': protected_drift,
        'benchmark_raws': len(benchmark_raws),
        'accounting_raws': len(accounting_raws),
        'initial_task3_source_manifest_sha256': digest(INITIAL / 'source-manifest.json'),
        'initial_task3_source_archive_sha256': digest(INITIAL / 'source-hold.tar.gz'),
        'initial_task3_final_hold_sha256': digest(INITIAL / 'final-hold.json'),
        'initial_task3_debug': identity(PRODUCT / 'target/native-task3/debug/llmgw'),
        'initial_task3_release': identity(PRODUCT / 'target/native-task3/release/llmgw'),
        'task2_release': identity(PRODUCT / 'target/native-task2-quality-fix/release/llmgw'),
        'task2_debug': identity(PRODUCT / 'target/native-task2-quality-fix/debug/llmgw'),
        'accounting_held': identity(PRODUCT / 'artifacts/accounting-ablation/task2/held-llmgw'),
        'native_held': identity(PRODUCT / 'target/native/release/llmgw'),
    },
    'host': platform.platform(),
    'rustc': rustc,
    'cargo': cargo,
    'cleanup': 'artifacts/native-task3/lock-fix/cleanup-final.json',
    'limits': [
        'the initial Task3 FINAL_HOLD remains immutable evidence of the reproduced lock defect and is superseded only for the cooperative lock contract',
        'the earlier overwritten pre-redaction capture remains unavailable and was not reconstructed',
        'external editors may still race the final metadata/hash check and atomic rename; no filesystem CAS is claimed',
        'Task4 adapters and CLI wiring remain out of scope',
    ],
}
(OUT / 'final-hold.json').write_text(json.dumps(final, indent=2) + '\n')
final_sha = digest(OUT / 'final-hold.json')
(OUT / 'FINAL_HOLD').write_text(f'FINAL_HOLD\nexecution_finished=true\nwriter_runtime_ownership_released=true\nfinal_hold_sha256={final_sha}\n')
print(json.dumps({'final_hold_sha256': final_sha, 'release': final['binaries']['release'], 'debug': final['binaries']['debug'], 'cleanup': cleanup}, indent=2))
