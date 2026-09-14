# Task 7 재시도 분류 근거

2026-09-12 공식 문서 재조회. Core Task 6 명세 리뷰와 독립적으로 준비한 후속 검증 자료다. 제품 retry 구현·실제 provider 실행 결과가 아니다.

## 문서에서 확인한 경계

Claude의 429 `rate_limit_error`는 속도 제한뿐 아니라 사용 한도 소진에도 쓰인다. 따라서 HTTP status와 이 type만으로 일시적인 오류라고 판정할 수 없다. SDK 기본 재시도와 stream 시작 후 오류도 별도 경로다. [공식 오류 문서](https://platform.claude.com/docs/en/api/errors).

Messages의 월 사용 한도 소진은 `error.details.error_code = enforced_spend_limit_reached`로 구별하며 이 응답에는 `retry-after`가 없다. 다만 Claude Code workspace의 사용 한도에는 `retry-after`가 있는 429도 가능하다. **헤더 유무만으로 영구·일시 오류를 완전히 구별할 수 없다.** [공식 rate limits 문서](https://platform.claude.com/docs/en/api/rate-limits#reaching-your-spend-cap).

표준 `Retry-After`는 HTTP-date 또는 0 이상 정수 초다. provider의 ms 확장은 이 문법과 구별해야 한다. [RFC 9110 §10.2.3](https://www.rfc-editor.org/rfc/rfc9110.html#name-retry-after).

Azure 공식 quota 문서는 `retry-after-ms`를 밀리초 단위 대기로 명시한다. Task 7은 이 이름의 0 이상 정수 ms를 지원한다. 유효한 표준·ms 대기가 함께 있거나 같은 헤더가 반복되면 가장 긴 대기를 적용해 서버가 요구한 최소 대기를 앞당기지 않는다. 음수·NaN·무한대·숫자 뒤 문자·표현 범위 초과를 검증하고, 현재 요청의 deadline을 이유로 공유 cooldown을 줄이지 않는다. 이는 선택한 gateway 계약이며 Azure의 모든 오류 body를 자동 replay한다는 뜻은 아니다. [공식 Azure quota 문서](https://learn.microsoft.com/en-us/azure/foundry/openai/how-to/quota).

OpenAI 현재 문서도 429를 속도 제한, 잔액 소진, 조직·프로젝트 지출 한도, 조직 사용 한도로 나눈다. `slow_down`은 급격한 요청 증가를 나타내며 대기 후 재시도를 안내한다. 반면 `credit_balance_exhausted`, `organization_spend_limit_exceeded`, `project_spend_limit_exceeded`, `organization_usage_limit_exceeded`는 짧은 재시도로 해결되지 않는다. broad type이 `insufficient_quota`일 수 있어 구체적인 code를 함께 본다. [공식 오류 문서](https://developers.openai.com/api/docs/guides/error-codes).

고정한 Codex 원문도 위 한도 코드를 별도 오류로 분류한다. SSE의 `rate_limit_exceeded`에서 대기 문구를 파싱하는 경로는 HTTP 429 전체에 적용되는 계약으로 확대하지 않는다. [HTTP 분류](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/codex-api/src/api_bridge.rs#L140-L182), [SSE 파서](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/codex-api/src/sse/responses.rs#L685-L709).

추가로 고정 Codex의 HTTP 오류 테스트는 `rate_limit_exceeded`와 `slow_down`을 quota 소진과 구별한다. OpenAI의 공식 troubleshooting 자료도 `rate_limit_exceeded`를 ordinary throttling으로 분류한다. 따라서 두 code를 좁은 opt-in 재시도 계약에 포함한다. `rate_limit_exceeded`의 type은 없거나 `rate_limit_error`·`requests`·`tokens`인 경우만 지원하고, 영구 marker·모순·중복 discriminator가 있으면 자격을 주지 않는다. 이는 공식 클라이언트·가이드에서 도출한 지원 계약이다. 실제 provider에 호출해 응답을 검증한 결과나 재시도가 성공한다는 보장은 아니다. [고정 HTTP 테스트](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/codex-api/src/api_bridge_tests.rs#L325-L356), [공식 troubleshooting 자료](https://github.com/openai/openai-developers-for-cursor/blob/main/skills/openai-api-troubleshooting/SKILL.md).

Pi의 provider 재시도는 `retry-after-ms`를 우선 읽고 `parseFloat`를 사용한다. 이는 참고 source의 동작이며 우리 정수 초 표준 parser의 수용 규칙을 대신하지 않는다. 예를 들어 숫자 뒤 문자가 붙은 값, 음수·무한대, 두 헤더의 충돌을 명시적으로 검증해야 한다. [고정 Pi 파서](https://github.com/earendil-works/pi/blob/f3c672245d25ef2283ffc0d9cdec8a5482651103/packages/ai/src/utils/provider-retry.ts#L51-L67).

양의 정수 문자열이 `u64` 범위를 넘는 경우는 음수·NaN 같은 잘못된 문법과 구별한다. RFC의 정수 문법에는 자릿수 상한이 없으므로, 숫자 전용 overflow의 공유 cooldown은 표현 가능한 최대 Duration으로 보수적으로 포화한다. 자동 replay 자격은 지원 범위를 넘어선 값으로 계속 차단한다. 유효한 0 대기가 함께 와도 overflow를 버려 새 client를 즉시 보내지 않는다. 이는 구현상 경계 결정이며 실제 provider가 그 크기의 대기를 보낸다는 관찰은 아니다.

명세 리뷰 중 optional discriminator의 null 의미도 명시했다. 필수 `error.code`가 지원하는 문자열이고 영구·모순 marker가 없을 때, optional `error.type` 등의 명시적 JSON null은 값 없음으로 취급한다. 필수 code가 null이면 재시도 자격이 없고, null이 포함된 중복 discriminator도 거절해야 한다. 이는 기존 대화에서 별도 합의했던 사실이 아니라 현재 runtime 계약을 확인한 뒤 내린 구현상 명확화다.

## 승인 명세에 연결할 검사

다음은 위 근거와 기존 코어 명세 §7에서 도출한 구현·검증 기준이다. 원본 provider의 모든 비공개 오류 변형을 안다는 뜻은 아니다.

| 입력 또는 경합 | 필요한 결과 |
|---|---|
| spend-limit marker와 rate-limit type 또는 Retry-After가 함께 존재 | 한도 소진 분류가 우선, 자동 replay 없음 |
| generic `rate_limit_error`만 있고 추가 식별 근거 없음 | 분류 불가로 원문 전달; 무조건 1초 후 replay하지 않음 |
| 유효한 Retry-After와 분류 불가 body | 공유 cooldown과 자동 replay 자격을 별도로 판단 |
| 지원하는 계약으로 일시적 rate rejection이 확인됐으나 대기 헤더 없음 | opt-in에서만 기존 1초+0~250ms jitter; 최대 추가 1회 |
| 모순·중복 discriminator, malformed/oversized/압축 body | 임의 문자열 검색으로 transient 판정하지 않음; bounded 관찰과 원문 전달 유지 |
| Retry-After가 현재 요청 deadline보다 김 | 요청은 자기 deadline으로 끝내고 group cooldown은 앞당기지 않음 |
| 헤더 대기 중 새로운 client 요청·다른 root·metadata 도착 | 같은 cooldown과 admission을 적용; 대기 중 execution slot을 점유하지 않음 |

위 allowlist와 ms header 계약을 Task 7 구현·테스트·runtime 문서에 명시한다. 의미가 확인되지 않은 provider 응답은 재시도 지원 범위를 좁혀 처리한다. 이를 해결하려고 범용 provider registry나 오류 문구 추측기를 추가하지 않는다.

기본 retry 0, opt-in 추가 1회, attempt마다 RPM 차감, 원래 deadline·aging 보존, partial stream/모호한 POST/503 replay 금지는 기존 승인 범위 그대로다. 이번 자료 때문에 현재 Task 6 수용 범위를 늘리지 않는다.

## 기존 의존성 확인

Task 6에서 고정한 `product/Cargo.lock`에 `httpdate 1.0.3`과 `rand 0.10.2`가 이미 있다. 설치된 registry 원문에서 `httpdate::parse_http_date(&str) -> Result<SystemTime, Error>`와 `rand::random_range`를 확인했다. 후자는 `thread_rng` feature 조건이 있으므로 직접 의존성으로 사용할 때 feature/MSRV를 확인해야 한다. 날짜 parser나 난수기를 새로 직접 구현하기 전에 이 경로를 검토한다. 아직 Cargo manifest 변경이나 해당 함수의 제품 실행 검증을 하지 않았다.
