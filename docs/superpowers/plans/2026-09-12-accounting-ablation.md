# Accounting Ablation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 같은 제품 RR 실행 파일에서 reserved/actual 정산만 바꿔, 같은 300초 동안 완료하는 요청의 차이를 재현 가능한 paired 실험으로 확인한다.

**Architecture:** 수용한 benchmark의 mock·요청 일정·transport·소유 자원 정리를 재사용한다. 기존 baseline matrix는 그대로 유효한 실험으로 유지하고 accounting mode만 명시적으로 추가한다. 별도 작은 Python 모듈이 accounting 구성과 paired 집계 검증을 맡으며 Rust 제품 정책은 바꾸지 않는다.

**Tech Stack:** 기존 Python stdlib·benchmark.py·benchmark_http.py·고정된 Rust release. 새로운 runtime/development dependency 없음.

---

상태: [계획 2차 검토 승인](../../../evidence/accounting-plan-review-round2.md). [실험 질문과 판단 기준](../../../reports/accounting-ablation-plan.md),
[기존 측정 명세](../../../reports/benchmark-spec.md),
[quota 계약](../specs/2026-09-12-local-quota-gateway-design.md)을 따른다.
Native Task 1은 SPEC과 QUALITY 재검토를 통과했다. 이 계획의 제품 하네스
Task 1과 Task 2는 **수용 완료**다. Task 2의 10회 측정과 부모 독립
검산(5 pair·1,000 ingress), 하위 그룹 실패 집계를 보완한 SPEC 재검토와
독립 QUALITY를 통과했다. [결과](../../../reports/accounting-ablation-results.md)는
고정 300초 완료 285/290과 긴 요청 timeout 15/15를 구분한다.
측정 소스 archive·binary·raw는 보존하며, 다음 Native 작업은 새 빌드 경로를 사용한다.
마법사·client 연결·OS 수용 작업도 별도 미완료로 유지한다.

작업 cwd는 `/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/product`다.
Git 저장소가 없는 승인된 연구 workspace이며 worktree/init/commit/release를
자동 생성하지 않는다. 참조 clone, 이전 source archives, 실행 파일, pilot40개와
기존 측정 보고서는 보존한다. 새 결과는 `artifacts/accounting-ablation/`에 둔다.
실행 구간에는 제품 수정·빌드·다른 부하 테스트를 겹치지 않는다.

## Chunk 1: 한 변수만 바꾸는 하네스와 paired 측정

### Task 1: accounting mode와 provenance·재개 검증

**Files:**
- Modify: `scripts/benchmark.py` — mode 선택, 실제 config 작성, 기존 소유 수명·재개 loop 연결.
- Modify: `scripts/benchmark_http.py` — client가 받은 숫자 usage와 유효성만 관측 결과에 추가. Mock 동작·응답·일정은 유지.
- Create: `scripts/benchmark_accounting.py` — 두 구성의 규칙, 조건 검증, paired 결과 집계.
- Create: `tests/test_benchmark_accounting.py` — 일정·설정 일치·분모·재개·중단 경계.
- Modify: `docs/benchmark-method.md` — accounting 실험과 원래 네-arm matrix의 목적·지표 차이.
- Read only: `src/`, `Cargo.toml`, `Cargo.lock`, 기존 원측정 artifacts.

- [x] **기존 경계와 실패를 먼저 확인한다.** `start_gateway`가 TOML과 반환 config
  metadata 모두 reserved를 고정하고, `main`이 네 arm을 고정한다는 현재 경계를
  확인한다. 새 mode CLI 테스트와 actual 구성이 담긴 TOML/provenance assertion을
  작성한다. `python3 -m unittest discover -s tests -p test_benchmark_accounting.py -v`
  에서 absent mode 또는 실제 reserved 값 때문에 FAIL함을 보존한다.

- [x] **명시적인 두 구성을 추가한다.** 기존 `production_rr`는 reserved이고,
  새 `production_actual`은 같은 일반 제품 executable의 actual이다. 비교용
  bench-harness executable로 actual을 대신하지 않는다. CLI는
  `--mode baseline|accounting`(기본 baseline), `--pilot-only`를 제공한다.
  accounting은 `--phase quota` 또는 `--smoke`에서만 허용한다. pilot-only는
  accounting quota에서만 허용하며 잘못된 조합은 파일 생성/child spawn 전에
  거절한다. 기존 내부 argparse Namespace fixture에도 mode를 명시하고
  이전 형식을 추측하는 getattr/legacy fallback을 추가하지 않는다.

핵심 규칙은 아래처럼 작고 순수하게 표현한다. 실제 인수 이름은 기존 API와 맞춘다.

