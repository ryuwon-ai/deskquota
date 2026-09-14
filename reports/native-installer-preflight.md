# Native installer 구현 전 확인

2026-09-13. 고정 clone의 설치 파일을 읽은 **소스 근거**다. 설치 스크립트를
실행하거나 현재 사용자 PATH·서비스·파일을 바꾸지 않았다. 새 공개 release나
Windows/Linux 설치 성공의 증거가 아니다.

- PromptForge의 `dist-workspace.toml`은 native archive를 깨끗한 머신에서
  설치·실행한 뒤 release하도록 별도 gate를 둔다. 파일 안에는 생성된 announce
  job 조건을 수동 보완해야 한다는 제약도 적혀 있다. 설정이 존재한다는 사실과
  실제 CI가 성공했다는 사실을 구분한다.
  [고정 소스](https://github.com/cppalliance/promptforge/blob/da1456b545f655f3743c4b76b099689fc70aa7b4/dist-workspace.toml).
- Helicone의 dist 설정은 OS target을 명시하고 updater 설치를 끈다. npm/Homebrew
  publish와 별도 tap은 해당 제품의 배포 구성이다. llmgw의 미확정 registry나
  release 주소로 복사하지 않는다. 이 clone의 commit 날짜는 2025-11-20이므로
  최신 배포 상태의 근거로 사용하지 않는다.
  [고정 소스](https://github.com/Helicone/ai-gateway/blob/9649b27bdc9fb0907d359e899894102a15f3a085/dist-workspace.toml).
- llama-swap의 Linux installer는 관리자 권한과 Python을 요구하고 system 사용자·
  서비스·기본 모델 설정도 만든다. llmgw의 사용자 경로·단일 실행 파일 설치와
  범위가 다르므로 스크립트를 그대로 채택하지 않는다.
  [고정 소스](https://github.com/mostlygeek/llama-swap/blob/41ec321b6216d838488b2a7d936274ed227c0c5e/scripts/install.sh).

승인된 Task 6에서는 작은 두 installer와 offline archive 경로로 진행한다.
검증한 실행 파일의 설치, 선택한 PATH 변경, 클라이언트 연결, 로그인 자동 시작을
각각 관찰한다. 설치 성공만으로 나머지 기능이 활성화됐다고 표시하지 않는다.
checksum 불일치·artifact 부재·권한 오류 때 기존 설치 보존, 동일 artifact 재설치,
새 터미널의 PATH 적용을 임시 디렉터리와 fake release server에서 확인한다.
현재 shell에서 절대 경로로 실행되는 것과 새 native 계정에서 서비스명만으로
실행되는 것은 별도 결과다. 개발 runtime이 없는 실제 계정·다른 OS가 없으면
그 항목은 미검증으로 남긴다.

현재 제품 경로에는 독립 `README.md`가 없으며 사용자 문서는
`docs/runtime-contract.md`, `docs/client-compatibility.md`,
`docs/benchmark-method.md`에 나뉘어 있다. Task 6의 설치 안내를 제품 진입점에서
찾을 수 있도록 짧은 README도 연결하는 것이 적절하다. 연구 보고서 전체를
설치 안내에 복제하지 않고, 실제 확보한 artifact로 설치·첫 setup·on/off·복원하는
경로와 지원 한계를 안내한다. 이는 문서 구성 제안이며 제품 파일은 아직 바꾸지
않았다. 공개 release 주소나 실제 OS 통과 결과는 해당 증거가 생기기 전까지
기입하지 않는다.

제품 `.gitignore`에는 현재 `/target/`만 있다. 검증용 `artifacts/`와 Python
`__pycache__/`는 설치 archive의 입력이나 공개할 제품 소스에 자동 포함하면 안 된다.
Task 6에서 archive의 입력을 검증한 실행 파일과 필요한 사용자 문서로 한정하고,
개발 생성물의 ignore 규칙도 정리한다. 기존 증거 파일은 지우거나 이동하지 않는다.
최종 archive 내용 검사는 이 제외가 실제로 지켜지는지 확인해야 하며, binary만의
크기를 개발 작업 디렉터리 전체 크기와 혼동하지 않는다.

최종 측정은 새로 고정한 실행 파일을 식별해 첫 설정 명령의 메모리와 상주 worker의
유휴 RSS, `on/off` 시간 값을 따로 기록한다. 첫 설정에서 실행하는 client version
조회 자식 프로세스의 메모리를 gateway 자체와 합친 값으로만 표시하지 않는다.
표본 최댓값과 OS가 제공하는 peak 값의 의미도 구분한다. 기존
[Native Task 1 관찰](native-lifecycle-observation.md)의 세 표본은 당시 binary의
관찰이며 새 binary의 p95/p99로 재사용하지 않는다. [벤치마크 명세](benchmark-spec.md)의
추가 p95 2ms·idle RSS 50MiB는 잠정 제품 예산이며 실사용 우위의 증거가 아니다.
설치·CLI 변경 검증 때문에 이전 quota/accounting 전체 행렬을 반복하지 않는다.

기존 `scripts/probe-native-footprint.py`도 읽었다. 이 스크립트는 macOS 전용 세 표본, 해당 시점의 상태 구조, 상속한 subprocess 환경을 사용하며 wizard를 측정하지 않는다. 보존된 원측정의 도구이므로 수정·재실행해 최신 native acceptance로 대체하지 않는다. Task 6은 새 경로의 driver에서 HOME/USERPROFILE/XDG·임시 경로와 명령 환경을 명시적으로 격리하고, 실패 결과도 새 파일에 남긴다. 실제 관찰한 upstream 시도 수와 코드에서 요청하지 않도록 구성한 사실은 구분한다.

[읽기 전용 도구 확인](../evidence/native-packaging-host-preflight.json)에서 이 macOS arm64 환경에는 sh/tar/curl/shasum/plutil이 있고 PowerShell·systemd 도구는 없었다. 설치나 등록 명령을 실행한 결과가 아니다. Task 6은 POSIX 설치 fixture를 이 호스트에서 직접 확인할 수 있으나, PowerShell 실행 검증은 도구가 없는 이유와 함께 미검증으로 남긴다. 검사를 위해 현재 계정에 패키지를 추가 설치하지 않는다.
