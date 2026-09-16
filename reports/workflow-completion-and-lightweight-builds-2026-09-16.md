# 재시도를 포함한 완료 시간과 기본 실행 파일 경량화

기준일: 2026-09-16. 목표는 **Get more done within your LLM limits.** 실패 응답이 빨리 돌아온 시간을 작업 완료로 세지 않으며, 모든 요청을 오래 기다리게 하는 정책도 자동으로 유리하게 평가하지 않는다.

**연속 보충 한도에서는 원본 18건의 요청별 완료 시간 p95를 38.13초에서 2.51초로 줄였다.** 같은 바이너리의 기존 설정을 비교한 5쌍 모두 18/18건을 upstream 18회로 완료했다. 원인은 제공자 정책과 다르게 추가된 로컬 rolling 제한이었다. 실제 rolling 제한에서는 이 결과가 재현되지 않으며, 제공자의 한도를 늘리거나 새 스케줄러로 달성한 결과가 아니다.

## 이번 변경

1. **BPE 어휘를 선택 빌드로 분리했다.** 기본 Cargo 의존성 그래프에서 `bpe-openai`가 빠진다. 기본값은 기존 바이트 추정이며, `--features bpe`를 명시하면 기존 BPE 추정을 사용할 수 있다. 미지원 설정을 조용히 다른 추정기로 바꾸지 않는다. 설정 파싱·저장·시작 전에 명확히 거절하고, 이미 실행 중인 BPE worker의 `status`·`off`는 기본 바이너리에서도 유지한다.
2. **제공자의 재시도 금지를 따른다.** `x-should-retry: false`를 기존 두 재전송 분기에서 검사한다. 앞뒤 SP/HTAB 및 중복 헤더를 처리하고, `true`가 기존 재시도 자격을 넓히지는 않는다. 공유 429 cooldown·원문 전달·마감·취소·정산은 유지한다. 실행 코드 11줄, 새 의존성·설정 없음.
3. **설정에서 로컬 한도의 의미를 설명한다.** 숫자는 로컬의 rolling 60초 한도를 설정한다. `unknown`은 로컬 한도를 추가로 집행하지 않고 upstream에 맡기며, 실제 제공자 한도는 미확인이다. `unlimited`는 로컬 한도를 두지 않겠다는 명시적 선택이다. 세 선택 모두 기존 동시성·공유 cooldown 규칙을 유지한다. 기본값이나 저장 형식은 바꾸지 않았다.
4. **비교 단위를 논리적 작업으로 바꿨다.** 개발용 비교 스크립트가 같은 원본 요청의 재시도와 원래 마감을 추적한다. 클라이언트 HTTP 수와 실제 upstream 호출 수를 따로 세며, 이 스크립트는 설치되는 제품에 포함되지 않는다.

## 가벼운 기본 빌드: 직접 측정

MB는 십진 bytes, MiB는 2²⁰ bytes다. 두 옵션의 차이는 어휘 포함 여부이며, 선택한 BPE 어휘의 런타임 메모리를 줄인 변경은 아니다.

| 대상 | 기본 | BPE 포함 |
|---|---:|---:|
| macOS ARM64 실행 파일 | 10,197,600 bytes | 59,917,120 bytes |
| macOS tar.gz | 4,292,569 bytes | 29,924,495 bytes |
| Windows x64 GNU 실행 파일 | 16,377,290 bytes | 91,180,246 bytes |

직전 macOS 실행 파일 59,921,008 bytes 대비 기본 파일은 **82.98% 감소**했다. 현재 기본 실행 파일의 idle RSS는 **9.67 MiB**(10표본), 시작 **36.48–44.18ms**·종료 **34.74–48.54ms**(5회)였다. 이는 프로세스 수명 주기 측정이며 첫 요청 준비 시간은 아니다. M4 32 GiB에서 측정했으므로 저사양 성능으로 일반화하지 않는다.

