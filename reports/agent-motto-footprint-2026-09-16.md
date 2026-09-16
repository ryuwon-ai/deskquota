# 모토·경량성·클라이언트 의미 보존 검토

2026-09-16. 읽기 전용 감사와 에이전트 간 토론 결과다. 제품 변경·빌드·HTTP 실험·실제 API 호출은 이 감사에서 하지 않았다. 기존 결과 수치는 아래 보고서의 저장된 측정이며 이번 실행 성과가 아니다.

**판정 기준:** `Get more done within your LLM limits.`에 맞는 개선은 같은 작업의 중복 전송·불필요한 대기를 줄이고, 기존 도구의 요청·응답·취소 의미를 유지하며, 설치와 상시 실행 부담을 작게 유지해야 한다. 성공 집합을 줄여서 낮춘 p95나 새 모델로 바꾼 답변은 이 기준의 성과가 아니다.

## 우선순위와 토론 결과

| 순위 | 후보 | 판정 | 이득과 경계 |
|---|---|---|---|
| 1 | 캐시 완료 알림으로 quota 대기 중 같은 요청 재조회 | 이번 구현 지지 | 이미 완성된 답을 기다리던 요청에 전달해 upstream 호출과 RPM/TPM 대기를 없앨 수 있다. 기존 캐시 적격성·범위·TTL과 원본 응답을 유지한다. 이미 전송한 요청은 합치지 않는다. |
| 2 | 결과를 쓰지 않는 429 오류 본문 선행 읽기 제거 | 이번 재현된 범위에 한해 함께 적용 결정 | quota 감사 에이전트가 실제 EOF gate로 지연을 재현했다. 오류 헤더와 첫 본문 전달을 EOF까지 미룰 필요가 없는 조건만 제거한다. `retry=false` 전체 생략은 잘못이다. 최종 변경·검증 결과는 통합 보고서에서 확인한다. |
| 3 | 503의 명시적 Retry-After로 공유 대기 | 정책·fixture 검증 후 | 장애 중 불필요한 동시 재호출을 줄이는 후보. 자동 재전송은 추가하지 않는다. provider 전체 cooldown이 적합한 장애 범위를 먼저 확인한다. |
| 4 | BPE 배포 크기 개선 | 공개 패키징 전 비교 후 결정 | 기본 bytes 사용자까지 지는 약49.68MB 추가 실행파일 부담을 줄일 수 있다. build variant·setup 지원 조합이 늘어나는 비용이 있다. |
| 5 | BPE 상한 도달 시 계산 중단 | 현재 API 즉시 채택 반대 | 설치된 라이브러리에 API는 있지만 내부 조기 종료의 최악 경계가 미확정이다. 허용하던 입력을 잘못 거절하지 않는다는 근거가 먼저 필요하다. |
| 후속 | 독립 quota group·provider별 정산 | 실사용 계약부터 | 여러 모델·키가 같은 allowance를 쓸 수 있다. 자동 독립 판정은 과다 전송 위험이 있다. 원본 usage는 절대 바꾸지 않는다. |
| 보류 | 전체 singleflight·실시간 SSE fanout·새 scheduler·대시보드 | 지금 추가하지 않음 | 현재 cache notification으로 풀리는 문제에 leader 선출, follower 생명주기, 느린 reader 버퍼, 추가 UI를 얹을 근거가 없다. |

`cache_efficiency_audit`와 토론한 결과, 단순히 admission을 받은 뒤만 재조회하는 안보다 **cache commit 알림 + 하나의 유지되는 acquire future**를 지지했다. RPM=1을 이미 썼다면 admission 뒤 재조회는 최대60초 뒤에야 실행되어 캐시가 준비돼도 즉시 쓰지 못한다. 알림마다 acquire를 취소하고 다시 요청하면 FIFO와 aging을 바꾸므로 유지해야 한다.

`quota_efficiency_audit`와 토론에서는 `retry=false`면 모든 오류 probe를 없애자는 단순화에 반대했다. 실제 코드에서 타이밍 헤더가 없고 본문이 일시적429로 판별될 때는 재시도 비활성 상태에서도 fallback cooldown을 설치한다. 해당 에이전트도 이를 확인했으며, 명백한 비JSON 또는 타이밍 헤더가 있고 재시도가 꺼진429처럼 결과가 불필요한 조건으로 범위를 좁혔다. 이후 해당 에이전트의 실제 EOF gate 재현에서 불필요한 전달 지연을 확인해 이번 작업에 함께 적용하기로 결정했다. 이 감사 자체가 HTTP 실험을 실행한 것은 아니며, 적용 후 결과와 회귀 검증은 통합 보고서가 맡는다.

