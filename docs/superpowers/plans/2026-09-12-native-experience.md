# Native Setup and Client Connection Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 설치한 binary 이름만으로 setup/on/off를 수행하고, 로그인 자동 실행과 Pi/Claude/Codex 연결을 영향 preview·백업·안전한 역적용과 함께 제공한다.

**Architecture:** [코어 계획](2026-09-12-gateway-core.md)의 실행되는 server/control 위에 one-shot CLI 기능을 추가한다. wizard가 typed config와 ConfigPatch를 만들고, OS module이 native process/login registration을 담당한다. 서버 hot path는 wizard·config editor를 호출하지 않는다.

**Tech Stack:** 같은 Rust package, Clap 4.6.6, Dialoguer 0.12.0(default features off), toml_edit 0.25.15, serde_json. native locks/secure file replacement/platform APIs는 현재 설치 crate와 공식 API를 확인해 최소 라이브러리를 선택한다. Python stdlib은 fixture driver에만 사용한다.

---

상태: **Native Task 1–6의 제한된 구현·검증 범위 수용; 실제 OS·계정 수용 조건은 아래 미완료 항목에 유지**. Core Task 1~8은 수용됐으며, 현재 실행·검증·플랫폼별 미완료 항목은 [구현 기록](../../../evidence/implementation-progress.json)에 둔다. Windows 보호 모듈의 좁은 FFI 경계는 [구현 결정](../../../reports/native-platform-decision.md)을 따른다. 규범은 [설정 명세](../specs/2026-09-12-native-setup-design.md)와 [코어 명세](../specs/2026-09-12-local-quota-gateway-design.md)다. core Chunk 1이 먼저 동작해야 한다. 아래 명령은 모두 research `product/`에서 실행한다. `@test-driven-development`, `@verification-before-completion` 적용. template rendering과 실제 OS 등록/로그인 검증을 구분한다. 기본 테스트는 temp home/state만 사용하며 현재 사용자 startup/client 설정을 변경하지 않는다.

## File map

| 파일 | 책임 |
|---|---|
| `src/lifecycle/{mod,identity,process,macos,linux,windows}.rs` | control/lock/fingerprint, background start, OS 등록 |
| `src/setup/{mod,prompts,validate,summary}.rs` | wizard, 단계별 검증, preview 모델 |
| `src/clients/{mod,pi,claude,codex}.rs` | client 버전·scope·patch 생성 |
| `src/config_patch/{mod,apply,restore,journal}.rs` | 파일 변경·역적용·부분 완료 기록 |
| `tests/{lifecycle_contract,setup_contract,patch_contract,client_profiles,autostart_templates}.rs` | 행동/경합/상태 검증 |
| `scripts/{verify_clients,verify_native}.py` | 격리 client/OS acceptance driver |
| `packaging/{install.sh,install.ps1}`, `docs/{installation,client-compatibility}.md` | release 후 설치·지원 설명 |

한 파일이 역할 두 개를 품으면 API 경계로 분리한다. Windows/macOS/Linux의 구현 차이는 platform 파일에만 둔다. OS service library를 감싼 범용 plugin architecture는 만들지 않는다.

## Chunk 1: lifecycle과 간단한 첫 실행

### Task 1: on/off/status와 소유 identity

**Create:** lifecycle mod/identity/process/platform files, `tests/lifecycle_contract.rs`. **Modify:** `src/cli.rs`, `src/control.rs`, `src/server.rs`.

- [x] temp config로 두 `on`을 동시에 실행하는 fixture와 `off` 반복·stale PID·port 점유·다른 config fingerprint를 작성한다. `cargo test --test lifecycle_contract` → 기존 명령 부재 또는 상태 assertion FAIL.
- [x] 유지보수되는 file lock API를 선택해 worker lifetime lock을 만든다. PID가 아니라 canonical config hash+nonce+control token으로 본인 worker를 확인한다. Windows는 native detached creation, Unix는 session 분리; shell string으로 명령을 만들지 않는다. `on` readiness 5초, `off` 10초 drain+2초 확인.
- [x] `on` 같은 fingerprint→상태 반환, 다른 fingerprint→`restart_required`/exit1, `restart`→drain 후 새 worker, `status` stopped 조회→exit0를 구현한다. handshake 실패 시 pid kill 금지. quota cooldown은 running 상태와 별도 필드다.
- [x] `cargo test --test lifecycle_contract` → PASS. 실행한 fixture PID/port를 teardown하고 unrelated fixture process가 살아 있음을 확인한다. 실행 중 요청이 있는 off에서 연결이 닫히고 자원 정리가 끝나는지 검증한다.

