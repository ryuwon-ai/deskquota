# DeskQuota 경쟁 구현 적용과 실행 비교

2026-09-15, macOS ARM64 / Apple M4 / 32 GiB. 작은 상태·metadata 변경을 적용하고 고정한 경쟁 구현3종을 포함해 합성 loopback 실험44회를 완료했다. **낮은 메모리 비용과 특정 metadata 대기 해소는 관찰했지만, 일반적인 처리량·tail 우위는 입증하지 못했다.** 기본 정책은 RR로 유지한다. 실제 LLM API·Windows·저사양 PC·에이전트 작업 정답률은 이번 측정 대상이 아니다.

## 적용한 코드

- 일반 `llmgw status`에 기존 JSON의 admission 대표 차단 사유, 보호 root, quota capacity/debit/held, 추정·정산 방식을 표시한다. 대표 사유는 개별 요청의 정확한 원인이나 ETA가 아니다.
- Models·CountTokens를 TPM 미집행 생성 요청과 구분하고 ledger 정산까지 보존한다. 진짜 metadata만 TPM 0을 유지하며, 일반 zero-cost 요청의 늦은 positive usage 차감은 유지한다.
- 기존 실험 backfill에서 진짜 metadata만 모호한 zero-cost 제한에서 제외한다. RPM·실행 슬롯·동일 root FIFO·보호 요청의 입장 기회는 그대로 검사한다. 모델 목록과 권한 오류는 upstream 원문을 전달한다.
- 추가 제품 의존성 **0**. 기본 정책은 여전히 RR이다. 캐시·provider routing·토큰 예측 모델을 추가하지 않았다.

[제품 검사 기록](../evidence/adoption-product-2026-09-15/README.md): 관련 검사 221개 통과. 기본 feature 검사 64개는 이 검사들과 겹치므로 더하지 않는다. fmt·all-target/all-feature clippy도 통과했다. 독립 spec 검토와 quality 검토 모두 승인했다. 성능 주장을 승인한 리뷰는 아니다.

| 참고한 장점 | 이번에 반영한 범위 | 남긴 경계 |
|---|---|---|
| Bifrost의 운영 진단 | 이미 있는 admission/quota 정보를 일반 CLI에서 확인 | 대시보드·DB 없이 표시만 개선. 속도 개선으로 계산하지 않음 |
| PromptForge의 모델 선택 흐름 | 모델 조회의 의미를 ledger까지 보존해 기존 backfill에서 안전하게 취급 | upstream 목록·권한 오류를 그대로 전달. 로컬 catalog나 cache는 추가하지 않음 |
| LiteLLM의 추정·정산 구분 | 기존 reserved/actual 설정의 효과를 분리해 추가 비교 | 새 tokenizer·예측기를 추가하지 않음. 실제 제공자의 제한 계약 확인이 필요 |
| HiveMind의 한도 헤더 관측 | 비교 대상의 동작과 설정을 직접 실행해 확인 | 성공 헤더의 범위·시간 의미가 확정되기 전 범용 자동 제어는 도입하지 않음 |

참조 목적을 작은 기존 코드 변경에 맞췄으며 경쟁 구현을 통째로 이식하지 않았다. 배경과 다른 의견은 [교차 토론 기록](competitor-adoption-debate-2026-09-15.md)에 남아 있다.

## 실제 가져온 저장소와 실행 범위

| 대상 | 고정 버전 / commit | 이번 실행 |
|---|---|---|
| Bifrost | `transports/v2.1.1` / `c193745d2a71` | native HTTP source build. upstream load-test와 같은 UI placeholder; 공식 전체 UI 배포본과 동일하지 않음 |
| HiveMind | HEAD `0468db5db9d0` | 격리 Python 환경. 안정판 `v0.2.0`도 clone했지만 필요한 limit 설정을 제공하는 HEAD를 비교 |
| LiteLLM | `v1.100.1` / `1dba17b10ded` | 격리 Python 환경, 한 worker, usage-based-routing-v2와 pre-call checks |
| PromptForge | `promptforge-workshop-v0.2.0` / `5df5802e0e9f` | clone·소스 검토·공식 Workshop archive checksum 확인. headless gateway 실행 비교에는 포함하지 않음 |

