# 38초와 2초의 차이: 한도 정책·대기열·완료 조건 심층 비교

2026-09-16. 기존 기록의 재계산, 실제 비교에 사용한 소스, 현재 공식 문서를 함께 확인했다. 새 HTTP 성능 비교나 실제 제공자 검증은 하지 않았다. 제품 코드는 수정하지 않았다.

**이 고정 실험에서는 18건을 모두 완료할 때 p95가 최소 약 38.12초여야 한다. 현재 38.13초는 그 하한에 가깝다. 그러나 실제 서비스에 적용하는 한도 모델이 맞는지는 별개이며, 현재 제품에는 이 부분에서 큰 개선 여지가 있다.** 토크나이저의 계산 속도를 더 줄이기보다 한도의 충전 방식·공유 범위·차감 대상을 맞추고 중복 upstream 호출을 줄이는 일이 우선이다.

## 1. 직접 재계산한 차이

현재값은 `cl100k_base + input_token_overhead=256`의 보정 대조 1회다. 입력 추정 및 최종 예약 부족 0, 18/18 완료를 확인했던 실행이다. 경쟁값은 9월 15일의 고정 실행을 재검산했다. 입력·도착 계획·합성 비용·서비스 시간은 같지만 실행일과 바이너리가 다르므로 새로운 paired 성능 순위로 읽으면 안 된다.

| 대상 | 완료 | 성공 p95 | 미완료 요청 |
|---|---:|---:|---|
| 현재 DeskQuota, 선택형 BPE | 18/18 | 38.130초 | 없음 |
| 이전 DeskQuota | 18/18 | 40.634초 | 없음 |
| Bifrost transport v2.1.1 | 16/18 | 2.511초 | g0·g16을 gateway에서 429; upstream 시도 0 |
| LiteLLM v1.100.1 | 15/18 | 2.522초 | g14·g0·g16을 gateway에서 429; upstream 시도 0 |
| HiveMind 고정 HEAD | 17/18 | 40.278초 | g16은 약 30.55초 대기 후 upstream 429 |
| Direct | 16/18 | 2.505초 | g0·g16을 upstream에서 429 |

현재 DeskQuota의 마지막 두 요청을 분해하면 다음과 같다. 전송 전에는 transport도 포함하며 순수 내부 큐 타이머라고 부르지 않는다.

| 요청 | client 제출 | upstream 수신 | client 완료 | 전송 전 | 전송 후 |
|---|---:|---:|---:|---:|---:|
| g0 | 29.569초 | 62.258초 | 64.762초 | 32.689초 | 2.503초 |
| g16 | 31.711초 | 67.339초 | 69.841초 | 35.628초 | 2.502초 |

38.13초 중 약 35.63초는 발송 전이며, 모델 역할을 하는 mock의 서비스 시간은 2.5초다. Bifrost는 이 요청을 2.5초에 완료한 기록이 없다. g0는 약 2.05ms, g16은 약 2.31ms 만에 거절했다. LiteLLM도 두 요청을 약 14ms에 거절했다. 재시도 후 업무를 완료하는 시간은 이 표에서 측정하지 않았다.

공통 성공 ID만 골라 보면 현재 DeskQuota는 Bifrost의 16건에서 p95 **2.505초**, LiteLLM의 15건에서도 **2.505초**다. 상대의 2.511/2.522초와 같은 규모다. 이전 DeskQuota는 공통 집합에서도 2.775초였으며, 이번 변경에서 g11의 과대 예약 대기가 사라진 기록과 맞는다. **이 수 ms 차이는 서로 다른 시점의 저장된 실행이므로 현재 제품의 속도 승리로 홍보하지 않는다.**

## 2. 모두 끝내면 왜 최소 38초인가

이 fixture는 임의의 연속 60초 안에 upstream 시작을 16회까지만 허용한다. 캐시와 요청 합치기는 껐고 18건이 각각 새로운 생성을 요구한다. 요청별 서비스 시간은 고정되어 있다.

