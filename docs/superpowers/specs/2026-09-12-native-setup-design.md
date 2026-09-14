# 네이티브 실행·첫 설정·클라이언트 연결 설계

2026-09-12. 사용자가 요청한 Docker/WSL 독립 실행, 간단한 on/off, Hermes 같은 첫 설정, 모델 목록과 설정 파일 변경 고지를 구체화한다. [코어 명세](2026-09-12-local-quota-gateway-design.md)의 v0.1을 사용한다. `llmgw`는 개발용 이름이며 아래 명령은 구현할 계약이다. 배포된 명령이라고 안내하지 않는다.

## 1. 작게 배포하고 쉽게 실행하기

Rust native executable 한 개와 작은 사용자 config/state만 필요하다. 사용자는 Python·Node·Redis·Docker·WSL·컴파일러를 설치할 필요가 없다. 개발/테스트 도구의 의존성과 최종 사용자 runtime을 구분한다. Windows x64, macOS arm64/x64, Linux x64/arm64 artifact를 대상별 빌드·smoke 뒤 배포한다. OS 기본 시스템 라이브러리까지 전혀 사용하지 않는다는 의미의 “무의존”은 쓰지 않는다.

설치는 사용자가 실행하는 얇은 OS별 installer가 검증된 archive를 사용자 쓰기 가능 경로에 풀고 PATH 반영 방법을 보여주는 형태다. 셸 profile/PATH 변경도 diff를 보여주고 선택받는다. 관리자 권한 상승, package manager 설치, 방화벽/백신 예외 추가는 하지 않는다. offline 환경은 archive+checksum과 수동 PATH 안내로 설치한다. 아직 release URL은 만들거나 광고하지 않는다.

## 2. CLI 계약

| 명령 | 동작 |
|---|---|
| `llmgw` | config가 없으면 setup. config가 있으면 on과 같은 동작. 토글하지 않음 |
| `llmgw setup` | 설정을 읽어 필요한 항목만 다시 묻고 변경 요약. 끝에서 실행/저장만 선택 |
| `llmgw on` | 같은 config의 실행이 있으면 상태 표시. 없으면 background 시작, readiness 확인 뒤 반환 |
| `llmgw off` | 새 입장 중단, queue 취소, 최대 10초 drain 뒤 자신 소유 worker 종료 |
| `llmgw restart` | 명시적으로 기존 worker를 10초 drain 후 새 config로 시작 |
| `llmgw status [--json]` | running/stopped/draining/starting/failed, port, queue, active, quota 상태, 자동 시작, 연동 상태 |
| `llmgw doctor` | config·포트·TLS/인증 방식·client 설정의 정적 진단. 생성 요청과 외부 연결은 하지 않음 |
| `llmgw doctor --network` | 지정 upstream에 제한된 TLS/GET models 확인. payload 전송/모델 생성 없음 |
| `llmgw doctor --inference` | model·출력 상한·호출 수를 표시하고 사용자가 선택한 1회 smoke 생성. 별도 명시적 실행 |
| `llmgw autostart on/off` | 다음 사용자 로그인 시 자동 실행 등록/해제. 현재 process 상태는 바꾸지 않음 |
| `llmgw connect pi\|claude\|codex` | 모델/route/scope를 선택하고 파일 변경·영향·복구 방법 preview. 명시적 apply 단계 |
| `llmgw disconnect pi\|claude\|codex` | gateway가 소유한 변경만 역적용. 사용자 후속 편집은 보존 |
| `llmgw run` | foreground. Ctrl-C로 같은 drain 수행. debugger/서버 manager용 |

공통 `--config <absolute-or-relative-path>`를 지원하고 상대 경로는 command 실행 시 절대 경로로 고정한다. 테스트는 temp config/state 경로만 사용한다. 비대화형 첫 실행은 질문을 기다리지 않고 setup 필요 오류와 sample config 경로를 반환한다. exit code는 정상/idempotent 0, 운영 실패 1, 잘못된 인수·미설정 2, 사용자 취소 130이다. `status`의 stopped는 성공적으로 조회했으면 0이고 상태는 JSON 필드로 판정한다.

