# 보호 규칙과 미사용 용량: 후속 조사

2026-09-12에 시작한 **Task 8 일부 관측과 고정 source 분석**이다. 당시 행렬의 scheduler·설정·측정 코드는 바꾸지 않았다. 이후 수용한 accounting 비교 결과는 아래 2026-09-13 기록에 구분한다. 새로운 scheduler 정책을 채택한 보고서가 아니다.

## 첫 seed에서 관측한 문제

`quota-seed1-production_rr.json`의 요청·mock 수신 시각과 status를 읽었다. 정확한 분모·해시 검사는 [부모 감사 기록](../evidence/product-task8-quota-seed1-parent-audit.json)에 있다.

| 합성 요청 | 실제 관측 | 의미와 한계 |
|---|---|---|
| `w0-mixed_lengths-2` | 예상3222, mock 실제806. direct는12.110초에 수신해12.412초 완료. production RR은12.111초에 송신했지만 mock 수신72.007초, 완료72.310초 | 예약 과대 추정에 따른 입장 대기의 구체적인 사례. 실제 provider token 수가 아니다. |
| `w0-shared_quota-2` | metadata 비용0. direct는25.037초 완료. production RR은25.027초 송신,72.033초 mock 수신,72.045초 완료 | 모델 목록 같은 짧은 경로도 약47초 대기할 수 있었다. 모든 metadata 요청의 평균이나 실제 Pi picker 측정은 아니다. |
| 첫 barrier snapshot |17.213초, active0, 대기1, RPM11/16, TPM5336/6000, barrier root0 | 큰 head를 위해 남은 예산을 보호하는 상태를 직접 관측했다. |
| 300초 내 대기가 있는 snapshot |285개 중275개에서 reason이 `starvation_barrier` | 약1초 간격의 표본 개수다. 시간 비율이나 각 지연 원인의 정확한 기여도로 바꾸지 않는다. |

제품 `src/admission/queue.rs`의 `protect()`는 5초 또는8회 우회 조건이 생기면 가장 오래된 root head를 보호한다. `drive()`는 barrier가 존재할 때 그 head가 현재 입장 가능한 경우에만 선택한다. 따라서 다른 root의 작은 head가 들어갈 수 있더라도 기다리게 하는 것은 현재 구현한 정책의 동작이다. 설계대로 동작한다는 검증과 실제 workload에 적합하다는 판단은 별개다.

`accounting=actual`은 완료된 유효 usage를 정산한다. **아직 입장하지 못한 요청의 실제 비용을 미리 알게 해주는 기능은 아니다.** 위 과대 추정 요청은 먼저 실행되어야806이라는 usage를 얻는다. 그러므로 [회계 설정 비교](quota-contract-followup.md)는 유용한 다음 분리 실험이지만, 입장 전 과대 추정이나 head blocking을 모두 해결한다고 예측하지 않는다.

## 기존 시스템이 나누는 두 문제