같은 바이트 추정·캐시 비활성·무대기 fixture에서 이전/현재 기본 바이너리를 교대로 5쌍 실행했다. 실행당 측정 100건과 별도 warmup 5건이 모두 완료됐다. 실행별 통계의 중앙값은 p50 **0.260→0.268ms**, p95 **0.349→0.348ms**, p99 **0.373→0.373ms**다. **파일 크기는 줄었으며, 이 대조에서는 의미 있는 속도 개선을 주장하지 않는다.** [원시 재집계](../evidence/workflow-completion-2026-09-16/validation/local-resource-audit.json).

## 완료 시간 비교의 계약

- 원래 18건의 입력·출력 상한·도착 시각·20/400/2,500ms 생성 지연을 보존했다. 도착 offset은 2.2514–31.7085초로 약 29.46초에 걸쳐 있으므로 전체 배치 완료가 2초라는 뜻은 아니다.
- 원본 18건의 각 작업은 원래 제출 시각부터 125초의 마감과 최대 4회 클라이언트 HTTP/실제 제공자 호출 예산을 가진다. 재시도해도 마감은 연장되지 않는다. 아래 짧은 프로파일의 마감은 3초 또는 5초이며, 호출 예산은 노드마다 적용한다.
- 비교 제품의 내부 재시도는 0이다. 원시 호출을 HTTP exchange마다 연결해 숨은 재전송도 검사한다. 모든 제품에 같은 클라이언트 재시도 정책을 적용한다.
- 완전한 오류 응답과 첫 출력 전의 재시도만 허용한다. `Retry-After` seconds/date, millisecond 헤더와 `x-should-retry: false`를 따른다. 힌트가 없을 때만 고정 seed의 bounded backoff를 사용한다. 부분 스트림은 재시도하지 않는다.
- 성공은 기대한 본문·finish reason·DONE·EOF로 확인한다. 원래 요청은 usage를 요구하지 않으므로, 정상적으로 생략된 usage는 별도로 센다. 관찰된 usage는 정확성을 검사한다. 별도 `include_usage=true` 사전 검사에는 정확한 usage가 필수이며 측정 인스턴스·장부와 분리한다.
- 원본 18건의 독립 upstream fixture는 동시성 2, RPM 16, TPM 6000을 집행한다. **strict rolling 60초**와 **RPM 연속 보충 bucket(용량 16, 초당 16/60) + rolling TPM**은 서로 다른 계약이다. 이 bucket 용량이 실제 Claude·NVIDIA 계정의 허용량이라는 뜻은 아니다. 이 fixture에서 거절은 시도 수에는 포함되지만 성공 admission 예산을 차감하지 않는다. 짧은 프로파일은 여유 있는 RPM·TPM을 사용한다.
- DeskQuota 두 설정은 같은 BPE 포함 일반 release·캐시 비활성·동시성 2를 사용한다. 원본 18건의 `local`은 로컬 16/6000·actual·cl100k+256, `managed`는 기존 `unknown` RPM/TPM이다. 짧은 프로파일의 `local`은 해당 fixture의 큰 한도 값을 사용한다. 결과 차이를 새 스케줄러 알고리즘의 우위로 표현하지 않는다.
- 모든 DeskQuota 측정은 `startup_hold_secs = 0`, 기본 RR, 게이트웨이 내부 재시도 0을 명시했다. 일반 첫 설정의 startup hold 기본값 60초까지 포함한 첫 요청 시간 측정이 아니다. 사용자 설정을 자동으로 변경하지 않았다.
- peer managed 비교는 동일 endpoint·동시성 2·추가 local quota 없음·캐시/내부 재시도 없음이다. 별도 native-quota 진단은 각 제품의 서로 다른 로컬 정책을 포함하며 주 비교 순위에 섞지 않는다.
- 18개 표본의 nearest-rank p95·p99는 최댓값과 같다. 동일 seed 101 반복은 실행 변동을 확인하며, 서로 다른 사용자 18명·독립 workload 5종을 검증한 것은 아니다. 성공 percentile만으로 실패한 작업을 대신 설명하지 않는다.
- 원본 fixture의 short/long은 생성 시간이 아니라 입력 바이트 수로 나눈 그룹이다(short <1,000 bytes, long ≥1,000 bytes). short 그룹에도 2.5초 생성 요청이 있다. 과거 metadata가 포함된 backfill 실험의 평균 14.72→12.32초·p95약 47초를 이번 결과로 갱신하지 않는다.

