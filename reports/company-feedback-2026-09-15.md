# 사내 Windows 검증 피드백 반영

사용자가 사내 PC에서 발견한 설치·인증·반복 호출 문제를 기준으로 변경했다.
이전 경쟁 제품 비교44회는 이전 소스·바이너리의 결과이며 이번 변경의 성능 수치로
재사용하지 않는다. 아래 신규 측정은 기본 기능의 릴리스 바이너리로 수행했다.

## Windows 빌드 경로

불안정한 `MetadataExt::volume_serial_number/file_index` 호출을 기존
`windows-sys`의 `GetFileInformationByHandleEx(FileIdInfo)`로 대체했다.
파일을 연 핸들 하나에서 내용·메타데이터·volume ID·128-bit file ID를 함께 읽는다.
동일 길이·동일 내용의 다른 파일을 같은 파일로 취급하는 임시 우회를 제거했다.

실제 ConfigPatch·platform·file_replace 소스를 포함한 Windows 대상 타입 검사는
변경 전 `E0658`8건에서 변경 후 통과로 바뀌었다. 인증·복원 수정 이후에도
다시 통과했다. 이 검사는 제품 코드를 복사하거나 대역으로 바꾸지 않는다.
다만 전체 Windows 빌드는 이 Mac에 Windows SDK가 없어 `aws-lc-sys`의
C 컴파일에서 중단되었다. **GNU 링크와 실제 Windows 실행은 아직 검증하지 않았다.**

[설치 문서](../product/docs/installation.md)에 실행 파일 사용과 GNU 소스 빌드를
구분하고, WinLibs의 compiler·assembler·dlltool 선택을 정리했다. 실행 자체에는
Docker·WSL·Rust 컴파일러가 필요하지 않다. 이번 작업에서 릴리스를 발행하지 않았다.

## 표준 인증과 설정 복원

전용 data token의 생성·필수 검사·클라이언트 헤더 주입을 제거했다.
`forward` 모드는 표준 `Authorization`과 `x-api-key`를 전달하고,
`env/none`은 설정대로 대체·제거한다. 관리 API의 control token은 유지한다.
loopback 바인딩, 실제 포트에 맞는 Host, Origin 거절, JSON 요청 검사를 함께 적용한다.
프로세스 락은 중복 실행을 막는 장치이며 다른 로컬 프로세스를 인증하지 않는다.

Pi·Codex에는 API 키의 환경변수 참조를 사용한다. Claude는 기존 API 인증 소스를
유지하며, 저장된 OAuth 로그인만으로 forward 연결을 허용하지 않는다.
설치된 Pi0.84.2의 실제 resolver에서 `$ENV` 참조를 합성 값으로 확인했다.
또한 변경 후 릴리스 바이너리로 설치된 Pi0.84.2를 격리 실행해 일반 응답과
파일 읽기·후속 호출을 확인했다. 각각 upstream1회·2회, 두 흐름 모두 통과했다.
이 검사는 로컬 모의 서버와 upstream auth `none`을 사용했다.
Claude·Codex의 변경된 전체 실행과 실제 제공자 인증 성공 검증은 남아 있다.

설정 복원 리뷰에서 private 파일의 권한이 넓어진 뒤 이전 API 키를 복원할 수 있는
회귀를 발견했다. `Scope::UserPrivate` 제약을 journal에 유지하고, 복원 후보 파일을
쓰기 전에 권한을 검사하도록 수정했다. 자체 placeholder도 실제 API 키로 인정하지 않는다.

## TPM 정산과 재시작 대기

새 설정과 생략된 `accounting`은 `actual`을 사용한다. 생성 중에는 추정치를 예약하고,
지원하는 스트림의 유효한 최종 usage를 HTTP EOF에서 정산한다. usage가 없거나
불완전하면 예약을 유지한다. **생성 전의 정확한 토큰 계산 기능은 아니다.**
명시적인 `reserved` 설정은 유지한다.

`startup_hold_secs`는 기본60초, 범위0–3600초다. 0은 재시작 대기를 끈다.
RPM/TPM의60초 rolling window와는 별개이며, known quota가 없으면 대기하지 않는다.
짧게 설정하면 재시작 이전 요청의 사용량을 알 수 없는 구간을 더 일찍 받아들이게 된다.

## Exact cache 및 검증

선택형 `[cache]`를 추가했다. 기본 TTL300초·history3, 요청32KiB,
캡처/엔트리256KiB·최대128개·전체 보관 예산4MiB다. 새 의존성은 없다.
캐시를 끄면 추가 eligibility JSON 분석·키 해시·응답 복사를 하지 않는다.

root·최종 URL/query·인증을 포함한 모든 전달 헤더·원본 body가 키를 구성한다.
도구·미디어·상태 참조·미지원 요청 옵션과 미완성·오류 응답은 저장하지 않는다.
첫 요청은 즉시 스트리밍하고, 정상 HTTP EOF에서 완결된 응답만 저장한다.
캐시 엔트리를 제거해도 재생 중인 body/frame이 있으면 해당 메모리 예산을 유지한다.

hit는 admission 이전에 반환하므로 upstream RPM/TPM과 usage를 중복 정산하지 않는다.
응답 안의 usage 원문은 그대로 재사용되며, 새로운 upstream 사용량을 뜻하지 않는다.
`status --json`의 `exact_cache`가 별도의 hit/miss·보관량을 제공한다.