### Task 2: wizard와 비대화형 동작

**Create:** setup 모듈 네 파일, `tests/setup_contract.rs`. **Modify:** cli/config/runtime docs.

- [x] scripted input/output interface를 tests에 둔다. local/corporate/external preset, back/cancel, unknown/unlimited/known, 잘못된 URL prefix, missing env, listing404, non-TTY, 저장만/저장후시작을 작성한다. `cargo test --test setup_contract` → FAIL.
- [x] UI와 질문의 결과인 typed SetupDraft를 분리한다. 각 답변은 명세 순서로 받으며 RPM/TPM 0 거절, protocol 미확인 경고, local server download/start 없음, login default off를 구현한다. Dialoguer는 command 때만 초기화한다. 유효하지 않은 URL로 network call하지 않는다.
- [x] summary에 config/state 경로·생성될 URL·estimated quota·auth reference·automatic login 상태·선택된 clients를 넣는다. network model 확인과 paid-capable inference smoke를 별도 명령/선택으로 둔다. doctor 기본은 offline이다.
- [x] `cargo test --test setup_contract` → PASS. `llmgw` 첫 실행 no-TTY가 prompt 무한 대기 없이 exit2, cancel은 exit130/파일 변화0, 저장만은 process0을 검증한다. tty 실제 navigation은 별도 native smoke에 포함한다.

### Task 3: ConfigPatch transaction과 안전한 disconnect

**Create:** config_patch 모듈, `tests/patch_contract.rs`. **Modify:** setup summary.

- [x] temp JSON/TOML에 unrelated keys·comments·synthetic credential을 넣고 apply/restore, 동시 변경, partial two-file write, connect 두 번, 사용자 편집 후 disconnect를 작성한다. `cargo test --test patch_contract` → FAIL.
- [x] `Patch { path, expected_hash, edits, owned_keys, warnings }`를 구현한다. parse/validate→protected backup→직전 hash 검사→same-directory atomic replace→parse 재확인. llmgw 편집끼리 lock, 다른 editor와 CAS 보장은 없음. 기존 권한/ACL이 다른 사용자에게 읽기 가능하면 token 쓰기를 거절하고 권한 수정 후 재시도하도록 안내한다. 자동 chmod/ACL 강화는 하지 않는다. permission/ACL 보존 및 이 거절 API를 Unix와 Windows에서 나눠 검증한다.

```text
restore(key):
  if current == written_by_llmgw: set previous_value_or_absent
  else: preserve current; record conflict
new-file restore:
  remove only if current bytes still equal the created file
```

- [x] 로컬 data token을 사용자 전용 config에만 복제하고 upstream credential/control token은 복사하지 않는다. journal에는 보호된 backup 참조·소유 key·hash·단계만 저장하며 일반 진단에 값은 내보내지 않는다. 보호된 before-image backup에 기존 credential이 포함되는 것은 복구용 예외이며 새로운 client/provider field로 credential을 복사하는 것과 구분한다. config 파일 내 command를 preview 과정에서 실행하지 않는다.
- [x] `cargo test --test patch_contract` → PASS. 비대화형 apply가 검토한 preview hash만 받는지, crash 후 partial state와 복구 방법이 표시되는지, disconnect가 사용자 후속 설정을 보존하는지 확인한다. upstream credential/local data token/control token에 서로 다른 synthetic sentinel을 넣고 diff/stdout/stderr/journal/doctor 결과에 원문이 없음을 assert한다. unrelated key 변경으로 전체 hash가 달라도 소유 key는 복구하며, 반복 connect 뒤 최초 소유 이전 값으로 복원되는지 확인한다.

수용 근거: [최종 수정본](../../../product/artifacts/native-task3/nonfinite-fix/README.md), [품질 재검토](../../../evidence/native-task3-quality-review/rereview-q3/README.md), [명세 최종 확인](../../../evidence/native-task3-spec-review/final-confirmation/README.md). Mac 전체313·focused49개와 독립 QUALITY10·SPEC4개를 통과했다. 안전하게 복원할 수 없는 소유 TOML 날짜·비유한 값은 쓰기 전 거절하며, 다른 필드의 값은 보존한다. Windows/Linux 실제 실행은 아직 미검증이다.

## Chunk 2: 도구 연동·로그인 실행·실제 배포 수용

### Task 4: Pi/Claude/Codex adapter와 모델 고지

