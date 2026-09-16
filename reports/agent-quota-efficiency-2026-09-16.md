# Quota·retry 경로 독립 감사

2026-09-16. 담당: quota_efficiency_audit. 모토: **Get more done within your LLM limits.**

기존 38초 RPM 하한, 입력 예약, 독립 quota 그룹, queued cache 재조회 분석은 반복하지 않았다. 이번에는 이미 받은 응답을 불필요하게 붙잡는 경로와 서버의 대기 신호를 놓치는 경로를 확인했다. 제품 코드는 수정하지 않았다.

## 1. 우선 적용: 분류 결과를 쓰지 않는 429도 본문이 끝날 때까지 숨김

**직접 검증.** 저장된 release `llmgw-final`, SHA `fec36cfa06bbac56e0ff5ba655c2779f942f22c44b191a384cc4992bdb4af226`을 임시 config/state와 loopback fixture로 실행했다. Upstream은 429 header와 본문 한 바이트를 보낸 뒤 나머지 본문을 gate로 막는다. gate가 닫힌 동안 downstream head를 관측했다.

| 사례 | body gate가 닫혀 있을 때 head 도착 | 해석 |
|---|---|---|
| 직접 요청, JSON | 예 | fixture가 head 자체를 지연시키지 않음 |
| gateway, JSON, retry off, Retry-After 있음 | 아니오 | 분류 결과를 쓰지 않는 대기 |
| gateway, text/plain, retry off, Retry-After 있음 | 아니오 | 본문이 분류 대상이 아닌데 대기 |
| gateway, text/plain, retry on, Retry-After 있음 | 아니오 | 본문이 분류 대상이 아닌데 대기 |
| gateway, JSON, retry off, timing 없음 | 아니오 | transient 판정 후 fallback cooldown 설치에 필요한 대기 |
| gateway, JSON, retry on, Retry-After 있음 | 아니오 | 재시도 여부를 결정하기 위한 필요한 대기 |

각 사례는 gate 해제 후 정확히 1회 upstream 시도, 원본 429·본문·Retry-After 보존으로 끝났다. gateway 5사례는 공유 cooldown도 관측했다. Timing 없는 JSON 대조에서는 기존 1,000–1,250ms fallback이 남아 있었다. gate 관측의 300ms는 순서를 확인한 구간이며 평균·p95·proxy overhead 측정값이 아니다. 오류를 빨리 알리는 개선이지, 실패한 LLM 작업이 성공으로 바뀐 성과도 아니다.

현 동작 확인 실행은 6/6 통과했다. 아래 3개 불필요 대기만 즉시 전달해야 한다는 판정으로 같은 바이너리를 재실행하면 정확히 3개가 실패하고 3개 대조는 통과한다. 이 결과를 구현 전 RED로 보존했다.

- [현 동작 6사례](../evidence/quota-efficiency-audit-2026-09-16/rejection-head-baseline-02.json)
- [개선 요구 RED](../evidence/quota-efficiency-audit-2026-09-16/rejection-head-required-behavior-red.json)
- [재현 스크립트](../scripts/probe-rejection-head.py)

**코드 근거.** `product/src/transport/stream.rs:188–230`은 모든 429에서 probe를 호출한다. `449–479`는 Content-Length 초과와 encoding만 먼저 제외하고 EOF 또는 16KiB 초과까지 읽는다. Content-Type과 body 사용 여부는 그 뒤에 검사한다. `product/src/admission/retry.rs:28–44`는 비JSON 응답을 항상 분류에서 제외한다. 또한 `stream.rs:233–244`에서 retry off이고 timing header가 있으면 body 판정 결과를 사용할 곳이 없다.

### 최소 변경 명세

기존 429 header→shared cooldown은 그대로 먼저 실행한다. Body probe는 다음 두 조건을 모두 만족할 때만 수행한다.

1. 기존 classifier가 수용하는 media/encoding이며 알려진 Content-Length가 16KiB 이하이거나 미상.
2. `missing_timing(headers)`이거나, `retry_transient_429 && !retried && !disconnected && timing_allows_retry(headers)`.

