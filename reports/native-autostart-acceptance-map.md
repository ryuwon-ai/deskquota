# Native Task 5 구현·검증 연결표

2026-09-13. 승인된 [네이티브 설계](../docs/superpowers/specs/2026-09-12-native-setup-design.md)와 [Task 5](../docs/superpowers/plans/2026-09-12-native-experience.md)의 실행 준비다. 제품 구현·OS 등록·로그인 성공 증거가 아니다. 기존 제품·공식 문서 근거는 [사전 확인](native-autostart-preflight.md)에 있다.

| 계약 | 기존 연결점 및 최소 변경 | 필요한 검증 |
|---|---|---|
| config별 사용자 등록 | `lifecycle::identity::canonical` / `StatePaths::path_hash`의 identity 사용. 기존 보호 파일·argv primitive부터 확인. core worker에 감시 루프 추가 금지 | 두 config의 등록이 충돌하지 않고, 표시한 absolute 실행 파일/config 경로와 생성한 등록 내용이 일치 |
| `autostart on/off`는 등록만 | OS manager가 직접 foreground `run`을 실행하도록 연결. `on`의 detached child를 감시하는 wrapper 불필요. 수동 worker의 기존 lifetime/control 계약 유지 | stopped에서 등록 후 worker 0; running에서 해제 후 같은 nonce worker 생존. 실제 OS 등록 없이 소유한 fixture로 우선 검증 |
| macOS | 사용자 LaunchAgent, ProgramArguments, RunAtLoad, KeepAlive=false. 설치만 하고 즉시 bootstrap하지 않음; 해제 시 bootout으로 runtime을 끄지 않음 | 공백·한글·XML 메타문자 argv와 plist read-only validator. 다음 로그인 실행은 별도 실제 계정 관찰 |
| Windows | 현재 사용자 LogonTrigger, InteractiveToken, LeastPrivilege, IgnoreNew, 재시작 없음. 실제 executable/arguments, 작업 등록 후 Run 없음 | quoting 및 task XML. 장기 실행 시간·전원 조건을 명시. 정책 차단·실제 창·다음 로그인은 Windows에서만 판정 |
| Linux | user default target, Restart=no, 절대 ExecStart; enable/disable에 `--now` 없음 | `$`, `%`, quote/backslash 포함 literal argv, user manager 부재·이미 설정된 linger 표시. 관리자 작업이나 대체 supervisor 금지 |
| 상태 분리 | CLI의 일회성 출력에서 `autostart: not_implemented`를 실제 조회 결과로 교체. 내부 readiness용 `lifecycle::status`에는 OS manager 실행을 넣지 않음. worker 실패와 등록 상태를 따로 보고 | 등록 상태, 현재 인증된 runtime, 로그인 환경의 인증 가용성을 각각 확인. manager 확인 불가를 disabled나 running으로 단정하지 않음 |
| 마법사 연결 | `SetupDraft::login_requested()`와 pending metadata는 의도만 보관함. 저장된 config를 바탕으로 실제 OS 대상/경로/내용을 새로 보여주고 등록 단계의 결과를 별도로 표시 | 선택 취소·등록 차단 시 core config와 수동 on/off 보존. SaveOnly·client 연결 실패가 실제 등록 성공을 뜻하지 않음 |
| 비밀값·소유 범위 | OS 등록에는 실행 파일/config 경로만. 현재 shell auth를 복사하거나 조직·타 config의 등록을 수정하지 않음 | 빈 login 환경에서 auth 누락을 configured-but-unavailable로 구분. token/credential이 등록 내용에 없는지 합성 값으로 확인 |

템플릿 통과, 명령 fixture 통과, 실제 manager 등록, 다음 로그인 실행은 서로 다른 증거다. `verify_native.py --templates-only`는 가능한 read-only OS validator만 실행한다. `--exercise-user-service`는 생성/제거할 label/path를 먼저 보여주는 명시적 수동 실행 경로이며, 이번 자동 작업에서는 현재 사용자 계정에 실행하지 않는다. 실제 임시 OS 계정과 사람의 로그인 관찰이 없으면 그 항목을 미검증으로 남긴다.

구현 착수 전 소스에는 autostart CLI가 없었으며, `setup/persist.rs`의 pending `login_requested`와 `setup/prompts.rs`의 보류 안내가 있다. 구현자는 새 command와 실제 등록 결과가 연결된 뒤 보류 문구를 갱신한다. client patch·core config·OS 등록을 하나의 추측성 transaction framework로 합치지 않는다. 필요한 최소 module과 OS별 primitive만 추가하고, HTTP/SSE·quota·fairness 경로와 의존성을 유지한다.

## 등록 조회가 readiness 대기에 들어가지 않도록 유지

소스에서 `lifecycle::start`는 최대 5초 동안 `lifecycle::status`를 25ms 간격으로 다시 호출하며, `on_inner`와 `restart`도 같은 내부 상태 경로를 사용한다. 이 내부 함수의 현재 placeholder를 OS manager 명령 실행으로 단순 교체하면 on/readiness 한 번에 등록 조회 subprocess가 여러 번 생길 수 있다. 이는 코드에서 확인한 호출 구조이며 실제 overhead 측정 결과는 아니다.

구현 시 worker/control 상태 조회와 사용자에게 보여 줄 등록 조회를 구분한다. OS 등록 조회는 필요한 일회성 CLI `status`/`doctor`/`autostart` 단계에서 수행하고, readiness loop·HTTP 요청 처리에는 넣지 않는 방향이 적절하다. 범용 observer나 cache를 새로 만들기보다 기존 출력 조합 경계에서 필요한 필드만 붙인다. 최종 on/off 측정은 이 구분이 실제 구현에 유지됐는지 함께 확인한다.

## 확인한 실제 파일 경계

계획의 `lifecycle/{macos,linux,windows}.rs`는 예정 경로다. 현재 구현의 보호 파일·분리 primitive는 `lifecycle/platform/{mod,unix,windows}.rs`에 있다. Task 5는 이 구조와 `fs4`, `rustix`, `windows-sys`, `serde_json`, `sha2` 등 설치된 의존성을 먼저 확인한 뒤 작은 autostart 모듈을 추가한다. 기존 platform 보호 기능을 서비스 템플릿으로 재작성하거나 범용 서비스 추상화를 만들 필요는 없다.

현재 CLI에서 `run`은 `lifecycle::worker`를 직접 기다리며 hidden `Worker` 명령만 `prepare_worker`의 session 분리를 호출한다. 따라서 foreground `run`을 OS 등록의 실행 대상으로 삼는 방향은 현재 소스 구조와 맞는다. 이는 읽기 전용 호출 구조 확인이고 실제 OS 실행 결과는 아니다. worker의 lifetime lock·인증된 control identity는 그대로 사용한다.