수용 완료: [최종 구현본](../../../product/artifacts/native-task4/quality-fix/README.md), [품질 재검토](../../../evidence/native-task4-quality-review/rereview-q1-q2/README.md), [최종 명세 확인](../../../evidence/native-task4-spec-review/final-confirmation/README.md). 같은 release에서 전체 Rust339개·release 연동24개·Python7개와 실제 설치된 세 client의 격리 도구 왕복을 확인했다. 독립 QUALITY7개·PTY2회, 최종 SPEC8개를 통과했다. Claude canonical/Git 공유 범위, 확인 후 readiness, 연결 해제 후 오래된 계획의 소유권을 보완했다. 원인 미확인 Mac ACL 오류 1회는 보존하며 실제 Windows/Linux 실행과 새 성능 측정은 미검증이다.

**Create:** clients 모듈, `tests/client_profiles.rs`, `scripts/verify_clients.py`, `docs/client-compatibility.md`. **Modify:** setup/cli/ConfigPatch.

- [x] 기본/override home에 synthetic client config를 둔다. resolved home의 `~/.pi/agent/models.json` provider 추가와 선택적 `~/.pi/agent/settings.json` default 변경, Claude user `~/.claude/settings.json` 또는 명시한 project-local `.claude/settings.local.json`의 env/custom headers 보존, resolved Codex home의 별도 `llmgw.config.toml` profile을 절대 경로 preview fixture로 만든다. project-local token 저장은 파일이 사용자 전용 권한이고 Git 추적/공유 대상이 아닌 것으로 확인한 경우만 허용한다. 공유 `.claude/settings.json`에는 token을 넣지 않는다. unsupported version/managed policy/higher-priority env도 넣는다. `cargo test --test client_profiles` → FAIL.
- [x] 설치 version을 읽고 adapter가 실제 지원하는 format만 생성한다. Pi baseUrl+api+model metadata, Claude Messages base와 opt-in discovery, Codex Responses와 `supports_websockets=false`, 세 도구의 `X-LLMGW-Token`을 각각 native custom header로 설정한다. root route는 고정 config에 먼저 반영한다.
- [x] 새 route/port이면 preview에 running worker 영향과 restart를 표시한다. restart 선택→새 fingerprint readiness→client patch 순서를 지킨다. restart 실패 때 client patch가 적용되지 않는 실패 fixture를 반드시 추가한다. 저장만이면 client를 미완성 route로 바꾸지 않는다. model listing 성공·model 선택·inference·tools는 서로 다른 검증 필드다. unverified capability를 자동 true로 만들지 않는다.
- [x] `cargo test --test client_profiles` → PASS. `python3 scripts/verify_clients.py --binary target/debug/llmgw --client pi --output artifacts/pi-profile.json`에서 picker/list·선택·tool 왕복을 검증한다. Claude와 Codex도 설치/인증 없이 fixture 실행 가능한 버전만 같은 driver로 검증하고, 못 실행한 항목은 skipped 사유로 남긴다. 최신 docs만으로 installed version PASS를 만들지 않는다.
- [x] connect preview·off notice·disconnect conflict 메시지를 문서 예시와 맞춘다. chat subscription 인증을 임의 upstream에 전달하거나 가입/결제하지 않는지 fixture에서 확인한다. 실제 client가 별도 수행하는 telemetry/업데이트까지 gateway가 막는다고 쓰지 않는다.

### Task 5: 사용자 로그인 자동 시작

[제한된 구현 범위 수용](../../../evidence/native-task5-accepted.json). 최종 명세 2개·품질 14개 사례를 통과했다. [사전 확인](../../../reports/native-autostart-preflight.md)과 [구현·검증 연결표](../../../reports/native-autostart-acceptance-map.md)에 따라 진행한다. 실제 OS 등록·로그인은 아직 실행하지 않았다.

**Modify:** lifecycle platform files. **Create:** `tests/autostart_templates.rs`, `scripts/verify_native.py`.

