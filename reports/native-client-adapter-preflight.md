# Native 도구 연결 구현 전 확인

2026-09-13. Native Task 2 구현 중 수행한 **읽기 전용 소스 확인**이다.
Task 4 adapter 구현·picker·인증·tool 왕복의 실행 증거가 아니다.
[승인된 연결 계약](../docs/superpowers/specs/2026-09-12-native-setup-design.md)과
[기존 연결 조사](native-setup-research.md)를 적용할 때 주의할 경계를 기록한다.

## 버전과 근거를 구분하기

현재 실행 경로를 resolve하고 package manifest만 읽었다. Pi는
`@earendil-works/pi-coding-agent` **0.84.2**, Codex는 `@openai/codex`
**0.154.0**이다. Claude 실행 경로는 Homebrew Cask의 `2.1.63` directory를
가리킨다. 마지막 값은 directory 이름이며 `claude --version` 실행 검증은 아니다.
실제 CLI를 시작하거나 사용자 설정·auth 파일을 읽지 않았다.

Pi reference는 `f3c672245d25ef2283ffc0d9cdec8a5482651103`의 **0.85.1**,
Codex reference는 `944d6fd1ba4baab69dbedd205282dc72ec20abb5`다.
설치 버전과 clone 소스를 같은 실행 증거로 취급하지 않는다.
확인한 파일의 SHA는 [정적 확인 기록](../evidence/native-client-adapter-preflight.json)에 둔다.

## Pi: gateway 인증과 모델 표시 조건

고정된 Pi `docs/models.md`는 key 없는 로컬 서버도 configured auth가 없으면
모델이 `/model`과 `--list-models`에서 unavailable일 수 있다고 명시한다.
설치된 0.84.2 `provider-composer.js`의 `configuredRequestAuthStatus`도
custom headers의 존재와 별도로 configured API key 상태를 판정한다.
따라서 `X-LLMGW-Token`만 넣었다고 모델 picker까지 정상이라고 표시하면 안 된다.

Task 4는 실제 격리 Pi로 다음을 각각 관찰해야 한다: 모델 표시, 선택,
gateway data token 통과, upstream auth 전달, tool 왕복. keyless 연결의
표시용 placeholder가 필요하다면 실제 credential과 구분하고 upstream의
auth 모드에 맞춰 검사한다. core의 `Auth::None`은 Authorization/x-api-key를
제거하고, `Auth::Env`는 두 헤더를 제거한 뒤 지정한 header 값을 주입하며,
`Auth::Forward`는 전달한다. placeholder 사용을 Forward 모드의 upstream 인증
완료로 간주하지 않는다. 사용자 subscription credential을 대신 사용하지 않는다.

Pi 설정의 apiKey/headers 값은 단순 문자열만이 아니다. 설치된
`resolve-config-value.js`는 `!command`, `$ENV`, `${ENV}` 해석을 지원한다.
connect preview에서 그 resolver를 호출하지 않고 설정을 데이터로만 읽는다.
기존 helper를 실행하거나 새 helper를 만들어 gateway worker에서 호출할 이유가 없다.

## Codex: clone 문법과 설치 버전 실행

고정 clone의 `core/src/config/mod.rs`는 기존 `profile = "name"` 문법을
거절하고 `--profile name`과 `name.config.toml`을 안내한다.
`model-provider-info/src/lib.rs`는 Responses wire API, `http_headers`,
`env_http_headers`, `requires_openai_auth`, `supports_websockets`를 구분한다.
MCP 서버의 같은 이름 header 설정을 model provider 설정과 혼동하지 않는다.

이는 승인된 별도 profile·HTTP/SSE 설계의 소스 근거다. 설치된 0.154.0에서
해당 profile을 읽고 적용하는지는 Task 4의 임시 Codex home과 loopback fixture로
확정해야 한다. 최신 clone만으로 installed-version PASS를 만들지 않는다.
지원하지 않는 버전에 legacy profile을 추가하는 fallback은 만들지 않는다.

이 확인에서는 product를 수정하거나 실행하지 않았고 reference clone도 그대로다.
도구 연결은 ConfigPatch의 보호·부분 적용·복원 검증이 수용된 뒤 진행한다.

## 파일별 문법 추가 확인

설치된 Pi 0.84.2와 고정 reference 모두 settings는 일반 JSON으로 읽고,
models는 주석을 제거한 뒤 JSON으로 읽는다. `models.json`만 JSON comments 지원이
확인됐으며, 모든 Pi 파일을 JSONC라고 가정하지 않는다. 실제 파서 위치와
Task 3의 주석 보존 편집 후보는 [ConfigPatch 사전 확인](native-config-patch-preflight.md)에
추가했다. CLI 실행 또는 새 dependency 설치는 하지 않았다.

## Pi 모델 한도와 gateway 예약량은 별개

설치된 0.84.2의 `dist/core/model-config.js:134–144`와 고정 reference의
`src/core/model-config.ts:162–172`는 `contextWindow`와 `maxTokens`를 선택
필드로 받는다. 그러나 설치본 `dist/core/provider-composer.js:72–73`는 custom
model에서 빠진 값을 각각 **128,000 / 16,384**로 채운다. 이는 Pi의 기본값이며
연결할 사내·로컬 모델이 그 한도를 지원한다는 확인이 아니다.