## 기존 제품에서 가져올 것

| 읽은 고정 소스 | 직접 확인 | 적용 범위 |
|---|---|---|
| Pi `f3c672245d25ef2283ffc0d9cdec8a5482651103` | coding-agent README의 작은 core·확장 방식, compaction 계산 | 사용자 도구와 workflow를 gateway가 다시 설계하지 않는 방향. Pi와 gateway는 역할이 다르므로 바이너리·RSS 성능 비교 근거로 쓰지 않는다. |
| Bifrost transport v2.1.1 `c193745d2a713e9f58f021d43e138df5eb7e038a` | `framework/lrucache/lrucache.go:166–215`의 same-key fill coordination, follower 취소와 오류 시 알림 | 완료 후 알림·재조회·정리 패턴. 범용 resource cache 구현이며 LLM 응답 전체가 이미 coalesce된다는 근거는 아니다. |
| LiteLLM v1.100.1 설치본 | `litellm/proxy/common_utils/cache_coordinator.py:96–157`의 event 대기, 역할 획득 후 double-check, finally signal | 주기적 polling 없이 알림 받고 기존 cache를 다시 읽는 패턴. DB/config 공유 읽기용이며 LLM 응답 병합의 성능 증거는 아니다. |

Pi tracked tree가 깨끗하고 위 SHA인 것을 직접 확인했다. Bifrost/LiteLLM 실행 버전의 출처·소스 해시는 [직전 심층 분석](quota-policy-deep-dive-2026-09-16.md) 및 해당 evidence를 따른다. 비교용 원본 clone은 수정하지 않았다.

## BPE의 실제 비용과 안전한 판단

[기존 입력 추정 실험](input-estimation-results-2026-09-16.md)의 기록:

| 항목 | 이전 bytes | 새 bytes | 새 cl100k | 새 o200k |
|---|---:|---:|---:|---:|
| 실행파일 | 10,188,304bytes | 59,869,408bytes | 동일 | 동일 |
| idle RSS | 9.66–9.77MiB | 9.83–9.91MiB | 41.72–41.75MiB | 76.72–76.80MiB |
| 128KiB no-wait p95 | 0.370ms | 0.374ms | 1.064ms | 1.051ms |

선택형 BPE가 작은 TPM fixture의 완료 수를 늘린 것은 이득이다. 하지만 runtime 선택형과 배포 선택형은 다르다. 현재 `product/Cargo.toml:19`의 필수 의존성과 `product/src/input_estimate.rs`의 양쪽 tokenizer 참조 때문에 bytes 모드 배포도 사전 크기를 부담한다. selected-only 초기화는 이미 구현되어 있고, unknown/unlimited TPM에서 사전을 준비하지 않는 것도 기존 측정에서 확인했다. 이 부분을 재구현할 필요는 없다.

설치된 `bpe-openai=0.3.1` manifest에는 사전별 feature가 없다. `src/lib.rs`는 사전을 `include_bytes!`하고 LazyLock으로 역직렬화하며, `build.rs`는 사전 파일을 생성한다. 실행 때 다운로드가 없는 장점도 함께 유지해야 한다. Cargo optional feature를 붙이는 방향은 가능하지만, 이 감사에서는 빌드하지 않았으므로 실제 절약량을 주장하지 않는다. 지원하지 않는 BPE 설정의 조용한 bytes fallback은 허용하지 않고 setup과 오류 메시지까지 일관돼야 한다. 라이브러리 fork·다운로드 관리자·추가 daemon부터 만드는 것은 권하지 않는다.

처음에는 이미 설치된 `Tokenizer::count_till_limit`을 이용하면 초과 입력 계산을 일찍 끝낼 수 있다고 봤다. 그러나 하위 `bpe=0.2.2/src/byte_pair_encoding.rs:428–444`를 읽으니, 현재 token 수가 `limit+10`을 넘으면 `None`으로 종료하며 최악 경우 필요한 buffer 크기를 아직 결정하지 않았다는 TODO가 있다. 따라서 전체 `count`와 **모든 입력에서 동일한 허용/거절 결과를 보장한다고 결론내릴 수 없다**. 실제 false reject를 재현한 것은 아니지만, API가 있다는 이유만으로 요청 수용 계약에 연결할 근거도 부족하다. 이 후보는 이번 적용에서 제외하는 것이 맞다.

