# 중복 호출·응답 보류 개선과 경쟁 패턴 재검토

2026-09-16. 두 변경의 구현과 macOS·Windows 검증 완료. 아래 성능 수치는 macOS의 합성 HTTP 전후 비교이며, 경쟁 제품의 새 순위나 실제 API 성능을 뜻하지 않는다.

## 모토에 따른 선택

**Get more done within your LLM limits.** 같은 작업의 불필요한 upstream 호출과 대기를 줄이고, 기존 도구·응답 의미·설치 편의성을 유지한다. 제한된 사내 API뿐 아니라 외부 API와 로컬 모델에도 적용하되, 한도를 없애거나 모델의 생성 속도가 빨라졌다고 표현하지 않는다.

이번 범위는 두 가지다. 기존 exact cache가 준비됐는데도 기다리는 요청을 깨우고, 분류 결과가 필요 없는 429 응답의 헤더를 EOF까지 붙잡지 않도록 한다. 새 provider registry, 자동 모델 변경, dashboard, 외부 저장소, 토크나이저, 설정 옵션은 추가하지 않는다.

## 1. 완료된 캐시가 초기 대기 요청을 깨움

변경 전에는 요청 도착 때 한 번 캐시를 확인하고, miss면 quota와 동시성 입장을 기다렸다. 그동안 동일한 응답이 저장돼도 다시 읽지 않았다. 입장 뒤 한 번 재조회하는 방식도 RPM=1을 소진한 경우에는 여전히 60초 만료까지 기다린다.

변경 후에는 완료된 캐시 저장 알림을 기존 admission future와 함께 기다린다. 같은 키의 응답이 저장되면 대기 중인 요청은 그 응답을 반환하고, 기존 Ticket/미시작 Hold 정리가 큐와 예약을 회수한다. 관계없는 캐시 알림은 같은 키를 다시 확인할 뿐 FIFO 순서·aging·deadline을 초기화하지 않는다. 입장 성공 직후에도 한 번 확인한다.

- 원본 body와 usage는 그대로 재사용한다. upstream RPM/TPM·시도·worker 사용량은 다시 세지 않는다.
- 처음 miss였던 요청이 나중에 hit가 되면 한 번만 재분류한다. 알림 횟수를 miss 횟수로 세지 않는다.
- 기존 root·URL·인증·본문 격리, history 제한, no-store/Vary, tools/stateful 제외, TTL·4MiB payload 예산을 유지한다.
- 완전한 응답과 실제 HTTP EOF 뒤에만 저장한다. 부분 SSE, 취소된 leader, 오류 응답을 공유하지 않는다.
- 이미 발송된 중복 요청과 내부 retry 대기는 이번 변경 대상이 아니다. 동시성3에서 최초3건이 이미 발송됐다면 3건은 남는다. 모든 burst를 하나의 생성으로 합치는 singleflight가 아니다.

## 2. 필요 없는 429 본문 선행 읽기를 생략

변경 전 normal release에 직접 접속해 upstream이 헤더와 첫 바이트를 보낸 뒤 본문을 gate로 막았다. JSON+retry off+timing 있음, text/plain+retry off/on의 세 경우 모두 gateway는 body gate 해제 전까지 downstream 헤더를 보내지 않았다. 본문 분류가 필요 없는 경로에서 생기는 지연이었다.

변경은 기존 timing header의 shared cooldown을 먼저 적용하고, 본문이 기존 JSON 분류 대상이며 실제 결정에 필요할 때만 선행 probe를 수행하는 것이다. 타이밍 헤더가 없는 transient 판정 또는 아직 가능한 내부 retry 결정에 필요한 본문은 계속 읽는다. JSON 판정 helper는 기존 함수를 공유하고, 기존 strict lowercase identity probe guard도 유지해 retry 범위를 넓히지 않는다.

