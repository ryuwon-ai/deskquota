# 경쟁 저장소의 장점을 DeskQuota에 적용하는 방법

2026-09-15. 대상은 `develop`의 `0121f7c6f2118a5e2a64bcdc7edbd92fab493c36` 제품 소스다. 스케줄링, 제공자 quota, 경량 제품·검증 담당 세 에이전트가 독립 검토 후 서로의 제안과 반례를 받아 2차 토론했다. 부모 에이전트는 실제 코드 경로를 확인하고 기존 실험을 재집계했다.

**결정 수준:** 다음 실험과 제품 개선에 대한 권고다. 이 문서로 새 알고리즘을 기본값에 적용하거나 새 성능을 입증한 것은 아니다. 이번에 제품 코드·설정은 바꾸지 않았고 빌드·LLM API 호출도 하지 않았다.

## 무엇을 가져올 것인가

| 참고 대상 | 배울 장점 | DeskQuota에 맞춘 최소 적용 | 현재 결정 |
|---|---|---|---|
| [Bifrost 운영 진단](https://docs.getbifrost.ai/providers/performance) | 큐·제한·자원 상태를 사용자가 이해하게 한다 | 이미 있는 `AdmissionStatus`의 대기 사유·한도·차감·예약량을 일반 `status` 출력에 표시 | 작은 제품 개선으로 권고. 속도 개선과 별도 |
| [PromptForge 모델 목록](https://github.com/cppalliance/promptforge/blob/da1456b545f655f3743c4b76b099689fc70aa7b4/crates/gateway/src/lib.rs#L938-L977) | 모델 선택이 생성 요청의 병목을 그대로 받지 않게 한다 | 설정 목록을 대신 응답하기보다, 실제 metadata 요청의 의미를 보존하여 기존 backfill에서 안전하게 취급하는 실험 | 첫 runtime 변경 후보. PromptForge 코드 이식이 아닌 목적을 재해석한 설계 |
| [LiteLLM의 비용 추정·예약 구분](https://github.com/BerriAI/litellm/blob/9a715df212d777bbd43f4cab05731978c708ec63/litellm/proxy/hooks/parallel_request_limiter_v3.py) | 입력·출력 추정과 예약·정산을 구분한다 | 실제 endpoint 하나의 입장 제한 계산에 맞춰 과도한 예약을 줄인다 | 생성 효율의 핵심 후속 가설. 계약 확인 전 제품화 보류 |
| [HiveMind의 성공 응답 헤더 관측](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/src/hivemind/scheduler/rate_limiter.py#L372-L443) | 429가 발생하기 전에도 제한 상태를 읽는다 | 한도 공유 범위와 시간 의미가 확정된 신호만 보수적인 제어에 사용한다 | 외부 소비자가 있는 fixture부터 검증. 범용 자동 제어 보류 |
| [Dynamo의 실행 가능성 우선 정책](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/kv-router/src/scheduling/policy_queue.rs#L612-L708) | 실행 가능한 요청에 공정성 정책을 적용한다 | 기존 backfill에서 안전하게 비는 구간만 활용한다 | DRR 전체 이식·후속 expiry 탐색 확대는 근거 부족으로 보류 |
| [Pi의 배포 검증](https://github.com/earendil-works/pi/blob/f3c672245d25ef2283ffc0d9cdec8a5482651103/README.md#supply-chain-hardening), [PromptForge 배포물](https://github.com/cppalliance/promptforge/blob/da1456b545f655f3743c4b76b099689fc70aa7b4/README.md#downloads) | 사용자가 소스 빌드 없이 설치하고 첫 작업을 수행한다 | 기존 패키저·설치 스크립트를 사용해 검증한 OS부터 배포한다 | 채택·사용성 과제. 릴리스 승인은 별도이며 Windows 실행 검증 필요 |

공동 quota 예약, root별 RR·FIFO, 긴 요청 보호, 공유 cooldown, 연결 재사용, 제한된 stream 버퍼, 취소 소유권, 실제 usage 정산은 이미 있다. 경쟁 저장소에서 새로 들여오는 기능으로 중복 계산하지 않는다. 새 DB·서비스·tokenizer 다운로드·LLM 분류기는 위 첫 실험에 필요하지 않다.

## 토론에서 실제로 바뀐 판단

- **스케줄링 담당:** 처음에는 로컬 모델 목록과 후속 expiry 탐색을 추천했다. 반론을 받은 뒤 upstream 목록의 최신성·권한 응답을 바꾸는 비용과 최대 8,192개 장부의 반복 탐색 비용을 인정해 두 항목을 보류했다. 제공자 계약 개선을 우선하고 metadata 의미 보존도 수용했다.
- **제공자 담당:** 처음에는 비용 추정과 성공 응답 헤더 제어를 추천했다. 실제 제한 계약이 없으면 합성 fixture에만 맞는 구현이 될 수 있다고 수정했다. 기존 상태 노출과 설정 기반 모델 목록을 제품 완성도 측면에서 우선했다.
- **경량·검증 담당:** 진단과 배포를 성능 개선으로 대체하면 안 된다고 지적했다. 로컬 목록이 현재의 상위 서버 응답 계약을 바꾸는 점에 반대하고, 실제 metadata의 의미를 요청부터 장부·정산까지 유지하는 좁은 실험을 추천했다.
- **재집계 후 검증 담당의 수정:** 기존 backfill에서 짧은 생성 지연 개선이 확인되었으므로, 새 알고리즘보다 기존 정책의 독립 workload 검증을 먼저 하자고 변경했다. typed metadata는 그 다음 작은 변경 후보로 유지했다.
- **최종 판단:** 기존 backfill의 적용 조건을 독립 workload에서 검증하는 것이 우선이다. 그 결과를 보고 기본값 승격 여부를 판단한다. 상태 출력 개선은 별도 작은 제품 과제, typed metadata는 첫 runtime 변경 후보로 둔다. 제공자 예약 개선은 한 계약이 확인된 뒤 수행한다. 로컬 catalog의 기본 도입은 보류한다. 제공자 담당의 catalog 우선 의견은 남아 있으며 전원 합의로 표현하지 않는다.

## 직접 재집계: 개선이 모델 목록에서만 나온 것인가

토론 중 나온 반증 제안을 실제 수행했다. [재집계 스크립트](../scripts/analyze-backfill-request-classes.py)는 기존 manifest의 원시 run SHA-256을 확인하고, 두 arm의 입력 목록·모든 terminal ID·outcome 일치를 검사한 뒤 저장된 `metadata` 필드로 분리한다. [전체 결과](../evidence/backfill-request-classes-2026-09-15.json)를 보존했다.

**기존 동시성 2, 60초, 5쌍의 RR 대 실험 backfill 결과를 재분류한 값이다.** 새 workload나 실제 모델 실험이 아니다.

| 분류 | arm별 전체 요청 / 최종 결과 | 성공 응답 평균 RR → backfill | 성공 응답 p95 RR → backfill | 60초 창 안 완료 RR → backfill |
|---|---|---|---|---|
| 짧은 생성 | 70 / 성공 65·취소 5 | 9.751 → 6.986초, 28.36% 감소 | 47.025 → 35.055초 | 50 → 50 |
| 긴 생성 | 20 / 성공 10·취소 10 | 30.254 → 30.255초 | 60.209 → 60.209초 | 5 → 5 |
| 모델 목록 | 10 / 성공 10 | 47.019 → 47.014초 | 47.027 → 47.022초 | 0 → 0 |
| 생성 전체 | 90 / 성공 75·취소 15 | 12.485 → 10.088초 | 60.205 → 60.207초 | 55 → 55 |

각 쌍의 upstream 시도는 17 대 17이며, 모든 ID의 최종 outcome도 같다. 생성 전체의 시간 내 완료는 55 대 55다. 동시성 1의 **별도 1쌍**에서는 짧은 생성 평균이 9.866 → 9.867초여서 개선이 나타나지 않았다.

검증 담당은 부모의 분석 스크립트를 재사용하지 않고 12개 원시 run을 독립 재집계해 `PASS_WITH_LIMITS`로 판정했다. 입력·terminal 집합과 해시, 정책·바이너리·재시도 설정, metadata와 실제 `/models` 경로의 연결, elapsed 계산·측정창 포함 여부, 평균·p95를 확인했다. 이는 현재 보존된 manifest와 기록의 내부 일치 검증이며 외부 인증이나 새 실험을 의미하지 않는다.

이 결과는 “평균 개선이 모델 목록을 빠르게 처리해서만 생겼다”는 의심과 맞지 않는다. 오히려 모델 목록의 긴 대기가 기존 short 그룹 전체의 p95를 가리고 있었다. 다만 다음 한계를 함께 표시해야 한다.

- 사후 분류한 기술 통계이며 독립 holdout이 아니다. 다섯 seed는 같은 요청 구성에 도착 jitter를 바꾼 반복이다.
- 표의 p95는 합친 성공 표본의 nearest-rank다. 개별 요청을 독립 실험 반복으로 취급하거나 신뢰구간을 주장하지 않는다. 각 run의 짧은 생성 성공 표본은 13개뿐이다.
- 성공 latency에는 측정 창 이후 drain이 포함된다. 실패·취소를 숨기지 않고 별도 분모로 함께 표시했다.
- 고정 시간 안의 완료 수는 늘지 않았다. 실제 모델의 생성 속도·정답 작업 완료량·경쟁 gateway 우위·저사양 우위를 증명하지 않는다.
- backfill은 여전히 `bench-harness` 실험 경로이며 기본 제품은 RR이다. README의 기존 전체 short 통계가 틀린 것은 아니고 분류 기준이 다르다.

## 먼저 할 검증과 첫 runtime 변경 후보

기존 backfill은 이미 작동하므로 새 정책을 덧붙이기 전에 metadata 없는 생성 혼합, 다른 도착 순서·긴 stream, 동시성 1/2, RPM/TPM 병목을 분리한 독립 workload에서 현재 RR과 비교한다. 이전에 남은 120초 5쌍 반복과 장부 포화 비용 측정도 별개로 유지한다. 기존 workload를 재집계한 이번 결과가 이 검증을 대신하지 않는다.

독립 검증에서 이득이 유지되지 않으면 해당 조건의 기본 정책은 단순한 RR로 둔다. 이득과 안전성이 유지될 때만 검증된 범위의 backfill 기본값 승격을 설계한다. 이후 남는 모델 조회 지연에 다음의 작은 변경을 검토한다.

**가설:** 생성하지 않는 요청이라는 사실을 장부까지 보존하면, 보호된 긴 요청을 늦추지 않으면서 일부 metadata 대기를 줄일 수 있다.

현재 `product/src/protocol/request.rs:15–48`은 Models·CountTokens와 TPM unknown/unlimited 생성 요청에 같은 `RequestCost::Metadata`를 사용한다. `product/src/admission/quota.rs`의 `Entry`는 종류를 버리고 숫자만 보존한다. 그래서 “0이면 안전”이라고 검사 조건을 지우는 변경은 위험하다. 일반적인 0 예약도 늦게 양의 usage를 받아 새 차감을 만들 수 있다.

최소 변경 후보는 다음 흐름이다.

1. `protocol/request.rs`에서 실제 metadata와 비용을 추정하지 않은 generation을 구분한다.
2. `admission/quota.rs`의 장부·backfill 검사에 이 구분을 필요한 만큼만 보존한다. candidate 검사뿐 아니라 이미 active인 항목 검사도 같은 의미를 사용한다.
3. `admission/mod.rs`의 기존 endpoint별 정산과 late-positive 반례를 연결해 확인한다. Models·CountTokens의 현재 usage 처리와 일반적인 zero fixture를 혼동하지 않는다.
4. 기존 `tests/backfill_trace.rs`에 다른 root·같은 root, 긴 metadata, queued/started 취소, RPM 부족, cap 1, 늦은 usage 반례를 보태고 동일 HTTP workload에서 비교한다.

RPM, 실행 슬롯, 인증, 상위 서버의 응답, FIFO, 긴 요청 보호를 면제하지 않는다. 따라서 cap 1·RPM 소진·같은 root의 앞선 생성 요청에서는 대기가 남을 수 있다. 모든 47초 사례가 해결된다고 예고하지 않는다. CountTokens가 무거울 수 있다는 점도 슬롯 보호에 포함한다.

**합격:** 상위 시도와 응답 의미를 유지하며 metadata 대기를 줄이고, 보호된 head의 기존 입장 기회·긴 요청 완료/최대 대기·quota 정확성을 악화시키지 않는다. 생성 성능은 별도로 측정한다. 이득 없이 CPU·메모리·코드 복잡도만 늘면 폐기한다. 기본값 승격은 기존에 남아 있는 독립 workload·긴 시간창 반복·가득 찬 장부 비용 검증 이후 판단한다.

## 생성 효율의 다음 후보: 실제 한도에 맞는 예약

현재 `request.rs:92–99`는 JSON 전체 바이트와 출력 상한을 더한다. [기존 기록](scheduler-utilization-followup.md)의 3,222 예약 대 806 실제 사용은 합성 fixture 값이지만, 입장 전 과대 예약이 병목이 될 수 있는 구체적인 사례다. 완료 후 `accounting=actual` 정산은 아직 입장하지 못한 요청의 미래 사용량을 알려주지 않는다.

먼저 실제 endpoint 하나에 대해 RPM/입력/출력/캐시의 한도 공유 범위, admission 예약 방식, 정산·반환 조건, 시간창·재충전 규칙을 확정한다. [Azure 문서](https://learn.microsoft.com/en-us/azure/foundry-classic/openai/how-to/quota?view=foundry-classic)는 제한 산정 estimate와 과금 usage가 다를 수 있음을 설명한다. [Claude 문서](https://platform.claude.com/docs/en/api/rate-limits)는 입력·출력 제한과 캐시 처리, 반올림된 잔량과 제한 범위를 구분한다. 임의 `chars/4`나 실측 비율을 모든 endpoint에 적용하면 안 된다.

그 한 계약으로 **예약 계산만 바꾸는 비교**를 먼저 한다. 출력 상한을 임의로 줄이거나 응답 품질·도구 결과를 바꾸지 않는다. 성공 헤더 제어는 그 다음 독립 실험이다. 특히 reset이 완전히 재충전되는 시각인 제공자에서는 reset까지 무조건 정지하면 오히려 불필요한 대기가 늘 수 있다. 늦게 도착한 헤더, 서로 다른 모델 bucket, 다른 PC의 소비도 함께 검사한다.

## 경쟁 우위의 판정

[기존 benchmark 명세](benchmark-spec.md)를 따른다. 같은 입력·모델·정답 조건·quota·전체 retry 예산·deadline에서 직접 연결, 적절히 설정한 경쟁 gateway 하나, 현재 DeskQuota, 후보를 비교한다. 정책만 바꾸는 공통 transport 실험과 완제품 비교는 분리한다. 경쟁 소스 검토의 고정 SHA를 안정 배포판 성능 검증으로 간주하지 않는다.

주요 지표는 고정 시간 내 유효 작업 완료, 생성만의 p95, 긴 작업 최대 대기·완료율, root별 결과, 모든 ingress의 실패·취소·timeout, upstream 시도·토큰, CPU/RSS다. 오류를 대기로 바꾸거나 어려운 요청을 버려 얻은 수치는 합격이 아니다. 과거 direct 328 대 RR 285는 동시성 조건이 달라 알고리즘 단독 비교로 쓸 수 없다.

현재 보류할 것은 범용 semantic cache, 자동 모델 라우팅, 에이전트 DAG 추정, 전체 DRR/MLFQ 이식, 웹 대시보드와 분산 장부다. 실제 중복률·작업 의존성·엔진 선점 API가 확인되기 전에는 기능과 정확성 비용이 먼저 생긴다. 원격 추론을 CPU처럼 중단·재개하는 기능은 일반 HTTP gateway만으로 제공할 수 없다.

현재 차별화 가설은 **작은 네이티브 실행 파일 안에서, 제한된 quota를 여러 coding agent가 공유할 때 불필요한 대기와 재시도를 줄이되 긴 작업을 희생하지 않는 것**이다. 기능 개수, 경쟁사 발표 수치, 이 문서 자체는 사용자 채택·논문 수준·5,000 stars의 증거가 아니다.