클론·툴체인·빌드 출력은 `/Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/`에 있다. 기존 연구 checkout은 보존했다. Docker/WSL·전역 설치는 사용하지 않았다. 비교 프로세스는 순차 실행하고 각 실행 뒤 종료한다.

[Bifrost·PromptForge 준비/소스 근거](competitor-native-preparation-2026-09-15.md), [HiveMind·LiteLLM 준비/소스 근거](competitor-python-preparation-2026-09-15.md). 각각 실제 launcher·config·commit·binary/dependency hash와 초기 실패 기록을 포함한다.

## 비교 방법

[실행 스크립트](../scripts/compare-native-gateways.py)는 기존 stdlib HTTP/SSE client와 mock lifecycle을 재사용한다. 정상 content·종료 marker를 검사하고 usage 관측 여부와 모든 ingress의 완료·거절·취소·timeout을 남긴다. SSE `usage:null`은 관측 usage로 세지 않도록 공통 reader를 수정했다.

모든 arm의 client/gateway retry는 0, cache는 off다. 공통 mock의 실제 실행 동시성은 2이며 cap1은 DeskQuota의 별도 음성 대조다. upstream 모델 생성은 수행하지 않는다. 성공 지연은 요청 제출부터 SSE 종료까지이며 quota 대기와 mock service 시간을 포함한다.

### 대기 없는 전달

arm마다 독립 실행 3회, 각 5회 warmup 뒤 같은 연결에서 100회 순차 요청. mock service 0ms, upstream quota 없음. DeskQuota quota는 명시적 `unlimited`이고 Python 비교군의 limiter는 높은 비구속 값이다. Bifrost는 file-only로 governance를 끈 구성이다. 따라서 이 비교는 quota 정책의 비용을 똑같이 켠 실험이 아니다.

### quota 조건

사전에 seed101로 만든 생성 요청 18개를 32초 동안 제출한다. 사용자 메시지 내용 크기96/800/1900bytes, output bound64/256/768, mock service20/400/2500ms, root4개이며 한 요청은100ms 후 취소를 시도한다. 내용 크기는 전체 JSON 본문 크기와 다르다. 이미 종료된 요청은 취소로 재분류하지 않는다. 관측 창60초 뒤에도 각 요청의 최대125초 timeout까지 drain한다. mixed 내부 비교는 모델 조회4개를 더한다.

공통 upstream은 rolling60초 RPM16 / 비용6000을 입장 시 차감한다. JSON을 변환하는 제품을 불리하게 만들지 않도록 변환 전 동일 요청의 사전 비용을 사용한다. **두 합성 계약 모두 비교한다.**

- `byte_reserved`: 원본 JSON bytes + output bound. DeskQuota의 보수적 추정과 일치하는 합성 계약이다.
- `quarter_actual`: 각 항을4로 나누고 올림한 값. 같은 요청·도착·취소 계획에서 제공자 비용만 달라진다.

둘 다 실제 모델 tokenizer나 정답 작업이 아니다. 특히 첫 조건만으로 일반 TPM 효율을 주장할 수 없다. 각 제품은 원래 지원하는 limiter 계약을 쓴다. Bifrost는 주기 counter와 완료 usage, HiveMind는 완료 usage, LiteLLM은 UTC 분 단위 bucket과 입력 추정/완료 usage를 사용한다. DeskQuota는 held reservation을 포함한다. 이는 **완제품 구성 비교**이며 스케줄러 알고리즘만의 비교가 아니다.

DeskQuota quota 초기60초 hold는 새 비교의 관측 창 밖에서 기다린다. Bifrost 시작 시 모델 조회2회도 따로 기록한다. cold start가 빠르다는 근거로 쓰지 않는다. Bifrost quota 구성은 검증된 embedded SQLite config store를 켜므로 file-only 결과와 메모리를 섞지 않는다.