`off`는 다음 로그인 자동 실행 설정과 client URL을 유지한다. 종료 메시지는 “현재 gateway는 꺼짐 / 다음 로그인 자동 시작: 켜짐 또는 꺼짐 / Pi·Claude·Codex 연결: 유지됨 — 다시 on 하거나 disconnect 필요”를 명확히 표시한다. 같은 명령을 두 번 실행해 예상과 반대로 켜지는 toggle은 만들지 않는다.

worker config는 immutable이다. disk fingerprint가 달라지면 `status`에 running과 별도로 `pending_restart=true`를 표시한다. `on`은 이 경우 기존 worker를 새 설정으로 켰다고 말하지 않고 exit 1 `restart_required`를 반환한다. `restart`는 active/queued 영향과 적용 fingerprint를 출력하고 명시한 종료·시작을 수행한다. setup/connect의 apply preview에도 이 영향이 포함된다. 새 route/port가 필요한 client patch는 선택한 restart가 readiness를 통과한 뒤에만 적용한다. 사용자가 저장만 선택하면 worker/기존 client 설정을 유지하고 pending 작업을 기록한다.

## 3. 첫 실행 마법사

상세 설정을 한꺼번에 보여주지 않는다. back/cancel이 가능하며 저장 전 취소는 아무 파일도 바꾸지 않는다. 기존 config가 있으면 값 유지가 기본이다.

1. **환경**: 로컬 LLM / 사내 LLM / 외부 API. 이는 질문의 기본값을 정하는 preset이며 API protocol을 추정해 확정하지 않는다. 로컬 모델을 다운로드하거나 서버를 켜지 않는다.
2. **연결**: upstream API base, 지원 형식(Completions/Responses/Messages 중 실제 제공하는 것), auth 방식, env reference. 사내 CA·proxy는 필요한 경우에만 질문한다. TLS 검증 비활성은 해결책으로 제공하지 않는다.
3. **모델**: 허용한 network 확인으로 목록 조회 또는 model ID 수동 입력. 목록을 못 가져오면 그 이유를 보여주고 수동 입력으로 설정을 완료할 수 있다. model ID·출력 상한·알려진 기능과 근거를 표시한다. 모델 목록 GET 성공과 inference 성공을 다른 상태로 저장한다.
4. **제한**: RPM·TPM 각각 숫자/모름/제한 없음, 같은 제한을 다른 PC도 공유하는지, 입력+출력 또는 별도 제한인지. v0.1이 정확히 모델링하지 않는 계약은 estimated로 표시한다. 숫자를 모르면 임의 값을 “자동 감지 완료”로 저장하지 않는다. concurrency는 1에서 시작하며 known cap을 입력할 수 있다.
5. **실행**: local port 기본 4141, 로그인 시 자동 시작 여부 기본 꺼짐. 자동 시작에 필요한 credential이 그 실행 환경에도 존재하는지 확인한다. env 참조가 login worker에서 안 보이면 configured-but-unavailable로 표시하며 secret을 plist/task에 복사하지 않는다.
6. **도구**: Pi/Claude Code/Codex/수동 중 선택. 설치 버전과 설정 scope를 표시하고 API·모델 기능 불일치를 점검한다. 아래 변경 preview를 보여준 뒤 선택한 도구만 연결한다.
7. **적용**: 기록될 경로, 원래/새 URL, 알려진 지원·미확인 기능, quota 추정 여부, 자동 시작, 자격 증명 출처를 요약한다. “저장하고 켜기 / 저장만 / 돌아가기”를 선택한다. apply 후 실제 readiness 결과와 다음 명령을 표시한다.

기본 setup은 외부 provider 목록·가격·telemetry·업데이트 조회를 하지 않는다. `--network` 확인을 사용자가 선택해도 지정 upstream에만 제한된 호출을 한다. “외부 API”를 선택했다고 결제·구독 인증 재사용·새 계정 생성을 자동 수행하지 않는다.

