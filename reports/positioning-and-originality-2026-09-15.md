# DeskQuota의 포지셔닝과 독창성 재평가

기준일: 2026-09-15. 사용자의 최신 방향은 **사내·로컬에 한정하지 않고, 한도나 용량 제한이 있는 LLM 요청 전반에 적용**하는 것이다. 실행 형태는 여전히 사용자 PC의 가벼운 네이티브 프로그램이다. 적용 대상 확대가 다중 provider 변환·라우팅·분산 장부 구현 승인이라는 뜻은 아니다.

이번 결과는 **비교 조사와 방향 제안**이다. 제품 코드·참조 checkout을 바꾸거나 새 성능 실험을 실행하지 않았다. 현재 제품의 실행 증거는 [이전 35회 비교](current-competitor-comparison-2026-09-15.md)를 재검토했다. 최신 소개와 코드 근거, 실행한 버전, 전략적 판단을 구분한다. 시장 전체를 조사했거나 세계 최초를 입증한 결과가 아니다.

## 추천하는 정체성

> **DeskQuota — Get more done within your LLM limits.**
>
> A lightweight gateway that coordinates LLM requests around shared limits.
>
> 같은 LLM 한도를 나눠 쓰는 요청들의 낭비와 대기를 줄이는 경량 게이트웨이.

주 사용자는 여러 도구·스크립트·에이전트를 실행하면서 같은 한도에 부딪히는 개발자다. 상용 API·무료 API·사내 endpoint·로컬 모델이 모두 대상이 될 수 있다. 최초 사용 사례는 실제 제한과 반복 호출이 있는 환경으로 좁힌다. 제한이 걸리지 않고 재사용 가능한 요청도 없다면 체감 이득이 작을 수 있다.

현재는 **한 instance의 한 configured upstream과 그 instance를 통과하는 요청**을 관리한다. 지원하는 API 형식을 전제로 하며 모든 provider·모델 기능을 지원한다는 주장이 아니다. 다른 PC나 우회 경로가 소비한 한도를 직접 관측하지 못한다. 같은 프로토콜의 기존 gateway 앞에 두는 방식도 제품 방향에 맞지만, 조합별 인증·모델 목록·retry 동작은 검증해야 한다.

## 비교 기준과 최신성

독창성은 세 가지로 구분한다. **알고리즘 기여**는 선행 연구와 비교해 새 원리나 보장을 제시한 경우다. **제품 차별화**는 사용자가 선택할 이유가 되는 일관된 경험이다. **방어력**은 설치 기반·연동 검증·재현 자료·유지보수 경험처럼 쉽게 따라 하기 어려운 자산이다. 기능이 있다는 사실만으로 셋을 동시에 인정하지 않는다.

8개 핵심 저장소의 GitHub 소개, default-branch SHA, 고정 README를 새로 조회했다. 전체 SHA·조회 시각·README 해시·발췌는 [조회 기록](../evidence/positioning-review-2026-09-15.json)에 있다. 기존 구현 분석은 [참조 목록](reference-catalog.md), [초기 코드 분석](repository-analysis.md), [gateway 심층 분석](gateway-source-deep-dive.md), [추가 quota proxy 분석](additional-quota-references-2026-09-14.md)에 고정돼 있다. 최신 README에 과거 소스의 성능·지원 범위를 자동으로 붙이지 않는다.

