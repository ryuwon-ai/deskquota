# Windows 후속 검증과 개선 우선순위 — 2026-09-16

이번 검증에서 가장 큰 효율 개선 후보는 **동시에 들어오는 동일 요청의 중복 호출 제거**였다. 캐시가 비어 있을 때 동일 요청 10개를 동시에 보내면 upstream 호출도 10번 발생했다. 한편 Windows 자동 시작과 설정 확인 화면, 설치된 Claude Code 버전 검사에서 실제 사용을 막는 문제를 찾아 수정했다.

이 보고서는 Windows 10 Education x64 노트북 한 대에서 실행한 결과다. 기존 관리자 계정을 사용했다. 작업 스케줄러가 실행한 프로세스의 비승격 토큰 검증과 새 일반 사용자 계정 검증은 다른 범위다. Docker·WSL·MSVC를 사용하지 않았고, 실제 LLM 제공자나 회사 API는 호출하지 않았다.

## 직접 검증과 수정

| 항목 | 관찰한 문제 | 변경과 검증 범위 |
|---|---|---|
| Claude Code 연결 | 설치 버전은 2.1.76인데 `connect`가 2.1.63만 허용 | 2.1.76의 실제 Read 결과와 후속 Messages 요청을 확인한 뒤 버전 검사를 갱신. 관리되는 설정 적용·한글 경로 파일 읽기·연결 해제 성공을 확인했으나 최종 반복 검사에서 파일 교체 오류 1회도 관찰. 이전 버전 호환 계층은 추가하지 않음 |
| 설정 최종 확인 | `quota: configured`만 표시하여 실제 한도 확인 불가 | RPM·TPM·동시 요청 수 표시. 18/450000/3과 unknown/unlimited 조합의 회귀 검사를 실패부터 재현한 뒤 수정 |
| 자동 시작 XML 저장 | Windows가 생성된 UTF-8 XML을 거부 | UTF-16LE BOM으로 저장하고 기존 UTF-16 디코더를 재사용. 동일 XML의 UTF-8 BOM, UTF-8 선언 제거, UTF-16 대조 실험에서 UTF-16만 등록 성공 |
| 작업이 없는 최초 상태 | `0x80070003`을 알 수 없는 오류로 판단 | 파일 없음과 경로 없음 HRESULT 모두 미등록 상태로 분류. 권한 거부 등 다른 오류는 계속 차단 |
| 등록 후 상태·해제 | Windows가 Enabled 기본값을 생략하고 LogonTrigger 사용자 SID를 계정명으로 내보냄 | 스키마의 기본값을 적용하고 Windows 계정 조회 API로 SID를 비교. 표시 이름을 신뢰하여 소유권을 인정하지 않음 |

