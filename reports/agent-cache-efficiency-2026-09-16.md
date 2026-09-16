# Exact-cache 대기 경로 독립 감사

2026-09-16. 범위는 변경 전 소스 추적, 경쟁 구현 패턴 확인, queued-cache 명세/계획 독립 검토다. 이 감사에서는 제품 수정·빌드·HTTP benchmark·실제 API 호출·커밋을 하지 않았다. 최종 구현과 실행 결과는 상위 작업의 보고서에서 확인해야 한다.

## 결론

**완료된 exact-cache 응답으로 초기 admission 대기를 끝내는 변경을 채택한다.** 모토인 “Get more done within your LLM limits”에 직접 맞는다. 추가 생성·quota 소비 없이 같은 작업을 끝내며 현재 도구·원본 요청·응답 usage를 유지한다. 기존 Tokio와 캐시만 사용하므로 새 서비스나 라이브러리가 필요하지 않다.

admission을 받은 뒤 한 번 더 조회하는 것만으로는 부족하다. RPM=1에서 첫 요청이 완료돼도 RPM 차감은 60초간 남는다. 두 번째 요청은 캐시가 채워졌어도 permit을 받지 못해 재조회 위치에 도달하지 못한다. 이 설명은 소스에 근거한 원인 분석이며, 이 감사 자체에서 시간을 측정한 결과는 아니다.

[검토한 명세](../docs/superpowers/specs/2026-09-16-queued-cache-design.md)와 [계획](../docs/superpowers/plans/2026-09-16-queued-cache.md)은 **APPROVED**다. 초기 admission만 다루고 내부 재시도 대기와 완전한 singleflight는 구분하는 범위가 적절하다.

## 실제 수명 추적

아래 위치는 queued-cache 수정 전 읽은 함수 기준이다. 이후 구현으로 행 번호가 달라질 수 있다.

| 단계 | 변경 전 소스 | 의미 |
|---|---|---|
| 검증·정책·키 | `server.rs:786-808`, `cache.rs:108-161` | 본문 검증 뒤 root·최종 URL·유효 전달 헤더·원문 body로 키를 만든다. 초기 lookup 한 번만 수행한다. |
| 입장 대기 | `server.rs:811-831`, `admission/mod.rs:200-276` | root FIFO와 RR 장부를 유지하는 Ticket을 만들고 quota·동시성·deadline을 기다린다. |
| 발송 시작 | `transport/stream.rs:98-177` | WorkerGuard 생성 뒤 실제 send 첫 poll 전에 Hold.start와 upstream_attempts를 증가시킨다. |
| 응답 저장 | `transport/stream.rs:287-370`, `cache.rs:321-346` | 허용된 응답을 최대 256 KiB로 수집하고, 완전한 응답이며 실제 EOF를 확인한 뒤 저장한다. 저장 시점은 Hold 반환보다 앞이다. |
| 정산·반환 | `transport/stream.rs:581-594`, `admission/mod.rs:415-430` | 시작한 Hold는 usage를 정산한다. 시작하지 않은 Hold는 예약을 취소한다. 시작된 RPM 차감은 완료만으로 사라지지 않는다. |
| 대기 취소 | `admission/mod.rs:348-358`, `admission/queue.rs:90-105` | Ticket Drop은 아직 큐에 있는 항목뿐 아니라 다른 drive가 예약한 미수령 결과도 취소한다. |

기존 `a_hit_returns_while_the_single_request_rpm_window_is_exhausted` 검사는 이미 채워진 캐시의 재사용만 증명한다. cold miss로 대기하기 시작한 요청이 나중에 채워진 캐시로 빠져나오는 회귀 검사가 추가로 필요하다.

## 가장 작은 올바른 변경

1. 캐시에 기존 Tokio `Notify` 하나를 둔다. 완전한 엔트리를 삽입하고 mutex를 해제한 뒤 `notify_waiters()`를 호출한다.
2. waiter는 먼저 `notified()`를 만들고 같은 키를 조용히 재조회한 뒤 알림을 기다린다. 관련 없는 키의 저장은 단지 재조회 한 번으로 끝난다.
3. server에서 이 캐시 대기를 **한 번 만든 admission future**와 경쟁시킨다. 알림마다 admission future를 버리고 다시 enqueue하면 원래 FIFO·aging·deadline이 바뀌므로 금지한다.
4. stop·disconnect·deadline 분기를 우선한다. 캐시 hit는 server에서 바로 반환해 WorkerGuard를 만들지 않는다. acquisition future가 drop되면 기존 Ticket 정리가 실행된다.
5. admission 성공 직후에도 한 번 재조회한다. 이때 hit라면 아직 시작하지 않은 Hold를 drop한다. 이미 worker로 넘겼거나 발송한 요청은 변경하지 않는다.