```python
ACCOUNTING_ARMS = ("production_rr", "production_actual")

def accounting_for_arm(arm):
    if arm == "production_actual":
        return "actual"
    if arm in ("production_rr", "benchmark_fifo", "benchmark_rr"):
        return "reserved"
    raise ValueError("this arm does not launch a gateway")

def paired_schedule(seeds):
    schedule = []
    for index, seed in enumerate(seeds):
        order = ACCOUNTING_ARMS if index % 2 == 0 else ACCOUNTING_ARMS[::-1]
        schedule.extend(("quota", seed, arm) for arm in order)
    return schedule
```

두 제품 arm은 같은 binary 경로/SHA/features를 사용한다. 생성한 TOML의
`accounting`과 반환 `run.config.accounting`은 **한 변수**에서 나온다. 실제 생성
TOML을 parser로 읽은 sanitized snapshot과 원시 TOML SHA를 각 run에 저장해,
metadata만 actual인 오류를 막는다. fixture가 loopback·auth none임을 확인하고
비밀값은 저장하지 않는다. 비교 시 제외하는 값은 실행마다 배정된 loopback
port 번호와 생성한 **filesystem 임시 경로**뿐이다. upstream URL의 scheme,
host, API path와 query, root ID, endpoint, 모델과 출력 상한, quota 등은 보존한다.
정규화한 두 snapshot은 accounting 한 항목만 달라야 한다. auth none,
retry off, cancel close, concurrency 2, 4 root와 동일 quota를 유지한다.
API path나 모델 상한을 바꾼 뒤 run/manifest SHA까지 다시 계산한 음성 fixture도
재개 전에 거절한다. 실제 생성 TOML·snapshot·선언 metadata의 일치를 검증한다.

- [x] **identity를 정직하게 기록한다.** manifest에 mode와 전체 예정 schedule을
  저장한다. accounting mode는 사용하지 않는 benchmark executable을 요구하지
  않으며, 일반 제품 binary에 `bench-harness` feature를 표시하지 않는다.
  기존 `identities`의 reference를 명시적으로 unused/null 처리하거나 생략한다.
  source hashes에는 새 Python 모듈·tests·문서도 포함한다. run ID는 기존 형식의
  서로 다른 arm 이름을 포함하므로 두 결과가 같은 경로를 덮어쓰지 않는다.
  실제 config hash, accounting 값과 실행한 binary identity를 함께 남긴다.

- [x] **재개 전에 전체 결과를 검증한다.** 기존 orphan/failed/tmp/duplicate/
  wrong-path/wrong-hash 거절과 spawn 직후 PID journal·반복 취소 시 cleanup을
  유지한다. accounting mode는 각 등록 run의 arm/config.accounting/quota/
  concurrency/root 수/measurement duration/seed/request 일정도 계획과 대조한다.
  JSON의 accounting만 바꾸고 새 SHA를 manifest에 등록해도 다음 arm을 실행하기
  전에 거절해야 한다. 정규화된 두 config 차이가 다른 곳에 있으면 비교 실패로
  표시한다. 과거 artifact를 채택하거나 mode를 자동 변환하지 않는다.

- [x] **첫 pair 뒤에 정확히 멈추는 checkpoint를 만든다.** `--pilot-only`는 예정
  seed 전체를 manifest에 기록한 채 첫 seed의 두 완료 run을 검증·저장하고 모든
  child를 정리한 뒤 반환한다. 상태는 `accounting_pilot_completed`,
  `matrix_complete=false`, completed/planned pair 수를 표시한다. 첫 pair를
  전부 완료하지 못하면 pilot 성공이 아니다. 같은 전체 seeds/windows/mode로
  pilot-only 없이 재개하면 검증된 첫 pair를 재실행하지 않고 나머지만 실행한다.
  pilot-only는 실행 길이 제어이며 workload/source/config identity를 바꾸지 않는다.
  재개에서 schedule/seeds/window/mode가 바뀌면 기존 계약대로 거절한다.

- [x] **paired 집계를 별도 모듈에서 검산한다.** 같은 seed의 동일 submitted
  schedule과 고정 300초 경계를 확인하고 완료/거절/오류/timeout/cancel의 전체
  분모를 검산한다. 실제 schema인
  `outcome.ended_s <= run.start_monotonic_s + run.measurement_duration_s`로
  cutoff를 다시 계산하고 기존 `within_measurement`와 대조한다. flag만 믿거나
  존재하지 않는 timestamp alias/fallback을 만들지 않는다. quota는 300초,
  smoke는 3초다. 정확히 cutoff인 완료와 직후 완료, 조작한 flag를 테스트한다.
  각 seed의 `actual - reserved` 완료 차이, root/length별
  결과, mock429·추가 attempt·scheduled cancellation을 기록한다. drain 이후
  완료와 latency-success-only는 별도 필드다. 오래 기다린 성공만으로 성공률을
  높이지 않는다. 부분 pilot에는 한 pair만 집계하고 multi-seed 결론을 만들지 않는다.

