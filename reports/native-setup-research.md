# 네이티브 초기 설정·모델 목록·실행 수명 조사

확인일 2026-09-12. **공식 문서와 코드 확인**이며 Hermes 설치 실행, 실제 Codex/Claude 게이트웨이 연결, Windows 로그인 검증은 하지 않았다. 현재 사용자 client 설정도 변경하지 않았다.

## Hermes에서 참고할 것

공식 저장소 `NousResearch/hermes-agent`를 shallow sparse clone했다. commit `3b45681c25a880477a2a806cdebe91d2f1bfe9ce`, MIT. root와 `hermes_cli`, `scripts`, getting-started/user-guide 문서만 checkout했다. 기존 9개 gateway/client 분석 대상에 **설치 UX 참고 1개**를 추가한 것이며 gateway 경쟁 성능 비교 대상은 아니다.

| 직접 읽은 소스 | 확인한 내용 | 우리 선택 |
|---|---|---|
| `hermes_cli/setup.py:630`, `:654` | 첫 setup mode 선택, reset 전 backup | 짧은 wizard와 적용 전 backup 채택 |
| `hermes_cli/setup_quick.py` | 단계별 설정 후 summary, 기존 값 재읽기 | partial apply 상태·설정 소유권 명확화 |
| `hermes_cli/gateway_windows.py:423` | LogonTrigger, InteractiveToken, LeastPrivilege, instance policy | Windows 사용자 로그인 native 실행 참고 |
| `hermes_cli/gateway.py:3734`, `:4069` | launchd KeepAlive와 stop의 bootout 처리 | 우리는 v0.1 crash restart 제외, stop 의미 단순화 |
| root README Windows install | Python·Node·MinGit 등을 installer가 마련 | 실행 편의만 참고하고 의존성 묶음은 가져오지 않음 |