## 완료된 무대기 결과

아래 p50/p95/p99는 각 실행의 percentile **최소–최대**, 단위ms다. RSS는 자기 gateway PID의 idle 표본, 단위MiB다. 모든 arm이 각 실행100/100, 합계300/300 완료했고 protocol 오류는0이다. Direct는 동일 mock 직결이며 gateway 메모리가 없다.

| arm | p50 범위 | p95 범위 | p99 범위 | idle RSS |
|---|---:|---:|---:|---:|
| Direct | 0.156–0.173 | 0.196–0.252 | 0.199–0.344 | — |
| DeskQuota RR | 0.213–0.294 | 0.394–1.229 | 0.527–3.722 | 8.78–8.84 |
| DeskQuota 현재 backfill | 0.288–0.297 | 0.418–0.429 | 0.522–0.879 | 8.80–8.84 |
| DeskQuota 이전 backfill | 0.300–0.316 | 0.416–0.835 | 0.505–1.171 | 8.73–8.78 |
| Bifrost file-only | 0.435–0.444 | 0.781–0.851 | 1.131–1.864 | 59.55–60.70 |
| HiveMind HEAD | 0.958–0.980 | 1.190–1.282 | 1.352–1.421 | 50.02–50.38 |
| LiteLLM | 4.178–5.957 | 4.860–11.456 | 5.412–13.868 | 285.69–285.73 |

DeskQuota가 작은 메모리와 낮은 전달 비용을 보인 것은 직접 관찰했다. 하지만 RR의 p95는3배 넘게 변동했고 한 실행에서는 Bifrost보다 높았다. **항상 가장 빠르다는 증거는 아니다.** backfill이 선택될 대기열이 없는 입력이므로 두 DeskQuota 정책 사이 차이를 알고리즘 개선으로 해석하지 않는다.

자기 PID CPU 증분은100건당 DeskQuota0.01–0.02초, Bifrost0.04초, HiveMind0.07–0.08초, LiteLLM0.40–0.62초였다. `ps` 누적 CPU의 낮은 해상도와 짧은 실행 때문에 CPU 효율 배수는 계산하지 않는다. RSS도 process tree 전체나 연속 peak를 측정한 값이 아니다. DeskQuota 실행 파일은 `bench-harness` feature가 포함된 고정 benchmark binary이며 기존 배포 package와 동일하다고 주장하지 않는다.

반복 seed가 달라도 무대기 요청은 동일하다. 새로운 workload3종이 아니다. upstream body는 Direct/DeskQuota/HiveMind189B, Bifrost240B, LiteLLM229B였으며 입력 의미와 output bound는 같았다. LiteLLM은 요청에서 `include_usage`를 요구하지 않은 이 조건에서 client-visible usage가300건 모두 missing이었다. 다른 arm은 observed였다. 이는 성공 내용·종료의 실패나 실제 사용량0을 뜻하지 않는다.

## full-ledger 구성요소 비용

release 모드에서 기존 ledger의 `check`/`can_backfill`을 고정 시각으로 측정했다. retained entry0/1/8190/8192, 반복 head1/16, warmup32회, 25batch ×16iteration이다. Entry는64bytes다.

8190개 entry와16개 동일 head의 `can_backfill` 성공 경로는6400/6400회 통과했다. batch당 호출 평균의 p95는39.7μs,16head scan 환산 p95는0.635ms다. 8192개 포화 상태는 성공0회의 조기 거절이며0.263ms였다. 포화 거절을 정상 projection 비용으로 쓰지 않았다.

**Queue 선택·lock·HTTP를 제외한 microprofile**이다. 실제 요청 p95나 전체 큐의 최악 지연으로 해석하지 않는다. [명령과 원시 출력](../evidence/competitor-comparison-2026-09-15/full-ledger-profile.log), [구조화 결과](../evidence/competitor-comparison-2026-09-15/full-ledger-profile.json).

## quota 완제품 비교

