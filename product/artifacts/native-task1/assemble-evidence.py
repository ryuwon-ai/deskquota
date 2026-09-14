"""Freeze the Task1 handoff metadata; reads source and preserved artifacts only."""
from pathlib import Path
import datetime,hashlib,json,os,re,subprocess,tarfile,tempfile
ROOT=Path(__file__).resolve().parents[2]; RESEARCH=ROOT.parent; OUT=ROOT/'artifacts/native-task1'
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def load(name):return json.loads((OUT/name).read_text())
source=load('source-manifest.json');assert source['passed']
assert all(sha(RESEARCH/e['path'])==e['sha256'] for e in source['files'])
old=json.loads((RESEARCH/'evidence/product-task8-quality-fixed-source-manifest.json').read_text())
previous={e['path']:e['sha256'] for e in old['files']}
changed=[e['path'] for e in source['files'] if e['path'] in previous and e['sha256']!=previous[e['path']]]
added=[e['path'] for e in source['files'] if e['path'] not in previous]
helpers=[RESEARCH/'scripts'/f'probe-product-{name}.py' for name in ['cli','stream','fairness','retry']]
archive=OUT/'source-hold.tar.gz'
with tarfile.open(archive,'w:gz') as tar:
    for entry in source['files']:tar.add(RESEARCH/entry['path'],arcname=entry['path'])
    for file in helpers:tar.add(file,arcname=str(file.relative_to(RESEARCH)))
expected={'target/release/llmgw':'410645600d70a69e69d9558420ea1cda61bf127a878497a6b8a0e88b17ba342e','target/bench/release/examples/bench_gateway':'38b24b2934448716c74bad2dceaa91492e1bbc168b351e1f61460b90572c5faa','artifacts/pilot.json':'9e6cbbb57bfaac2149eb39efaf43169dbf8e47cf708075cf83b83716615854f6','../evidence/product-task8-quality-fixed-source.tar.gz':'c4bb57f143bc03e1c23fb51fee5bedba4d89ca01106f5f09106ec318a383cb29'}
preserved={p:{'sha256':sha(ROOT/p),'expected':v,'matches':sha(ROOT/p)==v} for p,v in expected.items()};assert all(v['matches'] for v in preserved.values())
pilot=json.loads((ROOT/'artifacts/pilot.json').read_text());assert len(pilot['runs'])==40
pilot_matches=[sha(ROOT/r['path'])==r['sha256'] for r in pilot['runs']];assert all(pilot_matches)
results={}
for name in ['acceptance-debug-tests.log','acceptance-release-tests.log','lifecycle-final-debug.log','lifecycle-final-release.log']:
    text=(OUT/name).read_text();counts=[int(x) for x in re.findall(r'test result: ok\. (\d+) passed',text)]
    results[name]={'passed':sum(counts),'groups':counts,'failed':'test result: FAILED' in text};assert not results[name]['failed']
assert results['acceptance-debug-tests.log']['passed']==210==results['acceptance-release-tests.log']['passed']
processes=[]
for line in subprocess.check_output(['ps','-axo','pid=,ppid=,comm='],text=True).splitlines():
    parts=line.strip().split(None,2)
    if len(parts)==3 and str(ROOT/'target/native') in parts[2]:processes.append({'pid':int(parts[0]),'ppid':int(parts[1]),'executable':parts[2]})
leftover=sorted(str(p) for p in Path(tempfile.gettempdir()).glob('llmgw-native-*'))
assert not processes and not leftover
native_binaries={str(p.relative_to(ROOT)):{'bytes':p.stat().st_size,'sha256':sha(p)} for p in [ROOT/'target/native/debug/llmgw',ROOT/'target/native/release/llmgw',ROOT/'target/native/release/examples/bench_gateway']}
release_sha=native_binaries['target/native/release/llmgw']['sha256'];assert release_sha==source['binary_sha256']
pi=load('pi-final-release.json');negative=load('pi-final-negative.json')
assert pi['passed'] and negative['expected_failure_observed'] and not negative['passed']
assert pi['gateway']['sha256_after']==release_sha==negative['gateway']['sha256_after']
smoke=load('benchmark-smoke.json');assert smoke['status']=='completed_smoke'
assert smoke['identity']['production']['sha256']==release_sha
assert all(sha(ROOT/k)==v for k,v in smoke['identity']['sources'].items())
run_summaries=[]
for r in smoke['runs']:
    run=json.loads((ROOT/r['path']).read_text());assert run['owned_processes_cleaned']
    run_summaries.append({'id':r['id'],'submitted':len(run['submitted']),'outcomes':run['summary']['all']['outcomes'],'attempts':len(run['attempts']),'owned_processes_cleaned':run['owned_processes_cleaned']})
