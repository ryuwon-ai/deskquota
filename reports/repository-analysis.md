# 참고 저장소 코드 분석

기준일 2026-09-11. 아래 표는 고정한 소스의 정적 분석이다. 실행 성능·안정성·OS 지원을 직접 입증한 표가 아니다. Overlaat의 일부 동작은 [별도 실행 검증](reference-validation.md)에 기록했다. 전체 SHA와 remote는 [manifest](../evidence/repositories.json), stars 등 변동 지표는 [GitHub 조회](../evidence/github-metadata.json)를 기준으로 한다. 이 9개는 목적에 맞춘 표본이며 시장 전체 목록은 아니다.

## 비교

| 저장소 | 확인한 핵심 동작 | 우리 목적과 차이 | 라이선스 파일 확인 |
|---|---|---|---|
| [Overlaat](https://github.com/tdamsma/overlaat) | 모델 cap + 자원 pool 비용, priority/aging, 큰 요청을 위한 예약, stream 종료까지 점유 | 가장 가까운 로컬 공정 대기열 사례. LiteLLM 앞의 Python 서비스 2개와 DB 구성. 기본 자원 비용은 TPM이 아님 | MIT |
| [PromptForge](https://github.com/cppalliance/promptforge) | 공유 dominion별 제한, bounded queue, 클라이언트별 round robin, RAII permit | Rust·네이티브 설치 참고. 토큰 비용 기반 DRR은 현재 파일에서 향후 작업으로 명시 | Boost Software License 1.0 |
| [obleth](https://github.com/thediymaker/obleth-gateway) | weighted served/weight, 계층별 슬롯 배분, 남는 용량 빌려 쓰기 | 공유 GPU/HPC 운영용. Postgres/Redis/ClickHouse와 관리 계층이 커서 PC 최소 제품과 거리 있음 | Business Source License 1.1; 아래 주의 |
| [LiteLLM](https://github.com/BerriAI/litellm) | RPM/TPM 검사·라우팅·재시도, 별도 priority scheduler, worker별 bounded admission | 주요 비교 기준. 현재 코드는 대기열이 있음. Python이라 무조건 느리다고 판단하지 않음 | MIT, enterprise 디렉터리 별도 |
| [Bifrost](https://github.com/maximhq/bifrost) | Go provider별 channel queue, connection/object reuse, governance, exact/semantic cache | 네이티브 실행도 있음. broad provider gateway와 기능 범위가 다름. 게시된 microsecond 수치는 미재현 | Apache-2.0 |
| [SMG](https://github.com/smg-project/smg) | Rust priority admission, class 예약, 선점 전 취소, response body와 permit 수명 결합 | 서버 fleet·KV-aware routing 중심. HTTP 연결 취소는 KV 상태 보존 재개가 아님 | Apache-2.0 |
| [llama-swap](https://github.com/mostlygeek/llama-swap) | 모델 프로세스 교체·그룹, 모델별 큐, 현재 전역 concurrency cap은 즉시 429 | 로컬 엔진 앞에서 재사용 가능. 엔진 실행·교체는 우리 v0.1에서 맡기지 않음 | MIT |
| [MindRouter](https://github.com/ui-insight/MindRouter) | deficit·burst credit·최근 사용량·대기시간을 이용한 배분, backend scoring | 대학 공유 자원 운영 사례. MariaDB·관리 기능·모달리티 범위가 큼. README의 WDRR 명칭만으로 공정성 정리를 가정하지 않음 | Apache-2.0 |
| [Pi](https://github.com/earendil-works/pi) | 모델 provider 설정, API 종류·base URL·사용자 header, 모듈식 agent harness | 게이트웨이 경쟁자가 아니라 초기 클라이언트와 최소 제품 설계 참고 | MIT |

## 요청 경로에서 확인한 것

### Overlaat: 큐만 아니라 취소의 실제 비용까지 본다

[`scheduler.py`](../references/overlaat/overlaat/scheduler.py)의 상단 불변식과 `Scheduler` 구현은 pool마다 독립 비용 장부를 갖는다. 모델별 동시 실행 상한, pool 비용 한도, exclusive pool의 서로 다른 모델 배제 조건을 모두 만족해야 보낸다. 큰 요청이 예산 때문에 막힐 때 해당 pool의 용량을 예약해 작은 요청이 계속 끼어드는 것을 막는다. 다른 pool까지 멈추지는 않는다. aging은 설정 가능하며 기본값은 0이다.

[`queue_proxy.py`](../references/overlaat/overlaat/queue_proxy.py)의 `stream_body`는 upstream 종료·오류·클라이언트 이탈을 구분한다. 엔진이 disconnect로 멈추지 않는 설정에서 upstream을 끝까지 drain하고 슬롯을 늦게 반환하려는 코드가 있다. 그러나 실제 TCP probe에서는 이 설정에서도 자연 EOF 전 연결 종료와 다음 요청 입장이 3회 관측됐다. **클라이언트 이탈과 GPU가 비었다는 사실을 구분하려는 설계 의도**를 참고하되, 실제 수명 보존이 검증됐다고 표현하지 않는다. 강제 usage 요청이나 metadata 제거로 본문을 재직렬화하는 경로도 있어, 우리 원문 전달 계약에 그대로 옮길 수는 없다.

[`test_scheduler.py`](../references/overlaat/tests/test_scheduler.py)와 [`test_queue_behavior.py`](../references/overlaat/tests/test_queue_behavior.py)의 57개 테스트를 격리한 lock 환경에서 실행해 모두 통과했다. 별도 원본 scheduler 재현에서 heavy → heavy → light는 잔여 예산 0.25가 있어도 light가 대기했고, heavy → light → heavy는 light가 입장했다. 원본 테스트 설명도 이 순서 의존성을 인정한다. ASGI 호출·직접 iterator 종료의 통과와 실제 TCP streaming·disconnect 검증을 구분해야 한다. 상세 조건과 한계는 [동작 검증](reference-validation.md)에 있다.

### PromptForge: 가장 작은 공정 큐의 기준점

[`gateway-routing/src/queue.rs`](../references/promptforge/crates/gateway-routing/src/queue.rs)에서 `DominionQueue::admit`, `Permit::drop`, `CancelOnDrop`를 추적했다. 즉시 실행 가능하면 슬롯을 주고, 그렇지 않으면 bounded queue에 넣거나 거절한다. fair 모드는 `X-PromptForge-Client`별 순환이며, 토큰량이 같은 비율로 배분된다는 보장은 아니다. 취소와 oneshot 슬롯 이전이 경합하는 경로도 처리한다.

클라이언트 header의 길이·개수 제한은 메모리 경계다. 사용자가 여러 유효 ID를 만드는 행위를 인증상 차단하는 장치는 아니다. 우리의 agent/session 힌트도 이 한계를 표시해야 한다.

### obleth·MindRouter: 계층과 비용 단위를 참고하되 복제하지 않는다

obleth의 [`fairshare/src/lib.rs`](../references/obleth-gateway/obleth/crates/obleth-fairshare/src/lib.rs), [`algorithm.rs`](../references/obleth-gateway/obleth/crates/obleth-fairshare/src/algorithm.rs)는 단일 scheduler task와 weighted/hierarchical 선택을 구현한다. root agent가 자식 20개를 만들었다고 전체 지분이 20배가 되지 않게 하는 설계에 참고가 된다. 실제 비용 정산과 공정성의 장시간 보장은 별도 동작 실험이 필요하다.

MindRouter의 [`fairshare.py`](../references/MindRouter/backend/app/core/scheduler/fairshare.py)는 `(deficit + burst_credits) / weight`, 최근 사용량 계수, 대기시간 보너스를 합친 우선순위를 계산한다. [`scheduler.md`](../references/MindRouter/docs/scheduler.md)의 명칭을 수학적 보장으로 받아들이지 않고, 비용·weight·음수 deficit 상황까지 trace로 검증해야 한다.

obleth의 현재 [LICENSE](../references/obleth-gateway/LICENSE)는 Business Source License 1.1이며 competing commercial offering에 관한 제한과 2030-06-11 변경일을 명시한다. PromptForge의 **Boost** 라이선스와 다른 문서다. 이 조사에서는 알고리즘과 동작을 비교하며 코드를 우리 제품으로 복사하지 않았다.

### LiteLLM·Bifrost: 오래된 경쟁 설명을 그대로 쓰면 틀린다

LiteLLM의 [`admission_control_middleware.py`](../references/litellm/litellm/proxy/middleware/admission_control_middleware.py)는 process별 semaphore, bounded queue, queue timeout, finally release를 가진다. [`scheduler.py`](../references/litellm/litellm/scheduler.py)와 quota pre-check는 별도 경로다. “LiteLLM은 초과 요청을 무조건 즉시 거절하고 큐가 없다”는 일반화는 현재 소스와 맞지 않는다. 이번 clone의 default branch는 `litellm_internal_staging`이므로 향후 비교 실행에서는 stable release도 별도로 고정해야 한다.

Bifrost [`core/bifrost.go`](../references/bifrost/core/bifrost.go)의 `ProviderQueue`와 enqueue 경로는 provider별 channel을 쓰며 full 시 대기/드롭 설정이 있다. [`semanticcache/main.go`](../references/bifrost/plugins/semanticcache/main.go)의 `Init`은 direct-only에서도 VectorStore를 요구한다. exact cache와 embedding cache는 비용이 다르고, 이 플러그인의 존재가 코딩 작업에 높은 hit rate를 보장하지 않는다.

### SMG: 선점이라는 단어의 의미를 좁혀 읽어야 한다

[`scheduler/engine.rs`](../references/smg/model_gateway/src/middleware/scheduler/engine.rs)는 첫 응답 body data 이전 요청을 선점 대상으로 고르고 슬롯 반환을 제한 시간 동안 기다린다. [`extract.rs`](../references/smg/model_gateway/src/middleware/scheduler/extract.rs)의 `PreemptionGuard`와 HTTP router의 streaming forwarding 경로에 취소 wiring이 있다. [`body.rs`](../references/smg/model_gateway/src/middleware/scheduler/body.rs)는 첫 data frame을 표시하고 body와 permit 수명을 묶는다.

첫 data frame은 의미 있는 첫 token과 항상 같지는 않다. [`admission.rs`](../references/smg/model_gateway/src/middleware/scheduler/admission.rs)는 queue 대기 중 disconnect 감지가 아직 연결되지 않았다고 명시하며 fresh token을 넘긴다. 모든 handler·엔진에서 실제 계산 취소가 보장되는지는 실행으로 확인하지 않았다. 이미 200 header가 나간 뒤의 선점은 503 전환이 아니라 body 중단일 수 있다. **CPU처럼 투명한 pause/resume을 제공한다는 근거가 아니다.**

### llama-swap·Pi: 조합할 경계

llama-swap [`internal/server/concurrency.go`](../references/llama-swap/internal/server/concurrency.go)는 전역 weighted semaphore에 `TryAcquire(1)`을 쓰고 초과 시 바로 429를 반환한다. README에는 Windows 바이너리·WinGet 경로가 있고 `runtime_windows.go`도 있다. 현재 코드 존재와 실제 Windows 검증은 구분한다.

Pi의 [`models.md`](../references/pi/packages/coding-agent/docs/models.md)는 API 종류와 base URL, custom headers를 제공한다. 초기 session 힌트 실험에 적합하다는 추론이다. 다른 도구에도 같은 설정이 있다고 가정하지 않고, 도구별 wire trace를 공개 mock으로 먼저 확인한다. Pi 저장소의 현재 canonical remote는 `earendil-works/pi`다.

## 채택할 패턴과 보류할 패턴

- 채택 후보: 하나의 자원/할당량 단위에 장부 하나, bounded queue, event 기반 wake-up, stream 수명 permit, 불가능한 요청 조기 거절, 취소 비용의 보수적 처리.
- 비교 실험 후 선택: request round robin 대 token-cost deficit, 예약이 만드는 유휴 시간과 긴 요청 기아 사이의 균형, header 관측 기반 admission 조정.
- v0.1 제외 후보: 엔진 lifecycle, 모델 대체, protocol translation, semantic response cache, MCP/agent 실행, billing/관리 UI, 분산 합의.

정적 검토로 “우리 구현이 더 빠르다”는 결론은 낼 수 없다. [벤치마크 명세](benchmark-spec.md)가 그 판단을 위한 다음 단계다.
