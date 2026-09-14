# Native client 연결 수용 관찰표

2026-09-13. 승인된 [Native Task 4 계획](../docs/superpowers/plans/2026-09-12-native-experience.md)의
실행 관찰 지점을 정리했다. **계획이며 아직 수행한 검증 결과가 아니다.**
버전·파일 문법·scope 근거는 [사전 확인](native-client-adapter-preflight.md)을 따른다.
ConfigPatch 명세·품질 수용 이후 하나의 구현자가 제품과 fixture runtime을 소유한다.

| 확인할 동작 | 최소 관찰·성공 기준 | 분리할 실패·미검증 |
|---|---|---|
| 실제 client와 설정 위치 | 실행한 binary/package 버전, override home, 절대 대상 파일, profile/scope 기록 | clone 버전·directory 이름·최신 문서만으로 실행 성공 판정하지 않음 |
| 변경 미리보기 | 선택한 client/root/model, 프로토콜, 변경 파일·소유 key, 가려진 credential 출처, 복원 명령, 정확한 apply hash | preview만 수행했을 때 파일·worker 변화 0; helper 실행 0 |
| 기존 gateway와 연결 | 검토한 config snapshot 적용 → 원하는 fingerprint의 인증된 readiness → client patch | 저장만 또는 재시작 실패 때 client bytes 변화 0; pending 결과 명시 |
| 모델 목록과 선택 | 해당 client가 실제 사용할 모델을 표시하고 선택한 ID로 요청 | 목록 404/unsupported와 inference 실패는 별도 필드; 모르는 capability는 미확인 |
| gateway 인증·전달 | client → loopback gateway → 합성 upstream의 route·model·프로토콜, local data token 수용 및 upstream 전달 제외 | raw headers/auth/body를 artifact에 저장하지 않고 구조·판정만 기록 |
| 실제 도구 왕복 | 합성 read 요청 → 격리 fixture 파일 결과 → 두 번째 모델 요청 → 최종 응답 | CLI exit 0만으로 통과하지 않음; 정확한 tool/응답 이벤트 검사 |
| gateway off 대조 | 같은 client 설정으로 요청 실패, upstream 요청 0; 다시 on 또는 disconnect 안내 | 예상 실패를 양성 성공 건수에 합치지 않음 |
| disconnect와 사용자 편집 | 현재 값이 llmgw가 쓴 값인 key만 이전 값/없음으로 복구; 바뀐 값·사용자 추가 내용 보존 | 파일 전체 덮어쓰기·공유 data-token 삭제 없음; partial/conflict 숨기지 않음 |

## 도구별로 더 확인할 것

- Pi: models는 comments-only JSON, settings는 strict JSON. custom header만으로
  picker에 보이는지 단정하지 않는다. keyless placeholder가 필요한 경우 실제
  upstream credential과 구분한다. 기존 명시 model metadata는 보존하고 Pi가
  채우는 기본 context/output 한도를 upstream 검증값으로 표시하지 않는다.
- Claude Code: Messages 경로와 native custom headers. user settings 또는 명시한
  private·untracked project-local 파일만 token 대상으로 사용한다. 설치 버전의
  scope와 env 우선순위를 실제 fixture로 확인한다. managed policy를 고치거나
  기존 auth database를 가져와 통과시키지 않는다. discovery는 별도 opt-in이다.
- Codex: 설치 버전이 지원하는 별도 `llmgw.config.toml` profile, Responses,
  `supports_websockets=false`. 임시 Codex home으로 실제 적용 여부를 확인하고,
  지원하지 않는 형식이면 정확한 skipped/unsupported 사유를 기록한다. 기존
  global config 변경이나 legacy profile fallback으로 대체하지 않는다.

모든 실제 client 실행은 개발 PC에 이미 설치된 버전과 임시 home/cwd/config,
합성 upstream만 사용한다. 구독 인증·로그인·유료 API·모델 다운로드를 이용하지
않는다. 프로토콜별 fixture가 없거나 인증 없이 실행할 수 없으면 그 항목을
미검증으로 남긴다. client 자체 telemetry·업데이트까지 gateway가 통제한다는
주장은 하지 않으며, fixture 실행 환경의 불필요한 외부 기능 비활성화와
제품 gateway의 통제 범위를 구분한다.

## 실행 비용과 증거 보존

실패 반례와 각 지원 도구의 한 정상 모델/도구 왕복을 먼저 수행한다. 이미 통과한
전체 성능 matrix를 이 설정 작업 때문에 반복하지 않는다. driver 산출물은 새
phase별 디렉터리에 쓰고, 실행 client/gateway/driver/config template의 해시,
실제 시도·응답 수, 예상 실패, child 종료·임시 상태 정리를 기록한다. Mac의 결과를
다른 OS 결과로 복사하지 않는다. latency/RSS와 저사양 효율은 최종 native artifact의
별도 측정이며 이 관찰표의 성공 항목으로 대체되지 않는다.

## 미리보기와 복원 후 실제 형식 확인

현재 ConfigPatch의 일반 preview는 공개 값도 `[reviewed value]`로 요약한다.
Task 4 adapter는 그 문자열만 출력하지 말고 검토할 주소·모델·프로토콜·scope와
정확한 대상 파일을 자체 요약에 표시해야 한다. 기존 주소·header에 민감한 값이
포함될 수 있으므로 기존 설정 전체를 그대로 출력하는 diff로 대체하지 않는다.