기본값은 Microsoft의 [Task Enabled](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-enabled-settingstype-element)와 [Trigger 기본 스키마](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-triggerbasetype-complextype)에 근거했다. [LogonTrigger.UserId](https://learn.microsoft.com/en-us/windows/win32/taskschd/logontrigger-userid)는 SID와 계정명을 모두 허용한다. 계정명은 [LookupAccountNameW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-lookupaccountnamew)로 확인한다. 오류 코드 구분은 [Win32 오류 정의](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-)와 실제 HRESULT 출력으로 확인했다.

### 네이티브 실행 결과

- Windows 전체 테스트: **404개 통과·0개 실패·2개 제외**, 19개 테스트 실행 단위. 제외 항목은 명시 실행용 프로파일링 검사와 심볼릭 링크 권한이 필요한 검사다. Windows Clippy는 실행하지 않았다. [최종 테스트 기록](../evidence/windows-followup-2026-09-16/native-tests.json).
- 자동 시작: 실제 작업 등록·수동 실행·비승격 토큰·소유권 상태·종료·등록 해제 통과. 실패 실험의 이전 작업도 수정된 CLI로 해제했다. [실행과 정리 기록](../evidence/windows-followup-2026-09-16/autostart.json).
- 최종 GNU 실행 파일: **16,341,338 bytes**, SHA-256 `5e9075d44765cc241be84e6ca42f701d6e45d4fcc49dae2dff056dd2d89c65b5`. [빌드 기록](../evidence/windows-followup-2026-09-16/release-build.json).
- 최종 실행 파일의 ZIP 설치·재설치·잘못된 체크섬 거부·개발 도구 없는 PATH·on/status/off 통과. 합성 요청 82개 중 80개 캐시 적중, upstream 2회, 실패 0개, actual 정산 2 RPM/46 TPM. [최종 실행 검증](../evidence/windows-followup-2026-09-16/release-acceptance.json).
- macOS: 전체 437개 통과·0개 실패·1개 제외 후 마지막 XML 해석 변경에 관련된 자동 시작 단위 검사 12개와 Clippy 전체 타깃을 다시 통과했다. macOS에서 Windows API 실행을 검증한 것은 아니다.
- 실제 Claude Code **2.1.76**: 관리되는 연결 설정, 선택 모델, Messages SSE, 정확한 Read 결과, 후속 요청, 연결 해제, 게이트웨이 종료 통과. 사용자 홈과 설정은 격리했으며 회사 데이터는 사용하지 않았다.
- 실제 Codex **0.154.0**: 관리되는 연결 설정, 모델 선택, Responses 응답 왕복, 연결 해제 통과. 파일 읽기는 Codex 자체 정책이 거부했다. CLI 종료 코드 0과 응답 마커가 있어도 도구 결과가 틀리면 하네스는 실패로 처리한다.
- Windows Pi, 실제 로그아웃/로그인·재부팅, 새 비관리자 계정, Windows 11/ARM64, 저사양 PC와 Linux는 미검증이다.

현재 지원 버전과 과거 실행 증거를 구분한 [클라이언트 표](../product/docs/client-compatibility.md), [Windows 기본 검증](windows-native-validation-2026-09-15.md), [최종 Claude 실행 성공](../evidence/windows-followup-2026-09-16/claude-managed-final.json), [Codex 부분 성공/실패 결과](../evidence/windows-followup-2026-09-16/codex-managed.json)를 함께 봐야 한다. Windows에서 gateway-off 대조군과 사용자의 무관한 설정 수정 보존을 네이티브 클라이언트로 다시 실행하지는 않았다. 해당 설정 계약은 Rust 테스트로 별도 검증했다.

### 남은 실제 오류: Windows 설정 저널 교체

최종 실행 파일의 첫 Claude 연결 검사에서 `ReplaceFileW` 오류 **1175**가 발생했고,
새 격리 경로에서 반복한 다음 검사는 전체 Read 흐름까지 통과했다. 최종 실행 파일의
두 관찰은 **1회 실패·1회 성공**이다. 이전 실행 파일에서는 같은 흐름이 통과했다.
따라서 일관된 재현 조건과 원인은 아직 모른다. 백신이나 외부 파일 잠금을 원인으로
단정할 근거도 확보하지 못했다.

[실패 기록](../evidence/windows-followup-2026-09-16/claude-managed-failure.json)과
[진단 메모](../evidence/windows-followup-2026-09-16/diagnosis-notes.json)를 남겼다.
Microsoft의 [ReplaceFileW 정의](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)에서
1175는 교체 대상 파일을 제거하지 못한 경우다. 기존 코드가 복구 원본과 교체 후보를
보존하고 중단했으며, 무조건 재시도하거나 검증을 생략하는 변경은 하지 않았다.
외부 공개 릴리스 전에는 이 실패의 재현 조건과 복구 동작을 먼저 확정해야 한다.
실패한 격리 경로의 복구 파일과 후보 파일 각 1개가 남아 있음을 직접 확인했다.
마지막 검사에서 테스트 게이트웨이 프로세스와 예약 작업은 각각 0개였다.
[복구 파일·정리 확인](../evidence/windows-followup-2026-09-16/recovery-check.json).

### 가벼운 실행 상태의 관찰값

최종 실행 파일의 합성 loopback 검사에서 Working Set은 시작 시 **8.57 MiB**,
64개 idle 연결 시 **9.55 MiB**, 요청 후 **10.41 MiB**였다. idle 연결을 열기
전후 스레드는 17개였고 요청 후에는 20개였다. 2초 idle 구간에서 Windows
CPU 시간 카운터의 증가는 관찰되지 않았으나, CPU 사용량이 항상 0이라는 뜻은 아니다.

캐시 hit 40개씩의 p95는 JSON **0.240ms**, SSE **0.266ms**,
p99는 각각 **0.296ms**, **0.313ms**였다. 100ms 지연을 인위적으로 넣은
upstream을 건너뛴 재생 검사다. 제공자 응답 지연, 높은 부하의 처리량,
경쟁 제품보다 빠르다는 주장은 이 수치로 판단할 수 없다.

## 가장 큰 효율 개선 후보: 동시 캐시 미스

18 RPM / 450000 TPM / 동시 요청 3개, 캐시 활성화, upstream에 150ms 고정 지연을 둔 합성 실험이다. 각 실험은 새 프로세스와 빈 캐시에서 시작했다. 요청은 동일한 짧은 분류 본문이며 요청 본문·모델·인증·경로가 달라지는 대화형 캐시 적중률 실험이 아니다.

| 요청 방식 | 제출/완료 | 캐시 적중 | upstream 호출 | 사용 RPM |
|---|---:|---:|---:|---:|
| 동일 요청 10개 순차 실행 | 10/10 | 9 | 1 | 1 |
| 동일 요청 10개 동시 실행, 1차 | 10/10 | 0 | 10 | 10 |
| 동일 요청 10개 동시 실행, 2차 | 10/10 | 0 | 10 | 10 |
| 동일 요청 10개 동시 실행, 3차 | 10/10 | 0 | 10 | 10 |

전체 40개가 완료했고 실패는 0개였다. [결과 JSON](../evidence/windows-followup-2026-09-16/cache-burst.json)과 [실험 소스](../evidence/windows-followup-2026-09-16/burst_probe.py)를 보존했다. 이는 **호출 수 검증**이며 p95·처리량 우위나 실제 회사 업무 적중률을 증명하지 않는다. 기존 Windows 검증 실행 파일로 측정했으며 이번 수정은 캐시·대기열 경로를 변경하지 않았다.

**코드 근거:** [서버](../product/src/server.rs)는 admission 이전에 캐시를 조회한다. 이후 [worker](../product/src/transport/stream.rs)는 대기 중 새로 저장된 캐시를 다시 조회하지 않고 upstream을 시작한다. 따라서 최초 조회 시 모두 miss면, 앞 요청이 완료되어 캐시가 생겨도 이미 대기 중인 요청들은 재사용하지 못한다.

**첫 개선안:** 대기 후 upstream 시작 직전에 적격 캐시를 다시 조회한다. 적중하면 upstream 시도·RPM/TPM 차감을 시작하지 않고 기존 예약과 슬롯을 정확히 반환한다. 현재 상태의 `considered = hits + misses + bypasses` 계약도 중복 집계 없이 유지해야 한다. 이 변경만으로 개선되는 호출 수와 지연을 먼저 측정한다.

**그다음 판단:** 이미 동시에 실행 중인 최초 3개까지 하나로 합칠 필요가 검증되면, 같은 캐시 키의 진행 중 요청을 공유하는 bounded singleflight를 추가한다. 이 방식은 완료된 응답을 기다리는 후속 요청의 첫 토큰 지연이 늘 수 있다. 리더 취소가 다른 요청을 취소하거나, 실패·불완전한 스트림을 캐시하거나, 다른 인증·root의 응답을 공유해서는 안 된다. 무한 대기자 목록이나 Redis 의존성도 필요하지 않다.

기존 제품의 패턴은 다음 소스에서 확인했다.

| 참고 소스 | 가져올 원칙 | 범위의 한계 |
|---|---|---|
| [Bifrost LRU](https://github.com/maximhq/bifrost/blob/1b23416533e0a8b36192f70b175f9babbd6308f1/framework/lrucache/lrucache.go) | 같은 키의 채우기 작업 공유, 실패를 저장하지 않기, 취소된 대기자 해제, 무효화와 경쟁하는 쓰기 방지 | DB 행·자격정보 등의 범용 캐시 구현. LLM 응답 중복 제거 성능을 증명하는 자료는 아님 |
| [LiteLLM Cache Coordinator](https://github.com/BerriAI/litellm/blob/9a715df212d777bbd43f4cab05731978c708ec63/litellm/proxy/common_utils/cache_coordinator.py) | 이벤트 기반 대기와 완료 후 재조회, 주기적 폴링 피하기 | 공통 리소스 읽기를 조정하는 구현. 이번 Windows 실험과 경쟁 성능을 비교하지 않음 |

## 다음 개발 순서

| 순서 | 개선할 것 | 채택 기준과 트레이드오프 |
|---|---|---|
| 0 | Windows 저널 파일 교체 오류 재현·복구 검증 | 동일 최종 실행 파일에서 1175 실패 1회, 성공 1회. 잠금 소유자와 ACL/파일 상태를 확보하고 안전한 실패·복구를 검증. 원인 모를 재시도로 덮지 않기 |
| 1 | 대기 후 캐시 재조회 | 동일 동시 요청의 upstream 호출 감소, 한도 초과·누수 0, 키 격리 유지. 반복 실험의 p95/p99와 취소·deadline 결과를 함께 보고 |
| 2 | 실제 업무에서 초기 토큰 예약 오차 확인 | 기존 actual usage 및 예약/실사용 진단을 활용. 모델별 과예약이 대기의 주원인일 때만 해당 모델의 추정 방식을 개선. 합성 fixture의 작은 usage를 실제 토큰 오차로 해석하지 않기 |
| 3 | Windows 배포·클라이언트 회귀 자동화 | 이번 네이티브 검사를 릴리스 전 실행 경로로 고정. 새 일반 사용자 계정, 실제 로그인, 회사 프록시/CA를 단계별 검증. GitHub CI 통과를 아직 주장하지 않음 |
| 4 | 실제 제한 API 비교 | 같은 요청 도착 패턴·재시도 정책·측정 구간으로 direct/DeskQuota/경쟁 제품 비교. 완료 goodput, 429, 실패·취소, p95/p99, 메모리, 실제 한도 초과를 모두 기록 |

추론 가속 엔진이나 대시보드를 확장하는 것보다, **동일 작업을 반복해서 보내지 않고 주어진 한도 안에서 더 많은 유효 작업을 완료하는 것**에 먼저 집중한다. 현재 결과만으로 논문 수준의 독창성, 경쟁 제품 전체에 대한 성능 우위, 5천 stars 달성 가능성을 입증할 수는 없다.

## 재현과 증거 경계

자동 시작 검사는 [verify_windows_autostart.py](../product/scripts/verify_windows_autostart.py)가 담당한다. 현재 로그인한 사용자의 임시 작업을 등록하고 Task Scheduler에서 수동 실행한 뒤 프로세스 토큰과 정리를 확인한다. 실제 로그인 이벤트를 발생시키지 않는다.

```powershell
py -3.11 product/scripts/verify_windows_autostart.py `
  --binary C:\approved\llmgw.exe `
  --work C:\approved\new-isolated-autostart-check
```

[버전·소스 변경 해시](../evidence/windows-followup-2026-09-16/patch-10-manifest.json)는 Windows에 옮긴 변경 파일을 원본 source manifest와 연결한다. 빌드 산출물은 iCloud/OneDrive 밖에 두었고 빌드 동시성은 2로 제한했다. 실행 도구의 인자·경로 오류와 제품 오류를 구분해 기록했으며, 이번 변경은 아직 커밋·푸시하지 않았다.
빌드 후 결과를 반영한 제품 문서 3개를 제외한 변경 입력은 네이티브 빌드 당시
해시와 일치한다. [검증 메타데이터](../evidence/windows-followup-2026-09-16/validation.json).
