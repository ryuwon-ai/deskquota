# 성능 하네스 참고 구현의 측정 경계

2026-09-12. **소스·공식 문서 분석**이며 외부 benchmark 실행 결과가 아니다. Bifrost 공식 문서가 연결한 [bifrost-benchmarking](https://github.com/maximhq/bifrost-benchmarking/tree/2c416fb234bf4abac25cf61f4470fc2528362afd)을 27번째 참조로 clone했다. SHA `2c416fb234bf4abac25cf61f4470fc2528362afd`, branch `main`을 고정했고 dependency 설치·build·mock 실행은 하지 않았다. 기존 26개 checkout을 변경하지 않았다.

## 소스에서 확인한 내용과 적용

| 관찰 | 고정 소스 근거 | 우리 실험에 적용 |
|---|---|---|
| mock의 `-tpm`은 토큰 예산이 아니라 시작 후 429를 켜는 초 단위 시각이다. auth key와 시작·종료 시각을 검사한다. | [옵션](https://github.com/maximhq/bifrost-benchmarking/blob/2c416fb234bf4abac25cf61f4470fc2528362afd/mocker/main.go#L399-L401), [판정](https://github.com/maximhq/bifrost-benchmarking/blob/2c416fb234bf4abac25cf61f4470fc2528362afd/mocker/main.go#L1083-L1112) | 시간대별 장애 fixture와 실제 rolling RPM/TPM fixture를 구분한다. 실제 수신 요청과 비용 장부가 있어야 예산 활용률을 계산한다. |
| users mode는 `client.Do()`가 반환한 직후 latency를 기록한다. 응답 body를 읽어 terminal·EOF를 확인하지 않고 2xx를 성공으로 센다. | [요청 측정](https://github.com/maximhq/bifrost-benchmarking/blob/2c416fb234bf4abac25cf61f4470fc2528362afd/pkg/concurrent/concurrent.go#L263-L285) | header·body·유효 output delta·terminal·EOF를 분리한다. synthetic 내용과 종료 조건을 충족해야 완료로 센다. |
| `Run`은 worker의 WaitGroup을 기다린다. worker가 만드는 request goroutine은 해당 WaitGroup에 추가하지 않고 요청에 context도 전달하지 않는다. | [Run](https://github.com/maximhq/bifrost-benchmarking/blob/2c416fb234bf4abac25cf61f4470fc2528362afd/pkg/concurrent/concurrent.go#L80-L109), [worker와 요청 생성](https://github.com/maximhq/bifrost-benchmarking/blob/2c416fb234bf4abac25cf61f4470fc2528362afd/pkg/concurrent/concurrent.go#L205-L243) | ingress와 upstream attempt를 모두 추적하고 측정 종료 후 drain 또는 명시적 timeout으로 닫는다. pending을 집계에서 삭제하지 않는다. |
| users mode의 변환은 평균·최소·최대를 채우고 P50/P99는 채우지 않는다. export는 해당 percentile 필드를 그대로 읽는다. throughput도 성공 수가 아닌 총 요청 rate로 대입한다. | [변환](https://github.com/maximhq/bifrost-benchmarking/blob/2c416fb234bf4abac25cf61f4470fc2528362afd/benchmark.go#L394-L439), [export](https://github.com/maximhq/bifrost-benchmarking/blob/2c416fb234bf4abac25cf61f4470fc2528362afd/benchmark.go#L771-L782) | 비어 있는 percentile을 0ms로 보고하지 않는다. ingress throughput·성공 완료량·upstream 시도량을 따로 계산한다. |
| RSS는 500ms 간격으로 표본을 수집하고 그 최대를 export한다. | [sampling](https://github.com/maximhq/bifrost-benchmarking/blob/2c416fb234bf4abac25cf61f4470fc2528362afd/benchmark.go#L565-L579), [최대 표본](https://github.com/maximhq/bifrost-benchmarking/blob/2c416fb234bf4abac25cf61f4470fc2528362afd/benchmark.go#L756-L768) | sampled RSS 최고값과 OS high-water mark를 구분하고 sampling 간격·실패 수를 기록한다. |

request goroutine이 `Run` 뒤까지 남을 수 있다는 판단과 users mode percentile이 기본값으로 export될 수 있다는 판단은 위 제어 흐름에서의 **추론**이다. 실제 남은 요청 수·경합·잘못된 export를 이 저장소에서 재현한 것은 아니다. rate mode의 Vegeta 경로까지 같은 특성이 있다고 일반화하지 않는다. 이 관찰을 근거로 Bifrost 제품 자체의 성능이 나쁘다고 주장하지 않는다.

## 공개 성능 숫자와 비교할 때

- [Bifrost t3.xlarge 문서](https://docs.getbifrost.ai/benchmarking/t3.xl)의 11µs는 JSON marshalling과 HTTP 호출을 제외한 overhead다. 평균 내부 비용을 우리 client 관측 p95 추가 지연과 비교하지 않는다. 문서의 부하 중 메모리를 우리 idle RSS 목표와 비교하지도 않는다.
- [LiteLLM benchmark 문서](https://docs.litellm.ai/docs/benchmarks)의 8ms p95 overhead와 POST latency는 관측 지점이 다르다. POST와 TTFT custom metric을 더하면 논리 요청 수를 중복 집계할 수 있다. `network_mock`은 실제 upstream socket 왕복과 구분한다. 이 문서의 대규모 배포 수치를 개인 PC baseline으로 사용하지 않는다.
- [Bifrost 직접 실행 안내](https://docs.getbifrost.ai/benchmarking/run-your-own-benchmarks)는 rate와 users 방식의 부하 모양을 구분한다. 기본 provider 집합에는 실제 OpenAI도 포함되므로 이 저장소의 기본 명령을 실행하지 않았다. 필요한 패턴만 검토하고 우리 synthetic loopback 하네스를 사용한다.

공개 수치가 빠르다는 사실보다 **어디서부터 어디까지 잰 값인지**가 공통 비교의 선행 조건이다. stable release 후보의 tag와 패키지 metadata는 [조회 기록](../evidence/benchmark-release-candidates-2026-09-12.json)에 별도로 고정한다. 실제 native 설치·같은 workload 비교는 아직 남았다.

## Task 8의 검증 조건

1. default RR와 feature-enabled RR의 결정적 trace가 같아야 하며 FIFO는 같은 transport·quota·한도를 사용한다.
2. 누락 terminal, 중복 attempt, 알 수 없는 ingress ID를 넣은 self-check는 실패해야 한다.
3. 성공·거절·오류·timeout·취소가 전체 ingress를 배타적으로 덮어야 한다. 짧은 요청 이득과 긴 요청 손해를 함께 보고한다.
4. 5-window composite는 네 workload 구간을 포함한 한 실행이다. 구간 사이 큐·장부·cooldown을 리셋하지 않는다. 각 구간을 독립적인 5-window 반복으로 표시하지 않는다.
5. 소스·binary·fixture·config·host와 부분 결과를 보존한다. 중단된 실행은 완료로 세지 않고 전체 행렬의 남은 run을 표시한다.

세부 측정 경계와 실행 시간은 [첫 실험 준비](benchmark-pilot-preparation.md), 수용 기준은 [benchmark 명세](benchmark-spec.md)를 따른다. 이 분석은 의존성·제품 기능을 추가하는 근거가 아니다.