Media/encoding 판정은 `transient()`의 현재 header 검사를 작은 공유 함수로 추출한다. 모든 Content-Type 값을 확인하고 header 부재를 거절한다. 별도 느슨한 파서를 만들지 않는다. **기존 probe의 `h != "identity"` 인코딩 guard도 그대로 유지한다.** Classifier는 대소문자를 무시하지만 probe는 그렇지 않으므로, helper로 기존 guard를 대체하면 `IDENTITY`의 재시도 범위가 넓어진다. 이번 변경은 probe를 줄이는 것만 허용한다. 기존 byte 상한·JSON 전체 유효성·중복 discriminator 거절·재시도 whitelist는 유지한다.

명시 timing이 잘못됐더라도 header가 존재하면 missing-timing fallback은 원래 금지다. 따라서 retry 불가가 확인된 경우 probe를 생략해도 되며, 다른 유효한 header에서 얻은 cooldown은 계속 유지한다. Retry off 전체에서 probe를 없애는 변경은 **기각**한다. Timing 없는 transient JSON의 공유 대기를 깨기 때문이다.

트레이드오프는 늦은 body 오류의 표면이다. Probe를 생략한 응답은 429 head를 먼저 보냈으므로 이후 truncation을 502로 바꿀 수 없다. 원본 429와 body-stream failure로 전달하는 기존 일반 streaming 의미를 사용한다. Retry 가능한 JSON을 여전히 probe하는 경우의 pre-head 502 검사는 유지해야 한다. 원본 usage, auto-compaction, quota 환급, retry 횟수는 바꿀 필요가 없다.

## 2. 후속 후보: 503의 명시 Retry-After를 공유하지 않음