## 4. 모델과 설정 변경 고지

사용자가 요청한 고지는 단순 경고 배너가 아니라 **적용할 차이의 요약**이다.

```text
연결할 도구: Pi (감지한 버전 표시)
범위: 이 사용자 계정
변경: ~/.pi/agent/models.json — gateway provider 1개 추가
추가 변경: ~/.pi/agent/settings.json — 기본 모델 변경을 선택한 경우만
주소: 기존 API → http://127.0.0.1:4141/r/pi-work/v1
모델: 선택한 ID만 등록. reasoning / vision / tools는 확인한 값만 표시
주의: gateway 경로에서는 원래 모델 목록과 기능이 다를 수 있음
주의: gateway를 끄면 이 설정으로는 연결 불가
백업·복원: 보호된 backup 경로 / llmgw disconnect pi
[변경 diff 보기] [적용] [취소]
```

| 도구 | 연결 규칙·파일 | 반드시 알려줄 경계 |
|---|---|---|
| Pi | `~/.pi/agent/models.json`에 명시적 gateway provider/model 추가. defaults 변경을 선택했을 때만 `settings.json` 수정 | apiKey/auth 설정이 없으면 model이 picker에서 unavailable일 수 있음. baseUrl이 model metadata를 자동으로 채워주지 않음 |
| Claude Code | user `~/.claude/settings.json` 또는 사용자가 고른 project-local file의 `env`에 base URL·custom header 추가. 기존 auth source를 유지/교체할지 명시 | Messages 형식 필요. discovery는 별도 opt-in이며 picker 설정에 따라 숨겨질 수 있음. API key/token과 구독 login 선택은 사용량/과금 경로를 바꿀 수 있음 |
| Codex | user config 경로를 기준으로 **별도 gateway profile 파일** 생성, `codex --profile llmgw` 사용. provider base_url·wire_api·모델을 명시 | 최신 공식 reference는 Responses만 허용. OpenAI `/v1/models`와 Codex catalog는 같은 구조가 아님. provider/auth 설정은 project `.codex/config.toml`에 써도 적용되지 않는 최신 규칙이 있음 |

Codex custom provider는 `supports_websockets=false`를 명시하고 HTTP/SSE 경로로 검증한다. profile의 정확한 문법은 지원하는 client 버전으로 fixture를 실행해 확정한다. 2026-09-12 공식 문서의 `name.config.toml` profile 형식을 기준으로 하며 legacy 문법을 자동 추가하지 않는다. 지원하지 않는 버전은 업그레이드 필요/수동 안내로 표시한다. 현재 사용자 global config를 직접 덮어써 실험하지 않는다.

Claude model discovery 응답은 Anthropic 형식, Pi 모델 정의는 Pi 형식, Codex catalog는 Codex 형식이다. `GET models`는 선택한 upstream의 같은 형식을 그대로 전달한다. 다른 형식의 catalog를 서버에서 합성하지 않는다. 클라이언트에 필요한 명시적 모델 정보는 connect 시 생성하며 context/reasoning/tool capability를 이름만 보고 만들어내지 않는다. 알 수 없는 capability는 “미확인”이며 정상 tool round-trip을 확인하기 전 호환 완료를 표시하지 않는다.

설정의 파일 경로는 실제 home override를 반영한다. 문서의 `~/.pi`·`~/.codex`는 기본 위치 예시다. client나 회사가 더 높은 우선순위로 base URL·모델·인증을 지정하면 충돌 이유와 실제 적용 범위를 표시하고 관리 정책을 수정하지 않는다. 현재 shell의 값은 확인해도 dock/Start menu/다른 IDE 환경까지 모두 알았다고 표시하지 않는다. gateway 자체는 모델 요청 경로만 다루며 각 client의 모든 외부 통신을 통제한다고 설명하지 않는다.

## 5. 설정 파일의 소유권·백업·복원