## 연속 보충 한도: 같은 DeskQuota 바이너리의 5쌍 비교

모든 실행의 원본 18건·upstream 한도·출력 내용·동시성·바이너리는 같고, 로컬 quota 설정만 다르다. 반복 순서를 교대로 바꿨다. 시간은 **실행별 통계의 중앙값**이다.

| 관측값 | 추가 로컬 rolling 한도 | 제공자에 제한 위임 |
|---|---:|---:|
| 작업 성공 p95·p99 | 38.134초 | 2.508초 |
| 작업 성공 평균 | 4.925초 | 1.130초 |
| short 입력 그룹 평균 | 3.758초 | 1.213초 |
| long 입력 그룹 평균 | 9.009초 | 0.837초 |
| 전체 배치 완료 시간 | 67.591초 | 31.964초 |
| 원래 마감 내 완료 / 전체 | 90/90 | 90/90 |
| 각 작업 시작 후 5초 안에 완료 | 80/90 | 90/90 |
| 클라이언트 HTTP / upstream 호출 | 90 / 90 | 90 / 90 |
| upstream 거절·취소·마감 초과 | 0 | 0 |

실행별 p95 범위는 로컬 **38.132–38.135초**, 제공자 위임 **2.507–2.514초**였다. 각 실행의 성공 admission은 같은 fixture 비용 4,924단위를 사용했다. 작업 도착이 약 29.46초에 걸쳐 있으므로, **2.51초는 요청별 완료 시간의 p95이며, 전체 18건을 2.51초 만에 처리했다는 뜻이 아니다.**

**코드·원시 기록에 근거한 원인 해석:** 늦어진 `g0`·`g16`은 앞선 16건이 끝난 뒤 도착했지만, 첫 두 admission에서 약 60초가 지난 시점에 upstream으로 전송됐다. 앞선 16건의 실제 비용 4,297에 두 요청의 재구성 예약량 583·549를 모두 더해도 5,429로 TPM 6,000보다 작다. 로컬 RPM 16이 늦은 두 요청을 막았다는 해석과 일치한다. 실시간 blocked-reason 표본은 없고 두 quota 설정을 함께 바꿨으므로, RPM만 바꾼 독립 실험으로 표현하지 않는다. [재구성·가정](../evidence/workflow-completion-2026-09-16/audit/local-late-admission-cause.json).

재시도 비용까지 비교할 수 있게 측정했지만, 이 bucket 조건에서는 양쪽 모두 재시도가 필요 없었다. 별도의 개인 rolling budget을 집행해야 하는 사용자에게 `unknown`을 일괄 적용하면 요구사항을 바꾸는 셈이다. setup·영문/한국어 README에서 이 선택을 명확히 안내하고 기본값은 유지했다.

## 같은 제공자 위임 조건의 경쟁 대조

Direct·Bifrost·LiteLLM은 순서를 회전해 각각 3회 반복했다. 위 DeskQuota 제공자 위임 5회와 같은 원본 18건·bucket 계약·호출 예산을 사용했다. 실행별 통계의 중앙값이며, DeskQuota만 반복 수가 다르다는 점을 유지한다.

| 경로 | 반복 | 요청별 성공 p95·p99 | 배치 완료 | 완료 / 전체 | HTTP / upstream |
|---|---:|---:|---:|---:|---:|
| Direct | 3 | 2.507초 | 31.963초 | 54/54 | 54 / 54 |
| DeskQuota 제공자 위임 | 5 | 2.508초 | 31.964초 | 90/90 | 90 / 90 |
| Bifrost 제공자 위임 | 3 | 2.510초 | 31.967초 | 54/54 | 54 / 54 |
| LiteLLM 제공자 위임 | 3 | 2.523초 | 31.975초 | 54/54 | 54 / 54 |