- [x] **완료한 generation의 usage를 관측한다.** 기존 client SSE JSON 처리에
  숫자 usage/validity만 추가하고 `exchange`에서 `request_once` 결과로 전달한다.
  fixture 계약은 `[DONE]` 전 usage 한 번이다. prompt_tokens/completion_tokens는
  `type(value) is int`, 0 이상 u64 상한 이하이며 bool·문자열·음수·중복·종료 뒤
  usage는 유효하지 않다. 누락은 명시적으로 기록한다. 일반 production SSE parser를
  새로 만들지 않으며 request/response body와 credentials를 artifact에 저장하지 않는다.
  완료 generation은 연결된 mock attempt 및 submitted row로 다음을 검산한다.

```python
expected_input = math.ceil(attempt["body_bytes"] * submitted["actual_ratio"])
expected_output = max(1, math.ceil(submitted["output_reservation"] * submitted["actual_ratio"]))
# observed prompt/completion tokens must equal these numbers;
# their sum must equal attempt["actual_cost_fixture_units"].
```

완료 metadata는 not_applicable이며 취소·timeout·거절·disconnect 등에는
not_observed가 정상일 수 있다. 이 정상 누락을 실패로 판정하지 않는다.
완료 generation의 usage 누락/변조와 정상 metadata/취소 누락을 음성·양성 fixture로
구분한다. 완료 payload의 기존 관측은 보존하되 accounting 검증 실패를 숨기거나
유리한 완료 수로 바꾸지 않는다. client가 받은 usage는 gateway 내부 환급 그 자체의
증명이 아니다. gateway 집계와 기존 Rust usage 계약 증거를 별도로 표시한다.
실제 attempt·terminal 연결과 취소/서버 write 경합을 보존하며 고정된 attempt 수나
known-usage 수를 발명하지 않는다.

- [x] **meaningful GREEN을 확인한다.** 위 unit/CLI tests, 기존
  `python3 scripts/benchmark.py --self-check`, 짧은 accounting smoke를 실행한다.
  smoke는 두 arm 각각 20 ingress·17 complete/3 scheduled cancel이라는 fixture
  분모와 terminal payload를 확인하고, mock attempts는 실제 dispatch/취소 시점에
  따라 기록한다. attempt 수를 임의의 고정 수에 맞추지 않는다.

```sh
python3 -m unittest discover -s tests -p test_benchmark_accounting.py -v
python3 scripts/benchmark.py --self-check
python3 scripts/benchmark.py --mode accounting --smoke --binary target/native/release/llmgw --seeds 1 --output artifacts/accounting-ablation/smoke.json
```

첫 두 명령은 PASS, smoke는 `completed_smoke`이며 효율 결론은 없어야 한다.
실제 gateway startup 중 취소를 주입해 같은 소유 cleanup 경계도 확인한다.
부분 pair·다른 mode로 재개·accounting metadata/TOML 불일치·두 번째 arm
실패가 완료 matrix로 보이는 사례는 반드시 음성 fixture로 거절한다.

- [x] **Task 1 HOLD와 독립 리뷰.** 소스/실행 파일 SHA와 새 source archive,
  정확한 실행 명령·RED/GREEN·소유 PID/임시 경로 cleanup을 보존한다. Rust 제품은
  바뀌지 않았으므로 기존 Rust suite를 반복하지 않는다. fresh SPEC, 이어서 fresh
  QUALITY를 통과한 뒤 Task 2를 실행한다. 현재 사용자 설정·OS 등록·유료 API는
  테스트에 사용하지 않는다.

### Task 2: first-pair correctness gate와 같은 schedule의 전체 측정

**Files:**
- Create: `artifacts/accounting-ablation/pilot.json`과 `pilot-runs/` — 신규 원시 결과와 journal.
- Create: research `scripts/audit-accounting-results.py` — 제품 하네스를 import하지 않는 독립 읽기 전용 검산.
- Create: research `evidence/accounting-ablation-parent-audit.json`, `reports/accounting-ablation-results.md`.
- Update: research `evidence/implementation-progress.json`, `evidence/work-items.json`, `evidence/loop-log.jsonl`.
- Product source, binary, workload, config 규칙은 측정 시작부터 종료까지 HOLD.

