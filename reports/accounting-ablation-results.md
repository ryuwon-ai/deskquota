# 회계 방식 ablation 결과

상태: **측정 완료, 독립 감사 PASS**  
측정일: 2026-09-12 UTC  
대상: 동일한 ordinary release binary에서 `reserved` 회계와 완료 후 `actual` 회계 비교

## 300초 고정 구간 결과

각 arm은 seed마다 동일한 100개 제출 trace를 사용했다. 다섯 seed는 서로 다른 실사용 workload가 아니라, 같은 synthetic fixture를 scheduling jitter seed만 바꿔 반복한 것이다.

| seed | 실행 순서 | reserved 완료 | actual 완료 | actual - reserved | generation 완료 (reserved / actual) | metadata 완료 (reserved / actual) |
|---:|---|---:|---:|---:|---:|---:|
| 1 | reserved → actual | 57 | 58 | +1 | 51 / 52 | 6 / 6 |
| 2 | actual → reserved | 57 | 58 | +1 | 51 / 52 | 6 / 6 |
| 3 | reserved → actual | 57 | 58 | +1 | 51 / 52 | 6 / 6 |
| 4 | actual → reserved | 57 | 58 | +1 | 51 / 52 | 6 / 6 |
| 5 | reserved → actual | 57 | 58 | +1 | 51 / 52 | 6 / 6 |
| **합계** | 5 paired seeds | **285 / 500** | **290 / 500** | **+5** | **255 / 260** | **30 / 30** |

이 fixture에서는 actual arm의 300초 완료율이 58%, reserved arm이 57%로, 요청 기준 1.0%p 차이를 보였다. 다섯 pair 모두 차이는 `+1`이었고 추가 완료는 generation 응답에서 나왔다.

집계 차이가 모든 요청의 개선을 뜻하지는 않는다. seed 1의 actual-only 완료는 `w2-mixed_lengths-2`, `w3-burst-7`이고 reserved-only 완료는 `w2-mixed_lengths-0`이다. seed 2~5의 actual-only 완료는 각각 `w3-burst-6`, `w3-burst-7`, `w3-burst-4`, `w3-burst-5`였다. 따라서 같은 long 완료 합계도 실제 완료 요청 집합의 차이를 숨길 수 있다.

## 전체 terminal 분모와 cutoff 이후 drain

300초 시점의 terminal과 그 뒤 bounded drain에서 끝난 terminal을 분리했다. 각 arm의 최종 분모는 제출 500개다.

| arm | 300초 내 완료 | 300초 내 취소 | 300초 내 timeout | drain 완료 | drain 거절 | drain timeout | 최종 완료 | 최종 취소 | 최종 거절 | 최종 timeout |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| reserved | 285 | 75 | 5 | 112 | 3 | 20 | 397 | 75 | 3 | 25 |
| actual | 290 | 75 | 5 | 120 | 0 | 10 | 410 | 75 | 0 | 15 |

| arm | ingress | upstream attempts | gateway started | mock received | mock 429 | 첫 시도 이후 추가 attempt |
|---|---:|---:|---:|---:|---:|---:|
| reserved | 500 | 400 | 400 | 400 | 3 | 0 |
| actual | 500 | 410 | 410 | 410 | 0 | 0 |

예약된 cancellation 75개는 두 arm 모두 75개가 취소로 끝났다. client retry와 gateway retry는 꺼져 있었고, 어느 ingress에도 두 번째 upstream attempt가 없었다. 실제 사용량 회계 arm의 upstream attempt가 10개 더 많았지만 이 실험만으로 admission budget이나 provider 측 quota 동작을 일반화할 수 없다.

## 길이와 root별 fixed, drain, 최종 결과

아래 표에서 모든 쌍은 `reserved / actual` 순서다. `300초 내` 열은 고정 측정 구간에 terminal이 기록된 요청만, `drain` 열은 300초 cutoff 뒤 bounded drain에서 terminal이 기록된 요청만 센다.

| 구분 | 제출/arm | 300초 내 완료 | 300초 내 취소 | 300초 내 timeout | drain 완료 | drain 거절 | drain timeout |
|---|---:|---:|---:|---:|---:|---:|---:|
| long | 100 | 25 / 25 | 50 / 50 | 5 / 5 | 8 / 10 | 2 / 0 | 10 / 10 |
| short | 400 | 260 / 265 | 25 / 25 | 0 / 0 | 104 / 110 | 1 / 0 | 10 / 0 |
| root 0 | 225 | 139 / 140 | 25 / 25 | 5 / 5 | 43 / 45 | 2 / 0 | 11 / 10 |
| root 1 | 125 | 64 / 65 | 25 / 25 | 0 / 0 | 33 / 35 | 1 / 0 | 2 / 0 |
| root 2 | 75 | 34 / 35 | 25 / 25 | 0 / 0 | 13 / 15 | 0 / 0 | 3 / 0 |
| root 3 | 75 | 48 / 50 | 0 / 0 | 0 / 0 | 23 / 25 | 0 / 0 | 4 / 0 |

고정 구간의 거절과 error, drain의 취소와 error는 모든 subgroup과 두 arm에서 0이다. 최종 열은 fixed와 drain을 합친 각 subgroup의 전체 제출 분모다.

