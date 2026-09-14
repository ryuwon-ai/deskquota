from pathlib import Path
import hashlib,json,subprocess,datetime
root=Path(__file__).resolve().parents[2];out=Path(__file__).resolve().parent
before=json.loads((out/'before.json').read_text())
def check(entries):
 result=[];drift=[]
 for e in entries:
  p=root/e['path'];b=p.read_bytes();a={'path':e['path'],'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()};result.append(a)
  if a!=e:drift.append(e['path'])
 return result,drift
source,sd=check(before['source']);protected,pd=check(before['protected'])
assert not sd and not pd
ps=subprocess.run(['ps','-axo','pid=,comm=,args='],capture_output=True,text=True,check=True)
owned=[]
for line in ps.stdout.splitlines():
 parts=line.strip().split(None,2)
 if len(parts)==3 and parts[1].endswith('/llmgw') and ('native-task2-quality' in parts[2] or 'native-task2-f8-fix' in parts[2]):owned.append({'pid':int(parts[0]),'command':parts[1]})
assert not owned,owned
cleanup=json.loads((out/'cleanup.json').read_text());assert cleanup['scratch_removed'] and cleanup['all_pty_waited']
(out/'after.json').write_text(json.dumps({'source':source,'protected':protected,'source_drift':sd,'protected_drift':pd,'held_or_review_workers_remaining':owned,'captured_at':datetime.datetime.now(datetime.timezone.utc).isoformat()},indent=2)+'\n')
report='''# Native Task 2 fresh quality review

**Verdict: CHANGES REQUIRED — 2 Important findings; 0 Critical; 0 Minor.**

The implemented first-run setup is reviewable and the existing focused tests pass, but unrelated user comments can be erased and a deterministic local CA error can unnecessarily stop a healthy worker. These are Task2 preservation and safe-runtime issues, not requests for future client patching or autostart.

## Review identity and scope

- WHAT_WAS_IMPLEMENTED: typed setup wizard, offline/explicit network diagnostics, protected config persistence, desired-versus-authenticated-worker runtime impact and expected fingerprint guards.
- PLAN_OR_REQUIREMENTS: `docs/superpowers/plans/2026-09-12-native-experience.md` Task2; `docs/superpowers/specs/2026-09-12-native-setup-design.md` Task2 boundaries; existing core/runtime contracts and user requirement for strict existing-data preservation.
- BASE: `product/artifacts/accounting-ablation/quality-fix/source-hold.tar.gz`, SHA256 `e057f32e5eb37c44eaa489fe15ab0f9846fde283261b7f9c690a4eefb4d5c9c4`.
- HEAD: `product/artifacts/native-task2/f8-fix/source-hold.tar.gz`, SHA256 `682f1dde419f2fa35ea4adadf1816b889e9c2b927ce6820282733b10cbec0b89`.
- Actual binary: held macOS ARM64 F8 release, SHA256 `c890b6a498095bdba572d19b921b1865e48927af6807b767156f2e2f3a9ef1ba`, 9,316,432 bytes.
- Full baseline-to-head archive diff is `source.diff`, not merely the F8 delta. No Git repository or Git actions. Product source remained read-only.

## Strengths

- Typed answers, immutable drafts, explicit runtime impact variants and the Dialoguer adapter keep wizard interaction out of request processing. Config validation is reused before writes. The network transport is built once per runtime, with setup/doctor remaining one-shot operations; no new hot-path discovery loop was found in the reviewed diff. This is source-supported, not a performance claim.
- Existing fs4 locking, toml_edit parsing/editing, reqwest TLS/proxy APIs, and macOS exacl are used instead of bespoke substitutes. The narrow Windows replacement boundary retains original/candidate recovery copies on ambiguous failure. Cross-OS correctness is not inferred from host tests.
- Tests cover meaningful independent behavior: snapshot conflicts, preserved worker identity, pending config, permissions/ACLs, auth availability, separate GET/inference calls, proxy loopback bypass, and explicit TLS trust. Fresh focused execution passed all 95 tests.
- Save-only, intent-only client/login selections, and unverified provider capabilities are clearly separated from runtime readiness and real client integration. No additional Task3–5 feature is required by this review.

## Important issues

### Q1 — Endpoint/model edits discard unrelated TOML comments

**Location:** `product/src/setup/mod.rs:410–414` (`SetupDraft::render_config`).

The preserving document editor is used only when both models and roots are unchanged. Adding a single endpoint changes a root, so setup falls through to `serialize_config`, replacing the entire original document. Unrelated heading and port comments disappear with no disclosure or retained original copy. An explicit endpoint edit does not authorize deleting unrelated user annotations.

**Direct reproduction:** run the held binary through a real PTY against the one-root fixture in `probe_quality.py`; keep defaults, add Completions alongside existing Responses, keep models, choose Save only. Exit is 0, both intended endpoints are present, but `# preserved user heading` and `# port comment` are absent. Both attempt1 and the corrected final run reproduced this same result. Before/after TOML and the final terminal transcript are retained.

**Impact:** silent loss of user-maintained configuration context whenever an endpoint changes; model edits reach the same full-serialization branch by source inspection. Existing no-op/scalar comment tests do not exercise this branch.

**Fix boundary:** use the already installed toml_edit document to update the selected root/model fields while retaining unaffected document entries and comments. Do not add a compatibility layer or implement Task3 transactions. Add a regression covering the actual endpoint edit and an adjacent model edit with unrelated comments.

**Evidence:** `probe-results.json` (`endpoint_edit`), `endpoint-before.toml`, `endpoint-after.toml`, `endpoint.pty.txt`.

### Q2 — Local CA validation occurs after the healthy worker is stopped

**Location:** `product/src/lifecycle/mod.rs:228–231` (`restart_inner`); newly fallible CA loading at `product/src/transport/upstream.rs:72–80`, reached by `product/src/server.rs:342` after worker launch.

Restart preflights configuration and credentials, then stops the current worker. The new explicit CA configuration is not read/parsed until the replacement worker constructs its HTTP client. A missing or invalid local bundle can be identified without any upstream request, but currently that error is discovered only after the healthy worker has been drained and stopped. The parent receives generic `worker_start_failed`, so the setup failure also hides the actionable CA cause.

**Direct reproduction:** start the valid temp fixture; run setup in a real PTY, choose Corporate, no proxy, explicit CA with an absolute nonexistent path, retain other defaults, choose Save and start. The preview does disclose restart. Nevertheless the binary saves the CA path, exits 1 with `worker_start_failed`, and authenticated status changes from running to stopped. The old worker is no longer available. A nonexistent bundle is the executed case; malformed/empty PEM follows the adjacent same failure path by source inspection.

**Impact:** deterministic local setup errors cause avoidable service interruption. Explicit restart authorization permits applying a viable new runtime; it does not make skipping cheap local preflight desirable. This is not a demand for an upstream availability test, seamless restart, rollback daemon, or guaranteed recovery from arbitrary runtime races.

**Fix boundary:** validate the configured local trust/proxy client construction before the destructive stop, retaining verification and no-network defaults. Return the sanitized actionable cause while preserving the authenticated old worker when local preflight fails. Test missing and malformed CA with the actual executable because integration-test `current_exe` cannot prove replacement worker spawn.

**Evidence:** `probe-results.json` (`missing_ca_restart`), `missing-ca.pty.txt`; the fixture cleans up through authenticated `off` and confirms stopped state.

## Verification and limitations

**Fresh direct execution:**

- Explicit isolated target: `CARGO_TARGET_DIR=/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product/target/native-task2-quality cargo test --locked --offline --lib --test setup_contract --test config_contract --test lifecycle_contract` — exit 0; lib11 + config19 + lifecycle25 + setup40 = **95 passed, 0 failed**. Full output is `focused-tests.log`, count summary `focused-tests.json`.
- Held release real-PTY probes: two endpoint-edit observations, one final missing-CA restart observation. The first CA attempt was a driver error: CR after an immediate Confirm answer caused subsequent input to reach the wrong prompt. It is retained as a fixture correction and is not counted as product evidence. All four PTY processes were waited; both attempt cleanups succeeded.
- All 70 live source files matched the frozen head archive. Before/after **70 source and 317 protected files show zero drift**. Archive/binary identities were separately checked. Owned worker inspection is empty; scratch was removed only after authenticated stops and PTY waits. No current-user client/config/auth state, startup registration, paid API, external LLM, download, Git action, or held binary mutation occurred.

**Reused evidence, not rerun here:** SPEC F8 review's 76 tests and 10 PTY sessions, prior F1–F7 fixes, implementer's all-targets261/fmt/check/clippy/release records, and host Windows replacement failure-model tests. They support context but do not override the two fresh counterexamples. No full benchmark/accounting matrix was repeated.

**Source-supported/inferred:** setup modules have coherent UI/draft/diagnostic/persistence responsibilities; the changed admission/test call sites only thread added transport fields. Other model edits can reach Q1; malformed/empty CA can reach Q2. These adjacent forms are not reported as independently executed.

**Unknown/unverified:** actual Windows/Linux runtime and ACL behavior, real provider inference/tool compatibility, native packaging/login acceptance, fresh RSS/latency and any superiority claim. Existing file length alone was not treated as a defect; no hypothetical future abstraction or backward-compatibility work is requested.

## Assessment

**Ready for Task2 acceptance: No, with the two fixes above.** The tests and architecture provide a sound base, but the fresh release counterexamples are concrete user-data and runtime regressions outside existing test coverage. The original implementer should make narrow fixes and return a new held source/binary set for re-review. Do not start Task3 as part of these fixes.
'''
(out/'README.md').write_text(report)
readme_sha=hashlib.sha256((out/'README.md').read_bytes()).hexdigest()
final={'status':'FINAL_HOLD','execution_finished':True,'result':'CHANGES_REQUIRED','critical':0,'important':2,'minor':0,'findings':['Q1','Q2'],'fresh_tests':95,'source_files':len(source),'protected_files':len(protected),'source_drift':sd,'protected_drift':pd,'cleanup_complete':True,'runtime_test_ownership_released':True,'readme_sha256':readme_sha}
(out/'final-hold.json').write_text(json.dumps(final,indent=2)+'\n');(out/'FINAL_HOLD').write_text('execution_finished=true\nruntime_test_ownership_released=true\nresult=CHANGES_REQUIRED\n')
print(json.dumps(final,indent=2))