이 개선은 **오류를 더 빨리 전달하는 것**이다. 실패한 LLM 작업을 성공으로 바꾸거나 성공 p95를 줄인 결과로 계산하지 않는다. 먼저 429 헤더를 전달한 뒤 본문이 끊기면 원본429와 body stream error가 보인다. 계속 probe하는 경로는 기존 pre-head502 동작을 유지한다.

## 3. 실행 결과

### 대기 중 캐시 재사용: 8조건 × 전후5쌍

Apple M4·32GiB RAM에서 기본 `utf8_bytes` 추정과 정상 release를 사용했다. 같은 조건 안에서는 요청·한도·모의 응답·캐시 설정을 고정하고 두 버전의 실행 순서를 교차했다. 실행마다 요청10개를 넣고, 첫 실행 요청과 나머지 실제 큐 진입을 관측한 뒤 응답 gate를 열었다. 모의 생성 지연은100ms이며 SSE에는 추가5ms chunk 간격이 있다.

아래 시간은 **gate 해제부터 완료까지의 성공 p95, 실행5회의 중앙값**이다. 요청당 표본10개라 각 실행의 p95와 p99는 최댓값과 같다. 전체 입력 대비 완료·실패를 함께 보고하며, RPM1의 이전 버전은 성공1건만 남으므로 성공 p95를 비교하지 않는다.

| 실행당 조건 | upstream 호출: 이전 → 개선 | 완료/제출: 이전 → 개선 | p95: 이전 → 개선 |
|---|---:|---:|---:|
| 동일 JSON, 동시성1 | 10 → 1 | 10/10 → 10/10 | 1,028.752 → 103.659ms |
| 동일 SSE, 동시성1 | 10 → 1 | 10/10 → 10/10 | 1,092.555 → 110.069ms |
| 동일 JSON, 동시성3 | 10 → 3 | 10/10 → 10/10 | 411.551 → 103.589ms |
| 동일 SSE, 동시성3 | 10 → 3 | 10/10 → 10/10 | 437.315 → 110.039ms |
| 동일 JSON, RPM1·동시성1 | 1 → 1 | 1/10 → 10/10 | 성공 분모가 달라 비교 제외 |
| 동일 SSE, RPM1·동시성1 | 1 → 1 | 1/10 → 10/10 | 성공 분모가 달라 비교 제외 |
| 다른 JSON, 동시성1 | 10 → 10 | 10/10 → 10/10 | 1,029.306 → 1,029.691ms |
| 캐시 OFF, 동일 JSON·동시성1 | 10 → 10 | 10/10 → 10/10 | 1,027.676 → 1,030.048ms |

동일 JSON·동시성1의 p95 범위는 이전1,027.826–1,028.961ms, 개선102.285–103.821ms다. gate 이전 준비 시간까지 포함한 **요청 제출부터 완료까지의 p95 중앙값은1,040.499→113.593ms**였다. RPM1은 요청 제출 후750ms의 클라이언트 마감으로 검사했다. 개선 버전의 전체10건이 매번 마감 안에 완료됐고, 이전 버전의 나머지9건은 client timeout으로 취소됐다. 이는 gateway deadline이나 upstream 거절이 아니다.

전체80회·800개 입력 중 이전 버전은310완료+90timeout, 개선 버전은400완료+0timeout이었다. timeout은 모두 이전 버전의 두 RPM1 조건에서 나왔다. 모든 성공 응답의 원본 body·usage와 각 실행의 캐시 집계 분모, upstream 시도·RPM/TPM 차감, 종료 후 큐·active·held=0을 확인했다. 큐에 남은 중복만 재사용하므로 동시성3에서는 이미 발송한3건이 남는다. 의도적으로 만든 중복 비율을 실제 hit rate로 해석하지 않는다.

### 대기가 없는 요청과 자원 비용