[공식 Quickstart](https://hermes-agent.nousresearch.com/docs/getting-started/quickstart/), [고정 소스](https://github.com/NousResearch/hermes-agent/tree/3b45681c25a880477a2a806cdebe91d2f1bfe9ce/hermes_cli). 현재 README는 Windows native 경로를 제공한다. 일부 문서의 beta/지원 표현은 서로 달라, 그 표현으로 우리 Windows 호환을 주장하지 않는다. “Hermes처럼 간편한 설치”가 동일 runtime 구성을 뜻하지 않는다.

## 클라이언트마다 다른 모델 목록과 설정

| 도구 | 출처에서 확인한 계약 | 설계 영향 |
|---|---|---|
| Pi | `models.json`의 custom provider/model과 auth availability. `settings.json`의 defaults는 별도 | discovery만으로 picker 생성 보장 금지. 선택한 provider 추가와 default 변경 분리 |
| Claude Code | gateway discovery opt-in, settings의 `env`, picker 설정에 따른 숨김 | 모델 목록·protocol·설정 scope를 적용 전에 보여주기 |
| Codex | provider `base_url`, Responses wire API, 별도 profile 파일, `model_catalog_json` | `/v1/models`가 자동으로 Codex model catalog가 된다고 가정하지 않기 |

Pi 코드/문서 기준은 clone 0.85.1이고 이미 실행한 fixture는 설치 버전 0.84.2다. [모델 문서](https://github.com/earendil-works/pi/blob/f3c672245d25ef2283ffc0d9cdec8a5482651103/packages/coding-agent/docs/models.md), [기존 실제 CLI probe](client-integration-validation.md)를 구분한다.

Claude는 `ANTHROPIC_BASE_URL`에 Messages gateway를 지정한다. `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1`로 추가 목록을 활성화해도 picker 설정에 따라 숨겨질 수 있다. settings-file env와 shell의 우선순위, user/project/managed scope도 달라진다. key/token을 설정하면 기존 subscription login과 다른 credential·과금 경로가 선택될 수 있으므로 connect에서 보여준다. [연결 문서](https://code.claude.com/docs/en/llm-gateway-connect), [설정 범위](https://code.claude.com/docs/en/settings).

Anthropic의 gateway 문서는 provider schema 차이와 최신 beta header/body의 쌍을 보존할 필요를 설명한다. non-Claude 모델로의 routing은 Anthropic이 지원하는 경로가 아니므로 Messages 형태를 맞췄다는 이유만으로 공식 지원이라고 말하지 않는다. 모델 inference·tool 흐름은 버전별 fixture와 실제 요청으로 확인해야 한다. [Protocol guide](https://code.claude.com/docs/en/llm-gateway-protocol), [gateway 개요](https://code.claude.com/docs/en/llm-gateway).

Codex 공식 문서의 현재 profile은 `~/.codex/name.config.toml`이며 CLI `--profile name`으로 선택한다. project config에서 provider/auth 변경은 무시되는 규칙이 있어 user 영역에 별도 profile을 만든다. reference는 `wire_api=responses`만 지원한다고 명시한다. advanced 문서의 일부 범용 provider 예제를 보고 Chat Completions 호환으로 확대 해석하지 않는다. `supports_websockets`, request/stream retry도 별도 항목이다. [Advanced config](https://learn.chatgpt.com/docs/config-file/config-advanced), [Configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference). 설치된 package 0.154.0과 docs 문법의 실제 fixture 검증은 아직 하지 않았다.

## 네이티브 자동 시작

Apple의 LaunchAgent/launchd, Microsoft의 logon trigger, systemd의 user manager가 기존 패턴이다. 기계 부팅 전 system service와 사용자 로그인 작업을 혼동하지 않는다. user manager/회사 정책에 따른 지원 여부를 setup에서 보여준다.

- [Apple launchd 문서](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html): ProgramArguments와 KeepAlive가 process 수명에 영향. 오래된 공식 문서로 안정된 OS 개념 확인에 사용한다.
- [Microsoft logon 실행](https://learn.microsoft.com/en-us/windows/win32/taskschd/starting-an-executable-when-a-user-logs-on): 로그인 trigger를 사용한다. 실제 native 계정 테스트는 별도다.
- [systemd 공식 원문](https://github.com/systemd/systemd/blob/main/man/systemd.special.xml): user service manager용 target은 system target과 구분된다. freedesktop HTML은 조회 시 403이라 공식 repository 원문을 확인했다.

추가 supervisor 없이 `on/off`와 자동 시작 상태를 분리한다. 자동 crash restart가 없는 v0.1 계약으로 수동 off의 부활 문제를 피한다. 추후 crash recovery를 추가한다면 동일한 수명 테스트를 먼저 확장한다.

## 확인하지 않은 것

구현 의존성 사전 확인: `cargo info`로 Axum 0.8.9(MSRV 1.80), Reqwest 0.13.5(1.85), Clap 4.6.6(1.85), Dialoguer 0.12.0(1.66), toml_edit 0.25.15(1.85)의 package metadata를 읽었다. 현재 Rust 1.88에서 각 직접 dependency의 최소 버전은 충족하지만 전체 dependency graph 빌드는 아직 하지 않았다. 사용자 실행에 Cargo가 필요하다는 의미는 아니다.

Reqwest의 내려받은 `src/retry.rs`와 `src/async_impl/client.rs`에서 default retry를 끄는 API, redirect policy, OS 검증 TLS feature와 CA merge API를 확인했다. [공식 retry API](https://docs.rs/reqwest/0.13.5/reqwest/retry/fn.never.html). Axum은 body를 직접 읽으면 `DefaultBodyLimit`이 적용되지 않을 수 있으므로 ingress의 전체 byte budget과 per-body 한도를 직접 연결해야 한다. [공식 body limit 문서](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html). 공통 기능을 재작성하지 않되 기본값만으로 자원 상한이 구현됐다고 간주하지 않는다.

모든 OS의 실행 파일·서명·백신·회사 설치 정책, 실제 client model picker와 tool-call 호환, actual TPM 추정 오차와 성능 우위는 미확인이다. source를 읽은 결과는 설치/연동 완료가 아니다. 최종 계약은 [코어](../docs/superpowers/specs/2026-09-12-local-quota-gateway-design.md)와 [설정](../docs/superpowers/specs/2026-09-12-native-setup-design.md)에 있다.