각 행은 같은 생성 요청18건의 **한 실행**이다. `창/최종`은60초 내 완료와 이후 drain 포함 완료다. 요청마다 최대125초를 기다리며 DeskQuota 큐 자체의 상한은120초다. p95는 성공 응답만의 값이고 성공 표본이7–17개여서 이번 nearest-rank 계산에서는 최댓값과 같다. 이 작은 표본을 안정적인 tail 성능으로 해석하지 않는다. 두 표의 DeskQuota는 같은 실험 바이너리의 reserved 설정이다.

| `byte_reserved` | 창/최종 완료 | 거절/timeout | 성공 p95, 초 | upstream 시도 |
|---|---:|---:|---:|---:|
| Direct | 7/7 | 11/0 | 0.405 | 18 |
| DeskQuota RR | 7/16 | 0/2 | 116.196 | 16 |
| DeskQuota backfill | 7/15 | 0/3 | 114.231 | 15 |
| Bifrost | 7/7 | 11/0 | 0.408 | 18 |
| HiveMind HEAD | 6/13 | 5/0 | 51.439 | 18 |
| LiteLLM | 7/7 | 11/0 | 0.616 | 18 |

DeskQuota는 이 조건에서 더 많은 요청을 기다려 완료했지만, 첫 창의 생성 완료가 늘지는 않았다. 다른 제품이 즉시 거절한 요청을 오래 기다린 뒤 성공시킨 결과다. RR에서도2건은 deadline을 넘었다.

| `quarter_actual` | 창/최종 완료 | 거절/timeout | 성공 p95, 초 | upstream 시도 |
|---|---:|---:|---:|---:|
| Direct | 16/16 | 2/0 | 2.504 | 18 |
| DeskQuota RR | 7/16 | 0/2 | 116.195 | 16 |
| DeskQuota backfill | 7/15 | 0/3 | 114.231 | 15 |
| Bifrost | 16/16 | 2/0 | 2.505 | 16 |
| HiveMind HEAD | 16/17 | 1/0 | 40.277 | 18 |
| LiteLLM | 16/16 | 2/0 | 2.526 | 18 |

제공자 차감량을 작게 바꾸면 보수적 예약을 유지하는 DeskQuota는 첫 창의 용량을 충분히 쓰지 못했다. **빠른 Rust 전달만으로 입장 비용 추정의 불일치를 해결할 수 없다는 관측**이다. Bifrost의 이 조건2건 거절은 gateway에서 발생해 upstream 시도는16회였다. 나머지 외부 arm의 거절은 mock에 도착한 뒤 발생했다. 모든 비교군을 같은 방식으로 거절했다고 뭉뚱그리지 않는다.

이17개 quota/mixed/cap 비교에서는100ms 취소 대상으로 정한 요청이 취소 시각 전에 종료돼 실제 취소0건이었다. 취소가 검증됐다고 주장하지 않으며 제품의 별도 lifecycle 회귀 검사와 구분한다. retry는 전부0이다. LiteLLM의 UTC minute bucket 시작 위상을 맞춘 실험도 아니므로 설정 계약 차이와 wall-clock 경계를 포함한 탐색 비교다.

## 동시성1 대조

동시성1의 generation / byte_reserved에서 RR와 backfill은 각각18건 중16완료·2timeout, 창 내 완료7건, upstream16회로 같았다. 성공 평균은50.976/50.975초, p95는117.195/117.193초였다. 이 조건에서 backfill의 이득은 관찰되지 않았다.

## 독립 입력에서 확인한 기본값 승격 반례

generation-only / seed101 / `byte_reserved` / cap2의18건에서 RR은16완료·2timeout, 현재 backfill은15완료·3timeout이었다. 첫60초 완료는 둘 다7건이다. 성공한 요청만의 평균은50.427→45.427초로 줄었지만, 성공 집합이 다르므로 성능 개선으로 판정하지 않는다.