`connect`는 version·format을 확인한 전용 adapter가 `ConfigPatch`를 만든다. 범용 설정 변환 framework를 만들지 않는다. TOML/JSON parser와 검증된 편집기를 사용하고 config 내 helper/command를 발견해도 preview 도중 실행하지 않는다. credential 값은 diff에서 가린다. 생성한 로컬 data token은 client의 기존 custom header literal 기능에 쓰되 사용자 전용 config·ACL로 보호한다. project 공유 파일에는 local token도 쓰지 않는다. control token은 복제하지 않는다. disconnect가 소유 header를 제거해도 다른 도구가 쓰는 gateway token 파일은 지우지 않는다.

apply 순서: 현재 bytes/hash 읽기 → patch와 검증 → 보호된 before-image와 소유 key 목록 기록 → 적용 직전 hash 재확인 → 같은 directory의 임시 파일에 쓰고 원자적 교체 → parse 재확인. 권한/ACL도 보존한다. 기존 client config가 다른 사용자에게 읽기 가능하면 local token 쓰기를 거절하고 사용자가 권한을 수정한 뒤 재시도하도록 안내한다. 자동 권한 강화는 하지 않는다. 도중 다른 프로그램의 수정이 감지되면 덮어쓰지 않고 새 preview가 필요하다. llmgw끼리는 편집 lock으로 직렬화한다. 비협조적인 외부 editor와 마지막 hash 검사/rename 사이의 경합을 완전히 차단하는 atomic compare-and-swap을 보장하지 않는다. backup과 post-write 재읽기로 확인 가능한 충돌을 보고하고 원본 복구 재료를 보존한다. 여러 파일 변경은 journal에 단계별 상태를 남기고 부분 성공을 감추지 않는다.

backups에는 기존 config의 credential이 있을 수 있다. 사용자만 읽을 수 있는 별도 state directory에 보관하고 공유/진단 bundle/저장소에 넣지 않는다. setup report에는 파일 경로와 결과만 남긴다. shell startup file·1Password 항목·client auth database를 몰래 수정하지 않는다.

disconnect는 “현재 값 == 우리가 쓴 값”인 key만 이전 값/없음으로 돌린다. 후속 사용자 편집은 conflict로 남긴다. 우리가 새로 만든 파일도 내용이 그대로일 때만 제거하며 달라졌으면 보존한다. 전체 backup을 무조건 덮어쓰는 restore는 없다. 여러 번의 connect는 같은 transaction을 중복 생성하지 않으며 새 변경은 이전 소유 정보를 이어받는다.

config, 자동 시작 등록, client patch는 각각 다른 resource다. 하나 실패했다고 전부 성공 표시하지 않는다. setup이 core config만 저장한 경우에도 사용자는 재실행해 남은 단계를 복구할 수 있다. 비대화형 apply에는 preview hash를 요구해 사전에 검토한 결과만 적용하게 한다.

## 6. 프로세스와 자동 시작

하나의 instance는 canonical config path와 그 전용 state directory로 식별한다. 배타적 OS file lock은 worker가 lifetime 동안 소유한다. `on` 두 개의 경쟁에서도 한 worker만 listen한다. readiness는 child spawn 성공이 아니라 인증된 local control handshake로 config fingerprint·instance nonce·listen 상태를 확인한 시점이다. upstream 연결 확인 결과는 별도 필드다.

`on`은 동일 executable을 shell 문자열이 아닌 argv로 background spawn한다. 표준 입출력은 terminal 수명과 분리하고 로그를 크기 제한한다. Windows creation flags, Unix session 분리는 platform module에서 처리한다. PID 파일만 보고 kill하지 않는다. `off`는 제어 채널로 stop 요청하고 소유 identity가 확인되지 않으면 다른 process를 죽이지 않는다. port 충돌·stale state·worker crash는 각각 구분한다.

