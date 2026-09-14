# Ollama·llama.cpp·Codex: 실제 요청 경로 심층 분석

기준일 2026-09-12. 아래는 고정 SHA의 코드와 원본 테스트 **본문을 읽은 결과**다. 이 세 저장소의 빌드·모델 추론·테스트 실행은 하지 않았다. 설치된 Codex/Ollama와 clone의 버전이 같다고 가정하지 않는다. 각 링크는 이동하는 main 대신 정확한 commit을 가리킨다.

## 제품 설계에 주는 결론

1. 프록시가 제어할 수 있는 것은 요청의 입장·대기·연결 수명이다. GPU의 토큰 단위 선점, KV 상태 저장·복구는 엔진의 협력이 필요하다.
2. 연결 종료, HTTP 처리 종료, 엔진 슬롯 반환은 별개다. endpoint·헤더·엔진 버전마다 취소 의미가 달라진다.
3. CPU·메모리가 작은 PC에서는 동시성을 무조건 늘리지 않는다. 기본 1개부터 측정하고, queued body·stream buffer·대기시간에 상한을 둔다.
4. upstream API 목록과 에이전트의 모델 선택 목록은 서로 다른 데이터 계약이다. URL 변경 전에 프로토콜과 모델 표시의 한계를 알려야 한다.

## Ollama: 모델 배치와 생성 요청의 동시성은 두 단계

체크아웃: `53fed26112817f7c55f664efb9e3f65f06cab7db`. MIT. native 실행 경로의 참조이며 우리 제품에 Ollama 런타임을 내장한다는 뜻은 아니다.