캐시 OFF·기본 바이트 모드의 순차 요청100개를 전후 각각5회 실행했다. 각 버전500/500측정 요청과 별도25warmup이 모두 완료됐고 원본 응답이 일치했다. 128KiB 입력도 포함한다.

| 항목 | 이전 | 개선 |
|---|---:|---:|
| 실행별 p95 범위 | 0.312–0.362ms | 0.324–0.375ms |
| 실행별 p95 중앙값 | 0.345ms | 0.359ms |
| 실행별 p99 범위 | 0.342–0.406ms | 0.353–0.456ms |
| 유휴 RSS 범위 | 9.875–9.891MiB | 9.938–10.016MiB |
| macOS 정상 release 크기 | 59,869,408bytes | 59,921,008bytes |

**일반 경로가 무조건 빨라진 것은 아니다.** 무대기 p95 중앙값은 약0.014ms 늘었으며, 이 반복만으로 통계적 차이나 무회귀를 주장하지 않는다. 실행 파일 증가는51,600bytes이고 의존성·설정 추가는 없다. 약59.92MB의 대부분 증가는 앞선 BPE 사전 도입 때 생겼다. 이번 queued matrix의 실행 전·gate·종료 표본 중 RSS 최댓값은 이전11.844MiB·개선11.563MiB였지만, 연속 측정한 peak는 아니다.

기존 순차 캐시 경로도 전후5쌍을 확인했다. 총1,800건은 gateway1,200건과 direct600건이며 모두 완료됐다. 각 버전의 gateway600건 중300건이 hit였다. JSON hit p95의 실행별 중앙값은0.116→0.097ms, SSE hit는0.380→0.432ms로 방향이 다르다. 개선의 근거는 **대기 중 중복 생성의 제거**이며 모든 캐시 경로의 속도 향상이 아니다. SSE miss에는 모의 chunk 간격이 남아 있어 순수 gateway 오버헤드로 해석하지 않는다.

### 429 전달·네이티브·자동 요약

- **429 gate 검사:** macOS와 Windows에서 각각6/6통과. 직접 연결 대조1개와 분류 불필요 gateway3개는 본문 gate가 닫혀 있어도 헤더를 전달했고, 분류가 필요한2개는 계속 기다렸다. 모든 사례의 원본429/body·시도1회와 gateway5사례의 shared cooldown을 유지했다. 순서 검증이지 성공 작업의 지연 측정이 아니다.
- **macOS:** Rust1.88 전체 target 검사453통과·0실패·명시적 profiling1제외. fmt·Clippy `-D warnings`·정상 release 빌드 통과.
- **Windows10 GNU:** cache/retry/fairness/stream/wire/library의185통과·0실패·권한이 필요한 reparse 검사1제외. 114개 입력 파일 해시를 검증하고 실제 재컴파일·정상 release를 확인했다. 실행 파일91,152,006bytes. Rust/MinGW가 없는 PATH에서도 loopback probe가 동작했다. Docker·WSL·전역 설정 변경은 없다. Windows 전체 suite·성능 비교·클라이언트 자동 요약은 이번에 다시 실행하지 않았다.
- **자동 요약:** macOS의 Pi0.84.2·Codex0.154.0·Claude Code2.1.63을 direct/gateway 양쪽에서 실행해 자동 요약 후 대화가 계속됨을 확인했다. 최종 바이너리·cache ON·기본 바이트 모드다. 모의120,000 input usage로 발동한 검사이며 실제12만 토큰 문맥의 요약 품질 검사가 아니다. 캐시 hit에서도 원본120,000 usage를 유지했다.

자동 요약의 기존 지원 한계도 재현했다. `/responses/compact`는404이고, TPM을 지정한 상태의 opaque compaction 항목은400이다. 바이트 추정량이 예산을 넘는 긴 텍스트 요약도400이며 TPM미지정 대조는 통과했다. 따라서9개 최상위 probe가 모두 통과했다는 것은 **지원 경로와 알려진 미지원 동작이 예상대로 유지됐음**을 뜻하며, 모든 compact API를 지원한다는 뜻이 아니다.

