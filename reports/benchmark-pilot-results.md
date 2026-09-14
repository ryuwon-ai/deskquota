# 첫 native HTTP pilot 결과

2026-09-12. **측정·부모 검산·독립 SPEC·QUALITY 재검토 통과.**
현재 결과는 작은 실행 부담을 확인했지만, 제한된 시간 안의 처리량 우위를
보여주지 못했다. 코어 Task 8 수용 여부와 제품 전체 완료 여부는
[구현 상태](../evidence/implementation-progress.json)를 따른다.

## 측정 조건

Apple M4 32 GiB Mac에서 실제 loopback HTTP를 사용했다. 네 구성 × 다섯 seed로
무대기 20run과 quota 20run을 순차 실행했다. 본 측정 요청은 총 4,000개이며
무대기 warmup 100개는 별도다. quota run마다 실제 60초 구간 다섯 개를 이어
300초간 측정한 뒤 남은 요청을 drain/timeout으로 모두 기록했다.

quota는 RPM 16 / TPM 6,000 **합성 단위**, gateway concurrency 2 / root 4,
예약량 유지(`accounting=reserved`) 조건이다. 비용은 요청 bytes와 출력 예약량을
기반으로 한 fixture 단위이며 tokenizer의 token이나 실제 공급자 TPM이 아니다.
클라이언트·게이트웨이의 retry와 cache는 껐다. 직접 연결에는 gateway의 동시성
상한이 없으며, 동일한 mock quota를 적용한다. 전체 조건은
[측정 방법](../product/docs/benchmark-method.md)에 있다.

## quota: 완료 시점과 최종 종료 결과

아래는 **각각 독립된 300초 측정 다섯 번**의 합계다. 구성별 제출은 500개다.
하나의 연속 1,500초 실험으로 해석하지 않는다.

| 구성 | 300초 안 완료 | 이후 완료 | 최종 거절 | 최종 timeout | 최종 취소 |
|---|---:|---:|---:|---:|---:|
| 직접 연결 | 328 | 0 | 150 | 0 | 22 |
| 제품 RR | 285 | 114 | 1 | 25 | 75 |
| 공통 transport FIFO | 285 | 115 | 0 | 25 | 75 |
| 공통 transport RR | 284 | 115 | 1 | 25 | 75 |

각 행의 모든 숫자를 더하면 제출 500개가 된다. 직접 연결은 mock에 500회,
각 gateway 구성은 400회 요청했다. Gateway의 started counter와 mock의 실제
수신 counter도 각 구성 400으로 일치한다. 실패한 요청을 제거하거나 재시도를
새 사용자 성공으로 중복 집계하지 않았다.

| seed | 직접 연결의 300초 안 완료 | 제품 RR | FIFO | 비교용 RR |
|---|---:|---:|---:|---:|
| 1 | 68 | 57 | 57 | 56 |
| 2 | 69 | 57 | 57 | 57 |
| 3 | 66 | 57 | 57 | 57 |
| 4 | 69 | 57 | 57 | 57 |
| 5 | 56 | 57 | 57 | 57 |

제품 RR은 고정 시간 안 완료가 직접 연결보다 합계 43개 적었다. 이후 완료를
포함하면 399개지만, 추가 대기와 이후의 quota를 사용한 결과이므로 이를 같은
시간의 처리량 개선으로 제시할 수 없다. FIFO와 RR의 차이도 이 조건에서는
뚜렷하지 않다. Seed는 고정 workload의 작은 도착 시각 차이를 바꾸며, 서로
다른 사내 workload 다섯 종류를 대표하지 않는다.

긴 요청만 보면 구성별 100개 중 시간 내 완료는 직접 연결 27개, gateway
각각 25개다. 사전에 독립 응답 두 개를 묶어 둔 합성 pair 지표는 구성별
250쌍 중 직접 연결 125쌍, 제품 RR/FIFO 130쌍, 비교용 RR 129쌍을 시간 내
완료했다. 이 작은 차이도 보존하지만, 의존 관계가 있는 실제 agent/tool
과제의 완료로 해석하지 않는다. [별도 집계 검산](../evidence/product-task8-aggregate-parent-audit.json)에서
seed 구분과 길이·root·workload별 분모 및 분포를 원시 결과와 대조했다.

