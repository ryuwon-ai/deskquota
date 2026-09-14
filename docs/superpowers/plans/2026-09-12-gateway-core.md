# Local Quota Gateway Core Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Native Rust 프로세스에서 실제 HTTP/SSE 전달을 완성하고 quota·공정 큐·429·취소를 같은 경로에 추가해 mock으로 효과와 손해를 측정한다.

**Architecture:** 한 binary, 한 upstream, 한 quota group. request/stream 수명은 transport, queue/ledger는 admission이 소유한다. CLI 설정 기능은 별도 [실행·연동 계획](2026-09-12-native-experience.md)에 둔다.

**Tech Stack:** Rust 1.88 시작, Tokio, Axum 0.8.9, Reqwest 0.13.5(rustls platform verifier), Serde/serde_json, TOML, Clap 4.6.6. lockfile은 실제 resolve로 생성하며 호환성은 빌드로 확인한다. 테스트는 Rust와 Python stdlib fixture, 설치된 Pi를 사용한다.

---

상태: **Core Task 1~8 수용 완료**. Task 8의 합성 측정·부모 검산·독립 명세·품질 재검토를 통과했다. 측정에서 고정 시간 내 처리량 우위는 입증하지 못했으며, 실제 모델·경쟁 제품·저사양 검증은 별도 미완료다. 다음은 Native Task 1 실행 기능 구현이다. 실제 명령·검증·미완료 경계는 [구현 기록](../../../evidence/implementation-progress.json)에 둔다. [코어 명세](../specs/2026-09-12-local-quota-gateway-design.md)가 규범이며 [벤치마크](../../../reports/benchmark-spec.md)에 따라 주장 범위를 제한한다. 아직 존재하지 않는 테스트 이름은 아래 task가 만들 이름이며 통과 기록이 아니다. 제품 코드는 research의 `product/`에 격리한다. 이 폴더도 아직 Git repo가 아니므로 worktree/commit을 흉내 내지 않는다. Git 초기화·commit/push/merge/rebase/release는 요청된 결재안을 준비한 뒤 승인된 범위에서만 수행한다.

모든 아래 명령의 cwd는 `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product`다. fixture는 loopback ephemeral port, temp config/state, synthetic body만 사용한다. 실제 `.pi/.claude/.codex`와 모델 서버는 건드리지 않는다. `@test-driven-development`와 `@verification-before-completion`을 사용한다. 기능 task는 실패하는 행동 테스트→최소 구현→같은 테스트 순서이며 compile 오류만 확인하고 끝내지 않는다.

## File map