upstream 시작 순서를 `b1 ... b18`이라고 하면 `b18 >= b2 + 60`이어야 한다. 그렇지 않으면 두 번째부터 열여덟 번째까지 17건이 같은 60초 안에 시작한다. 두 번째 요청은 계획상 7.3318초에야 도착하므로 18번째 시작은 최소 67.3318초다.

마지막에 실행할 요청을 어떤 스케줄러가 골라도 다음 부등식이 성립한다.

```text
최악 응답 시간 >= 두 번째 도착 + 60초 - max(각 요청 도착 - 각 서비스 시간)
               = 7.3318 + 60 - 29.2085
               = 38.1233초
```

기록된 실제 client 제출 시각으로 계산한 하한은 **38.122911초**, 현재 p95는 **38.130067초**다. 차이는 약 7.16ms다. 18개 성공 표본의 nearest-rank p95는 18번째, 즉 최댓값이라 이 하한을 적용할 수 있다. 일반적인 수천 요청의 p95에 그대로 적용할 수는 없다. 이 차이를 순수 gateway 오버헤드라고 부르지도 않는다.

따라서 **이 fixture의 한도·18건 완료·원본 요청을 유지하면서 scheduler 교체만으로 p95를 2초대로 만드는 것은 불가능하다.** 동시성을 높이거나 같은 한도에서 retry를 추가해도 이 RPM 하한은 남는다. 독립된 추가 한도, 요청 재사용, 요청 의미의 변경 또는 다른 실제 서버 계약이 있어야 하한 자체가 달라진다.

이는 모든 사용 방식에서 우리 제품이 최적이라는 뜻이 아니다. 이 실험이 이제 주로 quota 만료를 재고 있으므로 다음 개선을 고르는 기준으로는 한계가 있다는 뜻이다.

## 3. 저장소 내부에서 다른 부분

Bifrost는 `c193745`, HiveMind는 `0468db5`, Overlaat는 `4e3cd0f`를 읽었다. 현재 체크아웃도 그 SHA와 일치하고 tracked 변경이 없었다. LiteLLM은 초기 clone의 다른 HEAD 대신 실제 실행한 PyPI 1.100.1 파일을 읽었으며 기존 selector SHA와 일치했다. DeskQuota는 `b72961c` 위의 미커밋 입력 추정 변경까지 포함한다. [파일별 해시·행 범위](../evidence/quota-policy-deep-dive-2026-09-16/source-evidence.json)를 보존했다.

| 구현 | 한도 검사와 차감 | 대기·진행 방식 | 의미 있는 차이 |
|---|---|---|---|
| DeskQuota | 발송 전 입력 추정+출력 상한 예약. 시작 기준 60초 이동창. 완료 usage 정산 | root별 FIFO, 실행 가능한 root 사이 RR, 오래 기다린 root 보호 | 발송 전 사용량을 통제하지만 서버 계약과 다르면 과도한 대기 또는 잘못된 환급 가능 |
| Bifrost | 해당 governance 경로는 기록된 사용량의 한도 도달 여부를 검사. 요청 수는 성공 응답 종료 때, 토큰은 usage에 따라 갱신 | quota 위반은 429. 별도의 provider worker 큐와 fallback 경로 존재 | 출력 상한을 미리 잠그지 않아 낙관적으로 진행. 우리와 같은 발송 전 예약 보장은 아님 |
| LiteLLM usage-based-routing-v2 | deployment별 UTC 분 bucket. 기록된 TPM+입력 토큰으로 후보 선택, RPM은 pre-call, TPM은 성공 후 갱신 | 사용 가능한 deployment 선택. 없으면 429. 별도 priority scheduler와 retry/fallback 지원 | 독립 용량을 활용하고 과도한 출력 예약을 피함. 분 경계와 미완료 출력의 회계가 다름 |
| HiveMind | scope별 60초 request window와 완료 후 token window. 응답 remaining/reset·Retry-After 관측 | admission slot을 먼저 잡고 limiter에서 대기. 비교는 global·동시성 2·retry 0 | provider 피드백 활용은 참고할 가치가 있음. 로컬 발송과 서버 도착 시각 차이도 영향 |
| Overlaat | 모델별 cap과 명시적 resource pool별 cost budget | 우선순위+aging, 실행 가능한 요청 선택, 큰 head 예약, 다른 pool은 진행 | 독립 자원을 함께 막지 않음. 실행 용량 반환과 60초 RPM 소모는 달라 그대로 이식할 수 없음 |