취소 예약은 구성별 75개이고 모든 구성에서 이 요청들의 성공은 0개였다.
직접 연결은 그중 53개를 거절하고 22개가 취소됐으며, gateway는 75개 모두
취소됐다. 취소 예약이 없는 425개만 따로 보아도 위 완료 수는 그대로다.
전체 500개라는 원래 분모는 유지한다.

Gateway에서 나온 429 두 건은 원시 기록에 남겼다. 하나는 첫 seed 비교용 RR의
TPM 경계, 다른 하나는 네 번째 seed 제품 RR의 drain 중 RPM 경계였다.
[mock 시각 재계산](../evidence/product-task8-quota-boundary-observations-partial.json)은
앞선 debit 만료 약 30.667마이크로초 / 3.734밀리초 전에 도착한 상황과 일치한다.
Gateway 내부의 요청별 예약 시각을 저장하지 않았으므로 정확한 원인 분해까지
검증했다고 말하지 않는다.

## 무대기: 잠정 자원 예산

| 제품 RR 관측값 | 다섯 seed 범위 | 해석 |
|---|---:|---|
| 직접 연결 대비 paired 추가 latency p95 | 0.1985~0.3480 ms | 잠정 2 ms 예산 안 |
| sampled idle RSS | 8.625~8.6875 MiB | 잠정 50 MiB 예산 안 |

서로 다른 run의 같은 seed/index를 짝지은 end-to-end 차이다. 내부 CPU 실행
시간이 아니며, 두 marginal p95를 뺀 값과도 다르다. 모든 무대기 run에서
장부 retained가 0이었다. **장부가 찬 상태의 순수 overhead, 실제 저사양 PC,
다른 OS나 경쟁 제품의 상대 성능은 아직 검증하지 않았다.** RSS는 관측한
샘플 값이며 OS high-water mark를 뜻하지 않는다.

## 다음 판단

현재 RR의 성능 우위 주장은 채택하지 않는다. 요청 전달·자원 상한·실패 처리가
맞는지는 별도 명세/품질 검토로 판단한다. 보수적인 예약량과 긴 요청 보호가
실제로 만드는 지연은 후속 실험에서 분리해야 한다.

다음 성능 후보는 기존 `actual`/`reserved` 정산 차이의 작은 ablation이다.
아직 실행하지 않았으며, 실제 upstream이 실사용량 정산을 허용하는지도 별도
계약이다. 큰 요청의 진행을 유지하면서 작은 요청을 통과시키는 후보는 그 뒤
필요성이 확인될 때 검토한다. 관련 근거는
[quota 계약](quota-contract-followup.md)과
[대기열 활용 후속 분석](scheduler-utilization-followup.md)에 있다.

## 검산과 재현 자료

- [원시 전체 manifest](../product/artifacts/pilot.json)와 개별 run 파일.
- [40개 실행 목록·SHA·집계 대조](../evidence/product-task8-full-matrix-parent-audit.json):
  본 측정 4,000개와 별도 warmup 100개, 개별 summary와 원시 파일 일치.
- [quota 20run / 2,000개 검산](../evidence/product-task8-full-quota-parent-audit.json):
  분모·종료 결과·측정 cutoff·시각·fixture 비용·실행 조건 확인.
- [무대기 20run 검산](../evidence/product-task8-no-wait-parent-audit.json).
- [측정 당시 source archive](../evidence/product-task8-measured-source-archive.json).
- [최종 검토 source·binary·소유 PID 대조](../evidence/product-task8-final-review-parent-identity-audit.json):
  측정 이후 Python 재개 검증과 설명 문서 두 파일만 변경됐고, 실행 파일과
  원본 측정 자료는 유지했다.

이 검산은 제품을 재실행한 결과가 아니다. HTTP payload 원문은 artifact에
없으므로 producer의 검증 근거와 source review가 함께 필요하다. 실제 LLM,
실제 coding task, Windows/Linux native 설치·로그인, 현재 사용자 설정 변경은
이 pilot에서 수행하지 않았다.