연결 해제 후에는 JSON/TOML 문법 검사에 더해 동일한 실제 client가 복원된
파일을 다시 읽는지도 확인한다. 설치된 Pi0.84.2의 `provider-composer.js:81–95`는
단순 JSON 파싱과 별도로 provider 조건을 검사한다. 새 중첩 provider를 leaf별로
만들고 지웠을 때 남는 빈 container가 실제 client에 허용되는지 추측하지 않는다.
필요한 동작은 고정 adapter의 검증에서 확인하며, ConfigPatch에 범용 schema
변환기를 추가하는 근거로 삼지 않는다.

## 첫 setup의 client 단계

Task 4 구현 중 확인한 token 생성 순서는 기존의 resource별 결과 계약으로
처리한다. 첫 setup은 core config 저장·readiness 이후 **같은 실행 안에서**
선택한 지원 도구의 connect 흐름을 이어간다. 이때 실제 token이 준비된 상태의
구체적인 client 파일 preview와 hash를 새로 보여주고, 해당 client 변경을 선택한
경우에만 적용한다. 최초 도구 선택을 파일 변경 승인으로 취급하거나, readiness
뒤에 새로 만든 patch를 이전에 검토한 것이라고 표시하지 않는다.

독립 `connect`가 새 route/port를 추가할 때는 앞선 표의 전체 client preview →
gateway 적용·readiness → 동일 patch 적용 순서를 유지한다. 첫 setup의 config
단계와 client 단계도 결과를 따로 표시한다. 저장만·취소·미지원·실패는 pending으로
남길 수 있지만 정상적으로 선택한 지원 client를 단순히 명령어 안내만 남기고
연결 완료로 처리하지 않는다. 이는 승인된 별도 resource 계약의 구현 해석이며,
아직 이 흐름의 실제 검증을 마쳤다는 뜻은 아니다.

Pi의 dedicated provider는 token을 포함한 object를 소유 단위로 삼는 좁은 확장을
허용했다. 삭제 뒤 빈 provider가 남는 문제를 피하고, 사용자가 해당 object를
변경하면 보존·conflict로 남긴다. unrelated preexisting provider 충돌은 자동으로
인수하지 않는다. 새 typed object도 token-bearing private/ACL 검사, hash 결합,
완전한 값 가리기를 따라야 하며 범용 비밀값 변환 framework를 만들지 않는다.

## Codex provider 복원과 설정 버전 확인

위 typed object 소유 원칙은 Codex의 dedicated `model_providers.llmgw`에도
동일하게 적용한다. ConfigPatch에 별도 parent-pruning flag나 journal 확장을
추가하지 않는다. 복원 후 빈 상위 `model_providers`가 남는 경우에는 실제
설치된 Codex의 해당 profile 로더가 허용하는지 확인한다. 확인 명령이 named
profile을 실제로 읽는다는 음성 대조도 필요하다. 별도 profile을 일부러 잘못된
문법으로 바꾼 격리 fixture에서 로딩 실패가 나야 하며, 이 결과는 inference나
도구 호출 성공으로 집계하지 않는다.

연결 preview hash는 client patch와 원하는 gateway config fingerprint를 함께
결합하고, 잘못되거나 낡은 hash는 worker 변경 전에 거절한다. 기존 클라이언트
인증이 임의 upstream으로 갈 수 있는 `auth=forward`의 자동 연결은 지원하지
않는다. native override 디렉터리는 실제 유효 경로로 해석하여 preview·버전
조회·쓰기에서 일치시킨다. 이 항목들은 구현 중 검토 경계이며 최종 수용 증거는
Task 4의 독립 검토에서 확정한다.

대화형 첫 setup은 preview hash를 내부에 보존하고 확인 선택 뒤 동일한 계획을
적용한다. 사용자가 64자리 hash를 다시 입력하게 하지 않는다. 별도 비대화형
`--apply-hash` 계약은 유지한다. 확인 이후 다시 생성한 patch의 새 hash를
자동 승인하는 식으로 구현하지 않으며, 오래된 snapshot은 기존 검사로 거절한다.

## 실행 가능성의 최소 확인

`fixture를 아직 만들지 않음`은 installed client가 인증 없이 실행될 수 없다는
증거가 아니다. 고정 Codex 소스의
`codex-rs/core/tests/suite/stream_no_completed.rs`는
`requires_openai_auth=false`인 Responses 모의 서버를 사용하고,
`codex-rs/exec/tests/suite/server_error_exit.rs`는 CLI의 SSE 오류 종료를 검사한다.
Task 4에서는 installed profile이 실제 선택되는 음성 대조에 이어 loopback으로
제한한 한 번의 installed 실행 가능성 확인을 한다. 성공하면 정확한 tool 왕복을
완성하고, 실패하면 관찰한 blocker를 기록한다. gateway를 건너뛰는 내부 SSE
fixture 환경변수로 통과시키지 않는다.
[고정 Codex 소스](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/core/tests/suite/stream_no_completed.rs).

Claude의 현재 공식 문서는 `-p` 실행과 API credential·subscription 구분을 설명한다.
이는 설치된 2.1.63의 실행 증거가 아니다. 해당 버전의 격리 `--help`와 loopback
fixture로 가능한 경로만 확인한다. 필요하다면 모의 서버용 비실제 placeholder를
사용하되, 기존 인증을 가져오거나 실제 API로 전송하지 않는다. 최신 옵션은
설치 버전에서 확인 없이 적용하지 않는다.
[공식 headless 문서](https://code.claude.com/docs/en/headless),
[공식 gateway 문서](https://code.claude.com/docs/en/llm-gateway).