### Bifrost: worker 큐와 quota 처리는 별개

`plugins/governance/store.go:2040`의 `CheckRateLimit`는 현재 누적량이 한도 이상인지 검사하며, 현재 요청의 예상 입력·출력량을 받지 않는다. `main.go:869`는 해당 판단을 429로 돌려준다. `tracker.go:141`에서 streaming request count는 성공한 최종 chunk에 증가한다. 이 경로는 미완료 요청의 출력 예약을 우리처럼 선점하지 않는다.

`core/bifrost.go:6012`의 channel 큐와 `drop_excess_requests`는 worker/buffer 포화를 다룬다. 기존 buffer 64가 작아서 g0/g16이 거절됐다는 근거는 없다. 그때 먼저 실행한 16건은 이미 끝났고, quota가 거절 원인이었다. drop 설정만 바꾸면 이 요청들이 다음 분까지 기다린다고 가정해서는 안 된다.

실제 장점은 설정된 다른 provider로 이동할 수 있다는 점이다. 공식 문서도 제한된 provider를 routing에서 제외하고 fallback을 사용하는 흐름을 설명한다. 다만 같은 계정의 키를 여러 개 준다고 독립 한도가 생기지는 않는다. 고정 소스도 429 key rotation에 backoff를 유지한다. [Bifrost 한도](https://docs.getbifrost.ai/features/governance/budget-and-limits), [재시도·fallback](https://docs.getbifrost.ai/features/retries-and-fallbacks).

### LiteLLM: 한도 경계와 대기 설정이 다름

실행한 selector는 기록된 TPM과 입력 토큰을 더하며 우리처럼 출력 상한을 합산하지 않는다. 설치된 `_return_potential_deployments`를 그대로 분리해 실행했을 때, RPM 16 설정에서 기록량 14는 허용하고 15는 제외했다. 코드의 `rpm + 1 >= limit` 때문이다. 이전 15/18 결과와 일치하는 메커니즘이지만, 이 고립된 검증만으로 세 번째 거절의 전체 내부 실행 원인을 확정하지는 않는다. LiteLLM의 모든 limiter가 같다는 주장도 아니다.

후보가 없으면 이 strategy는 429와 `Retry-After: 60`을 생성한다. 공식 문서와 `router.py`에는 별도의 우선순위 큐가 있지만 비교 요청에는 priority가 없었고 retry도 0이었다. 따라서 “LiteLLM은 대기를 못한다”는 설명은 틀리다. 반대로 대기·retry를 켜면 같은 실제 rolling quota에서 18건 모두를 2.5초에 끝낸다는 근거도 없다. [라우팅](https://docs.litellm.ai/docs/routing), [현재 Beta로 표시된 우선순위 기능](https://docs.litellm.ai/docs/scheduler).

### HiveMind: 대기를 선택한 구성에서는 40초대

우리와 비슷하게 quota 회복을 기다린 global 구성에서는 p95가 40.278초였다. 실패한 g16은 첫 upstream RPM 만료보다 약 21.40ms 이른 시점에 mock에 도착했다. 이 차이는 원시 시각으로 확인했다. `wait_if_throttled`가 forward 전에 로컬 request timestamp를 기록하는 코드와 일치하지만, 당시 내부 limiter trace가 없으므로 특정 내부 시각을 직접 관측했다고 단정하지 않는다.

“대기 후 전송”만으로 429를 완전히 없앨 수는 없다. 네트워크 지연과 다른 사용자의 트래픽도 있어 provider 피드백과 만료 경계 처리가 필요하다. HiveMind의 기본 per-agent 모드는 이번 global 실험과 다르며, 에이전트마다 같은 RPM을 부여한다고 공유 API 한도가 늘어나지는 않는다. [고정 limiter 소스](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/src/hivemind/scheduler/rate_limiter.py).

### Overlaat: pool 격리와 조건부 우회

전역 우선순위 목록에서 실행 가능한 요청을 찾고, 자원 부족으로 막힌 큰 요청에는 pool별 예약을 건다. 다른 pool은 계속 진행할 수 있다. DeskQuota는 하나의 quota와 upstream을 공유하고 root 내부는 head만 보므로, 독립 모델이나 작업을 필요 이상으로 묶을 여지가 있다.

다만 Overlaat의 cost는 실행 중 점유했다가 release 때 돌려주는 용량이다. RPM은 완료해도 즉시 돌려받지 못한다. 이 차이를 없애고 같은 scheduler를 붙이면 상위 API 한도를 초과한다. [고정 scheduler 소스](https://github.com/tdamsma/overlaat/blob/4e3cd0ff02a0456d470d5b18744803a56c7e1ffb/overlaat/scheduler.py#L487-L594).

## 4. 현재 우리 설계의 더 큰 약점

### 같은 RPM이어도 충전 방식이 다름

DeskQuota는 요청마다 60초 뒤 차감을 만료시킨다. Anthropic 공식 문서는 지속적으로 용량이 회복되는 token bucket을 명시한다. **16 RPM이라는 숫자만 같다고 같은 알고리즘이 아니다.** [Anthropic rate limits](https://platform.claude.com/docs/en/api/rate-limits).

RPM만 다루는 산술 대조에서 0초에 16건, 30초에 2건이 도착하면 다음과 같다.

| 허용 계약 | 마지막 2건의 시작 | 이유 |
|---|---:|---|
| 임의의 60초에서 최대 16회 | 60초 | 처음 16개의 만료를 기다림 |
| capacity 16·분당 16개 충전 token bucket | 30초 | 30초간 8개 용량이 회복됨 |

실제 Anthropic의 burst capacity가 16이라는 주장은 아니다. 같은 RPM 표기에서 30초의 불필요한 대기가 생길 수 있는 조건을 보인 계산이다. 반대로 실제 서버가 엄격한 이동창이면 token bucket으로 바꾸는 것은 한도 위반이다. capacity·충전률·초단위 제약을 모르면 임의로 완화하면 안 된다.

### 출력 상한과 cache usage를 모두 같은 TPM으로 처리

현재 Messages 정산은 input+output+cache creation+cache read를 합산한다. 관측값을 보존하는 일과 그 합계를 모든 제공자의 quota 차감량으로 쓰는 일은 다르다.

현재 Anthropic 문서는 ITPM/OTPM을 구분하고 대부분 모델의 cache read를 ITPM에서 제외한다. OTPM은 실제 생성량을 실시간 평가하며 `max_tokens`는 OTPM 계산에 포함하지 않는다고 설명한다. 반면 Azure의 문서상 TPM 입장 계산은 prompt와 `max_tokens` 등의 예상 최대 처리량을 사용하며 최종 청구 usage와 다르다. [Anthropic](https://platform.claude.com/docs/en/api/rate-limits), [Azure 한도 계산](https://learn.microsoft.com/en-us/azure/foundry-classic/openai/how-to/quota?view=foundry-classic).

우리의 출력 상한 예약은 어떤 계약에서는 과도하고, `actual` 환급은 다른 계약에서는 너무 낙관적일 수 있다. BPE도 서버의 한도 계산기를 대체하지 못한다. **원본 usage는 그대로 전달하면서 quota 장부의 차감 정책을 구분해야 한다.** 요청 본문이나 auto-compaction에 쓰이는 usage를 수정해서 풀 문제가 아니다. 실제 제공자에서 이 개선 효과를 측정한 상태는 아니다.

### 독립된 한도도 하나로 묶음

현재 `Config`에는 `upstream`과 `quota`가 각각 하나다. 모델별 입력 추정은 선택할 수 있지만 장부는 공유한다. 제공자가 모델 그룹·deployment별로 독립 한도를 주면 서로 기다릴 필요가 없는데도 묶일 수 있다. 반대로 키만 다르고 조직 한도는 같으면 함께 관리해야 한다. LiteLLM의 deployment 선택과 Overlaat의 명시적 pool 구분에서 가져올 가치가 큰 부분이다.

### 큐에서 기다리는 사이 채워진 캐시를 재조회하지 않음

`server.rs:808`에서 캐시를 한 번 확인한 뒤 admission을 기다리고, `transport/stream.rs`는 permit을 받으면 발송한다. 기다리는 사이 동일 응답이 캐시에 들어와도 재조회하지 않는다. 이전 Windows 실험의 동시 동일 요청 10건→upstream 10회와 맞는 경로다. 이번에는 Windows 성능을 재측정하지 않았다. [기존 직접 검증](windows-followup-and-improvements-2026-09-16.md).

Bifrost의 `framework/lrucache`는 동일 키의 진행 중 fill을 공유하고, LiteLLM의 `cache_coordinator.py`는 event 대기 후 재조회한다. 범용 리소스 캐시 패턴이며 “경쟁 LLM 응답이 이미 10→1이었다”는 증거는 아니다. 우선 기존 exact-cache 정책 안에서 발송 전 재조회하는 작은 변경을 검증할 수 있다. 완전한 single-flight는 첫 fill을 기다리는 동안 quota 슬롯을 차지하지 않도록 별도 수명 관리가 필요하다.

## 5. 끌어올릴 때의 이득과 트레이드오프

아래는 구현 효과 예측이며 새 성능 검증 결과가 아니다.

| 후보 | 빨라지는 조건·장점 | 비용·손해·적용 조건 |
|---|---|---|
| 제공자 계약에 맞는 창·충전 모델과 응답 헤더 보정 | 서버 capacity가 회복됐는데 로컬이 막는 수초~수십초 대기 제거 가능 | 오래된 헤더·동시 요청·공유 계정·clock 차이 처리 필요. 잘못 완화하면 429 증가 |
| 한도 공유 그룹별 장부 | 독립 모델·deployment가 서로 막지 않음. 추가 외부 서비스 없이 가능 | 공유 한도를 독립으로 오인하면 초과. 관련 장부를 함께 예약해야 함 |
| 발송 전 cache 재조회→필요하면 single-flight | 반복 스크립트·재시도 burst에서 RPM/TPM과 생성 시간을 직접 절약 | 고유 요청에는 효과 없음. 취소·실패·stream buffer 경계 필요. 재조회만으로 10→1 보장 안 됨 |
| 독립 작업의 제한적 우회·우선순위 | 작은 interactive 요청이 큰 batch 뒤에 갇히는 지연 감소 | 큰 요청의 대기 증가·기아 위험·순서 변경. 기존 18건 RPM 하한은 못 줄임 |
| 독립된 추가 upstream으로 routing | 포화된 서버 대신 남는 한도 활용; 완료 시간을 크게 줄일 수 있음 | 비용·모델 품질·데이터 경로·설정 복잡도. 같은 quota의 키 교체는 추가 용량 아님 |
| 짧은 deadline 뒤 거절 | 불가능한 대기를 빨리 알리고 client 자원을 돌려줌 | 업무는 미완료. 성공 p95만 좋아 보일 수 있어 재시도까지 평가해야 함 |
| BPE 추가 최적화·출력 예약 축소 | TPM 과대 예약이 남은 조건에서 유효 | RPM 포화에는 제한적. 현재 BPE 배포 크기 59.87MB·선택 RSS 41.7–76.7MiB를 이미 부담. 예측 출력은 과소 예약 위험 |

단일 provider 사용자에게 fallback 플랫폼 전체를 만드는 것은 범위가 크다. **구현량 대비 우선순위는 ①기존 캐시의 발송 전 재조회, ②실제 제공자 한 곳의 명시적 quota 계약, ③독립된 한도 그룹, ④독립 작업 우회**로 본다. 캐시 효과가 없는 사용자에게는 ②가 먼저다. 현재 38초를 조금 더 깎으려고 scheduler를 복잡하게 만들거나 출력 예약을 무조건 없애는 일은 우선하지 않는다.

## 6. 다음 비교가 증명해야 할 것

기존 18건은 엄격한 RPM 준수와 모든 요청 완료를 확인하는 대조로 남긴다. 새 비교에서는 같은 논리 작업에 동일 deadline·client retry 예산을 주고 첫 제출부터 최종 완료까지 잰다. 거절 후 재시작한 시간을 초기화하지 않는다.

| 사용 방식 | 확인할 우위 | 같이 보존할 반례 |
|---|---|---|
| 한도가 넉넉한 짧은 호출·긴 context | 추가 p95/p99·CPU·RSS가 작음 | BPE 때문에 무대기 요청이 느려지는 경우 |
| RPM 중심 burst | 가능한 최초 입장에 가까운 완료, 불필요한 retry 감소 | 실제 상한 준수; 빠른 거절을 완료로 계산하지 않기 |
| TPM 중심 다국어·큰 출력 상한 | 제공자 예약 규칙에 맞는 대기 감소 | output reserve 계약과 actual 계약을 각각 검사 |
| 동일 요청의 동시 재실행 | upstream 시도·토큰·완료 시간 감소 | 인증·Vary·tools·고유 본문은 공유 금지 |
| 여러 agent·독립 모델·background batch | interactive 최대 대기·전체 과제 완료 시간 감소 | 큰 작업 기아·순서 보존·공유 quota 초과 |
| 긴 대화와 provider prompt cache | quota 차감 정책과 auto-compaction 보존 | context usage와 quota usage를 혼동하지 않기 |

우리가 내세울 가치는 **남는 한도를 불필요하게 묶지 않고, 같은 일에 쓰는 upstream 호출을 줄이며, 모든 작업의 완료를 가볍게 조정한다**는 방향이 적합하다. 현재 코드는 그 일부를 갖췄지만 다양한 실제 과제 완료 시간의 경쟁 우위는 아직 입증하지 못했다.

## 재현과 범위

`python3 scripts/probe-quota-policy-gap.py`는 기존 5제품 원시 기록과 보정 대조를 읽어 동일 입력·종료 분모·공통 성공 ID·RPM 하한을 assert하고, 설치된 LiteLLM selector 하나를 격리 실행한다. HTTP·실제 API·새 설치는 수행하지 않는다. 충전 방식 예제는 별도의 산술 대조다. 독립 검토에서도 하한·서비스 시간·종료 분모가 일치했다.

[결과 JSON](../evidence/quota-policy-deep-dive-2026-09-16/result.json), [재계산 코드](../scripts/probe-quota-policy-gap.py), [소스 근거](../evidence/quota-policy-deep-dive-2026-09-16/source-evidence.json).

이번 작업은 분석 스크립트·근거·보고서만 추가했다. 이전 입력 추정 개발 변경은 보존했다. 커밋·푸시·release·실제 provider 호출은 하지 않았다.
