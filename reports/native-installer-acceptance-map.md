# Native Task 6 구현·검증 연결표

2026-09-13. 승인된 [Task 6](../docs/superpowers/plans/2026-09-12-native-experience.md)의 실행 준비다. [설치 사전 확인](native-installer-preflight.md)의 고정 제품 패턴을 적용한다. 제품 설치나 성능 측정 결과가 아니다.

| 계약 | 구현·검증 범위 | 결과를 구분할 기준 |
|---|---|---|
| 작은 native 설치 | POSIX shell / PowerShell의 두 진입점, 사용자 쓰기 가능한 경로, 검증한 실행 파일만 설치 | 개발용 Python driver와 사용자 runtime 의존성을 구분 |
| 실제 artifact 출처 | 명시한 로컬 archive 또는 실제 제공한 download source와 checksum manifest 사용. fake localhost release fixture로 검사 | 공개 release URL·registry·서명·배포를 만들었다고 주장하지 않음 |
| 설치 실패 시 보존 | checksum 불일치·artifact 누락·쓰기 권한 부족·재설치 fixture. 임시 검증이 끝나기 전 기존 실행 파일을 교체하지 않음 | 명령 exit와 기존 설치 bytes/실행 가능 여부를 함께 관찰 |
| archive 경계 | 파일 목록을 검증한 binary와 필요한 사용자 문서로 한정. 경로 이탈·예상하지 않은 link/entry로 다른 파일을 건드리지 않도록 추출 범위를 제한 | 개발 artifacts·target 전체·cache·사용자 config가 archive에 포함되지 않았는지 목록 확인 |
| PATH와 첫 실행 | 선택할 변경을 먼저 표시하고, 격리 HOME과 새 child shell에서 서비스명 해석 및 version/help/setup 진입 확인 | absolute path 실행과 PATH에서 찾는 실행을 별도 결과로 기록. 현재 사용자 profile/PATH는 변경하지 않음 |
| offline 경로 | 준비된 archive를 풀고 명시한 실행 파일로 시작하는 사용자 안내 | 이 개발 머신의 제한된 PATH smoke가 Python·Node·Redis·Docker·WSL이 없는 실제 계정 검증을 대신하지 않음 |
| 문서와 지원 표 | 짧은 제품 README → installation/runtime/client 문서. `on/off`, 자동 시작, 연결 변경 preview·복원, gateway-off 안내 | OS·client 버전·protocol·listing·선택·tools를 실제 검증 조합별로 표시 |
| 최종 build | source 마지막 변경 뒤 fmt/check/Clippy/full tests/release, lock/toolchain/source/archive/binary identity 기록 | build 완료와 다른 OS 실행·CI/CD 통과·공개 release를 구분 |
| native 관찰 | 새 held binary의 설치 smoke, 첫 설정 memory, idle worker RSS, on/off raw 시간. version 조회 자식 프로세스는 분리 | sampled maximum와 OS peak, 표본 수와 percentile을 구분. 이전 세 표본을 새 binary 수치로 재사용하지 않음 |
| 성능 경계 | setup와 OS 조회가 HTTP/SSE·quota/fairness 경로에 상주 감시·반복 subprocess를 넣지 않았는지 확인 | native 설치·CLI 검증은 효율 우위 비교가 아님. 이전 quota/accounting 전체 행렬을 반복하지 않음 |

새 evidence 파일은 실패와 성공 모두 고유한 경로에 남긴다. held binary는 strip하거나 덮어쓰지 않는다. 변경된 binary를 측정하려면 그 binary에 맞는 새 빌드·검증·identity가 필요하다. 기존 원측정·실패·archive는 그대로 보존한다.

이 호스트에서 PowerShell과 systemd 실행 도구는 없다는 [읽기 전용 확인](../evidence/native-packaging-host-preflight.json)이 있다. Windows installer의 정적 검토와 PowerShell 실제 실행을 혼동하지 않으며 검증용 패키지를 현재 계정에 설치하지 않는다. 실제 임시 OS 계정·로그인·저사양 PC가 없는 항목은 미검증으로 남긴다.