- [x] **측정 전 usage correctness 증거를 확인한다.** 현재 Rust source와 기존
  정산 계약의 유효 usage/누락·잘림·종료 뒤 usage/과소 추정 debt 회귀의 identity가
  맞는지 확인한다. source가 관련 부분에서 바뀌었거나 새 미해결 의문이 있을 때만
  해당 focused test를 다시 실행한다. 현재 제품 release는 ordinary RR이며 두 arm이
  동일 SHA인지 재확인한다. 사내·외부 endpoint 또는 실제 모델은 호출하지 않는다.

- [x] **전체 예정 5-seed schedule의 첫 pair를 실행한다.**

```sh
python3 scripts/benchmark.py --mode accounting --phase quota --pilot-only --binary target/native/release/llmgw --seeds 1,2,3,4,5 --windows 5 --output artifacts/accounting-ablation/pilot.json
```

예상: 새 session, 2 completed runs/1 pair, 10 planned runs/5 pairs,
`accounting_pilot_completed`, `matrix_complete=false`, 모든 child cleanup.
각 arm은 startup hold를 유지하고 같은 5×60초 measurement와 기존 drain을 사용한다.
첫 pair gate는 provenance/usage/payload/denominator/cleanup 정확성이다.
유리한 완료 차이가 나와야만 다음 seed를 실행하는 gate로 만들지 않는다.

- [x] **부모가 독립적으로 첫 pair를 검산한다.** config의 accounting 한 항목 차이,
  binary/source hash, submitted schedule, 모든 terminal·attempt 연결, 실제 cutoff
  기준 완료 수와 사용량 관측을 raw JSON으로 다시 계산한다. 원본 SHA와 파생
  통계를 별도 파일로 남긴다. 수치가 같거나 불리해도 correctness가 통과하면 진행한다.
  완료 generation의 usage가 누락/변조되거나 request trace 변화가 있으면 원시 자료를 보존하고 원인을 해결한
  새 세대로 시작하며, 실패 자료를 정상 결과에 섞지 않는다.

독립 auditor는 제품 하네스 모듈을 import하지 않으며 다음 CLI를 제공한다.
실패 시 exit 1이고 auditor source SHA를 측정 source identity와 별도로 기록한다.
조작한 결과 복사본을 실제로 거절하는 음성 검증을 먼저 수행한다.

```sh
python3 ../scripts/audit-accounting-results.py --manifest artifacts/accounting-ablation/pilot.json --expected-pairs 1 --output ../evidence/accounting-ablation-first-pair-audit.json
```

- [x] **동일 schedule을 재개해 남은 네 pair를 실행한다.**

```sh
python3 scripts/benchmark.py --mode accounting --phase quota --binary target/native/release/llmgw --seeds 1,2,3,4,5 --windows 5 --output artifacts/accounting-ablation/pilot.json
```

예상: 첫 pair 재사용 검증 후 나머지 8 run만 실행, 총 10 completed runs,
1,000 ingress(arm별500), `completed_accounting_matrix`, `matrix_complete=true`.
기존 네-arm pilot을 이번 세대의 reserved 대조군으로 재사용하지 않는다.

- [x] **모든 결과와 한계를 기록한다.** 독립 검산은 10개 artifact hash·5개 paired
  일정·1,000개 ingress의 terminal 분모·실제 cutoff를 확인한다. 보고서는 seed별
  fixed-time 차이를 먼저 제시하고 모든 거절/timeout/cancel, 긴 요청·root별 변화,
  mock429, drain 포함 완료를 함께 표시한다. synthetic byte/output 비용과 실제
  tokenizer·provider 계약을 구분한다. 실제 agent task 성공률, 저사양/경쟁 제품,
  기본값 actual 전환이나 광범위한 성능 우위는 이 실험의 결론 범위가 아니다.
  차이가 없거나 악화된 결과도 그대로 보존한다.

```sh
python3 ../scripts/audit-accounting-results.py --manifest artifacts/accounting-ablation/pilot.json --expected-pairs 5 --output ../evidence/accounting-ablation-parent-audit.json
```

- [x] **결과 리뷰와 다음 작업.** 소유 프로세스·port·임시 상태 종료, source/binary
  HOLD 불변을 검증하고 fresh SPEC→QUALITY의 결과 검산을 받는다. 그 뒤 native
  wizard 작업을 이어가고, 관측으로 정당화되는 다음 성능 질문만 별도 범위로 정한다.
  공개 결과/commit/release는 구체적 변경·검증·대상·롤백을 갖춘 별도 사용자 결재 전까지
  생성하지 않는다. 현재 계획 자체에 추가 결재 질문은 필요하지 않다.