후속 조사 시에도 전체 JSON·모델·출력 상한 검증은 유지해야 한다. 거절 입력 최적화를 정상 입력 지연 개선이나 RPM 대기 해결로 표현하지 않는다.

## 이미 적절한 경량 구현과 건드리지 않을 경계

- `product/src/transport/upstream.rs:57–83`: reqwest client를 재사용하며 idle connection pool이 있고, 자동 retry·redirect를 명시적으로 끈다. 새 transport pool이나 retry layer가 필요하지 않다.
- `product/src/transport/body_budget.rs:61–106`: `BudgetedBody`의 clone은 `Bytes` 소유권을 공유한다. `forward()`에서 `body.clone()`을 본문 전체 복사라고 오인하면 안 된다. 마지막 소유자까지 budget permit을 붙잡는 검사도 이미 있다.
- `product/src/protocol/request.rs:1–109`: selective serde visitor로 원본 JSON을 읽고 그대로 전달한다. 큰 tool schema·argument를 모두 `serde_json::Value`로 만드는 파서로 바꾸지 않는다.
- `product/src/cache.rs:17–20,197–209`: 현재 payload4MiB, 요청32KiB, entry256KiB, 최대128entry로 한정되어 있다. 새 notification은 현재 등록된 대기자에게 broadcast한다. 대기열 상한64건 외에도 admission을 먼저 받은 최대16개의 future가 캐시 응답과 경합할 수 있으므로, 알림 대상을 단순히 최대64명이라고 표현하면 부정확하다. 관련 없는 fill의 재조회 비용은 있지만, per-key waiter registry보다 현재 상한에서 단순하다. 새 부하 측정 없이 비용0이라고 하지는 않는다.
- `product/src/transport/stream.rs:485–546`: 응답 전달의 `source.to_vec()`는 계량하는 byte만 소유하도록 잘라 복사한다. 이를 무조건 `Bytes::slice()`로 바꾸면 작은 slice가 큰 backing allocation을 붙잡을 수 있다. 'zero-copy'라는 이름만으로 현재 buffer 소유권·상한 계약을 약화시키지 않는다.
- `product/src/cache.rs`의 32KiB 이하 적격성 JSON 파싱과 `request.rs` 검증을 무리하게 합치는 것은 보류한다. 서로 다른 정책이며 parser 통합으로 새 결합을 만드는 것보다 현재 계측상 큰 낭비를 먼저 없애는 편이 낫다.

## 자동 요약·취소 호환성의 합격 조건