예를 들어 짧은 생성g11은116.196→55.349초로 빨라졌지만 g8은56.277→108.046초로 느려졌다. g14는 RR에서113.885초에 완료했고 backfill에서는 큐의120초 deadline으로 timeout됐다. metadata 없는 입력이므로 이번 metadata 타입 분리의 개선 효과로 귀속할 수도 없다. **기본값은 RR로 유지한다.** 이 반례는 이후 기존 workload 반복이 좋아도 삭제하지 않는다.

독립 구현 검토자는 입력·시도 순서·비용과 코드를 재구성했다. 먼저 실행된g11의446 비용이 남으면서g8 입장 시 잔량878이 비용957보다79 부족해졌다. 이후g14도 잔량367이 비용446보다79 부족했고 다음 만료 전에 deadline에 도달했다. 당시 보호 요청으로 추론되는g2의 실제 upstream 도착은 RR75.098초/BF75.101초로 거의 같았다. 현재 보호 head의 다음 기회를 유지하는 조건과 모순되는 관측은 발견하지 못했지만, 뒤따르는 모든 요청의 deadline을 보장하지는 않는다. barrier ID를 매 선택마다 기록한 실험이 아니므로 이는 재구성한 원인 해석이지 모든 불변식의 실행 증명은 아니다.

## 독립 혼합 입력에서는 metadata 효과가 나타나지 않음

같은 seed101 생성 입력에 모델 조회4개를6/10/14/18초에 더한 비교에서 이전/현재 backfill 모두22건 중19완료·3timeout이었다. 모델 조회는 양쪽 모두4/4 완료했고, 평균은13.443/13.589ms, 최대는14.792/14.223ms였다. 조회가 barrier에 막히는 시점에 도착하지 않아 이번 변경의 이득이 드러나지 않았다. 이 작은 지연 차이를 개선이나 퇴보로 해석하지 않는다.

## 막힌 큐에서 metadata 변경의 효과

별도 `metadata_barrier` 입력에서는 root0의 큰 비용 요청 뒤 root1 생성 요청이 quota에 막힌 상태에서 root2 모델 조회를 넣었다. 조회를 넣기 전7초 시점에 양쪽 모두 실제 status에서 `barrier_root=r1`, 대기1건·실행0건·차감5095/6000을 확인했다. upstream 기록도 첫 생성 요청1건의 완료만 나타냈다.

| 같은3요청, 이전/현재 실험 backfill | 이전 | 현재 |
|---|---:|---:|
| 모델 조회 제출→완료 | 52.022초 | 13.773ms |
| 보호된 생성 요청 제출→완료 | 59.032초 | 59.037초 |
| 보호 요청 입장: 첫 quota 만료 이후 | 0.924ms | 1.898ms |
| 최종 완료 / upstream 시도 | 3/3건 / 3회 | 3/3건 / 3회 |
| 첫60초 생성 / metadata 완료 | 1 / 0건 | 1 / 1건 |

이 한 쌍의 기전 검사에서는 모델 조회가 빈 슬롯을 사용했고 큰 요청도 양쪽 모두 첫 만료 기회에 입장했다. retry·429·timeout·protocol 오류는0이었다. 이는 **바꾼 실험 분기가 동작한다는 근거**다. 일반 workload 성능 우위·생성 처리량 증가·현재 RR 기본값의 개선을 뜻하지 않는다. 혼합 입력의 이득 없음과 생성 입력의 반례는 그대로 남긴다. [사전 조건과 쌍 검증](../evidence/competitor-comparison-2026-09-15/metadata-mechanism-validation.json).

## 일반 바이너리의 기존 정산 설정 비교

보수적 예약과 실제 차감량의 불일치가 드러나 추가한 설정 민감도 검사다. 동일 소스의 일반 release 바이너리 하나를 `--no-default-features --bin llmgw`로 별도 빌드했고, `bench-harness` 없이 RR의 `reserved`/`actual` 정산만 바꿨다. 같은 비용 계약 안에서 입력은 완전히 같으며 다른 설정 차이는 임시 loopback 포트뿐이다. 새 알고리즘이나 독립 holdout 결과로 계산하지 않는다.