Task 4는 기존의 명시된 model metadata를 보존하고, 새 모델의 한도가 미확인일 때
이 client 기본값이 적용된다는 점을 preview와 검증 결과에서 구분해야 한다.
gateway의 요청 예약용 output fallback을 provider의 실제 출력 한도로 복사하지
않는다. `/models` 목록 성공만으로 context 크기·출력 한도·tool 지원을 확정하지
않으며, 실제 installed-client fixture가 전송한 요청도 확인한다.

## Claude 최신 문서와 설치본의 차이

2026-09-13 [공식 설정 문서](https://code.claude.com/docs/en/settings)를 다시
확인했다. 설정 파일은 주석·trailing comma를 거절하는 strict JSON이다.
project-local 위치는 v2.1.211부터 일부 환경에서 repository root로 바뀌었고,
그 이전은 시작 directory를 사용했다. 현재 Cask directory 표기 2.1.63에 최신
경로 규칙을 그대로 적용하지 않는다. Task 4에서 실제 버전과 격리 fixture로
지원 경로를 확정한다. llmgw가 만든 local 파일이 자동으로 Git 제외됐다고
가정하지 않으며, 기존 `.claude.json` 인증·상태 파일은 편집 대상이 아니다.

같은 문서는 `env` 블록이 설정 우선순위를 따르되, shell 변수와 설정 키의
우선순위는 각각의 조합에 달렸다고 설명한다. 모든 환경변수가 항상 설정을
덮어쓴다는 일반 규칙을 만들지 않는다. 최신 `modelPicker`는 v2.1.242 이상이며
project/local 설정에서는 무시되므로 현재 설치본의 기능으로 간주하지 않는다.

[공식 gateway 연결 문서](https://code.claude.com/docs/en/llm-gateway-connect)는
`ANTHROPIC_CUSTOM_HEADERS`의 줄바꿈 구분과 settings `env` 사용, opt-in
gateway model discovery를 설명한다. 이는 Task 4의 조사 근거이며 설치본의
picker·요청 헤더·인증 성공 증거는 아니다. capability·context·tool 지원은
모델 목록과 구분해 검증하고, managed policy를 우회하는 설정은 생성하지 않는다.

## 기존 setup 적용 경로와 연결 순서

2026-09-13 현재 제품 소스를 읽어 확인했다. `setup/persist.rs`의 `RuntimeImpact`와
`ApplyMode`는 이미 미리보기한 worker identity·설정 fingerprint에 따라 저장,
시작, 명시적 재시작을 구분한다. 같은 `setup::apply` 경로가 config·pending
metadata를 저장하고 인증된 readiness를 반환한다. Task 4는 이 계약을 이어서
사용해야 한다. 별도 route writer나 PID 종료 경로를 만들면 Native Task 2에서
검증한 설정 경합·CA 사전 확인·기존 worker 보존을 우회할 수 있다.

`SetupDraft`는 현재 선택한 clients를 pending metadata로 보존하지만 실제
adapter를 실행하지 않는다. `cli.rs`의 setup 성공과 client 연결 성공은 아직
같지 않다. Task 4에서 선택한 도구의 대상 파일·scope·지원 버전·변경 hash를
먼저 미리보기하고, config 적용과 원하는 worker의 readiness 이후에만 해당
ConfigPatch를 실행해야 한다. 저장만, worker 시작 실패, 일부 client patch 실패를
각각 pending/실패로 표시한다. 이전에 저장한 client intent를 읽었다는 이유만으로
새로운 파일 변경까지 이미 검토됐다고 판단하지 않는다.

기존 TOML 편집기 `setup/document.rs`는 root/model 배열의 내부 주석과 변하지
않은 필드를 보존한다. 도구 전용 root 추가도 기존 사용자 root/model을 재작성하거나
없애지 않는 좁은 변경으로 연결하고, 프로토콜 허용 목록은 명시적으로 확인한
설정에서 가져온다. 어댑터가 endpoint를 만든다고 upstream이 그 프로토콜을
지원한다는 검증이 생기는 것은 아니다.

## route 하나와 에이전트 하나는 같지 않음

현재 공정 큐 단위는 등록된 root다. `config/validate.rs`는 최대 16개 root를
검증하며, [runtime 계약](../product/docs/runtime-contract.md)은 같은 root의
모든 session이 하나의 FIFO·배분 단위를 공유한다고 명시한다. session header는
관찰 정보이며 새로운 root나 지분을 만들지 않는다.

따라서 `connect pi`로 URL 하나를 설정한 뒤 Pi 여러 개를 실행했다는 이유만으로
각 Pi가 독립된 공정 배분을 받는다고 안내하면 안 된다. Task 4의 preview와
지원 문서는 실제 선택한 root를 표시하고 이 공유 범위를 설명해야 한다. 별도
에이전트별 자동 분류, 동적 root 등록, 세션 수에 비례하는 지분 확대는 이번 연결
작업에 덧붙이지 않는다. 이는 현재 제품 코드의 경계이며 새로운 효율 실험이나
성능 우위의 근거가 아니다.