Windows 사용자 PATH의 registry 쓰기도 HOME/USERPROFILE 변경만으로 격리되지 않는다. 자동 fixture는 임시 설치 경로와 PATH 미리보기까지만 사용하거나 실제 registry 쓰기를 대체한 검사를 사용한다. 현재 계정의 사용자 PATH를 변경해 Windows fixture 통과를 만들지 않는다.

## Source manifest의 설치 스크립트 포함

현재 `scripts/fingerprint-product.py::source_files`는 src/tests/examples/fixtures/docs/scripts와 정해진 루트 파일을 수집하며 **packaging/은 포함하지 않는다**. 따라서 Task 6에서 이 기존 도구의 출력만으로 installer까지 고정했다고 쓰면 안 된다. 새 단계의 source 수집에 packaging/install.sh와 packaging/install.ps1을 명시적으로 포함하고, 캡처 중 안정성과 archive 안의 동일 bytes를 확인한다. 기존 도구·manifest·archive가 이전 증거에 포함되어 있다면 그대로 보존하고, 새 수집 도구 또는 명확한 새 단계 manifest를 사용한다. `archive-product-source.py`는 제공한 manifest의 product 하위 파일을 묶으므로 packaging 입력도 검증할 수 있다. 이는 도구 소스의 읽기 전용 확인이며 아직 설치 파일을 만든 것은 아니다.

검증 기록 생성 시 경로 기준도 명시한다. 기존 source manifest의 `product/...` 경로는 research 루트 기준이고, 단계별 identity 기록의 `target/...`·`artifacts/...`는 product 기준일 수 있다. 두 기준을 혼합하거나 여러 위치를 추측해 찾지 않는다. README의 해시는 검증한 identity 데이터에서 직접 출력하고 64자리 SHA-256인지 확인한다. 실패 로그에 합성 테스트 코드 문자열이 보여도 원본 로그를 이동·삭제하지 말고 필요한 분류와 공개 범위를 별도로 기록한다.

## 사용자 문서의 오래된 실행 안내

2026-09-13 읽기 전용 확인에서 `product/docs/runtime-contract.md`의 `Development run precondition`은 아직 Task 2 시절 `.llmgw/data-token`·`control-token` 수동 생성을 안내한다. 현재 CLI `run`은 `lifecycle::worker`로 들어가 config별 `StatePaths`에 `identity::provision_tokens`를 호출한다. Task 6에서는 이 오래된 경로와 수동 준비 안내를 제거하고 현재 `setup`·`run/on` 경로를 설명한다. 이는 소스에서 확인한 문서 불일치이며 이번 확인에서 새 worker를 실행한 것은 아니다. 최신 설치 smoke가 실제 명령과 문서의 일치를 확인해야 한다.

## macOS 메모리 집계 범위

이 호스트의 `man 2 wait4`는 종료한 프로세스와 그 자식들의 자원 집계를 반환한다고 설명하고, `man 2 getrusage`는 `ru_maxrss` 단위를 bytes로 명시한다. 따라서 wait4/getrusage 값을 버전 조회 자식과 분리된 wizard 자체의 peak RSS라고 표시하지 않는다. command tree의 OS peak와 PID별 sampled RSS를 따로 기록하고, 자식 포함 범위·단위·샘플 간격을 명시한다. PID별 표본 최댓값은 관찰 사이의 peak를 보장하지 않는다. 이 확인은 OS 매뉴얼 조회이며 제품 메모리 측정은 아직 수행하지 않았다.

## 설치 후 첫 명령 안내

현재 `src/cli.rs`는 인수 없는 첫 실행에 설정이 없으면 터미널에서 wizard를 열고, 설정이 있으면 `on`을 실행한다. 비대화형 첫 실행의 오류는 `examples/fixture.toml`과 저장소 기준 문서 경로도 표시한다. 설치본에는 개발 fixture가 없으므로 Task 6에서 이 오류를 실제로 실행 가능한 `llmgw setup` 안내로 줄인다. 개발 fixture를 설치 archive에 추가할 이유는 없다. 이는 소스 확인이며, 새 설치본의 첫 실행과 PATH 검사는 Task 6에서 직접 수행한다.