Pi의 [`compaction.ts:146–147`](https://github.com/earendil-works/pi/blob/f3c672245d25ef2283ffc0d9cdec8a5482651103/packages/coding-agent/src/core/compaction/compaction.ts#L146-L147)은 native `usage.totalTokens` 또는 input/output/cacheRead/cacheWrite 합계로 context 사용량을 계산한다. 같은 파일202–240행은 마지막 유효 usage와 이후 메시지를 이용해 compaction 여부를 결정한다. **quota에서 prompt-cache 할인해 주는 것과 context가 작아지는 것은 다르다.** 제공자별 장부 정산을 추가해도 원본 usage에 할인 수치를 써 넣으면 안 된다.

캐시 hit도 원본 응답 body와 usage를 보존하되 로컬 upstream usage/attempt에는 다시 더하지 않아야 한다. 순수 응답 replay를 위해 stream event를 합성하거나 텍스트만 추출하지 않는다. tools·stateful·opaque compaction 요청을 새로 cache 대상으로 넓히지 않는다.

캐시 대기 개선은 다음을 확인해야 한다.

1. RPM이 소진된 동안 cache가 채워지면 같은 key 대기 요청이 quota expiry 전에 끝난다. 실제 upstream 횟수는 첫 요청만 세고 replay body가 같다.
2. 관련 없는 key 저장 알림이 기존 요청의 FIFO·deadline·aging을 재설정하지 않는다.
3. 여러 알림에서 하나의 요청을 hit로 중복 집계하지 않는다. pending miss→late hit 재분류가 정확히 한 번 일어난다.
4. stop·disconnect·deadline이 먼저면 replay나 새 upstream을 시작하지 않고 ticket 또는 미시작 hold를 기존 Drop으로 회수한다.
5. credentials/root/query/body 차이, no-store/Vary, incomplete stream, TTL 만료는 여전히 적중하지 않는다.
6. 동시성2라서 이미 두 요청이 발송됐다면 두 호출은 남는다. 이를 singleflight나 모든 중복 제거라고 주장하지 않는다.

기존 [자동 요약 감사](auto-compaction-audit-2026-09-16.md)와 [입력 추정의 native client 검사](input-estimation-results-2026-09-16.md)는 과거 특정 버전·fixture 범위다. 이번 캐시 변경 이후의 회귀 검사는 부모 작업에서 별도로 수행하고, 이 읽기 전용 감사의 완료와 섞지 않는다.

## 결론

지금 경쟁력은 큰 플랫폼 기능을 따라 붙이는 데서 나오지 않는다. **이미 끝난 동일 요청을 다시 보내지 않도록 하고, 전달만 하면 되는 응답을 불필요하게 붙잡지 않으며, 필요한 quota 의미만 정확하게 처리하는 것**이 현재 모토에 가장 가깝다. 이번 적용은 cache completion을 기다리는 기존 대기열을 깨우고, 재현된 조건의 불필요한429 선행 읽기를 제거하는 변경으로 좁혔다. BPE·provider 정책·배포 variants는 각각 실제 근거가 생긴 뒤 한 단계씩 추가하는 편이 타당하다.

## 캐시 구현 독립 품질 리뷰

**APPROVED.** `evidence/queued-cache-2026-09-16/implementation/implementation-summary.json`의 `cache.rs`, `server.rs`, `cache_contract.rs` 세 파일 SHA가 현재 파일과 일치하는 것을 직접 검사했다. 해당 변경과 기존 Ticket·Queue.cancel·Hold Drop 경로를 읽었으며 수정이 필요한 결함을 발견하지 못했다. 429 변경은 이 품질 리뷰 범위에 포함하지 않았다.

- 알림 등록 후 재조회하는 순서와 cache mutex 해제 후 notify가 누락된 깨움과 lock 순서 문제를 피한다.
- 하나의 select에서 admission future를 유지하므로 관련 없는 cache fill이 FIFO·aging을 재설정하지 않는다. cache가 먼저 준비되면 기존 Drop 경로가 queued ticket 또는 미시작 예약을 회수한다.
- 준비된 stop·disconnect·deadline 분기를 cache·admission보다 먼저 검사한다. admission 직후 최종 cache hit도 worker를 만들지 않고 hold를 회수한다.
- `Pending`의 비복제 outcome 상태로 원래 miss를 hit로 한 번만 재분류한다. cache key·적격성·TTL·응답 완료·원문과 payload 소유권은 변경하지 않았다.
- 기록된 RED2건, 최종 contract23건·unit5건 통과 로그를 읽어 확인했다. 이 리뷰에서 테스트·빌드를 다시 실행하지 않았으며 통합 회귀·성능·플랫폼 검증의 완료를 뜻하지 않는다.

## 429 선행 읽기 변경 독립 품질 리뷰

**APPROVED.** `rejection-implementation/final.json`의 runtime 두 파일과 최종 `no_proxy()` fixture 수정을 포함한 `retry_contract.rs` SHA가 현재 파일과 일치한다. `retry::transient`, representation helper, `probe_rejection`과 모든 production 호출의 의사결정을 추적했으며 수정이 필요한 결함을 발견하지 못했다.

- shared helper는 기존 classifier의 헤더 조건을 동치로 추출한다. probe의 더 엄격한 lowercase `identity` 조건·선언된 크기·16KiB 한도를 유지하므로 replay 적격성을 확대하지 않는다.
- `needs_body=false`인 조건에서는 본문 판별 결과가 retry 또는 fallback cooldown 결정에 쓰이지 않는다. 명시 timing의 `header_delay`는 먼저 적용하고, timing이 없는 recognized JSON의 fallback은 retry 비활성 상태에서도 보존한다.
- probe를 생략한 빈 prefix는 원본 response stream으로 이어진다. 429의 usage 미관측·cache 미저장과 기존 취소·deadline·hold 수명을 유지한다.
- 후속 본문이 잘리면 pre-head502 대신 원본429와 body error를 전달하는 변경이 명세와 검사에 명시돼 있다. 이는 의도된 전달 시점 변화이며 오류 은폐가 아니다.
- 기존 로그에서 RED1건 통과·2건 실패, 변경 후 retry22건·admission5건·stream3건 통과, 최종 fixture 격리 변경 후 targeted3건 재통과를 확인했다. 직접 테스트·빌드를 다시 실행하지 않았다. 최종 release·플랫폼 검증은 통합 작업 범위다.
