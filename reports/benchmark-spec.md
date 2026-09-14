# 공개 재현 벤치마크 명세

상태: 설계 명세. 제품 Task 8의 무대기20run·quota20run 측정과 [원시 기록 검산](../evidence/product-task8-full-matrix-parent-audit.json), 독립 명세·[품질 재검토](../evidence/task8-quality-review/rereview/README.md)를 완료했다. [결과](benchmark-pilot-results.md)는 고정 시간 내 처리량 우위를 보여주지 못했다. 경쟁 제품 성능 비교와 별도 후속 가설 검증은 아직 남았다. 최종 수용 상태는 [구현 기록](../evidence/implementation-progress.json)을 따른다. 별도 [기존 구현 동작 검증](reference-validation.md)은 성능 결과가 아니다. 목표는 같은 모델·작업·quota·자원에서 gateway가 만든 이득과 손해를 모두 측정하는 것이다.

## 실험 층

| 층 | 목적 | 증명할 수 없는 것 |
|---|---|---|
| 결정적 scheduler trace | quota·공정성·기아·취소 경합의 반례 | 실제 네트워크/엔진 latency |
| native HTTP/SSE mock | wire 보존, 429/503, deadline, 느린 stream, end-to-end 요청 지표 | 답변 품질·실제 GPU 이득 |
| 실제 coding agent + mock | 도구가 정상 대기·실패·취소·tool 흐름을 처리하는지 | 실제 LLM의 task 해결 능력 |
| 비용 없는 실제 endpoint / 이미 실행 가능한 로컬 모델 | smoke와 공개 가능한 작은 실험 | 대규모/모든 사내 환경의 성능 |
| 실제 과제와 제한된 API | 정답이 검증된 task goodput | 다른 모델·다른 요금제에 대한 일반화 |
| 사내 구두 피드백 | 실제 사용 문제·개선 방향 | 독립 재현·공개 검증된 수치 |

무료 API도 명시적 호출·token cap을 두고 무료 한도 밖 자동 과금이 없음을 확인한 endpoint만 쓴다. 이 단계 전에는 API credential을 찾거나 호출하지 않는다. 사용자 모델 서버를 임의로 켜거나 모델을 다운로드하지 않는다.

## 비교 대상

같은 workload를 direct, FIFO+concurrency cap, session round robin, 추정 비용 deficit, 최소 설정의 LiteLLM/Bifrost에 보낸다. Overlaat는 자원 스케줄링 비교에 추가할 수 있다. 현재 clone SHA는 코드 분석용이고 실행 baseline은 stable release·설정·의존성까지 고정한다. default branch와 stable release를 혼동하지 않는다.

모든 arm에서 provider retry, client retry, gateway retry의 실제 횟수를 기록한다. cache는 첫 비교에서 off, 이후 동일 key/scope 조건으로 독립 실험한다. scheduler만 공통 transport에서 바꾸는 ablation과 완제품끼리의 비교를 별도로 수행한다.

[Pi 0.84.2 실행](client-integration-validation.md)에서 기본 429는 4회 요청, agent/provider 각 retry=1은 4회 요청을 만들었다. gateway가 retry를 소유하는 arm은 client 재시도를 임시 설정에서 명시적으로 끈다. 실제 기본 설정과 조합한 arm도 별도로 평가한다. “gateway retry 1회” 설정만으로 작업 전체의 attempt budget을 주장하지 않는다.

## workload와 반례

| 차원 | 고정 fixture |
|---|---|
| agent 수 | 1, 4, 8, 16. root 1개+child 15개 대 독립 root 4개도 구분 |
| 작업 길이 | 짧은/긴 입력, 짧은/긴 출력, 큰 max_tokens지만 실제 짧은 응답, heavy/light 혼합 |
| 도착 | 일정 간격, 동시 burst, 공개 arrival trace, agent turn 간 tool 실행 간격 |
| 제한 | RPM만, TPM만, 동시 제한, 짧은 시간창 rate limit, 외부 quota 소비자 |
| 추정 오차 | 출력량 과소/과대 추정, cached token, 빠진 usage, 늦은 usage, out-of-order headers |
| 실패 | 429 Retry-After 초·HTTP date·provider ms·없음, 영구 spend cap, 503, 연결 종료, stream 중간 오류 |
| 경합 | 큐 취소, admission 직후 취소, 첫 body 전 취소, 첫 token 뒤 취소, timeout과 완료 동시 발생 |
| 공정성 | 작은 요청이 계속 오는 동안 큰 요청 대기, ID 다수 생성, idle 후 재참여, 여러 alias가 같은 quota 공유 |
| 경계 | 계약상 수용 불가능한 비용, 큰 body·큰 SSE event, 느린 downstream, 재시작·절전 후 burst |
| 캐시 후속 | exact 반복, 대화가 한 turn씩 늘어남, tool 상태 변화, scope 분리, 독립 stochastic sample |