| 구분 | 최종 완료 | 최종 취소 | 최종 거절 | 최종 timeout | 최종 error |
|---|---:|---:|---:|---:|---:|
| long | 33 / 35 | 50 / 50 | 2 / 0 | 15 / 15 | 0 / 0 |
| short | 364 / 375 | 25 / 25 | 1 / 0 | 10 / 0 | 0 / 0 |
| root 0 | 182 / 185 | 25 / 25 | 2 / 0 | 16 / 15 | 0 / 0 |
| root 1 | 97 / 100 | 25 / 25 | 1 / 0 | 2 / 0 | 0 / 0 |
| root 2 | 47 / 50 | 25 / 25 | 0 / 0 | 3 / 0 | 0 / 0 |
| root 3 | 71 / 75 | 0 / 0 | 0 / 0 | 4 / 0 | 0 / 0 |

최종 timeout 감소 `25 → 15`는 short의 `10 → 0`에서만 발생했고, long은 `15 / 15`로 같았다. actual arm의 최종 timeout 15개는 모두 root 0에 남았다. drain 결과를 300초 고정 구간 throughput이나 fairness 개선으로 해석하지 않는다. 여기서 root는 설정된 route-level fairness 경계다. 같은 agent profile의 session은 같은 root를 공유하며, agent별 자동 분리를 의미하지 않는다.

## 성공 요청 latency

아래 값은 각 seed의 **최종 성공 요청만** 포함한 completion latency 범위다. 실패, 취소, 거절을 제외한 값이므로 위 terminal 분모와 함께 읽어야 한다.

| arm | 성공 n | seed별 p50 범위 | seed별 p95 범위 |
|---|---:|---:|---:|
| reserved | 397 | 72.355–72.371초 | 119.970–119.981초 |
| actual | 410 | 72.369–72.378초 | 119.959–119.974초 |

이 latency는 client 제출부터 synthetic 응답 완료까지의 혼합 경로 측정이다. gateway 내부 실행 시간이나 provider 처리 시간으로 해석할 수 없다.

## 사용량 관측과 보조 queue 관찰

| arm | numeric usage 관측 | metadata `not_applicable` | usage `not_observed` |
|---|---:|---:|---:|
| reserved | 347 | 50 | 103 |
| actual | 360 | 50 | 90 |

독립 감사는 numeric usage가 연결된 attempt 응답과 fixture의 actual ratio에 맞는 정수인지, metadata와 실패·취소의 usage 상태가 올바른지 확인했다. client가 usage를 관측했다는 사실만으로 provider admission budget 환급을 입증하지는 않는다. 공식 quota 계약과 이 한계는 [quota 계약 후속 기록](quota-contract-followup.md), 코드 근거의 범위는 [frozen usage source basis](../evidence/accounting-usage-source-basis.json)에 기록돼 있다.

seed 1의 이미 측정된 status sample을 별도로 읽었을 때 두 arm 모두 300초 안의 298개 sample 중 276개가 gateway 보고 `starvation_barrier`였고, 283개는 queue가 양수이면서 active가 0이었다. 그중 274개가 같은 barrier를 보고했다. 첫 과대 추정 요청은 약 12.108초에 제출되고 양쪽 모두 약 72.004초에 mock에 도달했으며, estimate 3222와 fixture actual 806이었다. metadata 응답도 약 107초가 걸린 사례가 있었다. 이 수치는 시간 비율이나 독립적으로 입증된 원인, counterfactual throughput이 아니다. 완료 후 회계 변경만으로는 admission 전 추정과 protected-head 대기를 없애지 못할 수 있다는 **한 seed의 보조 관찰**이다. 원자료와 한계는 [queue observation](../evidence/accounting-first-pair-queue-observation.json)과 [scheduler 후속 기록](scheduler-utilization-followup.md)에 있다.

## 범위와 판정

실험은 auth 없음, retry/cache 없음, cancel-close, concurrency cap 2, root 4개, synthetic combined rolling 60초 quota 16 RPM/6000 TPM 조건에서 실행했다. 두 arm은 `accounting` 값 외 normalized config, 제출 schedule, binary와 source identity가 같았다. 5 pair, 10 raw run, 1,000 ingress와 terminal/attempt 연결은 [독립 전체 감사](../evidence/accounting-ablation-parent-audit.json)에서 PASS했고, 종료 뒤 PID와 원본·보존 binary identity는 [runtime hold 확인](../evidence/accounting-ablation-parent-runtime-hold-check.json)에 기록했다. 실행 명령, 로그, source hold와 cleanup은 [Task2 MATRIX HOLD](../product/artifacts/accounting-ablation/task2/MATRIX_HOLD.md), 최종 manifest와 raw 링크는 [pilot manifest](../product/artifacts/accounting-ablation/pilot.json)에 있다.

이 결과는 synthetic byte/output fixture에서 완료 후 actual 회계가 고정 구간 완료를 seed마다 1개 늘린 관찰이다. 실제 provider tokenizer·admission 계약, 실제 agent task 성공률, low-end 장비, 경쟁 제품, 광범위한 성능이나 기본 정책 변경의 근거로 확대하지 않는다. 이 측정만으로 default 회계 정책 변경을 권고하지 않는다.
