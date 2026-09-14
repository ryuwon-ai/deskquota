import pathlib,json,hashlib,tarfile,datetime,sys
O=pathlib.Path(__file__).resolve().parent;R=O.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();phase=sys.argv[1]
source=json.loads((R/'product/artifacts/native-task3/nonfinite-fix/source-manifest.json').read_text())['files'];protected=json.loads((R/'evidence/native-task2-quality-review/rereview/after.json').read_text())['protected'];extra=json.loads((R/'evidence/native-task3-quality-review/rereview-q3/after.json').read_text())['extra_protected']
def check(xs):return [x['path'] for x in xs if not (R/x['path']).is_file() or sha((R/x['path']).read_bytes())!=x['sha256']]
a=R/'product/artifacts/native-task3/nonfinite-fix/source-hold.tar.gz'
with tarfile.open(a) as t:members={m.name:t.extractfile(m).read() for m in t.getmembers() if m.isfile()}
ids={
'product/artifacts/native-task3/nonfinite-fix/final-hold.json':'4bec8201c2e0b02c08815a61a907d8dbcbe5a2f9140acf33fc296ecec83aa313',
'product/artifacts/native-task3/nonfinite-fix/source-manifest.json':'68535b446b76c5f5f5fdf726f86232eb831b250340f3f5c5a9ccc58ac7e4c391',
'product/target/native-task3-nonfinite-fix/debug/deps/libllmgw-eba4665eb2ddaf45.rlib':'7adb91e195f05aa349401d7ee1d36b3a984af664e0fb012ee5245880f01616f2',
'evidence/native-task3-quality-review/rereview-q3/README.md':'8e6bafe074a25aeed5e0dc183fdc6c80a0cfc6735f295f30815ea0f19650fcb5',
'evidence/native-task3-quality-review/rereview-q3/FINAL_HOLD.json':'cd986ecd22743dc9e70fdc183887d32331e5a7925e08075696a2053719528548'}
out={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_count':len(source),'source_drift':check(source),'archive_members':len(members),'archive_drift':[x['path'] for x in source if sha(members.get(x['path'],b''))!=x['sha256']],'archive_sha256':sha(a.read_bytes()),'protected_count':len(protected),'protected_drift':check(protected),'extra_protected_count':len(extra),'extra_protected_drift':check(extra),'identity_drift':[p for p,h in ids.items() if sha((R/p).read_bytes())!=h]}
assert out['archive_sha256']=='7fceb9632164aa4709791b98ecd114a0cf9ce624fb38ef784c6ca7354d98146a';(O/(phase+'.json')).write_text(json.dumps(out,indent=2)+'\n');print(out)