| 비용 계약 / 정산 | 첫60초 완료 / 18건 | 최종 완료 / 18건 | timeout | 성공 p50 / p95, 초 | upstream 시도 |
|---|---:|---:|---:|---:|---:|
| byte_reserved / reserved | 7 | 16 | 2 | 56.190 / 116.199 | 16 |
| byte_reserved / actual | 7 | 16 | 2 | 56.190 / 116.196 | 16 |
| quarter_actual / reserved | 7 | 16 | 2 | 56.191 / 116.197 | 16 |
| quarter_actual / actual | 16 | 18 | 0 | 0.404 / 40.633 | 18 |

실제 차감량이 작은 합성 조건에서는 `actual` 정산으로 첫 창 완료가7→16건, 최종 완료가16→18건으로 늘었다. 같은18건을 모두 완료해도 RPM16 때문에 일부 요청은 다음 창까지 기다렸다. 반대로 예약량과 실제 차감량이 같은 조건에서는 이득이 없었다. **기존 설정을 제공자 계약에 맞춘 효과이며 이번 코드 변경의 속도 개선이 아니다.** 성공 집합이16건과18건으로 다르므로 p95 감소율을 동일 요청 집합의 개선율로 제시하지 않는다. 각1회 검사이며 모든 환경에서 `actual`을 기본값으로 삼을 근거도 아니다.

`actual`은 새 요청의 입장 비용을 예측하는 기능이 아니라, 완료 usage를 관측했을 때 기존 예약을 정산하는 설정이다. 제공자가 예약량으로 제한을 차감한다면 응답 usage만 보고 예산을 돌려주는 정책은 한도를 넘을 수 있다. 이번 fixture는 usage를 항상 보냈으며 missing usage·늦은 usage·실제 제공자별 제한 계약은 이4회로 검증하지 않았다. 그런 상황의 코드 회귀 검사와 실제 API 검증을 구분한다.

일반 바이너리는10,038,208B이며 이4회 quota 실행의 idle RSS는9.55–9.59MiB, 관측 최대 RSS는10.78–11.06MiB였다. 자기 PID CPU 증분은0.01–0.02초였다. 앞의 실험용 바이너리 무대기 표와 섞지 않는다. [빌드·바이너리 hash](../evidence/competitor-comparison-2026-09-15/native-build.json), [실행 순서](../evidence/competitor-comparison-2026-09-15/native-accounting-schedule.json).

## 결과 감사와 재현 범위

44회는 무대기21회, 생성 비용 계약 비교12회, 혼합 입력3회, cap1 대조2회, metadata 기전2회, 일반 바이너리 정산4회로 구성된다. 관측 요청2,496건은 **2,423완료 + 45거절 + 28timeout**으로 전부 종료됐다. upstream 시도2,466회, retry0회, 실제 취소0건, protocol 오류0건이었다. warmup105건과 시작 시 discovery10건은 이 분모에서 제외했다. 이 합계를 서로 다른 조건을 합친 성공률 순위로 사용하지 않는다.

원시 outcome에서 class별 p50/p95/p99·완료/거절/timeout·창 내 완료를 재계산하고 요청 ID와 upstream 시도·사용량 관측·프로세스 종료를 검사했다. [실행별 집계](../evidence/competitor-comparison-2026-09-15/completed-run-summary.json)에는 root별 최대 종료 지연과 CPU/RSS 표본도 있다. 예를 들어 생성 byte_reserved의 RR root별 최대 종료 지연은120.004/114.231/114.314/116.196초였고, backfill은120.004/114.231/120.004/55.349초였다. timeout을 빼고 공정성이 좋아졌다고 판단하지 않는다.

같은 집계의 `submission_to_upstream_ms`는 제출부터 upstream 도착까지이며 transport와 admission을 모두 포함한다. upstream에 도착하지 못한 요청은 포함하지 않으므로 순수 queue wait나 전체 요청의 대기 percentile로 부르지 않는다. 무대기 결과의 CPU/RSS 한계는 quota 측정에도 적용된다. Bifrost의 quota 구성은 idle83.09–83.23MiB로 file-only 무대기 구성보다 컸다. 외부 limiter의 서로 다른 차감·거절 계약과 shared Mac의 시스템 부하는 통제하지 못했다.