**이 조건에서 DeskQuota는 경쟁 제품과 비슷한 완료 속도에 도달했다. 압도적인 지연 우위는 입증하지 못했다.** 모든 경로가 전부 완료했고 추가 호출이 없었다. 캐시를 끈 이 fixture에서 고유 요청에 지정한 2.5초 생성 지연은 게이트웨이가 제거할 수 없다.

LiteLLM은 요청하지 않은 usage를 성공 응답 54건 모두에서 생략했다. 정상 완료로 세되 `missing_unrequested`로 따로 기록했다. 다른 경로의 usage는 모두 관찰·검증했다. 별도 명시적 usage 사전 검사에서는 다섯 경로 모두 정확한 usage를 반환했다. 이 차이를 LiteLLM 실패나 토큰 정산 오류라고 단정하지 않는다.

## 엄격한 60초 한도: 한 번의 진단 실행

같은 원본 18건과 클라이언트 재시도 예산을 사용했다. 이 표의 Bifrost·LiteLLM은 **native local quota 진단 설정**이며, 앞의 추가 로컬 한도 없는 주 비교와 다른 설정이다. p95가 낮더라도 미완료 작업이 사라지지는 않는다.

| 경로 | 완료 / 전체 | 작업 성공 p95·p99 | 클라이언트 HTTP / upstream | 최종 실패 |
|---|---:|---:|---:|---|
| Direct | 18/18 | 38.512초 | 21 / 21 | 0 |
| DeskQuota 로컬 한도 | 18/18 | 38.135초 | 18 / 18 | 0 |
| DeskQuota 제공자에 제한 위임 | 18/18 | 40.516초 | 20 / 20 | 0 |
| Bifrost native quota | 16/18 | 2.508초 | 24 / 16 | 재시도 예산 소진 2 |
| LiteLLM native quota | 16/18 | 5.842초 | 27 / 16 | 재시도 예산 소진 2 |

Direct도 실패분을 모두 마칠 때는 긴 대기가 생긴다. 그렇다고 DeskQuota의 공유 대기가 언제나 빠른 것도 아니다. 이 실행에서 제공자 위임 설정은 로컬 한도 설정보다 약 2.38초 늦었다. 이는 한 번의 정책 진단이며 안정적인 제품 순위나 모든 SDK의 재시도 동작을 뜻하지 않는다. [Direct·첫 대조](../evidence/workflow-completion-2026-09-16/matrix-initial.json) · [후속 진단·반복](../evidence/workflow-completion-2026-09-16/matrix-repeat.json).

## 짧은 요청·장애 복구·연속 작업

각 프로파일·경로를 순서를 회전해 3회 실행했다. 아래 값은 **실행별 성공 p95 중앙값 / upstream 호출 합계**다. 괄호의 완료 분모는 경로마다 동일하다. 전체 원시 기록에는 p50·p99·평균·root/입력 그룹별 결과도 보관한다.

| 프로파일 | Direct | DeskQuota 로컬 | DeskQuota 위임 | Bifrost 위임 | LiteLLM 위임 |
|---|---:|---:|---:|---:|---:|
| 한도 여유, 8건 동시 제출 (24/24 완료) | 0.272초 /24 | 0.278초 /24 | 0.256초 /24 | 0.258초 /24 | 0.293초 /24 |
| 1초간 429 후 복구 (24/24 완료) | 1.313초 /48 | 1.417초 /27 | 1.418초 /27 | 1.221초 /48 | 1.496초 /63 |
| 1초간 503 후 복구 (24/24 완료) | 1.313초 /48 | 1.310초 /48 | 1.209초 /48 | 1.223초 /48 | 1.495초 /63 |
| 3단계 의존 작업 4개 (12/12 작업 완료) | 0.960초 /36 | 0.965초 /36 | 0.964초 /36 | 0.973초 /36 | 1.128초 /36 |