작은 control interface: `health`, `status`, `stop`. config canonical path hash·instance nonce·시작 시각·bound address·진행 수를 반환한다. 로컬 토큰과 원문 URL 인증정보는 반환하지 않는다. `on` readiness deadline 5초, `off` 10초 drain+2초 확인. 응답 불능이면 failed/unknown으로 보고하고 무차별 강제 종료하지 않는다. Ctrl-C와 OS 종료도 같은 drain을 요청한다.

자동 시작의 공통 의미는 **로그인 후 사용할 사용자 서비스**이며 로그인 전 기계 부팅 daemon은 v0.1에서 제공하지 않는다.

| OS | 등록 방식 | 운영 계약 |
|---|---|---|
| macOS | 사용자 `~/Library/LaunchAgents` | `RunAtLoad`로 시작. `KeepAlive=false`로 수동 종료 즉시 부활 방지 |
| Windows | 해당 사용자 Task Scheduler logon trigger | least privilege, shell/WSL 없이 native executable. 재시작 policy 없음, 다중 instance 금지 |
| Linux | 사용 가능한 `systemd --user` service | user default target에 enable, `Restart=no`. linger 활성화/관리자 작업은 수행하지 않음 |

v0.1은 automatic crash restart를 제공하지 않는다. crash는 status/doctor로 드러내고 `on`으로 복구한다. 이 선택은 항상 재시작하는 manager와 수동 stop의 충돌·watchdog process를 없애기 위한 것이다. off와 autostart off의 차이를 사용자가 보게 한다. autostart on은 현재 실행을 켜지 않고 등록만 한다. OS가 등록 시 즉시 실행한다면 등록 절차에서 RunAtLoad 동작을 억제하고 다음 login 등록만 완료해야 한다.

Linux user manager가 없거나 회사 정책이 task 등록을 막으면 “자동 시작 미지원/차단”과 이유를 표시한다. core foreground/background 수동 on/off는 계속 제공한다. 다른 supervisor·Startup 폴더·WSL 우회 체인을 덧붙이지 않는다. 기존 user manager가 이미 linger 설정된 Linux는 login 전 실행될 수 있어 이 환경 차이를 표시한다. 바이너리 이동으로 등록 경로가 깨지면 doctor가 원인을 표시한다.

## 7. 구현 순서와 acceptance

1. loopback mock ↔ Rust HTTP/SSE ↔ 실제 Pi의 첫 경로. status와 foreground 종료까지 확인한다.
2. quota·fair queue·429·cancel을 동일 transport에 얹고 direct/FIFO와 비교한다.
3. native on/off, wizard, config patch/restore, OS별 자동 시작을 작은 독립 task로 완성한다.
4. 고정한 Pi/Claude/Codex 버전의 model 목록·선택·tool round-trip·retry를 격리 설정에서 확인한다.
5. Windows/macOS/Linux 실제 사용자 계정에서 설치→setup→on→요청→off→로그인→자동 실행→disconnect를 확인한 artifact만 지원 완료로 표시한다.

필수 실패 경로: non-TTY 첫 실행, setup cancel, 잘못된 URL/prefix, port 점유, 인증 env 누락, unknown quota, model listing 404, protocol 불일치, config 동시 편집, 부분 patch 후 crash, 사용자 후속 편집 후 disconnect, 두 on 경쟁, stale PID가 남의 process를 가리킴, off 뒤 즉시 자동 부활, API proxy가 loopback에도 적용되는 환경, 한글/공백 경로, installer PATH 미반영, login 시 secret 부재, 관리 정책 충돌.

네트워크 client는 loopback을 명시적으로 proxy bypass한다. upstream HTTP(S) proxy·OS/corporate CA는 사용자가 지정한 경로로만 적용한다. daemon 자체에 자동 discovery·업데이트·tokenizer download·credential helper를 주기 실행하는 loop를 추가하지 않는다. 설치/설정 command의 RSS는 상주 server의 RSS와 따로 측정한다.

근거와 조회일은 [설치·연동 조사](../../../reports/native-setup-research.md)에 기록한다. 설계 리뷰와 mock 통과는 실제 Windows 로그인 테스트를 대신하지 않는다.
