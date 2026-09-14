# Native lifecycle 구현 전 점검

2026-09-12. Task 8 측정 중 수행한 **로컬 소스·설치 metadata·공식 API의 읽기 전용 점검**이다. 제품 변경, 새 dependency 설치/resolve/build, worker 실행, 사용자 client 설정 변경, OS 자동 시작 등록은 하지 않았다. Native Task 1의 구현·수용 결과가 아니다.

## 이미 사용할 수 있는 것

제품은 Clap 4.6.6, Reqwest 0.13.5, Tokio 1.49.0, toml_edit 0.25.15, SHA-256, rand를 이미 사용한다. control의 health/status/stop과 10초 drain도 존재한다. 새 supervisor framework, 별도 상주 서비스, HTTP client/parser를 추가할 이유는 없다. 현재 `Cargo.toml`은 Rust 1.88 및 제품 소스의 `unsafe_code = forbid`를 명시한다. `rustup toolchain list`에서 설치된 toolchain은 1.88 하나였다.

로컬 registry에는 rustix 1.1.4와 Dialoguer 0.12.0 소스가 있다. fs2/fs4/nix 소스는 해당 registry inventory에서 발견하지 못했다. **다운로드 캐시에 존재하는 것과 제품 dependency인 것은 다르다.** rustix의 source manifest는 MSRV 1.63, `process` feature를 제공하며 `src/process/id.rs:234`의 `setsid()`는 safe API다.

| 책임 | 확인한 기존 API | 구현 판단에 사용할 경계 |
|---|---|---|
| worker lifetime lock | Rust 표준 `File::try_lock`은 1.89부터 제공 | 현재 1.88에는 없다. toolchain을 조정해 표준 API를 쓰거나 유지보수되는 작은 library 한 가지를 선택한다. 두 경로를 자동 fallback으로 유지하지 않는다. |
| 1.88을 유지하는 lock 후보 | fs4 1.1.0의 sync feature, MSRV 1.75 | Unix는 rustix, Windows는 windows-sys를 사용한다. async runtime feature는 lifetime try-lock에 불필요하다. 아직 설치·제품 빌드 검증 전이다. |
| Unix session 분리 | rustix 1.1.4 `process::setsid()` | child가 worker 준비를 시작할 때 호출하는 경로를 검토한다. std `CommandExt::process_group(0)`은 process group 설정이며 session 분리와 같지 않다. std `CommandExt::setsid`는 조회한 stable 문서에서도 nightly API다. |
| Windows native spawn | std `CommandExt::creation_flags` | shell 없이 같은 executable과 argv를 전달한다. DETACHED_PROCESS와 CREATE_NEW_CONSOLE은 함께 쓰지 않는다. CREATE_NO_WINDOW도 DETACHED_PROCESS와 쓰면 무시된다는 OS 규칙을 따른다. |