Tokio 1.49의 설치된 `sync/notify.rs:530-535,900-928`은 `notify_waiters()`의 경우 Notified 생성 이후 발생한 알림이 아직 poll되지 않아도 보존된다고 명시한다. 따라서 **생성→조회→대기** 순서가 핵심이다. `enable()`은 `notify_one()`의 미등록 경쟁에 필요한 장치이며 이 broadcast 경로에는 필수 조건이 아니다. 기존 admission과 일관성을 위해 써도 되지만 별도 추상화는 필요하지 않다.

## 보존할 계약

- 재조회는 최초 miss를 hit로 한 번 재분류한다. 관계없는 알림마다 miss를 늘리지 않는다. `considered = hits + misses + named bypasses`라는 요청 기준 해석을 유지한다. 취소·deadline·캐시 불가 응답은 miss로 남는다.
- replay의 원본 usage bytes는 유지한다. client의 context 계산과 auto-compaction에는 동일 응답을 전달하되, gateway의 upstream 시도·usage 정산·worker 종료 지표를 한 번 더 증가시키지 않는다.
- root·인증·URL·본문·정책 격리와 no-store/Vary 검사를 유지한다. 키가 다른 요청에는 broadcast 알림이 발생해도 응답을 공유하지 않는다.
- terminal marker만으로 저장하지 않는다. 실제 EOF, 응답 완결성, 살아 있는 downstream을 확인한 기존 commit 조건을 유지한다. 취소된 leader의 부분 응답이나 오류를 follower에게 재사용하지 않는다.
- 재조회 자체가 큐 정렬을 바꾸지 않는다. 완료된 캐시 hit가 큐에서 나가는 것은 기존 warm-hit가 admission을 생략하는 의미와 같다.
- TTL 300초 기본값, 전체 payload 4 MiB, 엔트리 256 KiB/128개 한도는 유지한다. broadcast 범위는 기존 대기·진행 요청 수로 제한된다. 큐 상한은 64이며 잠깐 예약된 뒤 아직 poll되지 않은 future까지 고려하면 listener 수를 무조건 64라고 표현하지 않는다.

## 경쟁 구현에서 가져올 것과 가져오지 않을 것

