# 로그인 자동 시작 구현 전 확인

2026-09-13. Native Task 4 구현과 병행한 **공식 문서·고정 참고 소스·제품 코드의 읽기 전용 확인**이다.
Task 5 구현이나 OS 등록·다음 로그인 성공의 증거가 아니다. 승인된
[Task 5 계약](../docs/superpowers/plans/2026-09-12-native-experience.md)을 바꾸지 않고
템플릿에서 놓치기 쉬운 기본값과 검증 경계를 정리한다.

## 실행할 프로세스와 등록 상태

현재 제품의 `cli.rs`에서 `run`은 `lifecycle::worker`를 직접 기다리고,
`on`은 별도 worker를 띄운 뒤 readiness를 확인하고 반환한다.
`lifecycle/process.rs`가 그 child의 native 분리를 담당한다.
Unix의 `server.rs::shutdown_signal`은 Ctrl-C와 SIGTERM을 받는다.
이는 소스 확인이며 이번 조사에서 signal이나 gateway를 실행한 것은 아니다.

systemd 문서는 ExecStart 프로세스와 서비스 수명의 관계를 설명한다.
따라서 **설계 적용 제안**은 OS manager가 실제 장기 실행 프로세스를 관리하도록
foreground `run`을 사용하는 것이다. `on`이 반환한 것을 장기 실행 서비스라고
취급하거나 새 셸·PID 감시 wrapper를 덧붙이지 않는다. OS의 시작 판정과 gateway의
인증된 readiness는 계속 구분한다.
[공식 service 원문](https://raw.githubusercontent.com/systemd/systemd/main/man/systemd.service.xml).

macOS의 plist 설치와 현재 launchd job 로드는 다른 상태다. 승인 계약대로
등록 시 즉시 bootstrap하지 않고, autostart off도 현재 worker를 종료하는
bootout으로 구현하지 않는다. Linux의 enable/disable은 `--now`가 없으면 현재
프로세스의 시작/종료를 뜻하지 않는다. 같은 이름이 global scope에도 enable되어
있으면 user disable만으로 다음 시작을 막았다고 단정할 수 없다는 공식 설명도 있다.
소유한 config별 label/unit만 조작하고 조직·다른 범위의 등록을 지우지 않는다.
[공식 systemctl 원문](https://raw.githubusercontent.com/systemd/systemd/main/man/systemctl.xml).

## Linux 인수는 셸 문자열이 아님

`ExecStart`는 따옴표 처리 외에도 환경변수 확장을 수행한다. 파일 경로에 있는
달러 기호를 무조건 안전한 literal로 취급하면 안 된다. 공식 원문은 `$$` 또는
환경변수 확장을 끄는 `:` prefix를 설명한다. 구현 시 한 방식으로 통일하고
실제 지원하는 manager에서 인수가 원래 경로로 전달되는지 확인한다.
퍼센트 기호도 specifier이므로 literal은 `%%`다. 공백·한글뿐 아니라
`$`, `%`, backslash와 quote가 있는 config 경로를 fixture에 포함한다.
셸을 추가해서 escaping을 우회하지 않는다.
[service 인수 규칙](https://raw.githubusercontent.com/systemd/systemd/main/man/systemd.service.xml),
[percent specifier](https://raw.githubusercontent.com/systemd/systemd/main/man/standard-specifiers.xml).

## Windows 장기 실행 기본값

Microsoft 문서에 따르면 Scheduled Task의 실행 시간 제한 기본값은 72시간이며,
`ExecutionTimeLimit=PT0S`로 제한을 없앨 수 있다. `StopIfGoingOnBatteries`의
기본값은 true다. 로그인 후 켜 둔 gateway가 일정 시간이나 전원 전환만으로
꺼지는 동작을 의도하지 않았다면 해당 값을 템플릿에 명시해야 한다.
[실행 시간 제한](https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-executiontimelimit),
[배터리 전환 중지](https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-stopifgoingonbatteries).

고정 Hermes SHA `3b45681c25a880477a2a806cdebe91d2f1bfe9ce`의
`hermes_cli/gateway_windows.py:423–480`도 무제한 실행 시간, 배터리 중지 비활성,
network/idle 조건 비활성, `WakeToRun=false`, `IgnoreNew`, `InteractiveToken`,
`LeastPrivilege`를 명시한다. 이 파일에는 VBS launcher·자동 재시작·Startup fallback도
있으나 승인된 llmgw 범위에는 포함되지 않는다. 필요한 native task 조건을 참고하고
실행 파일을 직접 지정한다. Task Scheduler의 Hidden 속성이 console 창을 숨긴다고
추정하지 않으며, Windows의 실제 창·프로세스·로그인 동작은 별도 확인 대상이다.
[고정 참고 소스](https://github.com/NousResearch/hermes-agent/blob/3b45681c25a880477a2a806cdebe91d2f1bfe9ce/hermes_cli/gateway_windows.py#L423).

## 검증 범위

템플릿 검증은 argv/escaping·등록 명령·무재시작·무비밀값·현재 runtime 보존을 확인한다.
실제 user manager, Windows scheduler 정책, 다음 로그인과 login 환경의 auth 가용성은
별도 관찰이다. 빈 service 환경 테스트에 현재 shell의 자격 증명을 복제하지 않는다.
등록은 되었지만 auth가 없는 상태를 configured-but-unavailable로 표시한다.
이 사전 확인에서는 사용자 등록, client 설정, 제품 소스, reference clone을 바꾸지 않았다.

## Windows 등록 해제와 현재 실행

2026-09-13 추가 확인: Microsoft의 `schtasks delete` 문서는 작업 등록을 지워도 그 작업의 프로그램 파일이나 현재 실행 중인 프로그램을 중단하지 않는다고 설명한다. 따라서 정확히 소유한 task 하나를 `/Delete /TN <label>`로 해제하고 `/End`를 호출하지 않는 접근은 현재 worker를 보존하는 계약과 맞는 문서 근거가 있다. wildcard나 전체 task 삭제를 사용하지 않는다. 이 문서 확인은 실제 Windows에서 nonce 생존을 관찰한 결과가 아니다. [Microsoft 공식 문서](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/schtasks-delete).