summary={'status':'DONE_WITH_CONCERNS / HOLD for fresh SPEC then QUALITY review','created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'product_source_files':len(source['files']),'source_manifest_sha256':sha(OUT/'source-manifest.json'),'archive':{'path':str(archive.relative_to(ROOT)),'bytes':archive.stat().st_size,'sha256':sha(archive),'files':len(source['files'])+len(helpers)},'changed_product_files':changed,'added_product_files':added,'research_helpers':{str(p.relative_to(RESEARCH)):sha(p) for p in helpers},'binaries':native_binaries,'rust_tests':results,'quality_logs':['fmt-final.log','check.log','clippy-final.log','python-final.log','benchmark-final-selfcheck.log','windows-api-final.log'],'windows':{'full_product_build':'blocked: aws-lc-sys requires missing Windows stdlib.h/windows.h SDK headers','full_build_log':'windows-check-attempt1.log','isolated_api_type_clippy':'PASS x86_64-pc-windows-msvc Rust1.88, no runtime execution','isolated_log':'windows-api-final.log','unsafe_scope':'src/lifecycle/platform/windows.rs only; parent explicitly approved changing Cargo forbid to deny','runtime':'unverified'},'unix_session':load('unix-session.json'),'fixture_probes':{'cli':{'artifact':'cli-final-release.json','upstream_attempts':load('cli-final-release.json')['upstream_attempts']},'pi':{'artifact':'pi-final-release.json','attempts':[c['upstream_attempts'] for c in pi['cases']],'cleanup':[c['gateway_cleanup'] for c in pi['cases']]},'pi_negative':{'artifact':'pi-final-negative.json','expected_failure':True,'attempts':[c['upstream_attempts'] for c in negative['cases']]},'stream':{'artifact':'stream-final-release.json','cases':len(load('stream-final-release.json')['cases']),'all_passed':load('stream-final-release.json')['passed']},'fairness':{'artifact':'fairness-final-release.json','ingress':sum(c['ingress'] for c in load('fairness-final-release.json')['cases']),'attempts':sum(c['upstream_attempts'] for c in load('fairness-final-release.json')['cases'])},'retry':{'artifact':'retry-final-release.json','ingress':sum(c['ingress'] for c in load('retry-final-release.json')['cases']),'attempts':sum(c['upstream_attempts'] for c in load('retry-final-release.json')['cases'])},'benchmark_smoke':{'artifact':'benchmark-smoke.json','runs':run_summaries,'performance_claim':False}},'preserved':preserved,'pilot_run_hashes_matched':sum(pilot_matches),'owned_cleanup':{'remaining_native_processes':processes,'remaining_native_temp_directories':leftover},'limitations':['Actual macOS runtime only; no Windows/Linux runtime or login registration.','Mac ACL uses no-follow path inspection plus opened dev/inode check, not atomic protection against hostile same-user concurrent ACL edits.','Linux protection relies on POSIX ACL mode-mask semantics; non-POSIX filesystem behavior is not verified.','Windows whole-product compile and ACL runtime await a Windows SDK/runtime environment.','setup/default resolution, client adapters, autostart and installers remain outside Task1.','No real provider/model calls, client settings edits, cloud/OS registration, Git operations or long performance matrix.']}
(OUT/'summary.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'status':summary['status'],'source_files':len(source['files']),'changed':len(changed),'added':len(added),'archive':summary['archive'],'cleanup':summary['owned_cleanup']},ensure_ascii=False))