모든 실행은 동일 PC에서 순차 수행했다. 제품 검토 때 고정한9파일을 빌드·실험 동안 유지했고, 하네스v1과 native/기전 추가v2의 소스 snapshot을 보관했다. v2의 사전 조건 검사 실패가 발생해도 형제 요청·client·소유 프로세스를 정리하는지 강제 실패 검사와 독립 리뷰로 확인했다. 실제 성능 실행44개에 이 실패 검사를 섞지 않았다.

저장소 루트에서 결과만 다시 검사하는 명령:

```sh
python3 evidence/competitor-comparison-2026-09-15/aggregate.py
```

고정한 runtime/cache와 준비 보고서의 설정을 사용해 새 출력 디렉터리에 실행하는 예:

```sh
python3 scripts/compare-native-gateways.py --arm bifrost --profile generation \
  --seed 101 --cost-contract byte_reserved --cap 2 --output /tmp/deskquota-bifrost-repeat
python3 scripts/compare-native-gateways.py --arm deskquota_native_actual --profile generation \
  --seed 101 --cost-contract quarter_actual --cap 2 --output /tmp/deskquota-actual-repeat
```

하네스는 이번 로컬 실험의 cache 절대 경로를 사용하므로 다른 PC에서는 준비 보고서대로 고정 runtime을 확보하고 해당 경로를 맞춰야 한다. 범용 benchmark 배포 도구가 아니다. 초기 준비 실패와 소스 빌드 조건도 보존했다. 원시 evidence는 현재 로컬에 있으며, 공개 snapshot에 포함할 파일은 게시 전에 별도로 검토해야 한다. [집계 검사 결과](../evidence/competitor-comparison-2026-09-15/completed-run-audit.json), [제품 검토](../evidence/competitor-comparison-2026-09-15/product-review.json), [v2 하네스 검토](../evidence/competitor-comparison-2026-09-15/harness-v2-review.json), [참조 root license hash](../evidence/competitor-comparison-2026-09-15/reference-licenses.json).

## 유지한 판단과 남은 검증

현재 강점은 작은 native 프로세스, 하나의 PC에서 여러 요청의 quota 대기를 통합하는 동작, 변경한 실험 metadata 경로의 대기 해소다. 경쟁 구현의 넓은 provider·운영 기능을 추가하지 않아 가벼움을 유지했다. quota 조건에서 더 많이 완료한 결과에는 오래 기다리는 비용이 있으며, 작은 응답과 전달 속도만으로 에이전트 작업 완료가 빨라진다고 보장할 수 없다.

다음 개선의 우선순위는 실제 대상 제공자 한 곳의 RPM/TPM 차감·usage 의미를 확인하고 기존 설정을 검증하는 것이다. `actual`을 무조건 켜거나 새 예측기를 추가하지 않는다. 모델 조회 기전의 효과를 실제 client 시나리오에서 검증하고, 생성 요청의 deadline 반례를 해결하기 전에는 backfill을 기본값으로 승격하지 않는다.

기존120초5쌍 반복은 이번에 실행하지 않는다. 원래 기본값 승격을 위한 반복이었지만 독립 입력의 완료율 반례가 이미 승격을 기각했다. 같은 옛 입력의 반복으로 이 반례를 덮지 않는다. 기존 미완료 실험은 별도로 남으며 통과했다고 표시하지 않는다.

NVIDIA 등 실제 API의 효율, 저사양 PC·Windows/Linux 실행, 장시간 동시 에이전트, 실제 작업 품질·사용 수요와 논문 수준의 일반화는 이번 결과로 입증하지 않았다. 코드 적용과 이 범위의 비교는 완료했지만 더 넓은 성능 목표V01은 진행 중이다.