**이전18건·38초의 한도 하한은 이번 변경으로 없애지 않았다.** 서로 다른18건을 동일한 strict quota 안에서 모두 생성해야 하는 조건과, 완료된 응답을 재사용해 필요한 생성 횟수를 줄이는 조건은 다르다. 실제 제공자·저사양·장시간 혼합 과제에서의 완료 시간 우위는 아직 미확인이다.

재현 스크립트와 원시 기록은 `evidence/queued-cache-2026-09-16/`에 보존했다. 핵심 파일은 `measurements/queued-matrix.json`, `result-audit.json`, `validation/results.json`, `compaction-final.json`, `windows/final-validation.json`이다. Windows의 초기 실행기는 Unix import·cp949·경로 표현 문제를 만나 검증 어댑터만 수정했으며, 그 실패 기록도 남겼다. 제품·원본429 probe는 변경하지 않았다.

## 4. 여러 에이전트의 토론과 결론

세 감사 에이전트가 각각 cache/수명, quota/retry, 경량성/클라이언트를 조사했고 서로 반론을 주고받았다. 별도 구현·벤치마크 에이전트의 결과는 독립 spec/quality 검토와 부모의 실행 검산을 거쳤다.

| 논점 | 제안·반론 | 결정 |
|---|---|---|
| 캐시를 입장 후 재조회 | 가장 작은 변경이지만 RPM 만료 전에 실행되지 못함 | 완료 알림을 초기 admission과 함께 기다림 |
| 중복 요청을 모두 합치기 | 추가 절약 가능. leader 실패·취소·SSE follower 수명과 버퍼가 커짐 | 현재 큐의 재사용부터 적용; 이미 발송된 요청은 유지 |
| retry off면 429 probe 전체 제거 | timing 없는 transient body로 fallback cooldown을 정하는 기존 동작이 사라짐 | 실제 판정이 불필요한 경우만 생략 |
| media/encoding 판정 통합 | 기존 classifier와 probe의 identity 대소문자 조건이 달라 replay 범위가 넓어질 수 있음 | 기존 probe guard를 보존하고 추가 제외 조건만 공유 |
| BPE 조기 종료 API | 설치된 API가 있으나 내부 `limit+10` heuristic에 최악 경계 미확정 TODO가 있음 | 정확 수용/거절 동치 근거가 없어 채택 보류 |
| 응답 copy를 slice로 변경 | 작은 slice가 큰 backing allocation을 보유하면 byte budget 회계가 달라짐 | 현재 소유권을 보존; zero-copy라는 이름만으로 변경하지 않음 |
| 별도 key별 알림 registry | 관계없는 알림을 줄이지만 등록·삭제·수명이 늘어남 | 현재 제한된 대기 요청에 Notify 하나; 부하가 측정되면 재검토 |

## 5. 경쟁 저장소에서 가져온 부분

| 대상·근거 | 장점 | 이번 적용과 한계 |
|---|---|---|
| LiteLLM 설치1.100.1의 `cache_coordinator.py` | event 대기 후 재조회, loader의 double-check | 주기적 polling 없이 기존 캐시 재조회. 해당 소스는 범용 config/spend 리소스용이며 LLM response 전체의 병합 성능 증거가 아님 |
| Bifrost v2.1.1 `c193745`의 `framework/lrucache` | 키별 in-flight fill, follower 취소, 실패시 대기자 해제 | 완료·취소 소유권 원칙 채택. full singleflight 구현 전체를 가져오지는 않음 |
| HiveMind `0468db5`의 response-header 관측 | 서버가 알리는 대기를 다른 호출과 공유 | 기존429 timing을 유지.503의 범위와 정책은 후속 검증 |
| Overlaat `4e3cd0f`의 pool/breaker | 독립 자원의 격리와 장애 범위 제한 | 향후 quota 그룹과503 범위를 설계할 근거. 실행 중 자원 budget을 RPM 장부와 동일시하지 않음 |
| Pi `f3c6722`의 작은 core와 usage 기반 compaction | 필요한 핵심에 집중하고 기존 context 계산 유지 | gateway가 도구 실행·요약을 다시 구현하지 않으며 usage bytes를 보존 |