| 코드에서 확인한 경로 | 해석과 채택 여부 |
|---|---|
| [server/sched.go:194](https://github.com/ollama/ollama/blob/53fed26112817f7c55f664efb9e3f65f06cab7db/server/sched.go#L194): 사용할 runner가 이미 있으면 직접 넘기고, 없으면 bounded pending channel에 enqueue. pending 처리 시 취소된 context를 먼저 확인 | 모델 로드 큐 전체를 HTTP 생성 큐와 동일시하지 않는다. 우리 ingress에서는 로드 상태를 알 수 없어 동시성 기본 1을 유지 |
| [envconfig/config.go:274](https://github.com/ollama/ollama/blob/53fed26112817f7c55f664efb9e3f65f06cab7db/envconfig/config.go#L274): NumParallel 기본 1, MaxQueue 기본 512 | 타 제품의 512를 복사하지 않는다. 긴 agent body가 많아질 수 있어 우리 계획의 64개와 전체 32MiB 두 상한을 함께 적용 |
| [server/routes.go:3132](https://github.com/ollama/ollama/blob/53fed26112817f7c55f664efb9e3f65f06cab7db/server/routes.go#L3132): ErrMaxQueue는 503, context 취소는 499 | 429만 관찰하면 로컬 과부하 일부를 놓친다. 다만 503은 다른 원인도 있으므로 자동 POST 재시도 조건으로 확대하지 않음 |
| [llm/llama_server.go:1545](https://github.com/ollama/ollama/blob/53fed26112817f7c55f664efb9e3f65f06cab7db/llm/llama_server.go#L1545): completion은 context-aware semaphore 획득 후 defer release. [llm/llama_server.go:1644](https://github.com/ollama/ollama/blob/53fed26112817f7c55f664efb9e3f65f06cab7db/llm/llama_server.go#L1644): runner HTTP 요청에 같은 context 전달 | 완료 전 permit을 맡기는 구조를 참고. Go 함수가 반환했다는 사실만으로 engine GPU 자원 반환이 입증되지는 않음 |
| [llm/llama_server.go:1671](https://github.com/ollama/ollama/blob/53fed26112817f7c55f664efb9e3f65f06cab7db/llm/llama_server.go#L1671): SSE scanner 크기 제한과 context 확인, 최종 callback을 body 종료 뒤로 미룸 | terminal 관찰과 수명 정리를 분리하는 이유. 우리 SSE observer는 bounded read-only 관찰, 전문 저장 없음 |

읽은 원본 테스트: [TestSchedAlreadyCanceled](https://github.com/ollama/ollama/blob/53fed26112817f7c55f664efb9e3f65f06cab7db/server/sched_test.go#L1191)는 이미 취소된 pending을 제거하고 결과 채널을 채우지 않는지 확인한다. [TestSchedLlamaServerPredictionUsesTotalParallelContext](https://github.com/ollama/ollama/blob/53fed26112817f7c55f664efb9e3f65f06cab7db/server/sched_test.go#L1335)는 병렬 2개 × context 요구로 메모리가 부족할 때 runner를 띄우기 전에 eviction이 필요한지 확인한다. **테스트의 존재는 확인했지만 여기서 실행하지 않았다.**

추론: 작은 PC에서는 concurrency 증가가 KV/context 메모리를 늘려 오히려 모델 eviction·재로드를 유발할 수 있다. 모델 크기만 보고 자동 병렬 수를 높이는 wizard를 만들 근거가 없다. 실제 측정값이 없으면 보수적인 기본값을 제안하고 명시적으로 변경하게 한다.

## llama.cpp: 실제 선점 범위와 재연결 예외

체크아웃: `d3146f2b56c2db4711ac8391871c9e529d1946d7`. MIT.

[개발 아키텍처](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/README-dev.md#L6)는 HTTP 작업 큐와 단일 주 inference context, 슬롯별 sequence를 분리한다. 여러 슬롯을 한 batch의 prefill/decode에 넣는 구현이므로 HTTP 요청을 쪼갠다고 같은 효과를 얻지 못한다.

| 코드에서 확인한 경로 | 게이트웨이에 가져올 부분 |
|---|---|
| [tools/server/server-context.cpp:1547](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/server-context.cpp#L1547): free slot의 prompt 공통 prefix를 비교하고, 적합한 것이 없으면 LRU slot 선택 | prefix/KV cache는 응답 cache가 아님. prompt를 재작성하거나 순서를 바꾸는 최적화는 기본 제외 |
| [tools/server/server-queue.cpp:90](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/server-queue.cpp#L90): 해제된 특정 slot 대상 작업을 먼저 선택, 없으면 deferred FIFO | 자원 친화성은 실제 슬롯 정보가 있을 때만 유효. 우리 root RR를 알 수 없는 GPU slot에 결합하지 않음 |
| [tools/server/server-context.cpp:2370](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/server-context.cpp#L2370): inference가 yielding 중일 때 read-only 작업만 허용하고 나머지는 보류 | 'CPU context switching'이라는 이름만 붙여도 실행 중 계산을 안전하게 선점할 수 있는 것은 아님 |
| [tools/server/server-queue.cpp:601](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/server-queue.cpp#L601): reader stop이 cancel task를 큐 맨 앞에 넣음. [tools/server/server-context.cpp:2454](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/server-context.cpp#L2454): 해당 task를 처리할 때 slot release | 취소 요청 접수와 반환 완료를 구분. 우리 모의 서버도 두 시점을 별도 gate로 재현 |
| [tools/server/server-queue.cpp:369](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/server-queue.cpp#L369): 일반 pending뿐 아니라 deferred와 yielding 중 unhandled에서도 취소 대상 제거 | 큐의 모든 상태에 동일 cancellation 소유권 적용. admission 직후 HTTP 시작 전 취소를 별도 회귀 테스트로 유지 |

**헤더 하나가 취소 계약을 바꾼다.** [tools/server/server-stream.cpp:598](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/server-stream.cpp#L598)는 `X-Conversation-Id`가 있으면 replay session을 만든다. [tools/server/server-stream.cpp:622](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/server-stream.cpp#L622)의 `should_stop()`은 이 경우 HTTP 연결 대신 session cancellation을 확인하고, [tools/server/server-stream.cpp:631](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/server-stream.cpp#L631)의 완료 처리가 나머지 생성을 drain한다. 명시적 stop은 [DELETE /v1/stream](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/server-stream.cpp#L563) 경로다.

따라서 일반 연결에서는 cancel이 전달되더라도 session streaming 모드에서는 연결을 닫는 것이 추론 중단과 같지 않다. v0.1에서 upstream 전용 resume/stop API까지 지원하지 않는다. 해당 기능이 필요한 클라이언트는 지원 범위 밖임을 안내한다. 단순히 `close` 정책만 선택해 quota 슬롯을 빨리 반환하도록 일반화하지 않는다.

읽은 원본 테스트: [test_cancel_request](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/tests/unit/test_completion.py#L611)는 HTTP timeout 뒤 2초 후 slot의 is_processing=false를 검사한다. [test_stream_resumes_after_reload_during_model_load](https://github.com/ggml-org/llama.cpp/blob/d3146f2b56c2db4711ac8391871c9e529d1946d7/tools/server/tests/unit/test_stream.py#L100)는 실제 socket을 모델 로드 중 끊고, 세션이 재연결되어 출력을 replay하는지 확인한다. 이 테스트는 로드 구간이 너무 짧으면 skip할 수 있다. GPU/모델이 필요한 테스트를 실행했다고 보고하지 않는다.

## Codex: 재시도 두 계층과 모델 카탈로그

체크아웃: `944d6fd1ba4baab69dbedd205282dc72ec20abb5`. Apache-2.0. **소스 snapshot의 계약이며 설치된 CLI에 대한 runtime 증명이 아니다.**

| 코드·원본 테스트 근거 | 설정 및 테스트에 반영 |
|---|---|
| [codex-rs/model-provider-info/src/lib.rs:61](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/model-provider-info/src/lib.rs#L61)와 [codex-rs/model-provider-info/src/model_provider_info_tests.rs:114](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/model-provider-info/src/model_provider_info_tests.rs#L114): wire_api=chat를 명시적으로 거절 | Chat Completions만 있는 사내 LLM에 Codex를 URL만 바꿔 연결할 수 있다고 약속하지 않음. 같은 Responses API 통과가 v0.1 경계 |
| [codex-rs/model-provider-info/src/lib.rs:28](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/model-provider-info/src/lib.rs#L28): request retries 기본4, stream retries 기본5. [codex-rs/codex-client/src/retry.rs:92](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/codex-client/src/retry.rs#L92): 0부터 max_attempts 포함 순회 | 이 HTTP loop에서 설정4는 최대5시도. 전체 agent 실행의 절대상한은 stream·별도 reconnect 정책까지 확인해야 함 |
| [codex-rs/model-provider-info/src/lib.rs:391](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/model-provider-info/src/lib.rs#L391): HTTP retry 429=false, 5xx/transport=true | Pi의 429 동작을 Codex에 그대로 적용하지 않음. gateway 재시도 기본0 유지, 클라이언트별 진단 필요 |
| [codex-rs/core/tests/suite/retry_after.rs:245](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/core/tests/suite/retry_after.rs#L245): 503 Retry-After 헤더가 있어도 local backoff를 쓰는 회귀 테스트 | Retry-After를 전달하는 것만으로 client의 대기 준수를 보장할 수 없음. gateway가 shared cooldown의 실제 admission을 제한해야 함 |
| [codex-rs/core/src/responses_retry.rs:52](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/core/src/responses_retry.rs#L52): 별도 stream retry 상태·server delay·feature 조건의 connection retry. [codex-rs/core/src/responses_retry.rs:94](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/core/src/responses_retry.rs#L94): transport fallback도 별도 | custom provider supports_websockets=false 고정 후 HTTP/SSE fixture. 'retry=0 하나면 전체 재시도 없음'이라고 표시하지 않음 |
| [codex-rs/core/src/config/mod.rs:2072](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/core/src/config/mod.rs#L2072): model_catalog_json은 ModelsResponse를 parse하고 빈 models 거절. [codex-rs/config/src/profile_toml.rs:41](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/config/src/profile_toml.rs#L41): profile 필드 | GET /v1/models를 그대로 저장하는 방식 금지. 공식 schema의 모델 metadata를 검증하고 표시 한계를 고지 |
| [codex-rs/models-manager/src/manager.rs:435](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/models-manager/src/manager.rs#L435): discovery 허용 정책 분기. [codex-rs/models-manager/src/manager.rs:552](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/models-manager/src/manager.rs#L552): cache에 client 버전·provider/auth identity 일치 요구 | 이전 provider의 캐시를 재사용해 모델이 보여도 gateway 연동 성공으로 판정하지 않음. 격리 profile/cache로 검사 |

읽은 원본 테스트: [model_catalog_json_loads_from_path](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/core/src/config/config_tests.rs#L9556)와 [model_catalog_json_rejects_empty_catalog](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/core/src/config/config_tests.rs#L9584); [mismatched_and_legacy_cache_entries_fetch_the_current_catalog](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/models-manager/src/cache_identity_tests.rs#L7). CLI 실행 테스트는 아직 별도 항목으로 남아 있다.

## 구현 계획에 연결하는 반례

| 반례 | 기존 승인 계획의 검증 위치 | 성공 판단 |
|---|---|---|
| downstream disconnect 뒤 upstream이 계속 계산 | core Task3 stream_lifetime | EOF/deadline 전 새 slot을 내주지 않으며 body는 버림, 저장량이 늘지 않음 |
| cancel이 pending/deferred/admitted 여러 상태에서 경쟁 | core Task5·6 | 실제 HTTP 시작 전 취소는 upstream0회, hold반환1회; 시작 뒤에는 정한 회계 계약 유지 |
| 503·429와 client retry 계층이 서로 다름 | core Task4·7, native client acceptance | 전체 시도 수를 실제 socket에서 기록, cooldown 동안 다른 root도 quota 초과 송신 없음 |
| 계층별 큐가 모두 길어져 tail latency 악화 | core Task7·8 | queued-time/TTFT/E2E/outcome과 전체 메모리 함께 측정, 실패/취소를 표본에서 제거하지 않음 |
| 모델 목록 표시가 upstream과 불일치 | native connect/doctor acceptance | 선택한 모델의 실제 fixture 왕복, profile patch preview, unsupported API 명확한 진단 |
| prefix cache를 응답 cache로 오인 | v0.1 범위 유지 | body byte 보존; semantic/response cache를 기본 핵심에 추가하지 않음 |

이 보고서가 추가한 것은 소스 반례와 테스트 조건이다. proxy latency·RSS·완료량 향상 또는 사용자 수요가 입증된 것은 아니다.
