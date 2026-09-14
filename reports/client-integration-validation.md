# Pi 클라이언트 연결·재시도 검증

검증일: 2026-09-11. 설치된 **Pi 0.84.2**, Node v25.7.0, macOS 26.5.1 arm64를 사용했다. 분석용 Pi clone은 별도 SHA `f3c672245d25ef2283ffc0d9cdec8a5482651103`, package version 0.85.1이다. **실행한 0.84.2와 현재 clone의 소스를 같은 버전으로 보고하지 않는다.** 최신 clone에서 같은 패턴이 보이더라도 그 버전의 실행 결과는 아니다.

## 직접 관찰한 결과

실제 Pi CLI의 print mode가 loopback HTTP fixture로 요청했다. 게이트웨이는 아직 없으며, 모델이 생성한 응답도 아니다. [smoke 기록](../evidence/reference-tests/pi/smoke.json)과 [7개 추가 사례](../evidence/reference-tests/pi/client-behavior.json)에 버전·소스 hash·설정·시도별 시각·HTTP 경로를 남겼다.

| 사례 | upstream에서 관측한 요청 수 | Pi 종료 | 확인 내용 |
|---|---:|---:|---|
| 정상 SSE | 1 | 0 | `/v1/chat/completions` POST, stream=true, 고정 응답 출력 |
| 계속 429, 기본 재시도 | 4 | 1 | 최초 1회 + agent 재시도 3회 |
| 계속 429, `retry.enabled=false` | 1 | 1 | 기본 provider retry=0과 조합하면 추가 요청 없음 |
| 첫 429에 `Retry-After: 5`, 다음은 성공 | 2 | 0 | 요청 도착 간격 4.008초. 이 경로에서는 5초를 기다리지 않음 |
| agent retry=1, provider retry=1, 계속 429 | 4 | 1 | 두 계층의 재시도가 중첩됨. agent backoff=50ms, provider header=25ms인 별도 fixture |
| 429 + `insufficient_quota` | 1 | 1 | quota 소진 오류는 추가 요청 없이 실패 |
| 첫 stream은 content와 DONE은 있으나 finish_reason 없음, 다음은 정상 | 2 | 0 | content를 받은 뒤에도 agent 차원에서 다시 요청 |
| session affinity 옵션 활성화 | 1 | 0 | 아래 세션 헤더 3개 전달 |

8개 사례 모두 사전에 정한 **요청 수·프로세스 종료 코드**와 일치했다. 이것이 모든 동작이 우리가 원하는 정책이라는 뜻은 아니다. 429의 1은 fixture가 의도한 실패 종료이며 harness 실행 오류가 아니다. 성공 사례는 `fixture-ok` 출력을 별도로 확인했다.

시각은 HTTP 서버가 기록한 monotonic clock이다. 기본 429 반복의 요청 간격은 약 4.009 / 4.013 / 8.009초였다. 설치된 설정 소스의 기본 backoff는 2/4/8초지만, **관측한 요청 간격을 순수 backoff 시간으로 단정하지 않는다.** 시작·클라이언트 처리·transport가 함께 관여하며 추가 시간의 원인은 이번 조사에서 분리 계측하지 않았다. 지연 수치를 Pi 또는 게이트웨이의 성능 벤치마크로 쓰지 않는다.

## 재시도 소유권에 대한 설계 영향

설치된 `pi-ai/dist/utils/provider-retry.js`는 provider retry 횟수가 남아 있을 때 Retry-After를 파싱한다. 기본 횟수는 0이다. 반면 coding-agent의 `_prepareRetry`는 agent backoff 설정으로 대기한다. 이 소스 경로와 5초 전에 재요청한 관측은 일치하지만, 모든 API·Pi 버전에서 Retry-After를 무시한다는 일반화는 하지 않는다.

현재 clone의 [settings 문서](../references/pi/packages/coding-agent/docs/settings.md)는 agent retry 기본 3, provider retry 기본 0을 설명하고 중첩을 주의시킨다. [`provider-retry.ts`](../references/pi/packages/ai/src/utils/provider-retry.ts)와 [`agent-session.ts`](../references/pi/packages/coding-agent/src/core/agent-session.ts)에도 두 정책이 분리돼 있다. 외부에서 읽을 수 있는 [공식 문서](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/settings.md)는 변경될 수 있어 clone SHA를 근거로 삼는다.

우리 비교 실험에서는 기본 Pi와 재시도를 조절한 Pi를 별도 arm으로 둔다. gateway가 재시도를 담당하는 실험에서는 임시 Pi 설정의 agent/provider 재시도를 명시적으로 끄고, 기본 설정을 그대로 쓰는 통합 실험도 별도로 남긴다. 사용자의 전역 설정을 몰래 바꾸지 않는다.

