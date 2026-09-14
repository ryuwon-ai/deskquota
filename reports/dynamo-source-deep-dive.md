# NVIDIA Dynamo: 취소 소유권과 정리 시간의 상한

2026-09-12, SHA `4b72f79cb97f144377a5df1ac392b5b72e6dab3e`. 이 문서는 checkout의 선택한 source/test body를 읽은 결과이며 Dynamo 빌드·GPU 실행·벤치마크는 하지 않았다. 전체 제품을 로컬 PC에 설치할 계획도 아니다.

## 직접 확인한 구조

[workspace manifest](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/Cargo.toml#L5)에는 runtime, tokens, llm, kv-router, KV memory 계층, backend sidecar와 inference gateway가 별도 crate로 나뉜다. [kv-router features](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/kv-router/Cargo.toml#L16)는 기본 feature를 비워두고 metrics/runtime/standalone 기능을 선택형으로 둔다. **전체 workspace 의존성 목록이 모든 router 실행의 필수 의존성이라는 뜻은 아니다.** 우리의 선택은 필요한 HTTP·quota 모듈만 빌드하는 단일 binary이며, KV routing crate 도입의 필요성은 현재 없다.

GitHub license metadata는 NOASSERTION이지만 [root LICENSE](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/LICENSE#L1)는 명시된 DeepSeek test data의 MIT 예외와 나머지 Apache-2.0를 설명한다. 메타데이터만 보고 라이선스 부재로 표시하지 않는다. 코드를 복사하지 않고 동작 원칙과 반례만 참조했다.

## 요청에서 자원 반환까지

| 소스 | 확인한 동작 | 로컬 gateway 적용 |
|---|---|---|
| [enroll_public_request_attempt](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router.rs#L1140) | 실패할 수 있는 routing update 전에 booking을 등록하고 임시 owner가 취소 시 정리. 성공하면 commit | 자원 차감 후 owner 등록까지 await 공백이 없어야 함. request attempt와 최종 정산 소유권을 함께 이동 |
| [find_best_match_details_with_lifecycle](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router.rs#L1317) | future가 반환 전에 drop되면 pending/admitted booking 철회, 성공 결과에 attempt identity 포함 | request ID와 HTTP 시도 ID 분리. 이전 시도의 늦은 terminal이 새 시도의 permit을 반환하지 못하게 함 |
| [RequestLeaseManager::finish](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router/request_lease.rs#L295) | claim_now로 중복 정리 차단, scheduler/LRU 명령을 모두 enqueue한 뒤 await | cancel·timeout·EOF가 경쟁해도 자원 반환 한 번. 한 subsystem의 await 취소 때문에 다른 정리가 생략되지 않도록 설계 |
| [RequestAttemptLease Drop](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router/request_lease.rs#L383) | Drop에서 owner에게 completion 전달. 미commit enrollment도 Drop으로 정리 | RAII는 cleanup 전달 수단. 실제 upstream 계산 중단을 증명하는 신호는 아님 |
| [CleanupBudget](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router/routing_host/cancellation.rs#L15) | 끊긴 요청의 선택·routing·dispatch가 각자120초를 새로 받지 않고 공통 budget을 소비 | queue/retry/drain 단계마다 전체 deadline을 재설정하지 않음. 긴 재시도로 lifetime이 무한 연장되지 않게 함 |
| [StagedKv와 DispatchCancellation](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router/routing_host/cancellation.rs#L43) | 원격 prefill이 KV를 남긴 decode는 끊겨도 정리용 dispatch가 필요. KV가 없거나 aggregated/prefill이면 stop 시 취소 | 상태를 모르는 HTTP gateway가 이 예외를 구현할 수는 없음. 연결 종료와 backend cleanup의 차이를 입증하는 사례 |
| [cleanup budget exhausted](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router/routing_host/cancellation.rs#L132) | 정리 deadline 초과 시 staged KV가 즉시 반환되지 않고 만료까지 남을 수 있다고 별도 경고 | timeout=모든 실제 자원 반환으로 표시하지 않음. 로컬 permit 정리와 backend 자원 상태는 구분 |
| [cancel_on_stop](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router/routing_host/cancellation.rs#L166) | 동시 ready 결과와 stop 중 ownership-bearing 결과가 먼저 처리되도록 biased select | 결과·취소 동시 도착에 자원 유실이 없는지 gate로 재현. select 순서를 임의로 바꾸지 않음 |

[two-tier selector](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/router-plugins/builtin/src/two_tier_cost_fn.rs#L4)는 실제 worker active count와 device KV overlap으로 부하·cache affinity를 비교한다. 이는 선택 가능한 정책 하나이며 전체 Dynamo의 유일 기본 정책이라는 뜻은 아니다. 사내 upstream URL 하나에 이러한 telemetry가 없으므로 점수식을 복사해도 계산할 근거가 없다.

## 읽은 테스트와 우리 수용 조건

[kv_cancellation_after_admission_still_stops_the_aggregated_dispatch](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router/routing_host/tests.rs#L1353)는 admission 후 실제 worker dispatch 전에 취소하면 worker 호출이 없는지 검증한다. [cleanup_budget_decays_and_saturates_at_zero](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router/routing_host/cancellation.rs#L222)는 가상 시간을90초·60초 진행해 공통 budget이 줄고0에서 포화되는지 확인한다. [drops_pending_operation_when_context_stops](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router/routing_host/cancellation.rs#L241)와 [ready_operation_wins_if_context_is_already_stopped](https://github.com/ai-dynamo/dynamo/blob/4b72f79cb97f144377a5df1ac392b5b72e6dab3e/lib/llm/src/kv_router/routing_host/cancellation.rs#L259)는 취소 시 pending drop과 동시 ready 결과의 소유권을 각각 확인한다. **원본 테스트를 실행한 기록은 없다.**

승인된 core Task3·5·7에서 유지할 테스트:

- admission gate 뒤 송신 전 cancel: HTTP0회, RPM 확정 차감0, hold정리1회.
- EOF·deadline·disconnect 동시 발생: terminal1회, permit정리1회.
- retry/cooldown/drain을 거쳐도 요청 전체 deadline 연장 없음.
- 이전 attempt의 늦은 usage/terminal이 현재 attempt의 reservation에 영향을 주지 않음.
- backend 종료를 확인할 수 없는 모드에서 cancel만으로 완료량·token 절감 성과를 계산하지 않음.

우리 v0.1에 넣지 않을 것: 분산 KV index, GPU swap, prefill/decode 분리, backend migration, policy plugin registry. 위 코드는 최소 gateway의 **수명 관리와 검증 조건**에 쓸 근거다. 대규모 serving 기능을 제품에 들여오는 근거는 아니다.