| 파일 | 책임 |
|---|---|
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.gitignore` | dependency/features/MSRV와 artifact 경계 |
| `src/lib.rs` | config/transport/admission의 공통 구현을 연결. integration tests와 binary가 동일 모듈 사용 |
| `src/main.rs`, `src/cli.rs` | CLI dispatch, exit code. 서버 구현을 넣지 않음 |
| `src/config.rs`, `src/config/validate.rs` | typed config, 경계 검증 |
| `src/server.rs`, `src/control.rs` | loopback listen, lifecycle control API |
| `src/transport/{mod,body_budget,headers,upstream,stream}.rs` | 각각 수신 예산·HTTP headers·pool·stream 수명 |
| `src/protocol/{mod,completions,responses,messages,models}.rs` | endpoint별 read-only 관찰과 오류 envelope |
| `src/admission/{mod,queue,quota,retry}.rs` | 이벤트 소유, RR/aging, quota, retry 분류 |
| `src/metrics.rs` | bounded counter/histogram; payload 없음 |
| `tests/{config_contract,wire_contract,stream_lifetime,quota_contract,fairness_contract,retry_contract,resource_bounds}.rs` | 기능별 관찰 가능한 불변식 |
| `tests/support/{mod,fixture}.rs` | 실제 socket mock, 제어 가능한 completion gate |
| `scripts/{probe_pi,benchmark}.py`, `fixtures/workloads/*.json` | 실제 agent 통과, 반복 비교와 모든 outcome |
| `docs/{runtime-contract,benchmark-method}.md` | 실제 지원 상태와 재현 명령 |

## Chunk 1: 처음부터 끝까지 통과하는 native 경로

### Task 1: Build boundary와 typed config

**Create:** manifest/toolchain/gitignore, `src/lib.rs`, `src/main.rs`, `src/cli.rs`, `src/config.rs`, `src/config/validate.rs`, `tests/config_contract.rs`, `examples/fixture.toml`, `docs/runtime-contract.md`.

- [x] `rustc --version`, `cargo --version`와 사전 조사한 crate manifest를 확인한다. Tokio는 `net,rt-multi-thread,macros,time,sync,signal,io-util`만, Axum은 필요한 HTTP feature만, Reqwest는 `default-features=false`, `rustls,stream,http2`로 시작한다. serde_json `raw_value` 사용 여부를 문서/타입으로 확인한다. UI/editor/compression/WebSocket 의존성을 추가하지 않는다.
- [x] 실행 가능한 테스트 뼈대에서 invalid config를 거절하지 않는 최소 stub을 만든다. `config_contract`에 loopback-only, unknown/unlimited/known 구별, zero limit 거절, root 중복·알 수 없는 endpoint 거절, TPM known이면서 모델 default 출력 상한이 없는 config도 허용하는 사례를 추가한다. 요청의 명시적 상한 유무는 Task 5의 request 검증에서 다룬다. `cargo test --test config_contract` → 행동 assertion FAIL 확인.
- [x] `src/lib.rs`에서 public 내부 모듈을 연결하고 main/tests는 같은 library를 사용한다. 파일을 tests에서 중복 include하지 않는다. typed config를 구현한다. `Limit::Known(NonZeroU64)|Unknown|Unlimited`, `Auth::Forward|Env{header,name}|None`, `Accounting::Reserved|Actual`, canonical config path와 worker fingerprint를 사용한다. env 원문은 Debug 출력 대상에서 제외한다. `examples/fixture.toml`은 credential 없는 loopback mock만 가리킨다.
- [x] `cargo test --test config_contract` → PASS, `cargo check --locked` → 성공. transitive MSRV 불일치가 나면 오류와 최소 필요한 toolchain을 기록하고 프로젝트 toolchain만 조정한다. 전역 default Rust를 바꾸지 않는다. 사용하지 않는 dependency는 제거한다.

### Task 2: 실제 socket fixture와 원문 HTTP 전달

**Create:** `src/server.rs`, `src/control.rs`, `src/transport/{mod,body_budget,headers,upstream}.rs`, `src/protocol/mod.rs`, `src/protocol/models.rs`, `src/metrics.rs`, `tests/wire_contract.rs`, `tests/support/{mod,fixture}.rs`.

- [x] fixture를 `127.0.0.1:0`에 bind하고 요청 bytes/header/path·attempt 수만 synthetic 기록한다. `wire_contract`에 `prefix_is_preserved`, `body_is_unchanged`, `no_cross_host_redirect`, `env_auth_removes_both_old_credentials`, `data_token_is_stripped`, `control_rejects_data_token`, `control_rejects_origin`, `forward_auth_is_unchanged`, `query_conflict_is_rejected`, `unknown_route_never_reaches_upstream`를 작성한다.
- [x] `cargo test --test wire_contract` → 실제 HTTP assertion FAIL 확인. in-process Router 호출로 대체하지 않는다.
- [x] Config와 explicit argv로 `llmgw run --config ...`를 시작하는 경로, local data/control token 검증, protocol endpoint allowlist와 URI join, header scrub, pool 재사용을 구현한다. HTTP retry/redirect는 아래처럼 명시적으로 끈다.

```rust
let http = reqwest::Client::builder()
    .retry(reqwest::retry::never())
    .redirect(reqwest::redirect::Policy::none())
    .no_proxy() // upstream explicit proxy는 별도 설정 분기, control은 항상 bypass
    .connect_timeout(std::time::Duration::from_secs(10))
    .pool_max_idle_per_host(16)
    .build()?;
```

- [x] `cargo test --test wire_contract` → PASS. 8MiB 단일 body, 전체 32MiB 예산, header·body timeout은 수신 경로에서 적용한다. HTTP 압축/body/header 일관성도 fixture에서 확인한다. `cargo run --locked -- --help`가 실제 구현된 명령만 표시하는지 확인한다.

### Task 3: SSE 수명·취소·backpressure

**Create:** `src/transport/stream.rs`, protocol 세 파일, `tests/stream_lifetime.rs`. **Modify:** server/transport/control.

- [x] socket fixture에 first chunk gate와 final EOF gate를 추가한다. `first_event_arrives_before_eof`, `split_utf8_and_sse_bytes_preserved`, `disconnect_drains_without_releasing_slot`, `close_policy_terminates_socket`, `huge_event_marks_usage_unknown`, `terminal_cleanup_happens_once` 테스트를 작성한다.
- [x] `cargo test --test stream_lifetime` → FAIL 확인. downstream reader를 실제로 닫고 EOF gate 전 slot 점유를 assert한다. iterator만 close하는 테스트로 대체하지 않는다.
- [x] upstream worker가 slot과 terminal event를 소유하게 한다. bounded channel로 downstream에 보내고 consumer drop 시 drain mode에서 계속 읽되 저장하지 않는다. cancel/timeout/EOF 경합 cleanup을 한 번만 수행한다. SSE observer는 capped buffer만 쓰고 넘치면 usage unknown, wire bytes는 보존한다.
- [x] `cargo test --test stream_lifetime` → PASS. completion marker와 TCP EOF, 첫 body byte와 첫 model token을 별도로 기록한다. control stop은 신규 입장 중단→10초 drain→worker 종료까지 검증한다. 서버 계산 중단 확인은 fixture HTTP 수명과 구분한다.

### Task 4: 설치된 Pi의 첫 E2E

**Create:** `scripts/probe_pi.py`, `tests/fixtures/pi-models.json`(synthetic template). **Modify:** `docs/runtime-contract.md`.

- [x] 기존 연구 `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/scripts/probe-pi-client.py`를 읽고, temp `PI_CODING_AGENT_DIR`·no extensions·timeout·process cleanup 방식을 재사용할 부분만 추출한다. 제품 script 경로에서 research 도구를 production dependency로 import하지 않는다.
- [x] `probe_pi.py`가 fixture→gateway→설치된 Pi의 tool-free completion과 1회 tool round-trip을 수행하도록 작성한다. tool은 임시 directory 안의 synthetic fixture 파일 read만 허용하며 셸 실행·실제 사용자 파일 접근을 금지한다. fixture 요청이 gateway route/custom header를 통과했는지, synthetic 출력·tool result의 다음 turn을 assertion한다. gateway를 끈 negative case로 FAIL을 확인한다.
- [x] wire/observer 동작만 필요한 만큼 수정한다. `python3 scripts/probe_pi.py --binary target/debug/llmgw --output artifacts/pi-e2e.json` → 두 흐름 PASS와 실제 attempt 수. raw payload와 credential을 artifact에 저장하지 않는다.
- [x] 설치된 버전, clone과의 차이, 실행한 protocol을 runtime contract에 기록한다. 이 단계 결과는 실제 LLM task 품질이 아니라 agent/mock 통과다.

## Chunk 2: quota·공정성·효율을 같은 경로에서 검증

### Task 5: 정확한 로컬 quota 회계와 추정 비용 분리

**Create:** `src/admission/{mod,quota}.rs`, `tests/quota_contract.rs`. **Modify:** config/protocol/server.

- [x] ledger 메서드에 명시적 monotonic timestamp를 전달하는 동일 library API를 사용한다. 가상 monotonic clock으로 RPM window, TPM reservation/settlement/debt/unknown, restart hold, long stream, metadata 비용을 테스트한다. known TPM 모드에서도 models/count_tokens가 출력 상한 오류 없이 처리되는 실제 wire case를 추가한다. fixture cost는 테스트 API로만 주입하고 HTTP header로 받지 않는다.

```rust
// 테스트가 관찰해야 할 관계. 실제 테스트 harness 타입에 맞춰 구현한다.
let permit = ledger.try_admit(now, Cost { requests: 1, tokens: 8 }).unwrap();
assert!(ledger.try_admit(now, Cost { requests: 1, tokens: 3 }).is_none());
ledger.complete(permit, now, Usage::Unknown);
assert_eq!(ledger.available_tokens(now), 2); // unknown을 0으로 환급하지 않음
```

- [x] `cargo test --test quota_contract` → FAIL 확인. 실제 `sleep(60)` 대신 clock advance를 사용한다.
- [x] 단일 owner ledger를 구현한다. 고정 60초 rolling window와 reserved/actual 계약을 적용하고 request 관찰로 만든 추정 Cost와 exact fixture Cost를 지표에서 분리한다. output bound 누락·unsupported multimodal·estimate exceeds budget은 설명 가능한 error로 반환한다. models/count_tokens는 RPM=1·generation TPM=0이며 생성과 같은 queue/cap을 사용한다. request 자체에 출력 상한이 있으면 default가 없어도 허용, 양쪽 모두 없고 known TPM이면 요청을 거절하는 사례를 검증한다. admitted 상태의 자원은 잠정 hold이며 실제 HTTP attempt 시작에서 RPM을 확정 차감한다. admission 직후 upstream 시작 전 취소를 gate로 재현해 attempt=0, hold cleanup=1을 assert한다.
- [x] `cargo test --test quota_contract` → PASS. 모든 terminal/cancel/late usage에서 double refund 없음을 검증한다. 서버별 rate policy를 자동 추론하는 abstraction은 넣지 않는다.

### Task 6: RR와 bounded bypass

**Create:** `src/admission/queue.rs`, `tests/fairness_contract.rs`. **Modify:** admission/server/control metrics.

- [x] root FIFO, RR, heavy→light→heavy와 heavy→heavy→light, 작은 요청 연속 중 큰 head, root 1개+child15개, queued cancel, 불가능한 head, 64개 overflow trace를 작성한다. `cargo test --test fairness_contract` → FAIL 확인.
- [x] 유효한 head만 scan하고 admission 확정 시 quota+slot을 함께 잠정 확보하고 RPM은 upstream 시작 시 확정 차감한다. bypass 카운트는 실제 다른 admission에서만 증가, 8회/5초 뒤 oldest valid capacity-fit head barrier, 만료/취소 때 제거한다. timer와 terminal event 외 polling 없음.
- [x] `cargo test --test fairness_contract` → PASS. 가상 시간에서 deadline 이내 가능한 heavy가 실제 완료하는지, 큐 cancellation이 upstream 0회인지 확인한다. cap1의 이미 실행 중인 long task는 선점 못하는 기대값을 유지한다.
- [x] 관리 API `/_llmgw/status`의 JSON 자료에 queue length, active, blocked reason, estimate mode를 추가한다. 이 자료를 읽는 `status --json` CLI는 네이티브 경험 Task 1에서 구현한다. root IDs는 설정 상한 내에서만 유지하고 arbitrary header로 새 root를 만들지 않는다.

### Task 7: retry 소유권과 자원 상한

**Create:** `src/admission/retry.rs`, `tests/retry_contract.rs`, `tests/resource_bounds.rs`. **Modify:** stream/protocol/metrics.

- [x] 429 seconds/date/없는 header·spend cap·분류 불가, 503, POST socket ambiguity, partial stream, cooldown 중 새 client 요청 fixture를 추가한다. resource suite에는 64개 partial body가 추가 메모리를 동시에 요구하는 사례와 느린 downstream을 넣는다. memory rejection 직후 다른 완독 가능한 요청이 진행하는지 assert한다. retry 대기 중 slot=free, 다음 attempt의 RPM 추가 차감, 원래 deadline/aging 시작 시각 유지도 각 gate에서 assert한다.
- [x] `cargo test --test retry_contract --test resource_bounds` → FAIL 확인. Reqwest layer의 실제 attempt 수까지 fixture counter로 센다.
- [x] retry off 기본, opt-in transient429 1회만, 모든 retry admission 재진입, cooldown은 request deadline보다 길어도 group에서 유지한다. memory permit 추가 확보 실패 시 즉시 부분 body 해제+429, 기다리지 않는다. cap 초과·timeout response는 protocol envelope로 반환한다.
- [x] `cargo test --test retry_contract --test resource_bounds` → PASS. library 자동 retry/redirect 0회, queue/stream/connection 한도와 observer overflow의 전달 유지 확인. local API token·upstream credential·prompt가 로그에 없는지 synthetic sentinel 검사한다.

### Task 8: 성능 비교와 채택 기록

**Create:** `scripts/benchmark.py`, `examples/bench_gateway.rs`, `tests/benchmark_trace.rs`, `fixtures/workloads/{burst,mixed_lengths,shared_quota,cancellation}.json`, `docs/benchmark-method.md`. **Modify:** runtime-contract/metrics.

- [ ] `bench-harness` feature와 required-feature가 지정된 `examples/bench_gateway.rs`를 만든다. 공통 library transport의 FIFO/RR을 example argv로 선택하게 하고 제품 CLI/config에는 노출하지 않는다. HTTP arm은 항상 실제 body 기반 추정 비용이다. exact-cost 비교는 `tests/benchmark_trace.rs`의 가상 clock trace로만 실행하고 HTTP latency와 합치지 않는다. deficit은 이 pilot 뒤 필요성이 입증되면 별도 실험으로 추가한다. direct/production RR/benchmark FIFO/benchmark RR, 5 seed, 5 quota window, failures/pending 포함 aggregation을 구현한다.
- [ ] 분모 유실·attempt 중복을 검출하는 fixture로 harness의 실패를 확인한다. `python3 scripts/benchmark.py --self-check` → 의도적 malformed run 거절 후 PASS. 실험 종료 시 pending은 drain 또는 timeout으로 반드시 기록한다.
- [ ] `cargo build --release --locked`로 production을 빌드하고 별도 target directory에 `cargo build --release --locked --features bench-harness --example bench_gateway --target-dir target/bench`를 실행한다. `cargo test --test benchmark_trace`로 exact-cost trace를 분리 검증한다. `python3 scripts/benchmark.py --binary target/release/llmgw --reference-binary target/bench/release/examples/bench_gateway --seeds 1,2,3,4,5 --windows 5 --output artifacts/pilot.json` 실행. 각 executable hash/features와 HTTP estimated/내부 exact trace 구분, mock workflow 완료 수, 성공/실패와 분포, idle/peak RSS, 짧은/긴 요청 완료율, unused budget·retry amplification을 기록한다.
- [ ] p95 추가 overhead <2ms와 idle RSS <50MiB 목표 충족 여부를 각각 평가한다. 실제 측정 변동보다 작은 차이를 우위로 말하지 않는다. 경쟁 제품 stable release fixture는 별도 run config를 고정한 뒤 추가하며, 이 task에서 없었던 benchmark를 수행했다고 기록하지 않는다.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo clippy --locked --features bench-harness --example bench_gateway -- -D warnings`, `cargo test --locked`를 실행한다. 통과 후 변경/실패가 없으면 같은 suite를 불필요하게 반복하지 않는다. 남은 OS/client/real-task proof를 문서와 work-items에 유지한다.

## 결재 및 완료 기준

이 계획의 실행은 사용자가 승인했지만 Git 작업 승인은 별개다. 변경을 review 가능하게 만든 뒤 결재안에 범위(core transport / scheduler / evidence), 메시지 후보 `feat: add native streaming gateway`, `feat: coordinate quota and fair admission`, `test: add reproducible quota benchmarks`, 대상 브랜치 미정/remote 미정, 검증 결과, 롤백(아직 배포 없는 새 product 경로)을 적는다. 승인 전 commit 명령은 실행하지 않는다.

Chunk 1 완료는 native mock→agent 경로, Chunk 2 완료는 correctness와 측정 가능한 공개 pilot이다. **최고 성능·실제 저사양 OS 지원·전체 사용자 목표 완료가 아니다.** 후속 native experience를 완료하고 실제 환경 검증을 이어간다.