Responses의 중간 이벤트가 잘못됐는데 마지막 완료 이벤트만 정상인 경우를
리뷰에서 발견해 수정했다. 앞선 오류를 이후 정상 완료로 지우지 않는다.
기존 SSE decoder를 재사용하며 JSON 응답의 usage 정산 범위를 추가하지 않았다.

Apple M4·32GiB에서 JSON/SSE별 direct→miss→hit 순서로 각30개씩 측정했다.
모의 서버에50ms 지연을 넣었으며 SSE는 텍스트 전달 뒤 추가5ms 지연을 둔다.
실제 모델이나 제공자 API를 호출하지 않았다.

| 형식·경로 | 표본 | p50 ms | p95 ms | p99 ms |
|---|---:|---:|---:|---:|
| JSON direct | 30 | 52.462 | 53.193 | 53.226 |
| JSON miss | 30 | 52.884 | 53.877 | 54.073 |
| JSON hit | 30 | 0.231 | 0.455 | 0.499 |
| SSE direct | 30 | 58.827 | 59.805 | 60.668 |
| SSE miss | 30 | 59.291 | 60.286 | 71.438 |
| SSE hit | 30 | 0.288 | 0.527 | 0.563 |

데이터 요청180건이 모두 성공했고 응답 body가 원본과 일치했다. 이 중 gateway 요청은
120건으로 miss60·hit60이며 upstream은60회 호출했다. direct60회를 포함한 모의 서버의
총 수신은120회다. 실패·취소는0건이다. **반복률50%는 실험에서 의도적으로 만든 값이다.**

cache hit/miss/store/entry는 각각60, 보관량은18,780바이트였다. SSE miss30건의
input90·output30만 관측했고, JSON miss30건은 기존 계약대로 usage unknown과 예약을
유지했다. 최종 RPM60, TPM4,250(JSON 예약4,130 + SSE 실제120), 실행 중 요청0이었다.
캐시 hit의 usage가 중복 정산되지 않았다는 별도 증거다.

바이너리 크기는9.69MiB, 실행 전후 RSS는9.70→11.17MiB였다. 약6.73초 구간의
프로세스 누적 CPU 시간은0.06초 증가했다. 메모리 두 표본으로 peak를 알 수는 없으며,
이 작은 캐시 표본으로4MiB 한도 전체의 압박 성능을 입증하지 않는다. 메모리 소유권
제한은 별도의 기능 검사로 확인했다. warmup 없는 고정 순서이고 각30개 표본의
p99는 최댓값이다. 일반적인 처리량·저사양 성능·실제 사용자 hit rate의 증거가 아니다.

[재현 스크립트](../product/scripts/benchmark_cache.py)는 실제 upstream 시도 수,
응답 원문 일치, SSE 파싱, 누적 usage와 cache 카운터까지 검사한다. 별도 리뷰에서
원시180행과 percentile·쿼터 집계를 독립 재계산해 일치를 확인했다.

사용자 제보의57ms hit와1.4–2.4초 miss는 사용자의 기존 사내 게이트웨이 관측이며,
DeskQuota의 측정값으로 표기하지 않는다. TTL·history threshold는 품질 불변이나
결정적 생성의 보증이 아니며, 캐시는 이전 생성 결과를 재사용한다.

## 최종 로컬 검증

- Rust1.88.0 전체 targets/features 테스트420개 통과, 실패0개. 별도 성능 계측용1개는
  명시적 ignored 상태이며 실행하지 않았다. fmt·clippy도 통과했다.
- 소스가 변경되지 않은 상태에서 일반 release 빌드 완료. Cargo 산출물은 iCloud 밖의
  `/Users/ryuwon/Library/Caches/llmgw-cargo`에 두고 빌드 jobs2를 사용했다.
- Windows 대상의 실제 소스 타입 검사 통과와 Windows 전체 빌드·실행 미검증을 구분했다.
- 캐시 측정180건, Pi 일반 응답·파일 읽기2개 흐름 통과.
- 실제 터미널 설정 마법사8개 사례 통과: 취소·뒤로 가기·Ctrl-C, 캐시 활성화와 저장,
  실행, Pi 연결·해제, 의도적 실패 정리와 복구 파일 보존. 첫 실행은 테스트 드라이버의
  오래된 질문 문구 때문에 실패했고, 드라이버만 수정해 재실행했다. 제품 바이너리는 동일하다.
- 실행 후 이번 release 바이너리·검사 스크립트에 해당하는 잔여 프로세스가 없음을 확인했다.

## 남은 현장 확인

- Windows Rust/WinLibs 버전과 실제 GNU 빌드·실행 결과.
- 실제 RPM/TPM, 동시 에이전트 수, 스트리밍 usage 제공 여부.
- 반복 호출 비율, 캐시 TTL·history 정책, hit/miss 표본 수.

승인된 GNU/WinLibs 환경을 [설치 문서](../product/docs/installation.md)대로 선택한 뒤,
사내 Windows에서는 다음 검사로 시작할 수 있다. 아래 명령은 이 Mac에서 실행한 결과가 아니다.

```powershell
$env:CARGO_TARGET_DIR = "$env:LOCALAPPDATA\llmgw-build"
$env:CARGO_BUILD_JOBS = '2'
cargo +1.88.0-x86_64-pc-windows-gnu test --locked --target x86_64-pc-windows-gnu --test patch_contract --test wire_contract --test cache_contract
```

사내 URL·키·프롬프트 원문은 필요하지 않다. 커밋·푸시·실제 제공자 API 호출은
이번 검증에 포함하지 않았다.