`not_exhausted`는 RPM 1,000,000·TPM 1,000,000,000의 여유 있는 한도와 동시성 2를 사용한다. 요청 8건이 동시에 실행 가능해지는 burst이며 순수 무대기 오버헤드 실험이 아니다. 마감은 3초, 측정 전 warmup 3건은 별도 분모다. 복구 프로파일은 100ms 응답 8건을 0.1초 간격으로 제출하고 마감은 5초다. `chains`는 앞선 단계의 출력을 다음 단계 요청에 넣고 50ms 도구 지연을 거치는 3단계 작업 4개이며, 작업 전체의 마감이 5초다. 실제 에이전트 코드 수정·LLM 품질 평가가 아니다.

**429에서 얻은 이득은 호출 절약이다.** DeskQuota는 24건 완료에 27회 호출을 사용해 Direct·Bifrost의 48회보다 43.75% 적었다. 공유 대기는 남은 요청을 같은 장애 중에 반복 발송하지 않게 했다. 대신 위임 설정의 p95는 Bifrost보다 약 0.197초 길었다. “모든 장애에서 더 빨리 끝난다”는 결론은 기각한다. 503에는 새 그룹 전체 대기를 도입하지 않았으며, 위 시간 차이를 503 최적화의 효과로 해석하지 않는다. 작은 반복 수와 스케줄링 변동을 고려해야 한다.

고정한 LiteLLM 설정에서는 두 복구 프로파일 각각 39개의 오류 응답에서 upstream의 `retry-after-ms`가 클라이언트 경계에 나타나지 않았다. 따라서 같은 클라이언트가 seed 기반 backoff를 사용했고, 1초 복구 전에 15회의 재시도가 계획됐다. 관측한 추가 시도의 설명이며, LiteLLM 모든 버전·설정의 특성으로 일반화하지 않는다. 숨은 내부 재시도는 없었다. [원시 헤더 대조](../evidence/workflow-completion-2026-09-16/audit/recovery-header-observations.json).

취소는 별도 동시성 1 기능 검사로 분리했다. DeskQuota 두 설정 각각 3건 중 2건 완료·1건 취소, 클라이언트 3회·upstream 2회였다. gate 해제와 drain이 끝난 뒤에도 취소한 요청은 upstream에 전송되지 않았고 최종 queue·active는 0이었다. 이 취소를 성능 표의 빠른 성공으로 세지 않는다.

## 검증 범위

워크플로 검증은 **총 91회 실행**이며 요청한 usage 사전 검사 5회, 원본 18건의 진단·반복 24회, 짧은 프로파일 60회, 별도 취소 검사 2회다. 측정 실행 누적 시간은 약 20.47분이다. 20분 검증 예산에 맞춰 peer와 짧은 프로파일은 3회 반복으로 유지했고, 원본의 도착·생성 시간을 줄이지 않았다. [독립 원시 재검산](../evidence/workflow-completion-2026-09-16/audit/final-raw-audit.json) · [재검산 스크립트](../evidence/workflow-completion-2026-09-16/audit/raw_audit.py) · [최종 소스 고정 확인](../evidence/workflow-completion-2026-09-16/timing-source-final.json).

성능·정책 진단 84회 전체 분모는 **작업 852개 = 완료 848 + 재시도 예산 소진 4**다. 단계별로는 **노드 972개 = 완료 968 + 예산 소진 4**, 클라이언트 HTTP 1,220회·upstream 1,201회다. 별도 사전 검사 5건은 모두 완료, 취소 검사 6건은 4완료·2취소다. 한도 여유 프로파일의 warmup 45건도 별도 분모이며, 여러 계약을 합친 이 합계를 제품별 성공률 순위로 사용하지 않는다.

macOS 전체 Rust 검사: **기본 456 통과 / BPE 456 통과**, 각각 실패 0·profiling용 ignored 1. 이 전체 검사에는 `bench-harness`를 포함했지만 시간 측정에는 해당 기능 없는 일반 release를 사용했다. fmt와 두 feature Clippy `-D warnings` 통과. 두 모드가 대부분 같은 검사이므로 고유 검사 912개라고 합산하지 않는다.

macOS 실제 바이너리: 모드별 setup PTY 8건, retry 11건, 자동 요약 9건 통과. 두 capability를 가로지르는 lifecycle 4건도 통과했다. 두 macOS 아카이브를 별도 임시 경로에 설치하고 실행 파일 해시와 `--help`를 확인했다. PATH·사용자 설정·자동 시작을 전역 변경하지 않았다.