출처: [Rust File lock](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock), [fs4 1.1.0 API](https://docs.rs/fs4/1.1.0/fs4/), [fs4 고정 manifest](https://docs.rs/crate/fs4/1.1.0/source/Cargo.toml), [Unix CommandExt](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html), [rustix setsid](https://docs.rs/rustix/latest/rustix/process/fn.setsid.html), [Windows CommandExt](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html), [Microsoft creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags).

표준 라이브러리 조회 문서는 1.98.1을 표시했다. 이것을 현재 제품의 1.88에서 해당 API가 빌드된다는 증거로 쓰지 않는다. POSIX 온라인 setsid 원문은 이 환경에서 403이었으며, 그 본문을 확인했다고 기록하지 않는다.

## 현재 소스에서 다음 Task가 연결해야 할 부분

- `src/config.rs`의 StatePaths는 현재 config의 **부모 디렉터리** 아래 `.llmgw`를 사용한다. 같은 디렉터리의 서로 다른 config는 현재 같은 state 경로를 계산한다. Native 명세의 canonical config별 identity를 충족하려면 이 동작과 관련 fixture를 Task 1에서 함께 정리해야 한다. 기존 token/state 경로의 자동 마이그레이션은 만들지 않는다.
- `LoadedConfig`에 canonical source path와 config bytes의 fingerprint가 이미 있다. 두 값을 섞지 않고 path identity와 running config fingerprint를 구별해 재사용한다. 같은 내용의 다른 config와 내용이 바뀐 같은 config를 각각 검사한다.
- control health는 현재 `status=ok`, status는 runtime counters를 제공한다. Native readiness에는 instance nonce, path identity, running fingerprint와 bound address가 추가로 필요하다. token과 원문 upstream URL은 반환하지 않는다.
- `src/main.rs`는 현재 `#[tokio::main]`으로 CLI 분기 전에 runtime을 만든다. child session 준비 시점과 one-shot setup/doctor 초기화 비용을 확인한다. 새 scheduler나 thread-count 설정을 덧붙이지 않는다.
- 현재 `src/server.rs`는 이미 준비된 token 파일을 읽는다. Unix에서는 regular file·no-follow open·group/other mode bit를 검사하지만, non-Unix permission validator는 통과를 반환한다. 이는 Windows ACL 검증 구현이 아니다. Native에서 state/token 생성 책임을 추가할 때 플랫폼별 보호·검사를 함께 연결하고, Windows build 성공이나 Mac의 mode-bit 검사만으로 Windows ACL 및 모든 파일 ACL을 검증했다고 표시하지 않는다. 기존 사용자 파일의 권한을 자동 수정하는 경로는 만들지 않는다.

위 항목은 기존 Core Task의 결함 판정이 아니라, 그 위에 승인된 Native lifecycle 계약을 연결할 때 필요한 변경 범위다. 측정 중인 Task 8 source는 그대로 유지한다.

Task 8 측정·부모 검산 뒤 fixture 경로도 확인했다. 현재 부모 디렉터리의
`.llmgw`를 직접 만드는 곳은 제품 `scripts/benchmark.py`, `scripts/probe_pi.py`,
`tests/config_contract.rs`와 research의 `scripts/probe-product-{cli,stream,fairness,retry}.py`다.
Config별 state 경로를 바꾸는 Native Task 1에서 이 연결 지점도 함께 갱신해야
이전 fixture가 다른 경로에 token을 써서 실패하지 않는다. 예전 경로 fallback은
추가하지 않는다. 기존 측정 source/archive는 그대로 두며 이후 binary용 fixture와
구분한다. 현재 doctor는 경로·fingerprint를 사람용 문장으로 출력하므로, fixture가
상태 경로를 얻는 방식은 기존 CLI/typed config 경계에서 단순하게 정한다.

## 수용 검사에 포함할 구체적인 경계

1. lock은 소유 worker가 종료할 때까지 동일 file object에 유지한다. lock 중 경로를 지우고 다시 만드는 방법은 서로 다른 file object에 lock을 잡는 위험이 있으므로 채택하지 않는다. stale runtime JSON과 lifetime lock을 별개로 취급한다.
2. 두 `on` 경쟁, 서로 다른 config, stale PID가 남의 프로세스를 가리키는 경우를 실제 임시 프로세스로 확인한다. PID 단독으로 stop/kill하지 않는다.
3. config의 listen 주소가 바뀌어도 이전 worker의 기록된 bound address에서 인증된 identity를 확인한 뒤 종료해야 한다. 새 config의 포트만 조회해 이전 worker를 잃어버리지 않게 한다.
   실행 후 TOML 내용을 잘못 편집한 경우에도 종료를 위해 새 설정의 전체 유효성 검증을 요구하는지 확인한다. 기존 worker의 소유 identity와 인증은 확인해야 하지만, 새 upstream 설정을 적용하는 `on/restart` 검증과 현재 worker를 정리하는 `off`의 책임은 구별한다. 파일 삭제나 깨진 symlink를 추측으로 다른 state 경로에 연결하는 fallback은 만들지 않는다.
4. control 연결은 loopback proxy bypass 및 짧은 deadline을 사용한다. 새 route/config의 readiness 전에는 client 설정을 적용하지 않는다.
5. background stdio는 terminal 수명과 분리한다. 오류 로그는 상한을 지키고 credential이나 요청 본문을 포함하지 않는다. stop이 응답하지 않는 경우를 성공으로 표시하지 않는다.
6. Windows detached flag 조합, Unix session 분리, OS 로그인 등록은 각각 해당 OS의 실제 동작으로 확인한다. Mac의 통과 결과를 다른 OS에 복사하지 않는다.
7. known RPM 또는 TPM의 시작 후 60초 admission hold와 authenticated control readiness를 구분한다. `on`의 5초 readiness는 요청을 즉시 upstream에 보낼 수 있다는 약속이 아니다. setup/on/status가 시작 보호·공유 cooldown·실행 상태를 설명해야 하며, 새 주기 작업 없이 기존 monotonic 상태를 조회한다. 현재 status는 큐의 blocked reason을 제공하므로 **빈 큐에서도 시작 보호 상태를 설명할 수 있는지** Native Task 1에서 확인한다. startup hold를 없애거나 벤치마크의 무대기 latency에 섞어 숨기지 않는다.

다음 결정은 Task 8 결과의 명세·품질 검토 후 Native Task 1에서 한다. 현재 product dependency, toolchain, lifecycle 구현은 변경하지 않았다.

## 권한 API 후보 추가 확인

2026-09-12 Task 8 QUALITY 재검토 중의 읽기 전용 후보 조사다. 로컬 registry의
manifest 경로 검색에서는 rustix 1.1.4 외에 fs4/fs2, Windows ACL 전용 crate를
찾지 못했다. 제품에 이미 있다고 가정하지 않는다.

- [windows-permissions 문서](https://docs.rs/windows-permissions/latest/windows_permissions/)는
  0.2.4와 SID/ACL/security descriptor의 safe wrapper를 설명한다. 개별 고정 버전
  manifest/trait 페이지는 이번 웹 조회가 실패했으므로 MSRV, 필요한 생성·검사 API의
  완결성, 유지보수 상태는 아직 확인하지 못했다. 이 조사만으로 채택하지 않는다.
- [Trail of Bits windows-acl 원 저장소](https://github.com/trailofbits/windows-acl)는
  ACL 조회·변경 API를 제공하지만 README의 테스트 절차는 관리자 실행을 전제로 한다.
  그 절차를 현재 사용자 PC에서 실행하지 않았으며, 우리 제품의 일반 사용자 동작을
  입증하는 근거로 쓰지 않는다.
- [exacl 문서](https://docs.rs/exacl/latest/exacl/)는 macOS/Linux/FreeBSD의 파일
  ACL을 다룬다. Windows까지 포함한 공통 구현으로 볼 수 없다.

이 후보들의 API 존재와 우리 token/state를 생성 시점부터 보호할 수 있는지는 다른
질문이다. 구현자는 선택한 library의 실제 타입·platform 코드를 확인하고, 검증하지
못한 OS를 성공으로 반환하지 않아야 한다. 새 의존성 설치나 제품 코드 변경은 이
추가 조사에 포함되지 않았다.

후속 탐색에서는 같은 문서의 탐색 링크로
[WindowsSecure](https://docs.rs/windows-permissions/latest/windows_permissions/trait.WindowsSecure.html)를
열 수 있었다. `T: AsRawHandle`에 대한 security descriptor 조회와 DACL 변경 API는
존재한다. 최초 고정 버전 URL 조회 실패와 후속 API 확인을 구분한다. 현재 사용자
token SID 획득 및 보호된 최초 생성까지 이 crate만으로 해결되는지는 미확인이다.

[WinSafe 원본 manifest](https://github.com/rodrigocfd/winsafe/blob/master/Cargo.toml)는
조회 시 0.0.29/MSRV 1.87, `advapi -> kernel`을 표시했고 dependency 항목은 없었다.
이는 master 조회이며 고정 배포본의 빌드 검증이 아니다.
[접근 token API](https://docs.rs/winsafe/latest/winsafe/struct.HACCESSTOKEN.html)의
safe `GetTokenInformation`은 typed `TokenInfo`를 반환한다.
[CreateFile](https://docs.rs/winsafe/latest/winsafe/struct.HFILE.html)과
[SECURITY_ATTRIBUTES](https://docs.rs/winsafe/latest/winsafe/struct.SECURITY_ATTRIBUTES.html)
setter도 존재한다. 그러나
[SECURITY_DESCRIPTOR](https://docs.rs/winsafe/latest/winsafe/struct.SECURITY_DESCRIPTOR.html)는
raw Owner/Dacl 포인터 필드를 노출한다. 이 조각들만으로 안전한 ACL 구성·생성·검사
체인이 완성됐다고 판단하지 않는다. 서로 다른 wrapper의 포인터를 임의 변환해
프로젝트의 unsafe 금지를 우회하지 않는다. 제품 채택이나 Windows 실행은 아직 없다.

## 2026-09-13 Native Task 2 품질 검토: 종료 전 준비 검사

품질 검토의 실제 loopback/PTY에서 잘못된 새 CA 경로를 저장하고 시작하면 기존
worker를 종료한 뒤 HTTP client 생성에 실패하는 사례가 발견됐다. 현재
`lifecycle::restart_inner`의 종료 전 검사는 config와 credential을 확인하지만,
Task 2에서 추가한 CA bundle의 읽기·PEM 해석은 worker의
`transport::upstream::build_client`에서 수행된다. 아직 수정 수용 결과가 아니다.

고정된 llama-swap `41ec321b…`의 `llama-swap.go:320–376`은 새 config를 읽고
새 server 구성을 만드는 데 실패하면 반환하며, 구성을 만든 뒤에 이전 server를
종료한다. 이는 기존 상태를 끄기 전에 새 구성의 준비 가능성을 확인하는 패턴의
근거다. 우리 제품에 hot reload·대체 worker·watcher를 추가하자는 뜻은 아니다.

최소 수정은 이미 있는 `build_client` 경로로 같은 proxy/TLS 설정을 **네트워크
호출 없이** 검사하고, 재시작의 stop 전에 실패를 반환하는 것이다. 별도 CA parser나
설정 감시 loop를 만들 필요가 없다. 실제 수정에서는 오류 시 기존 worker identity
보존, 정상 CA로 시작, 실제 upstream 요청 0을 확인해야 한다. setup의 저장 결과와
runtime 결과도 구분한다. CA 파일의 외부 편집까지 원자적으로 막는 보장은 하지 않는다.