각 계층이 요청마다 독립 예산을 재설정한다면 최대 시도 수는 `(agent retries + 1) × (provider retries + 1) × (gateway retries + 1)`로 커질 수 있다. 이번에 직접 확인한 것은 gateway가 없는 Pi의 2×2=4 사례다. gateway를 포함한 곱셈과 실제 task 비용은 아직 미검증이다.

게이트웨이가 시작한 stream을 재전송하지 않더라도, client가 오류를 보고 새 요청을 보낼 수 있다. 따라서 “우리 gateway 내부의 stream replay 없음”과 “사용자 작업 전체의 중복 생성 없음”을 구분한다. 후자를 보장하려면 client 협력과 논리 요청 식별 계약이 필요하다.

## 세션·요청 식별

기본 OpenAI Completions 설정에서는 검사한 `session_id`, `x-client-request-id`, `x-session-affinity`, `x-session-id` 헤더가 없었다. 우리가 설정한 `x-research-root: fixture-root`는 모든 사례에서 전달됐다. 이는 설정 header가 전달된다는 증거이며 인증이나 parent/child 관계 검증은 아니다.

`compat.sendSessionAffinityHeaders=true`를 켜면 `session_id`, `x-client-request-id`, `x-session-affinity`가 들어왔다. 세 값의 hash가 같았다. 설치된 `createClient`와 clone의 [`openai-completions.ts`](../references/pi/packages/ai/src/api/openai-completions.ts)는 세 헤더에 같은 sessionId를 넣는다.

따라서 **`x-client-request-id`라는 이름만 보고 요청마다 유일한 idempotency key라고 가정하면 안 된다.** 이 값은 공정 큐의 세션 힌트로는 검토할 수 있지만, 서로 다른 turn의 응답을 재사용하거나 중복 POST를 합치는 근거가 되지 않는다. parent/child agent가 같은 root budget을 쓴다는 보장도 별도로 필요하다.

헤더 값은 원문 대신 hash만 기록했다. 이 fixture의 세션은 임시로 만든 것이며 사용자 세션·계정과 관계없다.

## 실행 격리와 재현

[probe-pi-client.py](../scripts/probe-pi-client.py)는 Python 표준 라이브러리와 이미 설치된 Pi만 사용한다. 추가 npm 패키지 설치나 전역 업데이트를 하지 않았다.

- 매 사례마다 임시 `PI_CODING_AGENT_DIR`와 빈 working directory를 만든다. 사용자 Pi 설정·auth 파일을 읽도록 지정하지 않는다.
- `--offline`, `--no-approve`, `--no-tools`, `--no-extensions`, `--no-skills`, `--no-prompt-templates`, `--no-themes`, `--no-context-files`, `--no-session`으로 실행한다.
- startup 외부 네트워크를 끄고, 선택한 provider를 127.0.0.1 임시 포트로 고정한다. 환경은 PATH·LANG과 해당 격리 플래그만 전달하며 실제 API credential은 넘기지 않는다.
- request body·authorization 값·원시 CLI 출력은 저장하지 않는다. tool_count=0을 실제 요청에서 확인했다.
- 각 사례는 25초 process timeout을 가지며 임시 파일·HTTP server를 정리한다.

```sh
python3 scripts/probe-pi-client.py --case success-default --output evidence/reference-tests/pi/smoke-new-run.json
python3 scripts/probe-pi-client.py --output evidence/reference-tests/pi/all-new-run.json
```

설치된 Pi가 업데이트되면 결과의 package version·소스 hash가 달라진다. 그 실행은 새 baseline으로 기록해야 한다. `--offline`은 선택한 모델 endpoint에 대한 요청까지 차단하는 기능이 아니라 startup network 제어이므로 loopback provider 설정이 함께 필요하다.

## 아직 검증하지 않은 것

Claude Code·Codex 실제 실행, Pi RPC/TUI, 실제 tool loop·subagent, Responses·Anthropic Messages API, 최신 Pi 0.85.1 실행, Windows/Linux, 실제 사내 또는 로컬 모델, gateway를 포함한 성능·quota 정합성은 미검증이다. 이 결과만으로 세 도구 호환이나 사용자 과제 완료를 표시하지 않는다.

다음 제품 수용 조건에 반영할 것은 ① 실제 HTTP 시도 수 집계 ② Retry-After 동안 group admission 제어 ③ client의 stream 오류 후 재요청 ④ 명시적 session/root 설정 ⑤ session affinity와 논리 요청 ID의 분리다.