동시성 1에서 이미 실행 중인 긴 요청은 선점하지 못한다는 반례를 반드시 포함한다. 짧은 요청만 성공시키고 큰 요청을 timeout시킨 결과를 개선으로 통과시키지 않는다. 무한히 과부하가 지속될 때 queue가 무한히 커지지 않고 명확히 거절하는지도 평가한다.

예약 비교에서는 **heavy → light → heavy와 heavy → heavy → light를 모두 실행**한다. 미점유 예산과 큐의 예약 때문에 사용할 수 없는 예산을 구분한다. 작은 요청 보호를 유리한 한 가지 도착 순서로만 판정하지 않는다.

streaming 검증은 실제 loopback TCP로 첫 SSE 데이터가 upstream 완료 전에 도착하는지 확인한다. ASGITransport의 전체 응답 버퍼링이나 iterator `aclose()`만으로 대체하지 않는다. 정상 EOF·실제 socket close·body iterator 종료·task cancellation을 구별한다. no-abort 정책은 client close 뒤에도 별도 upstream 완료 gate까지 점유를 유지하는지 확인하고, 실제 엔진 계산 종료는 HTTP 연결 상태와 별도로 관측한다.

client가 content를 받은 뒤 `finish_reason` 누락 등으로 새 요청을 보내는 경우도 포함한다. gateway 내부 replay와 client의 재요청을 구분한다. `Retry-After: 5`가 있는 동안 새 client retry가 도착하는 fixture를 둔다. 세션별 공정성에 사용할 헤더가 기본 전달되는지 확인하고, `x-client-request-id`처럼 sessionId와 같은 값을 담을 수 있는 헤더를 요청 유일 ID로 취급하지 않는다.

## 측정값

primary는 **정해진 시간과 비용 안에 정답/완료 조건을 만족한 task 수**다. mock 단계에서는 이를 실제 task 성능이라 부르지 않고 fixture 요청/워크플로 완료 수로 표기한다.

- 모든 시도: submitted, admitted, upstream attempt, completed, rejected, error, timeout, cancelled. 중복 retry는 새 사용자 성공으로 세지 않는다.
- latency: queue wait, upstream TTFT, total TTFT, completion time, gateway forwarding overhead. 첫 HTTP byte와 첫 유효 token의 시각을 나눈다.
- 분포: 짧은/긴 요청 각각 p50/p95/p99, deadline 달성률, 긴 요청 최대 대기. 성공 표본만의 p95와 실패 비율을 반드시 함께 표시한다.
- 비용: 유효 작업 token, 실패/취소 후 남은 upstream 실행량, retry amplification, rate-limit utilization, 알 수 없는 usage 비율.
- 로컬 자원: idle/active CPU, peak RSS, queue/body/stream buffer 양, 연결 수, 장기 실행 시 메모리 증가.
- 공정성: root별 할당 지분과 실제 서비스량. token fairness와 작업 완료 fairness는 같은 지표가 아님을 표시한다.

Core Task 6 source에서 root head별 검사 비용이 rolling 장부의 보관 항목 수에 영향을 받을 수 있음을 확인했다. overhead 표본에는 등록 root 수·대기 수·해당 시점의 장부 항목 수를 함께 남긴다. 빈 장부의 결과만으로 오래 실행한 상태의 비용을 대표하지 않는다. 이는 측정할 가설이며, 아직 병목이나 최적화 필요성이 입증된 것은 아니다.

외부 API에서는 진짜 upstream queue 시간·GPU 시간이 보이지 않을 수 있다. 관측되지 않은 부분을 gateway overhead로 임의 배분하지 않는다. mock에는 별도 server timestamp를 두되 모두 같은 monotonic time 기준으로 비교 가능한 범위를 명시한다.

## 실험 방법

