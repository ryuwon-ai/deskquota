# 논문 artifact 소스 심층 검토

기준일: 2026-09-12. 대상은 LMetric의 Blitz router, LAPS, FastServe의 아래 고정 체크아웃이다. 목표 제품은 **GPU 엔진을 제어하지 않는 네이티브 로컬 Rust HTTP quota gateway**다. 이 보고서는 알고리즘의 관측 요구, 실행·취소·자원 수명, 테스트의 실제 범위를 코드에서 확인한 결과다. 논문 성능 재현이나 이 PC에서의 제품 성능 측정은 아니다.

가져올 것은 **작은 관측값으로 결정하기, 대기와 실행을 분리하기, 자원 회수를 한 번만 하기, 짧은 요청 보호와 긴 요청 기아를 함께 검증하기**다. KV cache 이동, token iteration 선점, CUDA Graph batching은 현재 gateway의 구현 범위에 들어오지 않는다.

## 출처와 검증 범위

| 저장소 | 고정 revision / branch | commit 날짜 | checkout 조회 시각 UTC | 학회 게재와 runtime 기준의 구분 |
|---|---|---|---|---|
| [Blitz router](https://github.com/blitz-serving/blitz-router/tree/a005ceb3a6689d30514bd27e45ede0db933d0fea) | `a005ceb3a6689d30514bd27e45ede0db933d0fea` / `lmetric/camera-ready` | 2026-05-22 +08:00 | 2026-09-12 02:13:06 | [OSDI 2026, 7월](https://www.usenix.org/conference/osdi26/presentation/zhang-dingyan). branch 이름은 대응 근거이나, 논문 모든 실험의 engine·trace·config가 이 SHA와 정확히 일치함을 증명하지 않음 |
| [LAPS](https://github.com/Jianshu-She/LAPS/tree/aa29962d501236ecd2a0c7a8470c5977df3fe057) | `aa29962d501236ecd2a0c7a8470c5977df3fe057` / `main` | 2026-04-08 UTC | 2026-09-12 02:13:11 | [MLSys 2026 proceedings의 PLA-Serve](https://proceedings.mlsys.org/paper_files/paper/2026/hash/bbb7506579431a85861a05fff048d3e1-Abstract-Conference.html), 방법·artifact 명칭은 LAPS. 앞선 원문 검토의 SGLang v0.4.6.post5 기준을 현재 main 의존성과 합치면 안 됨 |
| [FastServe](https://github.com/LLMServe/FastServe/tree/187a2742bd3eb8514d8609d80ad98e105621feed) | `187a2742bd3eb8514d8609d80ad98e105621feed` / `main` | 2025-09-26 +08:00 | 2026-09-12 02:13:13 | [NSDI 2026, 5월](https://www.usenix.org/conference/nsdi26/presentation/wu-bingyang). 앞선 원문 검토의 baseline은 vLLM v0.6.1·FasterTransformer v5.3. 2026 게재를 최신 엔진과의 비교로 표현하지 않음 |

논문 runtime·장비·baseline의 원문 검토는 [literature-review.md](literature-review.md), PDF 출처는 [manifest](../papers/manifest.json), checkout 시각·sparse 범위는 [repositories.json](../evidence/repositories.json)에 있다. 위 학회 페이지와 FastServe arXiv 메타데이터는 이번에도 직접 조회했다.

- **직접 검증:** 각 하위 저장소의 HEAD·remote·commit 날짜·Git status, 파일·함수·행 번호를 확인했다. 세 checkout의 tracked 변경은 없었다. sparse checkout에 없는 경로는 `git ls-tree -r HEAD`로 확인했다. 부모 작업에서 FastServe의 `benchmarks/artifact-evaluation`, `benchmarks/benchmark-serving`, `doc`와 LAPS의 Python router 경로를 추가한 뒤 읽었다. 최초 파일 미노출은 repository 부재가 아니었다.
- **코드/문서 근거:** 아래 제어 흐름, 의존성, 테스트 assertion과 문서의 차이. 저장소별 핵심 검토 경로 8개를 표로 한정하고 README·manifest·평가 안내는 보조 근거로 분리했다.
- **추론:** 해당 흐름에서 생길 수 있는 대기·취소 경합과 gateway 수용 조건. 실제 실행으로 재현한 버그로 표기하지 않는다.
- **미확인:** 원본 테스트 suite, TLC 모델 검사, GPU·모델 실행, 논문 수치 재현, Windows 실행, 실제 API 연동. 엔진 의존성 설치·build·모델 다운로드·데이터셋 재생을 하지 않았다. 아래 후속 probe만 격리 aiohttp 환경에서 실행했다. LAPS의 실제 데이터셋 prompt는 열거나 복사하지 않았다.

## Blitz / LMetric

### 핵심 경로 8개

| 고정 소스 | 실제 확인한 내용 | gateway 판단 |
|---|---|---|
| [policies/lmetric.rs:1–19](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/router/src/scheduler/policies/lmetric.rs#L1-L19) | `prefill_tokens(req, o) * (o.bs + 1)`의 최솟값 선택 | 단순 지표라는 원칙만 적용. 일반 API에는 worker batch size·KV hit가 없음 |
| [policies/dsl_runtime.rs:64–145](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/router/src/scheduler/policies/dsl_runtime.rs#L64-L145), [515–550](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/router/src/scheduler/policies/dsl_runtime.rs#L515-L550) | replica별 lock 안에서 hit·epoch·batch·queue snapshot. `prefill_tokens = queued_tokens + new_tokens`, 새 token은 입력 token 수에서 cached block 수×block size를 뺌. 반영 때 epoch가 달라지면 hit를 재조회하고 계수를 증가 | 점수 계산과 자원 반영 사이의 변경을 다루는 원칙은 유효. 이 lock은 replica별이며 모든 replica의 전역 동시 snapshot을 뜻하지 않음 |
| [engine/client.rs:115–155](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/router/src/engine/client.rs#L115-L155), [277–324](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/router/src/engine/client.rs#L277-L324) | 요청 송신과 engine step 수신을 분리. metric에는 block 생성·eviction·preemption·abort ACK가 포함. HTTP backend의 `abort_request`는 **no-op** | generic HTTP forwarding으로 대체할 수 없는 engine 계약. trait에 abort가 있어도 backend 취소 지원을 의미하지 않음 |
| [engine/colocation.rs:163–247](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/router/src/engine/colocation.rs#L163-L247), [730–839](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/router/src/engine/colocation.rs#L730-L839) | worker가 queue에서 꺼내 engine에 보내고 completion 쪽으로 ownership 전달. 응답 채널 send 실패는 frontend abort로 분류. exception context·finish·backend Term ACK를 나누어 계수를 정리 | enqueue·dispatch·response close·upstream 종료를 서로 다른 사건으로 다룰 근거 |
| [scheduler/kvcache.rs:695–712](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/router/src/scheduler/kvcache.rs#L695-L712), [1020–1085](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/router/src/scheduler/kvcache.rs#L1020-L1085) | insert/get/remove의 property test가 있음. 선택한 random-op test의 Get 검사는 panic 부재이며 모든 연산의 의미 동등성을 검사하는 것은 아님 | 원본 테스트의 목적을 구분. gateway는 quota ledger를 단순 oracle과 대조하는 것이 적합 |
| [radixtree/src/core.rs:1–59](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/radixtree/src/core.rs#L1-L59) | Patricia trie의 block hash·block id 표현, small Vec→HashMap 분기, raw pointer 기반 자료 구조 | 실제 KV 소유권·eviction telemetry가 없는 v0.1에 radix tree를 추가할 근거가 없음 |
| [policy-dsl/src/check.rs:57–160](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/policy-dsl/src/check.rs#L57-L160) | 함수 allowlist와 loop·unsafe·match 제한을 둔 compile-time DSL lint. method call과 field access는 제한하지 않음 | 정책·상태 반영 분리 원칙은 적용 가능. 설정 언어·proc macro 자체는 현재 단일 정책 요구에 불필요하며 보안 sandbox로 해석하지 않음 |
| [AbortRecovery.tla:1–32](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/spec/abort-recovery/AbortRecovery.tla#L1-L32), [554–629](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/spec/abort-recovery/AbortRecovery.tla#L554-L629) | ownership 유일성·resurrection 방지·계수 비음수 invariant. liveness는 engine finish와 abort ACK가 결국 온다는 fairness 가정에 의존. ACK 가정을 제거한 별도 spec도 있음 | 소스에 명세가 있다는 사실과 실제 transport에서 가정이 성립한다는 사실을 분리해야 함 |

**정책 데이터 의존성:** 원문 초록의 “새 prefill×batch”를 구현에 그대로 대입하면 중요한 항이 빠진다. 이 checkout은 queue에 이미 반영된 prefill 비용까지 더하고 `bs+1`을 쓴다. 입력 token IDs, block size, replica별 prefix hit, KV 변화 epoch, admission과 engine step이 갱신하는 부하 계수가 필요하다. 일반 사내 API의 RPM/TPM·HTTP latency·usage만으로 같은 수식을 관측할 수 없다. 반영 시 epoch 재조회는 비용 재계산이며, 선택된 replica 자체를 반드시 다시 최적화한다는 뜻도 아니다.

**실행·취소·KV 수명:** [요청 lifecycle 문서](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/docs/architecture/request-lifecycle.md#L24-L87)는 HTTP→validation/tokenization→Infer→PolicyRunner→engine 흐름과 별도 `/v1/metrics` SSE를 설명한다. KV tree는 engine metric을 받는 completion loop가 갱신한다. 클라이언트 응답 채널이 닫혔다는 사실은 GPU가 멈췄다는 ACK가 아니다. 특히 HTTP abort 구현이 no-op이고, 읽은 colocation 예외 경로는 entry를 `WaitingTerm` 쪽에 보관한다. 따라서 **실제 upstream 취소·ACK·최종 정리가 모두 완료된다고 이 코드만으로 주장할 수 없다.** 완료 이벤트와 ACK의 순서가 바뀌는 fixture가 필요하다.

**의존성과 실행 한계:** [router manifest](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/router/Cargo.toml#L14-L89)는 Axum 0.6.20, Reqwest 0.11.20, Tokio 1.32, tokenizers, 자체 radixtree·policy-dsl 등을 선언하고 기본 feature에 `vllm-backend`, `ngrok` 등을 둔다. 이는 version 요구사항이며 여기서 resolve·build한 설치 버전이 아니다. [README](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/README.md#L18-L39)는 patched vLLM인 yaullm과 별도 MetricsTestRunner E2E를 가리킨다. 이번에는 이를 clone하거나 실행하지 않았다.

**표시된 권리 정보:** GitHub metadata의 license는 null이지만 [LICENSES/Apache-2.0.txt](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/LICENSES/Apache-2.0.txt#L1-L9)와 [NOTICE](https://github.com/blitz-serving/blitz-router/blob/a005ceb3a6689d30514bd27e45ede0db933d0fea/NOTICE#L1-L25)가 존재한다. NOTICE는 TGI에서 유래한 부분과 변경을 설명한다. **null을 “라이선스 없음”으로 단정하지 않고, 포함된 Apache 문서만으로 저장소 전체의 재사용 조건을 확정하지도 않는다.** 이번 작업은 코드 복사 없이 원리와 검증 조건을 정리했다.

**최소 fixture 아이디어:** synthetic observation의 cached hit·epoch를 점수 계산과 반영 사이에 변경하기, frontend close와 engine finish/ACK의 순서를 바꾸기, ACK가 영원히 오지 않는 backend를 두기. 전자는 미래 telemetry adapter용이며 v0.1에서는 실제 quota 예약·회수 사건의 순서 검사로 축소한다.

## LAPS

### 핵심 경로 8개

| 고정 소스 | 실제 확인한 내용 | gateway 판단 |
|---|---|---|
| [server_args.py:607–615](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/python/sglang/srt/server_args.py#L607-L615), [2567–2589](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/python/sglang/srt/server_args.py#L2567-L2589) | `enable_laps`가 CUDA Graph·dual queue를 켬. prefill disaggregation 모드가 아니면 scheduler는 다시 비활성화. threshold 기본 256 | `--enable-laps` 하나로 모든 실행 모드에 dual queue가 적용된다고 해석하면 안 됨 |
| [disaggregation/prefill.py:363–468](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/python/sglang/srt/disaggregation/prefill.py#L363-L468), [650–717](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/python/sglang/srt/disaggregation/prefill.py#L650-L717) | bootstrapped 요청을 token 길이로 short/long 분리, short batch 우선. waiting queue와 subqueue를 재조정해 취소된 요청을 제거. KV transfer success/failure를 poll한 뒤 cache와 metadata buffer 정리 | 큐에서 빠진 요청이 다른 큐에서 되살아나지 않는 invariant는 적용. prefill chunk·KV transfer 자체는 엔진 기능 |
| [managers/scheduler.py:2751–2848](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/python/sglang/srt/managers/scheduler.py#L2751-L2848) | waiting·bootstrap·inflight transfer·running 상태별 abort가 다름. running 요청은 `FINISH_ABORT`를 표시하고 후속 forward/cleanup 경로를 이용 | “취소 표시”와 “자원 반환 완료”를 분리할 근거 |
| [piecewise_cuda_graph_runner.py:786–859](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/python/sglang/srt/model_executor/piecewise_cuda_graph_runner.py#L786-L859), [984–1069](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/python/sglang/srt/model_executor/piecewise_cuda_graph_runner.py#L984-L1069) | batch size·extend length에 맞는 captured graph 선택, 고정 buffer에 token/metadata를 채운 뒤 replay | HTTP body를 자르거나 다시 보내는 것과 전혀 다른 계층. GPU graph와 attention 구현 필요 |
| [mini_lb.py:136–205](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/sgl-model-gateway/bindings/python/src/sglang_router/mini_lb.py#L136-L205), [275–367](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/sgl-model-gateway/bindings/python/src/sglang_router/mini_lb.py#L275-L367), [540–592](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/sgl-model-gateway/bindings/python/src/sglang_router/mini_lb.py#L540-L592) | short/long prefill group 사이에 instance 하나씩 이동, 각 group 최소 1개 유지. stream 응답 객체 반환 뒤 handler finally에서 pending 차감 | admission 계수의 수명을 handler 함수가 아니라 실제 작업 수명과 맞추는 반례 |
| [bench_prefill_only.py:30–80](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/bench_laps_prefill_throughput/bench_prefill_only.py#L30-L80), [128–161](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/bench_laps_prefill_throughput/bench_prefill_only.py#L128-L161) | `resp.json()` 완료 시간을 TTFT라 부름. token throughput은 성공 요청의 문자 수 `//4` 합산. rate 모드는 일정 간격 pacing | first token, HTTP response 완료, 실제 token 수를 구분해야 함. 이름만으로 metric 의미를 판정하지 않음 |
| [bench_1gpu.sh:74–89](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/bench_laps_prefill_throughput/bench_1gpu.sh#L74-L89), [169–176](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/bench_laps_prefill_throughput/bench_1gpu.sh#L169-L176) | `max_new_tokens=1`; `laps` arm은 piecewise/batch CUDA Graph flags만 사용. benchmark 실패를 `|| true`로 넘기고 끝에서 PASSED 증가 | 이 스크립트의 LAPS label은 dual-queue 전체 효과가 아님. 종료·PASSED 출력을 수용 조건 통과로 쓰면 안 됨 |
| [archive/test_laps_dynamic_alloc.py:254–267](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/bench_laps_prefill_throughput/archive/test_scripts/test_laps_dynamic_alloc.py#L254-L267), [324–343](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/bench_laps_prefill_throughput/archive/test_scripts/test_laps_dynamic_alloc.py#L324-L343) | 실제 서버·router를 띄우는 수동 점검 스크립트. 상태·요청 결과를 출력하며 enabled가 아니면 print 후 return. summary의 OK는 streaming pending 수명 assertion이 아님 | GPU 서버가 없는 PC에서 가벼운 원본 unit test라고 실행할 대상이 아님 |

**논문·문서·현재 main의 차이:** [scheduler 문서](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/docs/laps_scheduler.md#L5-L57)는 branch별 dual queue·waiting window·dynamic allocation을 나누고 `laps_wait_window_ms`와 `_laps_should_dispatch_short`를 설명한다. 그러나 이번 main의 `prefill.py`·`server_args.py` 및 `python/sglang/srt` 검색에서는 해당 window 함수·설정을 찾지 못했다. **문서에 나온 waiting window가 이 pinned 실행 경로에 구현됐다고 표시하지 않는다.** 반면 MiniLB dynamic group allocation은 sparse 경로를 추가한 뒤 실제 코드를 확인했다. 이 구현은 short/long **prefill group**의 배정을 바꾸며, 코드 확인 없이 prefill↔decode GPU 자체를 재구성한다고 확대하지 않는다.

**입력과 엔진 의존성:** dual queue는 tokenization 뒤 `origin_input_ids` 길이를 사용한다. router의 [길이 추정 함수](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/sgl-model-gateway/bindings/python/src/sglang_router/mini_lb.py#L622-L664)는 input IDs이면 개수를 세고 text/messages면 문자 수를 4로 나누며 batch는 최대 길이를 쓴다. unknown은 분류 함수가 낮은 pending group으로 보낸다. 따라서 router 추정 token·engine 실제 token·quota 계수는 같은 데이터가 아니다. [pyproject](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/python/pyproject.toml#L17-L82)는 torch 2.9.1·sgl-kernel 0.3.21·FlashInfer 0.6.3·transformers 4.57.1 등을 선언한다. [artifact 안내](https://github.com/Jianshu-She/LAPS/blob/aa29962d501236ecd2a0c7a8470c5977df3fe057/bench_laps_prefill_throughput/README.md#L41-L76)는 CUDA·Mooncake 등 환경과 LMSYS 데이터셋을 요구한다. 공개되어 있다는 이유로 실제 대화 prompt를 검증 fixture에 재사용하지 않았다.

**구체적 반례 두 가지:** 첫째, `_get_next_laps_batch_to_run`은 short batch가 성립하면 long queue로 내려가지 않는다. 읽은 경로에는 aging이나 최대 bypass 수가 없어, short 요청이 계속 공급되는 경우 long 요청 기아 가능성이 있다. 이는 소스에서 도출한 조건부 추론이다. 둘째, `generate_stream`은 아직 실행되지 않은 async generator를 담은 `StreamingResponse`를 반환하는데, 바깥 handler의 finally는 그 반환 때 pending을 감소시킨다. 코드상 pending 수명이 stream body 수명보다 먼저 끝날 수 있다. **원본 TCP 재현은 하지 않았으며**, gateway에서는 slow-stream gate fixture로 이 경합을 검사해야 한다.

**측정 해석:** `max_new_tokens=1`에서 전체 응답 완료 시간은 prefill 중심 proxy metric으로 쓸 수 있으나, first-token arrival을 계측한 것은 아니다. benchmark의 rate 모드는 함수 설명의 “Poisson-like”와 달리 `i / request_rate`에 맞춰 일정 간격으로 보낸다. 문자 기반 token 추정과 성공 요청만의 latency 집계를 그대로 가져오지 않고, 실패·취소와 queue 대기까지 각각 기록한다. 문서의 expected results는 저자가 제공한 수치이며 이 PC에서 측정한 값이 아니다.

**최소 fixture 아이디어:** prompt 없이 길이 숫자만 가진 short/long trace, short가 계속 들어오는 동안 오래된 long의 admission, abort된 요청이 subqueue 재구성 때 부활하지 않는지, stream response 객체 반환 후 body를 gate로 막아 pending·permit이 유지되는지 확인한다.

## FastServe

### 핵심 경로 8개

| 고정 소스 | 실제 확인한 내용 | gateway 판단 |
|---|---|---|
| [api_server/fastserve_api_server.py:23–70](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/api_server/fastserve_api_server.py#L23-L70) | `/generate` 입력을 `SamplingParams`로 변환. streaming은 NUL 구분 JSON. background abort, non-stream은 output 반복 중 disconnect 확인 | 그대로 OpenAI SSE 호환 gateway라고 부를 수 없음. HTTP 취소 진입점의 존재와 backend cleanup 완성은 별도 |
| [llm.py:207–225](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/llm.py#L207-L225), [309–335](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/llm.py#L309-L335) | async wrapper가 engine step의 출력을 request event에 전달. abort는 engine 호출 뒤 event·output 추적 제거 | client event 수명과 engine state 수명이 연결되어 있음 |
| [engine.py:238–277](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/engine.py#L238-L277), [310–344](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/engine.py#L310-L344) | 매 step에 scheduler batch 선택→block allocate→worker forward. 완료 후 block/worker resource 정리. 별도 abort도 scheduler 호출 직후 block free | token iteration 선점의 실행 지점. HTTP 연결 단위 선점으로 대체 불가 |
| [scheduler.py:380–512](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/scheduler.py#L380-L512), [531–617](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/scheduler.py#L531-L617) | skip-join은 profiling DB의 prefill latency로 초기 queue 결정. quantum 10ms·배수 2; 실제 처리시간에 따라 강등. 기본 1,000 iteration마다 3초 이상 대기 요청 승격. block pressure가 batch 선택에 관여 | 시간·queue·KV 상태를 직접 가진 engine scheduler. 3초가 절대 대기 상한인 것은 아님 |
| [block_manager.py:25–42](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/block_manager.py#L25-L42), [63–95](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/block_manager.py#L63-L95), [151–163](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/block_manager.py#L151-L163), [280–307](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/block_manager.py#L280-L307) | request별 block table·CPU/GPU 위치·free/swapping pool, 이동 후 worker 호출. free는 allocation 존재를 assert | “이동 시작”과 “원본 공간 재사용 가능”을 분리해야 하는 자원 수명 사례 |
| [worker.py:48–95](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/worker.py#L48-L95), [209–284](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/worker.py#L209-L284) | Ray GPU worker, CUDA device·swap stream/event, K/V tensor를 model forward에 넘김. custom swap operator 호출 | transparent HTTP pause/resume에 필요한 상태가 gateway 밖에 있음 |
| [tests/test_scheduler.py:8–12](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/tests/test_scheduler.py#L8-L12), [43–79](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/tests/test_scheduler.py#L43-L79) | 선택한 scheduler tests는 FCFS batch/token 제한·pipeline 사례. MLFQ 취소·starvation 검사는 이 파일에 없음. FCFS 생성 시 block manager 인자를 넘기지 않음 | 테스트 파일 존재만으로 현재 scheduler 검증 완료라고 볼 수 없음 |
| [benchmark-serving.py:266–289](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/benchmarks/benchmark-serving/benchmark-serving.py#L266-L289) | dataset의 output 길이를 `max_tokens`로 넣고 `ignore_eos=True`, stream 사용 | 출력 길이를 고정한 serving 실험과 자연 종료·tool 반복을 포함한 agent 완료량은 다른 workload |

**정확한 paper 연결:** root [README의 citation](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/README.md#L64-L74)은 여전히 2023년 제목 “Fast Distributed Inference Serving for Large Language Models”를 적는다. 연결된 [arXiv](https://arxiv.org/abs/2305.05920)는 2023-05-10 제출, 2024-09-25 v3 수정으로 표시된다. 그러나 같은 checkout의 [artifact-evaluation/README:1–13](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/benchmarks/artifact-evaluation/README.md#L1-L13)은 NSDI'26 “Iteration-Level Preemptive Scheduling for Large Language Model Inference”를 직접 지칭한다. **저자 repo와 학회 논문을 연결할 문서 근거는 있다.** root README가 오래됐다는 이유만으로 다른 논문의 repository라고 단정하지 않는다.

**runtime 대응은 미확인:** [설치 안내](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/README.md#L22-L35)는 SwiftTransformer를 별도 clone·build하지만 그 명령에 commit pin이 없다. [environment.yml](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/environment.yml#L8-L65)은 Linux 패키지·Python 3.12.8·PyTorch 2.5.1·CUDA 12.4 계열을 고정한다. [평가 스크립트](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/benchmarks/artifact-evaluation/overall/overall_fastserve_alpaca.sh#L7-L23)는 저자 환경의 모델·데이터·로그 경로를 참조하고 [실행 부분](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/benchmarks/artifact-evaluation/overall/overall_fastserve_alpaca.sh#L53-L83)은 dummy weights·proactive offloading·profiling file을 사용한다. [vLLM arm](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/benchmarks/artifact-evaluation/overall/overall_vllm_alpaca.sh#L5-L16)은 기존 conda 환경을 활성화한다. 이 자료만으로 paper 실행 당시의 모든 binary·환경·dataset hash까지 복원했다고 할 수 없다. 모델·dataset·profiling binary는 실행하거나 내용 조사하지 않았다.

**취소 경로의 정적 검증 공백:** [FCFS abort](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/scheduler.py#L130-L145)는 running request를 즉시 삭제하지 않고 finished로 표시한다. 그런데 `engine.abort_request`는 바로 `free_blocks`를 호출하고, 정상 step 종료 경로도 finished requests의 block을 free한다. waiting 상태의 아직 allocation이 없는 요청도 같은 engine abort 진입점을 통과한다. `free_blocks`는 allocation 존재를 assert하므로 **대기 취소·실행 취소·완료와 취소의 경합을 실제 fixture로 검사해야 할 구체적 이유**가 있다. 이 보고서는 원본을 실행하지 않았으므로 실제 발생률·전체 엔진의 취소 안정성을 판정하지 않는다.

**테스트 drift:** 현재 [FCFS 생성자](https://github.com/LLMServe/FastServe/blob/187a2742bd3eb8514d8609d80ad98e105621feed/fastserve/scheduler.py#L91-L106)는 block manager를 필수 인자로 받는데, 위 테스트는 두 인자만 전달한다. 이는 현재 소스와 테스트 호출의 불일치를 직접 읽은 것이다. `pytest` 수집·실행 실패를 관찰한 것은 아니다. repo 전체의 다른 검증 경로까지 없다고 주장하지 않는다.

**최소 fixture 아이디어:** 실제 CUDA를 재현하는 대신 gateway ledger에 `queued → dispatched → finished/cancelled` 사건을 넣어 중복 반환을 검출한다. quantum 기반 token 선점은 제외하고, 요청·turn 경계에서 비용이 다른 세션들이 모두 admission 기회를 얻는지 검사한다. 코드에 승격 함수가 있다고 starvation이 해결됐다고 가정하지 않고 clock·도착 패턴·관측 기간을 고정한다.

## gateway에 적용할 판정과 수용 테스트

| 채택 수준 | 내용 |
|---|---|
| 현재 채택 | quota 예약·dispatch·완료의 책임 분리, body 수명에 맞춘 동시성 permit, 한 번만 회수하는 terminal 처리, 관측값/추정값 구분, 모든 outcome 계측 |
| 비교 실험으로 판단 | 큰 요청이 quota를 기다리는 동안 작은 요청을 허용하는 정책과 공정성. FIFO·round robin·승인된 비용 정책을 같은 transport·동일 fixture로 비교 |
| 현재 제외 | LMetric 수식의 임의 proxy 구현, KV prefix tree·GPU/CPU swap, token iteration MLFQ, request를 끊고 재전송하는 가짜 선점, CUDA batching window, 모델 profile DB, PD GPU 재배치, 정책 DSL |
| 별도 조건 충족 후 검토 | 실제 backend가 token/KV telemetry와 명시적 pause/resume·abort ACK 계약을 제공하고 사용자가 engine adapter 범위를 승인한 경우 |

아래 5개는 **제안하는 수용 테스트이며 아직 실행 결과가 아니다.** 공개 synthetic body·숫자 trace·loopback mock만으로 구성한다. 성능 비교의 전체 조건은 [benchmark-spec.md](benchmark-spec.md)를 따른다.

| 테스트 | 구체적 fixture | 통과 조건 |
|---|---|---|
| 1. 느린 stream의 점유 수명 | 동시성 1. A는 headers와 첫 SSE token을 보낸 뒤 완료 gate에서 멈춤. B를 enqueue | A의 body가 진행 중일 때 B upstream 시도 0. 첫 token은 A 완료 전에 client에 도착. A가 정해진 terminal 상태가 된 뒤 B를 한 번 dispatch |
| 2. 취소·완료 순서와 이중 반환 | queued 취소, dispatch 직후 취소, 첫 token 후 close, 완료와 timeout을 같은 시각에 발생시킴. upstream이 client close 뒤 계속 일하는 arm 포함 | queued 취소는 upstream 시도 0. dispatch한 요청은 자동 replay 0. 예약·permit 회수는 정책상 정한 시점에 정확히 1회, 계수 음수·요청 부활 0. close만으로 provider 계산 종료를 기록하지 않음 |
| 3. 작은 요청 유입 중 큰 요청 보호 | 같은 quota 아래 heavy→light→heavy와 heavy→heavy→light, 이후 지속 short 유입. large는 계약상 수용 가능한 비용으로 설정 | 승인 명세의 bypass/aging·deadline 규칙을 지키고 large의 무한 대기 방지. large를 timeout시켜 short 완료량만 늘린 결과는 개선 실패로 판정. 단일 실행 중 요청을 선점하지 않는 한계도 기록 |
| 4. 관측값 변경과 quota 원자성 | 모델 alias 2개가 quota pool 하나 공유. 동시 admission·늦은 usage·역순 rate-limit header·누락 usage 사건을 fake clock으로 주입 | RPM/TPM 예약이 선택한 provider 계약 아래 원자적으로 반영. 추정량과 실제량 별도 기록. stale header로 이미 소진한 예산이 되살아나지 않음. 사용량 감소를 upstream 예산의 즉시 환급으로 가정하지 않음 |
| 5. metric 의미와 실패 분모 | mock이 token 3개를 서로 다른 시각에 보내고 마지막 usage를 늦게 보냄. 성공·429·중간 close를 섞고 body 문자 수는 token 수와 다르게 설정 | queue wait·첫 HTTP byte·첫 유효 token·응답 완료를 별도 계측. actual token은 mock usage의 3, 문자 추정은 별도 필드. 실패·취소·upstream attempt가 누락되지 않고 중복 시도를 새 성공으로 세지 않음 |

이 감사로 확인한 것은 참고 코드가 요구하는 상태와 위험한 수명 경계다. 세 논문의 reported speedup을 우리 gateway의 예상 개선율로 옮기지 않으며, 원본 테스트·GPU 재현·제품 benchmark는 미실행 상태로 남긴다.

## 후속 실행 검증: LAPS 시간 측정 함수 1개

source 감사 뒤 원본 `send_request()`만 불러 **실제 loopback TCP 1회**로 확인했다. 원본 CLI·dataset·SGLang 엔진은 실행하지 않았다. [probe 코드](../scripts/probe-laps-metric.py), [결과](../evidence/reference-tests/laps/metric-boundary.json), [격리 환경 버전](../evidence/reference-tests/laps/probe-requirements.txt).

합성 서버가 text 값을 포함한 JSON prefix를 먼저 쓰고 마지막 닫는 괄호를 gate로 막았다. prefix 송신 뒤100ms 동안 함수가 완료되지 않음을 확인한 후 마지막 byte를 보냈다. 함수는 이후 성공 반환했다.

| 직접 측정한 값 | 관찰 |
|---|---|
| server 첫 body write | 시작 후1.092ms |
| server 마지막 body write | 102.386ms |
| 함수가 TTFT로 반환한 값 | 102.854ms |
| 함수 반환 | 103.155ms |
| 마지막 JSON byte 전 함수 미완료 | true |

이 숫자는 **JSON 완료를 기다리는 경계 확인**용 단일 합성 사례다. server write를 client 첫 token arrival로 부르지 않으며, LAPS의 지연 성능이나 논문 결과를 반박/재현하는 수치도 아니다. `max_new_tokens=1`의 prefill 중심 응답 완료 시간을 연구 목적의 대용 지표로 쓰는 것과, 일반 agent의 실제 TTFT를 계측하는 것을 구분한다. Python3.14.6/aiohttp3.14.3의 별도 probe 환경이며 논문 runtime과 같다고 가정하지 않는다.

재현:

```sh
python3 -m venv .venvs/laps-metric-probe
.venvs/laps-metric-probe/bin/python -m pip install -r evidence/reference-tests/laps/probe-requirements.txt
.venvs/laps-metric-probe/bin/python scripts/probe-laps-metric.py --output evidence/reference-tests/laps/metric-boundary.json
```