**코드/표준 근거, 직접 실행 미검증.** `stream.rs:188–199`의 shared cooldown 호출은 상태 429 내부에만 있다. 503을 받은 client는 header를 받지만 다른 root·client의 새 요청에는 공유 pause가 적용되지 않는다. RFC 9110은 503의 Retry-After를 해당 서비스가 이용 불가능할 것으로 예상하는 기간으로 정의한다. [RFC 9110 §10.2.3](https://www.rfc-editor.org/rfc/rfc9110.html#name-retry-after).

비교 소스에서 참고할 패턴은 다음과 같다. 현재 실행한 버전과 동일한 고정 소스를 읽었으며, 이 표는 경쟁 HTTP 성능 측정이 아니다.

| 고정 대상 | 읽은 경로 | 채택할 부분·한계 |
|---|---|---|
| HiveMind `0468db5` | `interceptor.py:418`, `rate_limiter.py:372–427` | 상태 분류 전 response header에서 공유 pause를 갱신. 수동 float 파서는 그대로 채택하지 않음 |
| LiteLLM 설치 1.100.1 | `router_utils/cooldown_handlers.py:205–255` | 5xx를 cooldown 후보로 취급. 다른 정책 조건도 있으므로 503마다 항상 global pause라는 뜻은 아님 |
| Overlaat `4e3cd0f` | `overlaat/breaker.py:1–38,138–225` | 실패 결과 기반 model별 breaker, half-open 1회 probe. 현재 one-upstream 제품에 전체 상태기계를 추가하는 것은 보류 |
| Bifrost `c193745` | `core/bifrost.go:6553–6582` | retryable server failure 분리. 같은 POST의 자동 재전송은 별도 소유권 문제이므로 함께 가져오지 않음 |

가장 작은 후속안은 **명시적이고 유효한 503 Retry-After만 기존 shared cooldown으로 반영하고 원본 응답을 그대로 전달**하는 것이다. 자동 5xx replay, 추정 backoff, 새 circuit breaker는 이번 수정에 넣지 않는다. 실패 기간 동안 불필요한 새 시도·예약을 줄일 가능성이 있지만, 실제 회복이 header보다 빨라지면 기다리는 손해가 있다. 현재 one-upstream 범위에서도 특정 모델만의 503인지 서비스 전체의 503인지 검증이 필요하다. 별도 fixture에서 다른 root의 시작 순서·deadline·취소를 검사한 뒤 채택해야 한다.

## 3. 낮은 우선순위: 명시적 재시도 금지 header와의 차이

**코드 근거, gateway 실행 미검증.** 설치된 OpenAI SDK `_base_client.py:830–840`과 현재 Anthropic 공식 SDK는 `x-should-retry: false`를 재시도보다 우선한다. DeskQuota `retry.rs`와 `stream.rs:240–244`에는 그 판정이 없다. 따라서 gateway retry opt-in·known transient code·유효 timing과 해당 header가 함께 오는 경우 SDK와 다르게 동작할 수 있다. 이는 비표준 header 계약 차이이며 일반 HTTP 위반으로 부르지 않는다. [Anthropic 공식 SDK](https://github.com/anthropics/anthropic-sdk-python/blob/main/src/anthropic/_base_client.py).

향후 검증한다면 false를 거부 신호로만 존중하는 작은 변경을 고려한다. `true`가 body whitelist·partial-stream 금지를 덮어쓰게 하면 안 된다. 이번 구현 범위에는 포함하지 않는다.

## 에이전트 토론과 기각

- `cache_efficiency_audit`: cache 대기 탈출과 429 불필요 probe 생략을 작은 우선 변경으로 지지. Timing 없는 retry-off fallback 보존, 공유 media 판정, 503의 shared-scope 검사를 요구했다. 추가 검토에서 probe와 classifier의 identity 대소문자 차이를 지적했고, 기존 probe guard를 남겨 retry 범위가 확대되지 않도록 명세를 좁혔다.
- `motto_footprint_audit`: 새 scheduler·토크나이저보다 기존 hot path 낭비 제거에 동의했다. 제공자 할인은 내부 quota 장부에만 적용하고 client의 원본 usage를 바꾸지 않아야 한다는 경계를 재확인했다.
- 범용 Anthropic `rate_limit_error`를 모두 재시도하는 확대는 기각했다. 최신 공식 문서상 429는 일시 rate limit뿐 아니라 spend-cap도 포함하며 SDK도 이미 자동 재시도를 제공한다. [Anthropic errors](https://platform.claude.com/docs/en/api/errors).
- 더 많은 retry, 높은 성공 p95를 숨기는 빠른 거절, 키 교체를 통한 한도 증가 주장은 개선안으로 채택하지 않았다.

모토에 맞는 우선순위는 **이미 있는 캐시를 제때 사용 → 불필요한 응답 보류 제거 → 실제 제공자의 명시적 pause·quota 계약 확인**이다. 후보가 많다는 사실을 경쟁 우위나 처리 속도 향상으로 홍보하지 않는다.

## 재현·범위

```sh
python3 scripts/probe-rejection-head.py \
  --binary /Users/ryuwon/Library/Caches/deskquota-input-estimator-20260916/llmgw-final \
  --expect current \
  --output evidence/quota-efficiency-audit-2026-09-16/rejection-head-baseline-02.json
```

수정 바이너리에서는 `--expect stream-ineligible`을 사용해 gate-before-head 계약을 검사한다. 추가로 retry on의 bounded body failure·malformed/duplicate timing·동일 media 중복·비JSON·encoding·취소를 기존 Rust 계약 검사와 함께 검증해야 한다.

첫 harness 실행은 macOS `/var`와 `/private/var` canonical path 차이로 state 격리 assertion이 실패했다. 직접 요청 대조 1회만 완료한 후였으며 gateway 제품 실패가 아니다. Path 정규화 후 정상 6사례와 RED 6사례를 실행했다. 전체 시도는 합성 loopback 13회이며 유효 두 실행은 12회다. 실제 provider·유료 API·Windows·새 빌드·성능 benchmark·commit/push는 수행하지 않았다. 자기 fixture/process만 종료하고 임시 state를 제거했다. [소스 해시·범위](../evidence/quota-efficiency-audit-2026-09-16/source-evidence.json).