**중요한 변경:** LiteLLM의 현재 GitHub 소개는 Rust core를 내세운다. 조회한 `7e3ca1421efc3907b13024578c2a9b5bb401a192`에는 core·token-counter·ai-gateway·Python interop/bridge를 포함한 [Rust workspace](https://github.com/BerriAI/litellm/blob/7e3ca1421efc3907b13024578c2a9b5bb401a192/litellm-rust/Cargo.toml)가 있다. 이는 소스·소개 확인이지 현재 안정 배포 전체의 Rust 전환이나 성능 검증이 아니다. 해당 하위 디렉터리의 README 두 경로는 404였다. 기존 비교 수치는 실행한 LiteLLM v1.100.1에만 유효하다.

## 저장소별 독창성과 우리와의 관계

아래의 ‘차별점’은 제품의 중심과 고정 소스에서 확인한 구현을 요약한 것이다. 저자의 속도·성공률 홍보 수치를 검증된 사실로 채택하지 않는다.

| 저장소·관계 | 차별점과 확인한 구현 | DeskQuota와 겹치는 부분 | 우리가 선택받으려면 |
|---|---|---|---|
| [HiveMind](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/README.md), 직접 경쟁 | OS식 에이전트 조정이 정체성. session 식별, per-agent quota, fair-share, AIMD, provider별 retry | 로컬 proxy, 동시 에이전트, quota·혼잡 조정이 크게 겹침 | 설치·상주 비용과 deadline 내 완료·재시도 절약으로 비교. ‘에이전트 OS’라는 발상을 새 기여로 내세우지 않음 |
| [Overlaat](https://github.com/tdamsma/overlaat/blob/4e3cd0ff02a0456d470d5b18744803a56c7e1ffb/README.md), 직접 경쟁 | 비용을 반영한 공정 대기열과 미완료·이탈까지 포함한 장부. 다자원 pool 예약으로 큰 요청 보호 | 대기, 기아 방지, 요청 수명과 자원 점유 회계 | 외부 API의 RPM/TPM 계약 대응과 단일 실행 파일 경험을 완성. 공정 큐·정직한 회계 자체는 선행 기능 |
| [PromptForge](https://github.com/cppalliance/promptforge/blob/9184895303285361159ab348017243f9d3936a1c/README.md), 구성요소 경쟁 | 제품 중심은 Markdown+Lua prompt pipeline와 Workshop. gateway는 자격 증명·모델·concurrency pool을 관리. 과거 고정 소스에 client RR·bounded queue | Rust gateway, 순환 큐, 네이티브 설치 | 기존 도구의 제한 조정만으로 독립적인 사용 가치를 보여야 함. 네이티브라는 사실만으로 구별되지 않음 |
| [LiteLLM](https://github.com/BerriAI/litellm/blob/7e3ca1421efc3907b13024578c2a9b5bb401a192/README.md), 범용 인접 경쟁 | 많은 provider를 같은 API로 호출하는 통합과 virtual key·예산·routing·운영 연동. 최신 Rust workspace도 확인 | RPM/TPM, cache, usage 정산, queue/retry | 제한된 요청을 조정하는 작은 제품에 집중. Python 대 Rust 비교를 장기 우위의 근거로 쓰지 않음 |
| [Bifrost](https://github.com/maximhq/bifrost/blob/0767e0da708e9b00626ff0f627350f7b1f6086a0/README.md), 범용 인접 경쟁 | Go 기반 다중 provider gateway의 전달 효율·설정 UI·확장 기능. 고정 소스에 provider queue·connection reuse·governance·cache | 빠른 전달, native 실행, cache·한도 관리 | 동등한 제한 조건의 완료·대기 결과로 비교. 현재 dev 소개의 enterprise 기능을 모두 OSS 실행 기능으로 세지 않음 |
| [Portkey Gateway](https://github.com/Portkey-AI/gateway/blob/669825cbe89ee51569918b8f78a9db486fd69dd4/README.md), 범용 인접 경쟁 | provider routing과 retry/fallback/hooks·guardrails를 조합하는 신뢰성 계층 | retry, caching, 기존 SDK 연결 | 중첩 retry와 실제 quota 소비를 다루는 좁은 동작을 검증. 분석한 main과 README가 안내하는 2.0 pre-release·hosted 제품을 구분 |
| [MindRouter](https://github.com/ui-insight/MindRouter/blob/3b1e2220dbbcaedb151262bad2058ffec2404368/README.md), 공유 자원 인접 경쟁 | 기관의 이종 GPU·모델을 API·사용자 quota·공정 배분·운영 관리로 묶음. 기존 소스에 deficit·burst credit·대기 보너스 | 비용을 반영한 배분, 사용자별 사용량 | 다중 사용자 운영 계층을 복제하기보다 작은 환경에서도 설치·설정·측정이 간단해야 함 |
| [obleth](https://github.com/thediymaker/obleth-gateway/blob/590824157f71f256a947c5823b8d11d4e9fa7ae5/README.md), 공유 자원 인접 경쟁 | Rust fair-share gateway, tenant·계층별 배분과 잔여 용량 공유. HPC/Slurm·비용·운영 회계까지 연결 | Rust, weighted/hierarchical fairness, cloud와 local 모두 대상 | 계층적 공정성도 독점 아이디어가 아님. 우리 workload에서 필요한 최소 단위를 검증해야 함 |
| [Otari](https://github.com/mozilla-ai/otari), 운영 gateway 인접 경쟁 | any-llm provider 통합, virtual key, 지출 전 검사와 usage 정산. 기존 고정 소스는 원자적 예약·terminal once의 근거 | 예약과 실제 정산의 분리 | 정산 원리를 재발명하기보다 instance 내부의 작은 장부와 설치 경험으로 제품 가치를 만듦 |
| [Helicone ai-gateway](https://github.com/Helicone/ai-gateway), 전달·관측 인접 경쟁 | Rust 전달 경로와 관측 연동. 기존 고정 소스의 GCRA·SSE·logger 수명을 분석 | Rust, rate limit, streaming | 관측 스택의 폭보다 원문 저장 없이 대기·누락·절약 원인을 설명하는 작은 범위를 선택 |
| [TensorZero 고정 구현](https://github.com/tensorzero/tensorzero/tree/62eb8f63e8ec62018d70420dbf1a8c5d1c026315), 역사적 설계 참고 | cache를 예약보다 먼저 조회하고, stream usage의 정확도에 따라 환급을 구별하는 Rust 구현 | cache-before-admission, reservation/settlement | 이 조합 자체도 새 알고리즘이 아님. 기존 조사는 archived 상태를 기록했으므로 현재 유지보수 추천·최신 성능 경쟁자로 취급하지 않음 |
| **DeskQuota 현재** | 작은 Rust 실행 파일에 shared RPM/TPM, RR/FIFO, bounded stream·queue, exact cache, actual 정산, native setup/복원을 결합 | 각 핵심 기술에는 선행 구현이 있음 | 제한 조정의 효과를 설치부터 실제 workload까지 연결해야 독립 제품으로 설득 가능 |

[LiteLLM 공식 문서](https://docs.litellm.ai/docs/proxy/caching)에도 local memory cache가 있다. Redis 없이 캐시를 쓴다는 것만으로 독창성을 주장할 수 없다. [Bifrost exact cache](https://docs.getbifrost.ai/features/semantic-caching)는 embedding 없이 동작하는 모드가 있지만 저장소를 사용하는 구조다. 기능 이름만 같거나 다르다고 비교를 끝내지 않는다.

Otari·Helicone은 현재 README도 열람했지만 이번 8개 SHA 재조회 세트에는 포함하지 않았다. 표의 세부 구현 판단은 2026-09-12의 고정 소스에 한정한다. 구현 지원 여부를 최신 main 전체에 일반화하지 않는다.

## 나머지 참조 저장소의 역할

전체 참조를 같은 종류의 경쟁자로 취급하면 비교가 왜곡된다. 아래는 [기존 27개와 추가 3개](reference-catalog.md)의 나머지 역할이다. 과거 고정 소스의 분류이며 이번에 최신 구현을 재검증한 표가 아니다.

| 참조 | 참고할 독창성 또는 역할 | 비교 경계 |
|---|---|---|
| SMG | engine fleet의 admission·priority·KV-aware routing | 원격 engine 제어가 가능한 router와 단일 endpoint client proxy를 구분 |
| llama-swap | 모델 프로세스의 시작·교체·수명 관리 | 요청 quota 관리와 별도 역할 |
| Ollama·llama.cpp | 로컬 모델 실행·slot·동시성·세션 동작 | backend이자 연동 대상 |
| vLLM·SGLang | KV 관리, batching, token 단계 scheduling·prefix reuse | HTTP proxy가 token 선점·KV 재개를 그대로 구현할 수 없음 |
| LMCache | KV cache의 저장·공유·전송 | 완성 응답의 exact cache와 다른 층 |
| llm-d-router·Dynamo | 분산 inference routing·취소·소유권 | fleet telemetry와 engine 협력이 필요 |
| Blitz Router·LAPS·FastServe | 측정 기반 분산 routing, prefill/decode 배분, token 선점 등 논문 artifact | 논문·artifact의 baseline과 환경에 묶인 결과 |
| Pi·Codex·Hermes | agent 실행, client protocol, 작은 core와 onboarding | gateway 경쟁자가 아닌 사용자 도구·설계 참고 |
| any-llm | provider SDK 통합 | HTTP gateway는 별도 Otari |
| NVIDIA-req-limit-proxy·nvidia-api-rate-limiter | NVIDIA API 제한 대응의 좁은 사용자 문제 | provider 특화 수요 신호. 보편적 효율·시장 규모 증거는 아님 |
| bifrost-benchmarking | mock·측정 harness | 제품 자체가 아닌 재현 방법 참고 |

## 현재 우리의 독창성 판정

| 주장 후보 | 판정 | 근거·한계 |
|---|---|---|
| Rust여서 빠르고 가볍다 | **측정한 구성에서의 강점** | M4 합성 무대기 p95 0.433–0.556ms, idle RSS 9.56–9.66MiB. 더 작은 장비·Windows·최신 Rust 경쟁 경로로 확대 불가 |
| 에이전트를 CPU처럼 스케줄링한다 | **새 발상 아님** | HiveMind와 기존 fair-share gateway가 직접 선행. HTTP proxy에는 token 상태 보존 선점 권한이 없음 |
| exact cache와 actual 정산을 묶었다 | **유용한 기존 패턴** | LiteLLM·TensorZero 등과 중첩. 현재 cache 대상도 짧은 text 위주로 제한됨 |
| 긴 작업을 지키며 짧은 작업을 더 잘 처리한다 | **미입증 연구 가설** | Backfill의 평균 개선과 p95·처리량 동률, 다른 workload의 완료량 반례가 있음. RR 기본값 유지 |
| 설치·표준 인증·설정 복원·제한 관리가 작게 연결된다 | **제품 차별화 후보** | 일부 macOS client flow 확인. Windows public binary·실사용 효과·반복 사용은 미완성 |
| 다른 제품보다 같은 한도로 더 많은 일을 끝낸다 | **현재 일반화 불가** | 최근 quota fixture는 최종 완료가 늘었지만 첫 창 완료는 늘지 않았고 timeout도 있었음. 모델 답의 품질·agent 전체 작업 성공을 평가하지 않음 |

따라서 현재 상태는 **차별화 가능한 작은 제품 기반은 있으나, 독자 알고리즘과 실사용 우위는 아직 입증하지 못한 상태**다. 새로운 코드가 많다는 것과 새 기여가 있다는 것은 다르다.

## 세 가지 방향 중 선택

| 방향 | 기대 가치 | 비용·위험 | 판단 |
|---|---|---|---|
| **제한을 공유하는 요청의 경량 조정기** | quota 낭비·불필요한 재시도·대기를 줄이고 기존 도구 유지 | provider 제한의 의미, cache 재사용 범위, client timeout을 정확히 다뤄야 함 | **추천. 현재 코드와 목표에 가장 가까움** |
| 범용 provider 운영 gateway | 모델·인증·routing·관측을 한곳에 통합 | 넓은 provider·운영 기능 유지보수, 경쟁 제품과 큰 중첩 | 지금 우선순위 아님 |
| 작업을 이해하는 agent/model router | 작업 분류·모델 선택으로 비용·품질 최적화 | 분류 자체의 비용과 오류, 품질 평가·도구별 상태 통합 필요 | 별도 문제이며 현재 방향의 선행 조건이 아님 |

추천 방향의 차별화는 **사용자가 지정한 모델·요청 의미를 유지하면서, 제한 때문에 생기는 손실을 관찰하고 줄이는 것**이다. 추정기가 요청의 max_tokens를 몰래 낮추거나, 의미가 다른 요청을 비슷하다는 이유로 합쳐 효과를 만들지 않는다. 재사용은 사용자가 허용한 exact cache 계약 안에서 수행한다.

## 개선 순서와 채택 조건

1. **누구나 실행할 수 있게 만든다.** 기존 package/install 경로를 활용해 Windows·macOS·Linux의 받은 실행 파일→setup→client 연결→off→복원 시나리오를 검증한다. 특히 MSVC 없는 Windows에서 소스 빌드가 필요 없어야 한다. 코드 수정과 runtime 지원을 구분한다.
2. **기존 status에 손실 원인을 드러낸다.** 전체 ingress와 cache 대상 분모, bypass 이유, usage 확인 여부, 추정 오차, RPM/TPM/slot/cooldown 대기, 완료·거절·실패·취소·timeout, upstream 시도를 원문 저장 없이 집계한다. 큰 관측 DB보다 이미 있는 counter·집계 경로를 먼저 쓴다.
3. **측정된 낭비부터 한 가지씩 줄인다.** cache key에 재시도 횟수 같은 변동 header가 들어가는 현재 경로를 검토한다. 인증·tenant·요청 의미 분리는 유지한다. 동시 중복 합치기는 실제 동시 중복이 많을 때만 추진하며 leader 취소·follower 기한·느린 reader·메모리 제한을 함께 검증한다.
4. **사전 예약을 제공자 계약에 맞춘다.** 지금 actual 정산은 완료 뒤 보정이다. 추정량 때문에 입장을 놓치는 경우를 먼저 확인하고, 서버의 차감 단위·reset 의미·공유 범위가 확인된 신호를 사용한다. actual usage가 작아도 제공자가 max_tokens로 제한을 차감하면 그만큼 무조건 재사용할 수 없다. 로컬 inference의 slot·응답시간 신호와 API의 RPM/TPM 신호를 구분한다.
5. **대기열 후보를 독립적으로 검증한다.** 현재 RR와 단순 FIFO, 명시적인 pacing, 검토한 경쟁 설정을 기준선으로 둔다. 비용 불확실성·외부 소비자·긴/짧은 작업·버스트·취소·usage 누락·reset 위상 변화에서 deadline 내 완료와 긴 요청의 손해를 함께 판정한다. 클라이언트 deadline을 알 수 없으면 설정된 대기 한도를 명시하며 전체 작업 deadline을 아는 것처럼 행동하지 않는다.

3–5는 구체적인 설계·수용 기준을 정한 뒤 구현할 제안이다. 이번 조사로 제품 동작을 바꾸지 않았다.

## 독창성을 연구 기여로 발전시키려면

검증할 질문은 **‘비용과 외부 잔여 한도를 완전히 알 수 없는 HTTP 요청에서, 추가 예측 모델 없이 긴 요청의 기아를 억제하면서 기한 내 완료율을 개선할 수 있는가’**다. 이는 가설이며 독창성 확정이 아니다. cache나 cancellation까지 한 논문의 기여로 한꺼번에 묶지 말고 admission/scheduling의 한 가지 가설로 좁힌다.

ICML 2026의 [UniBoost](https://www.microsoft.com/en-us/research/publication/beyond-prediction-tail-aware-scheduling-for-llm-inference/)도 길이 변화와 tail 지연을 다룬다. 다만 engine의 cache-aware preemption까지 포함하므로 우리 HTTP 계층의 새로움을 대신 입증하지 않는다. [기존 원문 검토](recent-scheduling-evidence-2026-09-14.md)는 평균과 tail의 tradeoff도 기록한다. 추가 문헌 검토와 강한 기준선 비교가 필요하다.

주 지표는 고정된 관찰 시간·동일한 quota·동일한 요청 의미에서의 **기한 내 완료 비율**이다. 실패·취소·timeout을 분모에서 숨기지 않고, 대기·upstream 시간, 긴/짧은 요청별 p95/p99, root별 최대 대기, 실제 시도 수, CPU·메모리를 함께 제시한다. HTTP 완료는 agent 작업 성공·정답 품질과 구분한다. cache만 켠 대조군과 admission만 바꾼 대조군을 나눠 개선 원인을 분리한다.

첫 창을 넘겨 기다린 요청이 더 많이 끝난 사실만으로 효율 우위를 선언하지 않는다. 429를 queue timeout으로 바꾼 결과도 성공으로 세지 않는다. quota가 거의 없거나 cache 적중이 없는 조건의 비용도 공개한다. NVIDIA 등 실제 API는 smoke와 외부 타당성 확인에 사용하고, 강제로 재현하기 어려운 제한 조건의 인과 비교는 공개 mock으로 보완한다.

## 채택과 방어력

단기 방어력은 알고리즘 이름보다 **설치가 되는 배포물, 도구별 protocol 검증, 작은 상주 비용, 실제 사용자가 확인한 절약, 재현 가능한 실패 사례**에 있다. 공개 전후 데모는 같은 요청·같은 시간 제한으로 보여 주고 cache 덕분인 결과를 scheduler 성과로 표시하지 않는다. 공개 가능한 결과는 사용자의 원문 없이 집계하며 업로드는 별도 명시적 선택으로 둔다.

5,000 stars나 논문 채택을 예측할 근거는 없다. 먼저 서로 다른 제한 환경에서 반복 사용되고, ‘없으면 불편해서 다시 켜는 이유’가 일관되게 나오는지를 확인한다. 새 기능은 그 이유를 강화할 때만 추가한다.

## GitHub Star 전체 조회

**2026-09-15 16:32:31 KST** GitHub GraphQL의 `stargazerCount`를 직접 조회했다. 기존 참조 30개와 DeskQuota 1개다. 별점은 누적 관심의 수치이며 실제 사용자 수, 성능, 코드 품질, 독창성 점수가 아니다. 소수 Star의 구현도 선행 사례가 될 수 있고, 큰 Star의 제품도 우리 workload의 우위를 자동으로 입증하지 않는다. [전체 원본](../evidence/positioning-stars-2026-09-15.json).

| 저장소 | Star | 이번 비교에서의 역할 | Archived |
|---|---:|---|---|
| [tdamsma/overlaat](https://github.com/tdamsma/overlaat) | 0 | 직접 경쟁 | 아니오 |
| [cppalliance/promptforge](https://github.com/cppalliance/promptforge) | 1 | 인접 gateway·구성요소 | 아니오 |
| [thediymaker/obleth-gateway](https://github.com/thediymaker/obleth-gateway) | 12 | 인접 gateway·구성요소 | 아니오 |
| [BerriAI/litellm](https://github.com/BerriAI/litellm) | 58,755 | 인접 gateway·구성요소 | 아니오 |
| [maximhq/bifrost](https://github.com/maximhq/bifrost) | 8,079 | 인접 gateway·구성요소 | 아니오 |
| [smg-project/smg](https://github.com/smg-project/smg) | 530 | engine·router·lifecycle | 아니오 |
| [mostlygeek/llama-swap](https://github.com/mostlygeek/llama-swap) | 5,670 | engine·router·lifecycle | 아니오 |
| [ui-insight/MindRouter](https://github.com/ui-insight/MindRouter) | 19 | 인접 gateway·구성요소 | 아니오 |
| [earendil-works/pi](https://github.com/earendil-works/pi) | 105,271 | client·SDK | 아니오 |
| [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent) | 245,611 | client·SDK | 아니오 |
| [openai/codex](https://github.com/openai/codex) | 124,223 | client·SDK | 아니오 |
| [ollama/ollama](https://github.com/ollama/ollama) | 181,001 | engine·router·lifecycle | 아니오 |
| [ggml-org/llama.cpp](https://github.com/ggml-org/llama.cpp) | 128,274 | engine·router·lifecycle | 아니오 |
| [vllm-project/vllm](https://github.com/vllm-project/vllm) | 91,790 | engine·router·lifecycle | 아니오 |
| [sgl-project/sglang](https://github.com/sgl-project/sglang) | 35,974 | engine·router·lifecycle | 아니오 |
| [LMCache/LMCache](https://github.com/LMCache/LMCache) | 11,810 | engine·router·lifecycle | 아니오 |
| [tensorzero/tensorzero](https://github.com/tensorzero/tensorzero) | 11,719 | 역사적 gateway 구현 | 예 |
| [Portkey-AI/gateway](https://github.com/Portkey-AI/gateway) | 12,995 | 인접 gateway·구성요소 | 아니오 |
| [mozilla-ai/any-llm](https://github.com/mozilla-ai/any-llm) | 2,193 | client·SDK | 아니오 |
| [llm-d/llm-d-router](https://github.com/llm-d/llm-d-router) | 338 | engine·router·lifecycle | 아니오 |
| [Helicone/ai-gateway](https://github.com/Helicone/ai-gateway) | 628 | 인접 gateway·구성요소 | 아니오 |
| [mozilla-ai/otari](https://github.com/mozilla-ai/otari) | 461 | 인접 gateway·구성요소 | 아니오 |
| [blitz-serving/blitz-router](https://github.com/blitz-serving/blitz-router) | 15 | 논문 artifact | 아니오 |
| [Jianshu-She/LAPS](https://github.com/Jianshu-She/LAPS) | 4 | 논문 artifact | 아니오 |
| [LLMServe/FastServe](https://github.com/LLMServe/FastServe) | 30 | 논문 artifact | 아니오 |
| [ai-dynamo/dynamo](https://github.com/ai-dynamo/dynamo) | 8,082 | engine·router·lifecycle | 아니오 |
| [maximhq/bifrost-benchmarking](https://github.com/maximhq/bifrost-benchmarking) | 10 | 측정 harness | 아니오 |
| [jayluxferro/hivemind](https://github.com/jayluxferro/hivemind) | 3 | 직접 경쟁 | 아니오 |
| [Tiger-sudo894/NVIDIA-req-limit-proxy](https://github.com/Tiger-sudo894/NVIDIA-req-limit-proxy) | 0 | provider 특화 proxy | 아니오 |
| [imviren/nvidia-api-rate-limiter](https://github.com/imviren/nvidia-api-rate-limiter) | 0 | provider 특화 proxy | 아니오 |
| [ryuwon-ai/deskquota](https://github.com/ryuwon-ai/deskquota) | 0 | 현재 제품 | 아니오 |

**해석:** 직접 경쟁인 HiveMind 3·Overlaat 0, 인접 fair-share 제품인 obleth 12·MindRouter 19는 큰 수요가 검증됐다는 근거가 아니다. LiteLLM 58,755·Portkey 12,995·Bifrost 8,079의 관심을 좁은 quota 조정 시장의 규모로 환산할 수도 없다. [llama-swap](https://github.com/mostlygeek/llama-swap)은 요청에 맞춘 모델 교체라는 선명한 역할과 단일 binary 경험을 제시하며 5,670 Star를 기록한다. 작은 핵심 역할을 제품으로 전달하는 참고 사례지만, 단순함이 Star 수의 원인이라고 입증한 분석은 아니다. TensorZero의 archived=true는 이번 API 조회에서도 확인했다.