캐시 자체는 독창적 기능이 아니다. LiteLLM도 local 메모리 캐시를 제공하므로 “경쟁 제품은 Redis 필수”라는 비교는 틀리다. [현재 LiteLLM 캐시 문서](https://docs.litellm.ai/docs/proxy/caching). 우리의 차별화 후보는 **설치가 가벼운 단일 프로세스에서 한도·공정한 대기·정확한 재사용·취소를 함께 처리하는 완성도**다. 이번 실험은 우리 전후 비교이며 경쟁 제품의 새 성능 순위가 아니다.

## 6. 다음 개선의 우선순위

| 순서 | 근거와 기대 이득 | 먼저 확인할 것·트레이드오프 |
|---|---|---|
| 1 | 실제 제공자 한 곳의 명시적인 quota 계약: 서버가 이미 허용할 요청을 로컬이 막는 대기 줄이기 | rolling window/token bucket, ITPM/OTPM, max_tokens 예약·actual 정산, 외부 소비자를 구분. 원본 usage는 할인해 전달하지 않음 |
| 2 | BPE 패키징 크기: 기본 bytes 사용자도 현재59.92MB macOS 실행파일 부담 | 이전10.19MB와의 차이는 대부분 사전 도입 비용. 빌드 feature가 실제로 줄이는 크기와 설치 variant 복잡성을 함께 실측. 조용한 fallback/런타임 다운로드는 추가하지 않음 |
| 3 | 명시503 Retry-After 공유 | 이번 소스는429만 공유한다. 특정 모델/전체 서비스 범위를 확인하고 자동재전송과 분리. 회복이 빨라지면 과도하게 기다릴 수 있음 |
| 4 | `x-should-retry: false` 존중 | 공식 SDK와 다른 source 기반 후보. 실행 반례부터 확인; `true`로 기존 whitelist를 덮어쓰지 않음 |
| 조건부 | 독립 quota 그룹과 full singleflight | 한도 공유 관계 또는 이미 동시 발송된 중복 낭비를 실제 workload로 확인한 뒤 적용. 추가 설정·소유권의 비용을 함께 평가 |

현재 캐시 적격성은 짧은 순수 텍스트 재실행에 맞춰져 있다. 매번 대화가 변하거나 도구를 포함하는 agent 호출에는 이번 재사용 이득이 적용되지 않을 수 있다. 다양한 실제 과제의 완료 시간, 저사양 장비, 실제 provider에서의 우위는 별도의 측정이 필요하다.

## 근거

- [cache 감사와 구현 명세 검토](agent-cache-efficiency-2026-09-16.md)
- [quota/retry 직접 gate 재현과 소스 감사](agent-quota-efficiency-2026-09-16.md)
- [경량성·원본 usage·BPE 반론](agent-motto-footprint-2026-09-16.md)
- [기존38초 하한과 경쟁 결과의 성공 분모](quota-policy-deep-dive-2026-09-16.md)
- [반복 캐시 비교 스크립트](../product/scripts/benchmark_queued_cache.py)
- [429 gate 재현 스크립트](../scripts/probe-rejection-head.py)
- [로컬 결과 재검산 스크립트](../scripts/audit-efficiency-results.py)

커밋·푸시·공개 release·실제 API 호출은 실행하지 않았다. 기존 BPE 작업과 이번 변경은 현재 작업 트리에 있으며, 새 증거 폴더는 Git ignore 대상이므로 향후 게시 전 검토·선별이 필요하다.