| 근거 | 확인한 패턴 | 채택·차이 |
|---|---|---|
| Bifrost v2.1.1, SHA `c193745d2a713e9f58f021d43e138df5eb7e038a`, `framework/lrucache/lrucache.go:160-230` | 키별 진행 중 fill을 공유한다. follower는 자신의 context 취소로 독립 종료하며 오류를 캐시에 저장하지 않는다. loader panic 때도 waiter를 풀어준다. | 같은 키 완료 알림과 독립 취소라는 원칙을 채택한다. leader registry·panic/error fan-out·generation invalidation 전체는 현재 작은 수정에 필요하지 않다. |
| 설치된 LiteLLM 1.100.1, `proxy/common_utils/cache_coordinator.py:41-210` | event 대기 뒤 캐시를 다시 읽고, leader도 load 전에 재조회한다. finally에서 대기자를 깨운다. | 알림 후 재조회와 double-check를 참고한다. 원본은 config/spend 등 범용 리소스 캐시용이다. 모든 LLM response가 동일하게 중복 제거된다고 주장하지 않는다. |
| Go `x/sync/singleflight` | 같은 키의 진행 중 함수 호출을 한 번 실행하고 결과를 공유한다. | 완전한 중복 제거의 표준 패턴이다. 우리는 이미 진행 중인 응답의 결과를 기다리도록 강제하지 않으므로 singleflight 구현으로 명명하지 않는다. [공식 문서](https://pkg.go.dev/golang.org/x/sync/singleflight). |

Bifrost checkout SHA와 tracked diff 없음, 설치된 LiteLLM METADATA 1.100.1을 직접 확인했다. 경쟁 제품의 새 response-cache benchmark는 실행하지 않았다.

## 가장 중요한 검증

| 사례 | 실패하면 드러나는 문제 |
|---|---|
| RPM=1, 첫 응답 EOF gate, JSON/SSE 동일 후속 요청을 큐에 넣고 첫 응답 완료 | post-admission 조회만 추가해 quota 만료까지 기다리는 잘못된 수정 |
| cap=1, quota 여유, 같은 cold 요청 여러 건 | 캐시 저장과 slot 반환 순서, 알림/입장 경쟁, 중복 카운트 |
| 동일 key follower 취소 또는 짧은 deadline 뒤 첫 응답 완료 | 취소된 Ticket 부활, 미수령 Hold 누수, 요청 수명 연장 |
| 다른 키 또는 no-store/불완전 응답 대조 | broadcast를 key 검증 없이 공유하거나 정책을 우회하는 오류 |

각 경우 upstream 실제 시도, 최종 성공·실패·timeout·취소 분모, 저장 횟수, hit/miss, queue/active/held 잔여량, usage 정산 횟수를 함께 확인한다. 기존 원문 replay·키 격리·Vary·tools·취소 테스트를 재사용하고 같은 로직을 복제한 테스트를 늘리지 않는다.

## 에이전트 토론과 다음 순서

모토·footprint 감사 에이전트와 합의한 범위는 **알림으로 불필요한 초기 quota 대기를 끝내되 full singleflight는 하지 않는 것**이다. 여유 slot이 두 개면 최초 두 중복 요청이 모두 발송될 수 있다. 이 상한은 측정·문서에 남기고 arbitrary burst가 항상 10→1이라고 주장하지 않는다. retry 내부 admission 대기의 캐시 재사용도 이번 범위 밖이다.

quota 감사 에이전트의 429 pre-head body probe 지연은 다음 작은 후보로 지지한다. 다만 `retry=false`라는 이유만으로 모두 없애면 timing 없는 transient 429를 분류해 공유 fallback cooldown을 설정하던 동작을 잃는다. **분류가 원래 불가능한 응답 형식**이나 **retry off이며 유효한 timing header로 필요한 판단이 이미 끝난 경로**를 먼저 gate로 재현한 뒤 최소 변경해야 한다. 503 Retry-After의 공유 cooldown도 자동 재전송 확대와 분리해 검토할 수 있다. 이 감사에서는 해당 변경이나 성능 효과를 검증하지 않았다.

현재 strict rolling RPM18건 실험의 약 38초 하한은 이 캐시 변경으로 일반적으로 사라지지 않는다. 고유 요청에는 캐시가 적용되지 않는다. 기대할 우위는 **원래 재사용 가능한 반복 작업이 불필요한 생성과 quota 만료를 기다리지 않도록 만드는 것**이다.

## 후속 429 명세 검토

별도 에이전트의 gate 재현 이후 [rejection-head 명세](../docs/superpowers/specs/2026-09-16-rejection-head-design.md)와 [계획](../docs/superpowers/plans/2026-09-16-rejection-head.md)을 검토했다. 초기 검토에서 실제 의미 차이를 하나 발견했다. `transient`는 encoding의 `identity`를 대소문자 무시로 허용하지만 기존 `probe_rejection`은 소문자 일치만 허용한다. helper 추출 시 후자의 guard를 없애면 `IDENTITY` 응답이 새로 retry 가능해져 “재시도 확대 없음”과 충돌한다.

명세는 **기존 probe guard를 유지하고 shared helper를 추가 필터로만 적용**하도록 수정됐다. uppercase encoding·서로 다른 Content-Type 값 대조도 포함한다. 수정된 명세/계획은 **APPROVED**다. header timing은 항상 먼저 적용하고, timing 없는 JSON transient의 fallback 분류는 retry off에서도 유지한다. 제외 경로의 429 head 조기 전달 후 body 실패가 502 대체 대신 stream error가 되는 차이도 명시했다. 이 승인은 설계·소스 검토이며 최종 구현 실행 결과가 아니다.

## Queued-cache 구현 명세 검토

구현자가 고정한 세 파일을 `/Users/ryuwon/Library/Caches/deskquota-queued-cache-20260916/before/`의 작업 직전 파일과 직접 비교했다. 기존 BPE 변경 전체를 이번 변경으로 취급하지 않았다. `implementation-summary.json`의 세 SHA256과 현재 파일이 모두 일치했다.

- `cache.rs`: `LookupOutcome`으로 중복 재조회 집계를 막고, 저장 후 mutex 해제→알림 순서를 지켰다. `wait_for_cached`는 알림을 먼저 등록한다.
- `server.rs`: 캐시 대기 루프가 하나의 select 분기 안에 있으므로 admission future는 그대로 유지된다. cache hit 분기와 admission 직후 재조회 모두 worker 생성 전에 반환한다.
- `cache_contract.rs`: JSON/SSE 각각 RPM 제한·동시성 제한, EOF 전 비재사용, credential/no-store/취소 격리, server deadline과 예약 정리를 검사한다. 기존 정책·wire 검사를 유지했다.

**구현 명세 검토 APPROVED, blocker 없음.** 보존된 RED 로그에서 변경 전 JSON/SSE 두 사례가 quota 대기 timeout으로 실패한 것을 읽었다. 최종 로그의 cache contract 23개와 cache unit 5개 통과도 확인했다. 검토자가 테스트를 재실행한 것은 아니며, final release 성능·no-wait·전체 회귀 검증을 대신하지 않는다. 이 단계의 근거는 [구현 기록](../evidence/queued-cache-2026-09-16/implementation/implementation-summary.json)과 해당 폴더의 RED/GREEN 로그다.