Windows 10 x64: 기본 전체 409 통과, BPE focused 236 통과, 각각 실패 0·profiling용 ignored 1. 두 release 링크 및 Rust/MinGW를 PATH에서 제외한 실행·교차 capability lifecycle의 25개 확인 항목, 모드별 retry 11건 통과. 빌드는 기존 GNU 1.88/WinLibs를 사용했으며 런타임에 개발 도구가 필요하다는 의미는 아니다. 초기 Python fixture 실패 2회는 `SystemRoot` 제거 때문에 소켓 초기화가 실패한 어댑터 문제로, 각 실행의 실제 요청은 0건이다. 원본 실패를 보존하고 Windows 최소 환경을 유지한 뒤 검증했다. [Windows 결과·재검산](../evidence/workflow-completion-2026-09-16/windows/final-validation.json).

자동 요약 검증은 설치된 Pi·Codex·Claude와 합성 upstream 사이의 일반 요약·다음 턴 흐름이다. `/responses/compact` 404, known TPM의 opaque 입력 400 등 기존 미지원 경계는 그대로다. 실제 초장문 세션이나 모든 버전의 auto compact를 보증하지 않는다.

## 채택하지 않은 기능과 한계

- 단일 503에서 그룹 전체를 멈추는 새 cooldown은 넣지 않았다. 한 모델·credential의 503만으로 그룹 전체 장애라고 판단할 근거가 없다.
- 새 token-bucket limiter·provider 자동 감지·별도 DB·다중 모델 라우팅을 넣지 않았다. 이미 있는 한도 선택으로 불필요한 대기를 줄일 수 있는지 먼저 검증한다.
- `unknown`은 별도의 개인 hard budget을 보장하지 않는다. 회사 정책이나 사용자가 실제 rolling 한도를 요구한다면 숫자 한도를 유지해야 한다. quota 계약이 맞지 않는 설정을 속도 때문에 강제로 바꾸지 않는다.
- CPU/RSS 관측은 gateway PID 표본이며 자식 프로세스 전체나 PC 전체 비용이 아니다. 로컬 합성 결과로 실제 LLM 품질·API 처리량·저사양 우위·경쟁 제품 전체의 우위를 단정하지 않는다.
- 현재 런타임의 `NVIDIA_API_KEY`, `NGC_API_KEY` 환경변수는 없었다. 다른 저장 위치의 키 존재 여부는 확인하지 않았고, 실제 API 요청·과금은 수행하지 않았다.

## 근거

- [설계](../docs/superpowers/specs/2026-09-16-workflow-completion-design.md) · [실행 계획](../docs/superpowers/plans/2026-09-16-workflow-completion.md) · [31개 하네스 검사](../evidence/workflow-completion-2026-09-16/implementation/self-check-v2-final.json) · [독립 품질 검토](../evidence/workflow-completion-2026-09-16/quality-review-v2.json)
- [최종 Rust 입력 해시](../evidence/workflow-completion-2026-09-16/validation/rust-source.json) · [전체 검사·빌드](../evidence/workflow-completion-2026-09-16/validation/results.json) · [패키지 설치 검사](../evidence/workflow-completion-2026-09-16/validation/packages/installer-check.json)
- [BPE 수용 기록](../evidence/bpe-packaging-2026-09-16/acceptance.json) · [재시도 금지 수용 기록](../evidence/retry-directives-2026-09-16/acceptance.json)
- 기존 구현과 공식 관례: [OpenAI Python retry](https://github.com/openai/openai-python/blob/main/src/openai/_base_client.py), [Anthropic Python retry](https://github.com/anthropics/anthropic-sdk-python/blob/main/src/anthropic/_base_client.py), [Claude rate limits](https://platform.claude.com/docs/en/api/rate-limits), [앞선 peer 소스 분석](quota-policy-deep-dive-2026-09-16.md). 최신 main 링크는 변할 수 있으며 이번 분석의 고정 버전·해시는 실행 결과와 이전 소스 조사에 남긴다.
