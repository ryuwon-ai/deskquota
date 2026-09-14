# Quota 산정 계약과 다음 실험의 경계

2026-09-12. Task 8의 첫 quota 결과를 해석하면서 확인한 **공식 문서와 후속 가설**이다. 진행 중인 행렬의 코드·설정은 바꾸지 않았다. 실제 provider API를 호출한 기록도 아니다.

**2026-09-13 진행 상태:** 아래 초기 가설 뒤에 전체 Task 8과 별도 정산 비교를
완료했다. [정산 비교의 수용된 결과](accounting-ablation-results.md)는 같은 300초
완료가 reserved **285/500**, actual **290/500**이며, 긴 요청의 최종 timeout은
**15/15**로 같았다. 따라서 아래의 “아직 비교하지 않았다”는 문장은 최초 조사
시점의 기록이다. 이제 남은 성능 과제는 긴 요청 병목과 실제 제한 계약에서의 효과를
구분하는 것이며, 이 결과만으로 스케줄러나 기본 정산 정책을 바꾸지 않았다.
현재 구현은 재현 가능한 native 설정·도구 연결을 완성하는 단계다.

## 현재 문서에서 확인한 차이

[Claude API rate limits](https://platform.claude.com/docs/en/api/rate-limits)를 다시 확인했다. 문서는 token bucket, RPM·입력 ITPM·출력 OTPM의 별도 제한, 실제 생성량에 따른 출력 제한을 설명한다. 현재 설명에서 `max_tokens`는 OTPM 산정에 들어가지 않는다. 대부분 모델의 cache-read 입력도 ITPM에서 제외하며 예외 모델이 있다. 따라서 “출력 상한을 낮추면 모든 API의 TPM 소모가 줄어든다”는 공통 규칙을 사용하면 안 된다.

이는 Claude API의 문서상 계약이다. 같은 Messages 형식의 사내 서버가 같은 산정을 한다고 추정하지 않는다. 사내 proxy나 로컬 모델의 실제 제한 방식은 별도 확인 대상이다. Vertex AI의 관련 공식 페이지는 이번 직접 조회가 timeout으로 실패했으므로 검색 요약만으로 현행 정책을 새 결론에 추가하지 않았다.

## 현재 pilot이 검증하는 범위

현재 mock은 하나의 **합산 RPM/TPM·60초 rolling window**를 사용한다. gateway는 실제 JSON bytes+출력 예약량을 비용으로 계산하고 `accounting=reserved`로 유지한다. mock은 한 종류의 요청에서 비용을 그 추정치의 약 0.25배로 산정한다. 이 설정은 예약 오차가 있는 합성 실험이며 Claude의 token bucket·분리 ITPM/OTPM을 구현한 실험이 아니다.

첫 seed에서 production RR의 300초 내 완료량은57, 직접 연결은68이었다. gateway의 뒤늦은 완료23건을 더해 같은 시간 안의 개선이라고 보고하지 않았다. 이 한 seed는 최종 정책 판정이 아니며 [전체 기록](../product/artifacts/pilot.json)의 반복이 끝나야 한다.

예약 과대 추정과 기아 방지 장벽이 각각 얼마만큼 영향을 주었는지는 이 결과만으로 분리할 수 없다. 이미 구현된 `accounting=actual`은 유효 usage가 확정되면 정산하는 별도 선택이다. 공급자 계약에 맞는 경우, 같은 fixture·스케줄러에서 회계 설정만 바꾸는 후속 실험으로 추정 오차의 영향을 살펴볼 수 있다. **아직 그 비교를 실행하지 않았으며 기본값 변경도 결정하지 않았다.**

## 다음 판단 순서

1. 현재 고정한 행렬을 끝내고 모든 seed의 시간 내 완료·짧은/긴 요청 손해·사용하지 못한 예산을 확인한다.
2. 측정 하네스와 해석의 독립 리뷰를 끝낸다. 빈 장부에서 얻은 무대기 지연을 populated-ledger 비용으로 대표하지 않는다.
3. 공급자 산정 계약에 맞는 설정과 실제 사용자 제한값을 확인한다. 기능을 늘리기 전에 기존 회계 선택의 효과를 분리한다.
4. 설정만으로 해결되지 않는 병목이 남으면 그때 estimator·scheduler 변경의 근거와 새 수용 조건을 만든다.

자동 provider 추측, 관리자 API 탐색, 새 tokenizer·cache 의존성 추가를 이 조사만으로 진행하지 않는다. 현재 v0.1의 단순함을 유지하면서 어떤 계약에서 효율이 떨어지는지 먼저 구분한다.


## 2026-09-13 공식 계약 재확인

Accounting 실험 중 문서만 다시 조회했다. 실제 API 요청·인증·provider 측정은 하지 않았다.

- [Claude 공식 rate limits](https://platform.claude.com/docs/en/api/rate-limits): RPM·ITPM·OTPM을 구분하고, ITPM은 요청 중 실제 입력량에 맞춰 조정하며 OTPM은 실제 생성량으로 계산한다. `max_tokens`는 OTPM 계산에 들어가지 않는다. Token bucket과 짧은 구간의 제한도 설명한다.
- [Azure OpenAI 공식 quota 문서](https://learn.microsoft.com/en-us/azure/foundry-classic/openai/how-to/quota?view=foundry-classic): 수신 시 prompt·`max_tokens`·`best_of`로 추정한 비용을 admission 제한에 사용하며, 과금 토큰 수와 구분한다. RPM은 1초·10초 구간에서도 평가할 수 있다.

따라서 이번 합성 서버의 actual 정산 결과로 모든 provider의 정책을 정할 수 없다. 모델 이름이나 Messages/OpenAI 프로토콜만 보고 환급 가능성을 추정하지 않는 기존 결정을 유지한다. 현재 16 RPM·6,000 TPM combined rolling-window fixture는 두 정산 방식의 효과를 분리하는 실험이며 두 회사의 limiter 복제본이 아니다. 제품 소스·정산 기본값·진행 중 workload는 변경하지 않았다.