- [x] absolute binary/config path에 공백·한글·XML metacharacters를 넣은 test로 argv/escaping·least privilege·run-at-login·no restart·no secret 주입을 확인한다. `cargo test --test autostart_templates` → FAIL.
- [x] macOS는 LaunchAgent file의 ProgramArguments/RunAtLoad/KeepAlive=false, Windows는 사용자 logon task의 executable argv와 interactive token/least privilege/no restart, Linux는 user unit의 absolute ExecStart/default target/Restart=no를 구현한다. autostart on은 등록만, off는 등록 해제만 한다. 현재 process를 stop/start하지 않는다. macOS는 plist를 설치만 하고 RunAtLoad job을 즉시 bootstrap하지 않는다. Linux enable/disable에 `--now`를 쓰지 않으며 Windows task 등록 후 Run을 호출하지 않는다.
- [x] 등록 상태와 runtime 상태를 각각 조회한다. user manager 부재/조직 정책 차단은 수동 run/on/off를 유지하면서 이유를 표시한다. 자동 privilege escalation, Windows Startup 폴더 fallback, Linux linger 변경을 추가하지 않는다.
- [x] `cargo test --test autostart_templates` → PASS. `python3 scripts/verify_native.py --templates-only --output artifacts/native-templates.json`으로 OS 도구의 read-only validation을 가능한 플랫폼에서 실행한다. stopped에서 autostart on 직후 worker=0, running에서 autostart off 직후 같은 nonce의 worker가 살아 있는지도 fixture로 확인한다. 빈 login service environment에서 auth env 누락이 configured-but-unavailable로 나오고 token이 OS 등록에 복제되지 않는지 확인한다. 이것을 실제 로그인 PASS로 기록하지 않는다.
- [ ] 실제 OS의 임시 사용자 계정에서 `python3 scripts/verify_native.py --exercise-user-service --output artifacts/native-runtime.json`을 실행한다. 이 모드는 생성/제거할 label/path를 먼저 출력하고 명시적 실행으로만 사용한다. 설치→수동 on/off→다음 login→autostart off→다음 login을 관찰하며 사용자 로그인 단계는 사람이 수행한 시각을 기록한다. terminal 수동 실행의 인증 가용성과 login worker의 인증 가용성을 다른 결과 필드로 기록한다. Mac 한 대의 결과를 Windows/Linux에 복사하지 않는다.

### Task 6: 작은 installer·지원 상태·측정 마무리

[구현 및 macOS 격리 패키지 검증 수용](../../../evidence/native-task6-accepted.json). 첫 SaveOnly 뒤 연결 미리보기가 실패하던 I1도 독립 재검토를 통과했다. 최종 Rust 370개, 실제 설치된 Pi·Claude·Codex, 설치·재설치·새 셸 PATH와 메모리·시작/종료 관측을 확인했다. 아래 두 미완료 항목의 설치 구현과 Mac 개발 호스트 검사는 완료됐지만, 서명·격리 및 다른 OS·깨끗한 네이티브 계정 조건까지 완료한 것은 아니다.

**Create:** packaging 두 파일, `docs/installation.md`. **Modify:** compatibility/benchmark/runtime docs.

- [x] archive/checksum manifest와 fake local release server로 installer fixture를 만든다. 잘못된 hash·없어진 artifact·PATH 미반영·권한 부족·재설치를 검사한다. installer가 registry URL을 발명하거나 현재 승인되지 않은 release를 게시하지 않게 한다.
- [ ] 사용자 경로에 검증된 binary를 설치하고 PATH 변경 preview와 offline unzip 방법을 구현한다. 공개 one-liner는 실제 release가 만들어진 후에만 문서에 넣는다. code signing/OS quarantine은 해당 OS 배포 정책대로 검증하며 백신 예외 추가로 통과시키지 않는다.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked` 실행. 각 OS artifact의 build/toolchain/lock SHA와 설치 smoke, wizard peak RSS, idle server RSS, on/off 지연을 기록한다. Python·Node·Redis·Docker·WSL이 없는 native 계정에서 offline archive 실행도 확인한다. 현재 확보하지 못한 OS는 unsupported/unverified 상태를 유지한다.
- [x] setup가 hot path에 file watch·discovery loop·secret helper·telemetry를 추가하지 않았는지 profile과 코드 경계로 확인한다. 지원 표는 실제 tested client/protocol/OS 조합별로 작성한다. 연결 설정 backup/restore와 끈 상태의 안내를 실제 사용자 흐름에서 확인한다.

## 승인 경계와 종료 조건

제품 구현은 진행 승인 범위다. **현재 사용자** client config나 login 자동 시작 등록은 테스트 fixture로 대신하고, 필요하면 적용할 diff/label을 먼저 구체화한다. Git 결재안 후보는 `feat: add native gateway lifecycle and setup`, `feat: add reversible coding-agent connections`, `build: package native gateway binaries`이며 실제 파일·분리·브랜치·검증·롤백을 채운 뒤 승인받는다. 아직 정해지지 않은 remote/branch를 만들거나 push하지 않는다.

완료 결과는 실제 명령의 지원 상태·OS 검증 표·변경 preview/복원 시연·측정 예산이다. 스킬의 명세/계획 검토가 끝났다는 이유만으로 구현 또는 성능 우위를 완료로 표시하지 않는다.