1. fixture·seed·binary SHA·config hash·OS·CPU/RAM·엔진 설정·시각을 결과와 함께 보관한다. prompt나 실제 credential은 결과에 넣지 않는다.
2. deterministic suite로 상태·회계 불변식을 확인한다. retry·cancel·queue 이벤트 순서를 바꾼 trace도 포함한다.
3. warm-up과 cold start를 분리한다. HTTP pool/cache 상태를 고정하고 arm 실행 순서를 바꿔 시간대 편향을 줄인다.
4. 주요 비교는 우선 5개 이상의 고정 seed로 실행하고, 변동이 결론을 바꿀 정도면 반복 수를 늘린다. 샘플이 부족한 p99를 확정적 숫자로 홍보하지 않는다.
5. quota 영향을 보려면 여러 reset window를 포함하고, queue에 남은 요청은 drain하거나 시간 초과로 결과에 포함한다. 테스트 종료 시 pending을 삭제하지 않는다.
6. 실제 API는 서로 다른 시각/날짜에 반복하고 모델·sampling·task·tool 자원을 동일하게 유지한다. 표본 간 의존성을 고려해 불확실성을 표시한다.

## native 수용 조건

Windows 표준 사용자 계정에서 Docker/WSL 없이 실행, 한글/공백 경로, corporate CA/proxy, Ctrl-C, client disconnect, 절전 복귀를 확인한다. macOS·Linux에서도 같은 HTTP fixture를 실행한다. Mac M4 32 GB는 현재 개발 장비이며 낮은 cap의 모의 실험은 실제 저사양 PC 실행을 대신하지 않는다.

Python/Node 사용은 허용된다. Redis를 쓰는 비교 arm은 설치·시작·RSS·round-trip 비용을 포함한다. Redis가 필요한 arm을 Windows 무의존 결과와 같은 구성으로 표시하지 않는다.

## 채택 기준 제안

| gate | 기준 |
|---|---|
| 정확성 | fixture별 payload/stream 의미 보존, permit 중복 반환 없음, queued cancel의 upstream 송신 없음, 영구 오류 반복 없음 |
| 효율 | 같은 task/시간/quota에서 goodput 개선 또는 짧은 요청 p95 개선. 다른 핵심 지표의 손해도 동시에 표시 |
| 공정성 | 긴 요청의 완료율·최대 대기를 악화시켜 얻은 이득인지 확인. 지속 과부하는 별도 결과 |
| overhead | 잠정 목표: 무대기 loopback proxy 추가 p95 < 2 ms, idle RSS < 50 MiB. 실제 baseline 측정 후 예산의 근거를 검토 |
| 실용성 | 고정한 Claude Code·Codex·Pi 버전으로 각 API 경로의 사용자 흐름 확인. 모든 도구 호환을 단일 HTTP 테스트로 대체하지 않음 |

수치 목표는 논문에서 가져온 결과가 아니라 제품 예산 초안이다. 우위의 최소 크기와 허용되는 trade-off는 pilot의 변동성과 사용자 체감으로 확정한다. 유의미한 이득이 없으면 더 복잡한 scheduler를 넣기 전에 simple baseline을 채택할 수 있다.

## 반복 루프에 남길 레코드

각 run은 hypothesis, baseline/candidate SHA, fixture/config hash, 자원, seed, 시작·종료, 모든 outcome count, latency 분포, unknown 항목, 채택/기각 이유를 남긴다. 체크 통과 여부와 성능 개선 여부는 독립 필드다. 최초 run 이전에는 결과 파일을 미리 채워 두지 않는다.

## 공개 artifact에서 얻은 측정 반례

2026-09-12 [source 감사](paper-artifact-source-deep-dive.md) 및 [원본 timing 함수의 loopback probe](../evidence/reference-tests/laps/metric-boundary.json)를 반영한다. LAPS의 선택 스크립트는 응답 JSON 완료 시간을 TTFT로, 성공 입력 문자 수//4를 token 수로 사용한다. 이는 해당 prefill 실험의 대용 지표이며 우리의 긴 SSE/agent benchmark로 그대로 옮기지 않는다.

- 각 timing 필드는 코드의 관측 지점을 기록한다. first body write/read, 첫 model token, terminal marker, EOF를 섞지 않는다.
- token 단위는 fixture actual, provider reported, byte/character estimate로 분리한다. 추정량으로 계산한 효율을 정확한 공급자 token 활용률로 표시하지 않는다.
- script exit0·PASSED 출력과 수용 조건을 구분한다. fixture 결과 JSON의 모든 outcome과 실제 assertion을 통과 기준으로 삼는다.
- sleep/gate로 일부러 만든 지연은 기능 검증이며, 순수 proxy overhead 표본으로 사용하지 않는다.