Slurm의 현행 [공식 scheduling guide](https://slurm.schedmd.com/sched_config.html)는 높은 우선순위 작업의 예상 시작을 늦추지 않을 때 낮은 우선순위 작업을 시작하는 backfill을 설명한다. 여기에는 비교적 정확한 실행 시간 제한이 필요하며, 예약 탐색 자체에도 비용이 든다고 명시한다. 이는 안정된 기존 패턴의 근거로 읽었으며 최신 LLM 논문이나 API gateway의 성능 결과로 제시하지 않는다.

NVIDIA Dynamo의 고정 clone `4b72f79cb97f144377a5df1ac392b5b72e6dab3e`에는 DRR과 실제 dispatch 가능 여부를 나눈 구현이 있다.

- [policy_queue.rs:239–259](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/kv-router/src/scheduling/policy_queue.rs#L239-L259): dispatch 가능한 head를 먼저 얻은 뒤 그 비용을 DRR에서 사용한다.
- [policy_queue.rs:612–708](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/kv-router/src/scheduling/policy_queue.rs#L612-L708): 막힌 class에는 credit을 추가하지 않고, 실행 가능한 큰 요청에는 필요한 가상 round를 한 번에 더한다. token 크기만큼 반복 스캔하지 않는다.
- [policy_queue.rs:721–739](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/kv-router/src/scheduling/policy_queue.rs#L721-L739): 사용한 비용을 차감하고 남은 배분을 이어 쓰며, 빈 class는 credit을 초기화한다.
- [원본 테스트:1657–1754](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/kv-router/src/scheduling/policy_queue.rs#L1657-L1754): 전부 막힌 경우 credit이 늘지 않는지와 큰 요청의 bounded progress를 검사한다. 이 테스트의 source를 읽었으며 실행하지 않았다.

이 코드는 실제 RPM/TPM 잔량을 가상 credit으로 보충하는 시스템이 아니다. **DRR로 이름만 바꿔도 큰 요청이 필요한 실제 quota를 확보한다는 보장은 생기지 않는다.** 회사 API에 없는 KV/worker telemetry, cache bucket, 여러 policy YAML을 가져올 근거도 아니다. 현재 clone의 source와 [NVIDIA dev 문서](https://docs.nvidia.com/dynamo/dev/knowledge-base/modular-components/router/deficit-round-robin)를 확인했으며 특정 stable release 실행과 동일시하지 않는다.

## 검토할 가설과 반례

현재 반복과 독립 리뷰를 끝낸 뒤, 기존 Actual/Reserved 설정의 효과를 먼저 분리한다. 그래도 보호 때문에 용량이 비는 현상이 남으면 **보호한 큰 head가 quota를 확보하는 시점을 늦추지 않는 작은 head만 통과시킬 수 있는가**를 검토한다. 이는 아직 구현하지 않은 가설이다.

단순한 산술 예: 용량100 중70의 기존 debit이10초 뒤 만료되고, 보호할 요청은80을 요구한다고 하자. 지금20을 소비해도10초 뒤80이 남으므로 **이 고정 장부 모델의 quota 가용 시점**은 유지된다. 반면 지금30을 소비하면10초 뒤70만 남아 큰 요청을 늦춘다. 먼저20을 허용했다면 그 다음 작은 요청도 누적 소비를 반영해 다시 판단해야 한다. 이는 실행 결과가 아니며, 다른 요청의 사용량 증가·외부 quota 소비·실행 슬롯 점유가 없는 단순 모델이다.

필요한 검사는 다음과 같다.

1. 현재 남은 용량만 보지 않고 큰 요청의 미래 quota 가용 시점도 보존하는지 확인한다. 작은 요청의 새60초 debit이 그 시점까지 남을 수 있다.
2. TPM뿐 아니라 RPM과 실행 슬롯을 함께 확인한다. quota가 준비되어도 새로 시작한 요청이 슬롯을 계속 쓰면 큰 요청이 늦어질 수 있다.
3. 실행 시간이 모르는 긴 stream과 concurrency1을 반례로 쓴다. HTTP proxy는 실행 중 요청의 engine 상태를 보관해 선점·복원할 수 없다.
4. 취소·deadline·429·모호한 usage·다른 PC의 quota 소비가 계획된 용량을 바꿨을 때도 기존 소유권·정산을 유지한다. 추측한 완료 시간과 로컬 장부의 가용 시점을 실제 provider의 시작 보장으로 쓰지 않는다.
5. 작은 요청의 개선과 큰 요청의 완료율·최대 대기,300초 내 전체 완료량을 같이 비교한다. scheduler 계산량과 장부가 쌓인 상태의 overhead도 측정한다.

전체 cluster scheduler, 주기적 탐색 loop, 별도 모델 예측기부터 추가하지 않는다. 현재16 roots·64 waiters와 기존 ledger에서 필요한 정책을 작게 표현할 수 있는지부터 판단한다. 효과와 복잡도에 대한 근거가 없으면 채택하지 않는다.


## 2026-09-13: 새 accounting 첫 pair의 상태 관측

[독립 검산한 새 첫 pair](../evidence/accounting-ablation-first-pair-audit.json)의
원시 상태를 [별도 읽기 전용 도구](../scripts/observe-accounting-first-pair-queue.py)로
집계했다. [관측 파일](../evidence/accounting-first-pair-queue-observation.json)은
고정한 checkpoint와 각 raw SHA를 확인하고, 300초 안에 수집된 status만 사용한다.
실험·정책을 추가하거나 변경하지 않았다.

| 관측값 | reserved | actual |
|---|---:|---:|
| 300초 안 status 표본 | 298 | 298 |
| reported `starvation_barrier` 표본 | 276 | 276 |
| 큐가 있으면서 active=0인 표본 | 283 | 283 |
| 위 조건에서 barrier가 보고된 표본 | 274 | 274 |
| 300초 안 완료 | 57 | 58 |

첫 overestimated 요청은 두 arm 모두 약 12.108초에 예정됐고 약 72.004초에
mock에 도착했다. fixture 예상 비용 3,222/실제 비용 806도 같다. 완료 뒤의
actual 정산이 이 요청의 최초 admission 전에 실제 비용을 알려 주지는 않는다.
metadata 요청 중 가장 긴 client elapsed는 두 arm 모두 약 107초였다.

표본 수를 시간 비율이나 인과 기여율로 바꾸지 않는다. `active=0`과 대기 큐가
함께 있다는 사실도 안전하게 우회 가능한 요청이 있었다는 증명은 아니다.
기아 방지와 실제 quota·실행 슬롯을 함께 지키는 조건이 필요하다. 여기서 얻는
**후속 가설**은 정산 방식 외에 admission 전 비용 추정과 보호 중인 큐 선두가
효율을 제한할 수 있다는 것이다. 이 상태 표본 분석은 한 seed 범위이며,
아래 전체 정산 비교가 끝났어도 새 scheduler의 개선율을 입증하지 않는다.

## 2026-09-13: 정산 비교 수용 뒤의 판단

[전체 정산 비교](accounting-ablation-results.md)는 10회·5쌍·1,000건의 독립 검산과
[명세 재검토](../evidence/accounting-task2-spec-review/rereview/README.md)·
[품질 검토](../evidence/accounting-task2-quality-review/README.md)를 통과했다.
같은 300초의 완료는 reserved 285/actual 290이며, 긴 요청 완료는 25/25다.
최종 timeout 감소 25→15는 짧은 요청의 10→0에서 발생했고 긴 요청은 15/15로
같았다. 성공 요청의 seed별 p95도 두 arm 모두 약 120초 범위에 남았다.

따라서 완료 후 정산의 소폭 효과와 긴 요청 병목의 해결을 구분한다. 위에서 제시한
입장 전 추정·보호 중 우회 가설은 여전히 검증 전이며, 이번 결과가 안전한 우회나
engine 선점을 구현해 준 것은 아니다. 다음 성능 검토에는 작은 요청 개선뿐 아니라
긴 요청과 root별 timeout·최대 대기 및 슬롯 점유 반례가 계속 필요하다.

제품의 정산 기본값이나 scheduler는 변경하지 않았다. [Native Task 1–6의 제한된
구현·Mac 격리 검증](../evidence/native-task6-accepted.json)을 마쳤으므로, 다음 성능
검토는 위의 quota 가용 시점과 실행 슬롯 점유 반례를 함께 다룬다. 새로운 정책은
그 검증 전까지 채택하지 않는다. 설치·실제 도구 연결 통과를 공급자 quota, 저사양
또는 경쟁 제품 처리량 우위로 확대하지 않는다.
