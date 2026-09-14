# 실제 사용량 정산의 후속 비교 계획

2026-09-12. **새 실험은 아직 실행하지 않았다.** 현재 Native lifecycle 구현과
별개로, 수용한 첫 pilot의 음성 결과에서 다음 검증 질문을 좁힌 문서다. 성능 개선
구현이나 기본값 변경 승인이 아니다. 기존 `accounting=actual`의 영향만 먼저
분리하고, scheduler 변경은 이 비교에 섞지 않는다.

## 현재 기록에서 확인한 것

[읽기 전용 검산 스크립트](../scripts/audit-accounting-opportunity.py)는 원본 pilot과
20개 quota run의 SHA를 확인한 뒤, 매 window에 하나씩 넣은 과대 추정 요청만
다시 집계했다. 추가 HTTP 요청과 제품 실행은 없었다.
[기계 판독 결과](../evidence/product-task8-accounting-opportunity.json)에 개별 요청의
실제 수신·종료 시각과 300초 경계를 보존했다.

| 실행 구성 | 해당 요청 제출 | 300초 내 완료 | 이후 drain 포함 완료 |
|---|---:|---:|---:|
| 직접 연결 | 25 | 19 | 19 |
| 제품 RR | 25 | 15 | 25 |
| 비교용 FIFO | 25 | 14 | 24 |
| 비교용 RR | 25 | 14 | 24 |

이 요청의 예약 추정 비용은 3,222, mock의 합성 비용은 806이다. 차이 2,416은
**한 요청의 비용 차이**다. 이를 동시에 쓸 수 있는 quota 잔액이나 추가 완료
가능 건수로 합산하지 않는다. 기존 실험은 실제 tokenizer가 아닌 합성 비용을
사용하고, 공급자에 실제 토큰을 보낸 실험이 아니다.

제품 RR의 첫 해당 요청은 각 seed에서 약 12초에 제출됐지만 mock에 약 72초에
도달했다. 유효 usage가 도착한 뒤의 정산은 아직 시작하지 못한 요청의 실제 비용을
알려주지 않는다. 따라서 `actual`만으로 이 첫 대기를 제거할 수 있다고 예상해서는
안 된다. 이 판단은 기록과 현재 정산 계약에 근거한 추론이며, 다른 정책 실행의
관측 결과는 아니다.

## 비교할 질문과 고정할 조건

질문은 하나다. **동일한 RR에서 완료 후 유효 usage로 정산하면, 예약량을 계속
유지할 때보다 같은 300초 동안 완료하는 요청이 늘어나는가?** 첫 관측 뒤에도
RPM, 실행 동시성, 기아 방지 장벽, 유효 usage가 없는 취소가 계속 병목일 수 있다.
효과가 없거나 실패가 늘어나는 결과도 그대로 남긴다.

현재 `product/scripts/benchmark.py`의 `start_gateway`는 TOML과 반환 provenance
양쪽에 `accounting = reserved`를 고정한다. CLI에는 accounting/arm 선택이 없고
기존 matrix는 네 arm을 전제로 한다. 따라서 기존 command에 설정만 넘겨 이
비교를 실행할 수 있다고 안내하지 않는다. 이 확인은 Native Task 1 검증 중의
읽기 전용 점검이다. 실행 전에 두 accounting 구성을 명시적으로 식별하는 작은
하네스 변경과 검토가 필요하다. 파일 문자열 치환이나 monkey patch로 설정만
바꾸고 manifest에는 reserved를 남기는 방식은 사용하지 않는다.

- 비교 대상은 같은 source/build의 제품 RR `reserved`와 `actual` 두 구성이다.
  config의 회계 설정만 바꾸고 artifact ID와 provenance에 이를 포함한다. 예전
  `reserved` 결과를 새 source의 동시 대조군인 것처럼 재사용하지 않는다.
- 최초 비교는 기존 seed·도착 일정·5개의 실제 60초 window·RPM 16·TPM 6,000·
  concurrency 2·4 root·출력 상한·timeout·startup hold를 그대로 사용한다.
  모든 요청 유형이 같은 장부를 공유하며 window마다 초기화하지 않는다.
- 출력 원문·요청 본문·usage의 유효성 검사는 그대로 둔다. mock은 두 구성 모두
  동일한 실제 합성 비용으로 차감한다. 캐시, 재시도, 출력 예측기, barrier 조정,
  자의적인 출력 상한 축소를 함께 켜지 않는다.
- arm 실행 순서를 seed마다 교차하고, 두 구성을 모두 같은 실행 세대에서
  측정한다. 실행 전 소스·바이너리·fixture·설정·일정을 고정한다. 최소 한 seed의
  correctness pilot과 가상 시계 정산 회귀를 먼저 확인한 뒤 장기 비교를 시작한다.

## 판정과 실패 경계

주 지표는 300초 내 완료 건수와 모든 제출의 terminal 결과다. seed별 차이를 먼저
보고, 장시간 drain 이후 완료를 더해 같은 시간의 이득으로 표현하지 않는다.
긴/짧은 요청과 root별 완료·timeout, mock 429, 취소와 추가 시도를 함께 제시한다.
전체 완료가 늘어도 긴 요청이 계속 만료되면 공정성 개선으로 부르지 않는다.

실행 전에는 유효 usage 정산, usage 부재·잘린 응답·종료 뒤 잘못된 usage의
예약 유지, 실제 비용이 추정치를 초과한 경우의 debt를 기존 계약대로 확인한다.
현재 과대 추정 fixture 하나의 결과로 과소 추정·다른 PC의 공유 소비·분리된
입출력 TPM에 대한 안전성을 주장하지 않는다. 정확한 공급자 계약을 모르는
환경에서 기본값을 자동으로 `actual`로 바꾸지 않는다.

빈 장부 forwarding overhead, 사용 중인 장부의 추가 비용, 저사양 PC·실제 coding
task·경쟁 제품 비교는 각각 별도 미완료 항목이다. 이 좁은 비교가 성공하더라도
제품 전체의 성능 우위나 실제 agent 작업 성공률로 확대 해석하지 않는다.

배경: [공급자 quota 계약의 차이](quota-contract-followup.md),
[scheduler 후속 가설](scheduler-utilization-followup.md),
[첫 pilot 결과](benchmark-pilot-results.md).
