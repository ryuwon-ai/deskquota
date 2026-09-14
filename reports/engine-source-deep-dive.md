# 엔진·KV 계층 소스 분석과 로컬 quota gateway의 경계

기준일: 2026-09-12. 분석 대상은 한 PC에서 실행하는 Rust binary 하나, upstream 하나, 공유 RPM/TPM 장부 하나다. 아래 엔진을 제품에 내장하거나 엔진용 분산 계층을 도입하는 제안은 아니다. 기존 [코어 설계](../docs/superpowers/specs/2026-09-12-local-quota-gateway-design.md)와 [벤치마크 명세](benchmark-spec.md)에 추가할 소스 근거와 실패 사례를 정리했다.

| 판단 | 근거와 적용 범위 |
|---|---|
| 먼저 적용 | 요청의 상태와 자원 소유권 분리, 종료 처리 한 번, 취소·완료 경합, bounded queue, root 간 공정 순환 |
| 현재 유지 | body와 의미 있는 헤더 보존, usage가 없으면 unknown, 취소 뒤 bounded drain, 실행 slot과 quota 장부 분리 |
| 별도 엔진 연동이 필요 | 실제 KV 적중률, prefill 처리량, GPU block 반환, 토큰 단위 스케줄링·retraction·KV 이동 |
| 현재 도입 근거 없음 | 엔진 통째 내장, prefix-hit 추정에 따른 quota 할인, 출력 길이를 안다고 가정한 예측 스케줄러, 중단한 POST 자동 재실행 |

**직접 검증:** 로컬 체크아웃의 SHA, 관련 README·AGENTS·manifest·구현·테스트 소스를 읽었다. 네 체크아웃 모두 조사 시작 시 `git status --short` 출력이 없었다. **코드/문서 근거:** 아래 동작은 그 SHA의 정적 분석이다. **미실행:** 원본 테스트, 모델 추론, GPU·CPU 성능, 엔진 설치, Docker/WSL, Windows 실행. 테스트 파일의 assertion을 읽었다는 사실을 테스트 통과로 표현하지 않는다. 성능 배수·마케팅 채택 수는 판단 근거에서 제외했다.

| 별칭 | 원본 저장소 및 고정 SHA | 분석한 실행 계층 |
|---|---|---|
| V | [vllm-project/vllm](https://github.com/vllm-project/vllm/tree/a0844fa6c6d6e1143917a033ad6984e40c212862) · `a0844fa6c6d6e1143917a033ad6984e40c212862` | V1 AsyncLLM → Scheduler → KVCacheManager → completion |
| S | [sgl-project/sglang](https://github.com/sgl-project/sglang/tree/e91c94805747b60d0b8e8012f3647ae82d4ae862) · `e91c94805747b60d0b8e8012f3647ae82d4ae862` | TokenizerManager → Scheduler·SchedulePolicy → RadixCache → result processor |
| L | [LMCache/LMCache](https://github.com/LMCache/LMCache/tree/588c5366f39cf1fbb21a614074aefdf942573d93) · `588c5366f39cf1fbb21a614074aefdf942573d93` | 현재 MP vLLM connector·adapter·lookup/transfer modules, 별도 V1 adapter abort 회귀 |
| D | [llm-d/llm-d-router](https://github.com/llm-d/llm-d-router/tree/6493998a3735ccf1b07450fe6308c9ec5dd929e1) · `6493998a3735ccf1b07450fe6308c9ec5dd929e1` | EPP admission·flow control → endpoint selection → HTTP stream lifecycle |

모든 파일·행 번호는 위 SHA에 고정된다. 아래 링크는 이동하는 `main`이 아닌 고정 commit을 가리킨다. 체크아웃은 `references/{vllm,sglang,LMCache,llm-d-router}`이며 sparse checkout이므로 전체 저장소 전수 감사라는 뜻은 아니다.

## 1. vLLM: token budget과 KV 소유권을 아는 엔진

| 단계 | 실제 경로와 관찰 | 고정 소스 |
|---|---|---|
| 요청 등록 | `AsyncLLM.generate`가 `add_request`를 호출하고 output collector를 기다린다. scheduler의 새 request는 waiting queue와 request map에 등록된다. | [vllm/v1/engine/async_llm.py:669–699](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/v1/engine/async_llm.py#L669), [vllm/v1/core/sched/scheduler.py:2478–2504](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/v1/core/sched/scheduler.py#L2478) |
| 스케줄링 | 각 step의 `max_num_scheduled_tokens`와 `max_num_batched_tokens` 안에서 running 요청부터 배치한다. 계산된 token 수가 요구 token 수를 따라가도록 배분한다. 정책은 FCFS/priority이고 root별 token fairness 보장은 이 경로에서 제공하지 않는다. | [scheduler.py:563–614](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/v1/core/sched/scheduler.py#L563), [request_queue.py:13–17,131–149](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/v1/core/sched/request_queue.py#L131) |
| prefix 조회·할당 | 완성된 KV block의 가장 긴 적중 prefix를 찾는다. 전체 prompt 적중이어도 logits를 얻기 위해 마지막 token을 재계산한다. 할당은 실제 free blocks에서 예약·watermark를 뺀 용량을 검사한 뒤 이뤄진다. | [vllm/v1/core/kv_cache_manager.py:245–279,479–563](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/v1/core/kv_cache_manager.py#L245) |
| 선점 | KV 할당 실패 시 priority/arrival 또는 running 마지막 요청을 골라 free 가능 여부를 확인한다. `_preempt_request`는 KV·encoder 자원을 정리하고 `PREEMPTED`, `num_computed_tokens=0`으로 바꾼 뒤 waiting에 다시 넣는다. in-flight 출력의 stale token 수도 추적한다. | [scheduler.py:729–795](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/v1/core/sched/scheduler.py#L729), [scheduler.py:1473–1514](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/v1/core/sched/scheduler.py#L1473) |
| 취소 | generator cancellation/exit → `abort` → output processor와 engine core 양쪽 abort. scheduler는 이미 끝난 요청을 건너뛰고 큐에서 제거한다. remote KV 수신 대기 중이면 block 해제를 지연한다. | [async_llm.py:701–709,834–843](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/v1/engine/async_llm.py#L701), [scheduler.py:2506–2567](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/v1/core/sched/scheduler.py#L2506) |
| 완료·회계 | stop 판단 후 `_free_request`를 호출하지만 connector가 async 전송 중이면 실제 block 반환과 request map 삭제를 늦춘다. HTTP usage는 prompt token IDs와 생성 token IDs에서 계산한다. streaming usage chunk는 include-usage 설정에 종속된다. | [scheduler.py:2122–2134,2569–2627](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/v1/core/sched/scheduler.py#L2569), [vllm/entrypoints/openai/chat_completion/serving.py:817–840,1119–1139](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/vllm/entrypoints/openai/chat_completion/serving.py#L817) |

이 선점은 엔진이 request token IDs, block allocation, 진행 중 GPU step, connector 상태를 함께 관리하기 때문에 가능하다. 선택한 V1 경로는 선점 때 계산 진행 수를 0으로 되돌린다. 따라서 모든 선점이 KV 상태를 그대로 보존하는 무료 context switch라고 해석해서도 안 된다. block cache 재사용과 재계산의 실제 양은 구성·cache 상태에 달려 있다.

**읽은 upstream 테스트:**

- [tests/v1/core/test_scheduler.py:1125–1181](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/tests/v1/core/test_scheduler.py#L1125) `test_preempt_during_execution`: KV가 꽉 차 두 번째 요청을 선점한 뒤, 이미 진행 중이던 실행의 token 42가 도착하는 상황을 검사한다. source assertion은 늦은 출력도 해당 요청 token 이력에 남는다는 것이다. terminal이 아닌 preemption 이벤트를 terminal cleanup처럼 처리하면 안 된다는 근거다.
- [같은 파일:5026–5052](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/tests/v1/core/test_scheduler.py#L5026) `test_abort_request_waiting_for_remote_kvs`: abort 직후 request map에 남고 `finished_recving`을 전달한 뒤 삭제되는지 검사한다. 외부 종료 요청과 자원 반환 완료가 다름을 직접 표현한 fixture다.

**우리 설계에 적용할 불변식:** 취소 의사, downstream 전송 종료, upstream attempt 종료, slot 반환, quota 정산은 각기 다른 사건이다. admission 소유자가 terminal 원인과 예약을 한 번만 정리해야 한다. mock에 EOF·cancel·deadline 이벤트를 같은 tick에 넣어도 slot 이중 반환이나 quota 이중 환불이 없어야 한다. 실제 upstream 완료를 보지 못한 경우 provider가 취소분을 과금하지 않았다고 추정하지 않는다.

**가져오지 않을 것:** `max_num_batched_tokens`는 한 engine step의 compute budget이다. 분당 TPM으로 이름만 바꿔 사용할 수 없다. FCFS/priority 존재도 root별 공정 서비스량 보장이 아니다. prefix hit는 prefill 계산 재사용이며 최종 텍스트 응답 재사용이 아니다.

## 2. SGLang: cache-aware 대기 순서와 decode retraction

| 단계 | 실제 경로와 관찰 | 고정 소스 |
|---|---|---|
| 요청 입장 | `handle_generate_request`가 Req를 만들고 `_add_request_to_queue`가 priority/queue limit 검사 후 prefetch와 waiting append를 수행한다. 큐 입장 시 누적 prefill token counter를 snapshot한다. queue overflow는 기본 신규 요청 거절, priority 모드에서는 더 낮은 priority의 기존 대기 요청을 퇴출할 수 있다. | [python/sglang/srt/managers/scheduler.py:2716–2747,3151–3160](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/scheduler.py#L3151), [scheduler.py:3217–3261](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/scheduler.py#L3217) |
| 순서·자원 검사 | `calc_priority` 뒤 PrefillAdder가 KV allocator·radix cache·prefill cap·chunk size로 배치 후보를 만든다. 채택한 요청만 waiting에서 제거한다. 예상 잔여 decode token에 clip과 `new_token_ratio`를 적용하는 자원 추정도 존재한다. | [scheduler.py:3759–3795,3939–3953](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/scheduler.py#L3759), [python/sglang/srt/managers/schedule_policy.py:686–717](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/schedule_policy.py#L686) |
| LPM·HRRN | LPM은 matched prefix 길이로 정렬한다. HRRN은 `waited processed-prefill-tokens / uncached input tokens`가 큰 순서이고 uncached=0은 먼저 보낸다. `processed_tokens`의 단조 증가와 입장 snapshot이 계약이다. LPM/HRRN도 queue가 128을 넘으면 FCFS로 바뀐다. | [schedule_policy.py:303–314,401–453](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/schedule_policy.py#L401) |
| KV 관리 | `match_prefix`는 token ID와 추가 namespace가 같은 prefix의 device KV index를 찾는다. finished request도 token/KV index를 radix tree에 삽입할 수 있다. 공유 중인 node는 lock reference로 보호하고 evictable leaf에서 공간을 회수한다. | [python/sglang/srt/mem_cache/radix_cache.py:400–450,482–537](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/mem_cache/radix_cache.py#L400), [radix_cache.py:616–674](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/mem_cache/radix_cache.py#L616) |
| retraction | 다음 decode step의 실제 KV 용량 검사 실패 → `retract_decode` → 일부 요청 자원 정리 → retracted 요청 재입장. 별도 retraction policy는 priority 또는 이미 생성한 output/input 길이를 사용한다. HRRN 대기 정렬과 같은 알고리즘이 아니다. | [scheduler.py:4065–4135](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/scheduler.py#L4065), [python/sglang/srt/managers/schedule_batch.py:3086–3114,3183–3222](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/schedule_batch.py#L3086) |
| 취소·완료 | tokenizer가 disconnect를 감지해 abort를 전송하며 중복 abort 전송을 막는다. 대기 요청은 pop하고 상태 정리용 출력을 보낸다. running 요청은 `to_finish`를 설정해 다음 decode forward 후 기존 완료 경로로 정리한다. async KV offload 설정이면 device→host 완료 후 release한다. | [python/sglang/srt/managers/tokenizer_manager.py:1747–1764,2009–2028](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/tokenizer_manager.py#L2009), [scheduler.py:5181–5233,5319–5334](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/scheduler.py#L5319), [scheduler_components/batch_result_processor.py:1310–1344](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/scheduler_components/batch_result_processor.py#L1310) |
| token 관측 | output streamer는 input ID 길이, 실제 output ID 길이/beam output, cached tokens를 각각 수집한다. retraction count도 별도 필드다. | [scheduler_components/output_streamer.py:462,490–498,516](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/sglang/srt/managers/scheduler_components/output_streamer.py#L490) |

HRRN의 aging은 짧은 uncached prefill을 우대하면서 오래 기다린 요청을 올리는 휴리스틱이다. 미래 completion 길이·전체 작업 완료시간을 맞히는 예측 모델이 아니다. prefix-hit 정보, 진행한 prefill 양, throughput이 일정하다는 대응 가정이 있다. API 프록시에서 wall-clock wait를 넣어 같은 알고리즘이라고 부르거나 `max_tokens`를 실제 실행량으로 간주하면 이 계약이 달라진다. token-counter가 움직이지 않는 decode-only 구간의 wall-clock 지연을 이 수식만으로 보장하지도 않는다.

**읽은 upstream 테스트:**

- [test/registered/unit/managers/test_schedule_policy.py:67–95](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/test/registered/unit/managers/test_schedule_policy.py#L67) `test_calc_priority_hrrn_aging_overtakes_short`: 오래된 100-token 요청과 막 도착한 1-token 요청의 counter snapshot을 다르게 두고 오래된 요청이 앞서는지 검사한다. [97–124행](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/test/registered/unit/managers/test_schedule_policy.py#L97)은 같은 입력 길이라도 cached 길이에 따라 순서가 달라짐을 검사한다. 무한 arrival에서 starvation이 없다는 증명은 아니다.
- [test/registered/unit/managers/test_scheduler_chunked_abort_race.py:47–65](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/test/registered/unit/managers/test_scheduler_chunked_abort_race.py#L47): abort 표시한 요청이 chunked slot을 이미 떠난 경우에도 실제 request에 abort를 적용하고, 이미 완료한 요청이면 marker만 지우는지 검사한다.

**우리 설계에 적용할 불변식:** queue 위치가 바뀌어도 request ID에 묶인 취소는 사라지면 안 된다. 부모 root의 자식 요청 수가 늘어도 새 공정 배분 주체가 생기면 안 된다. admission 직전/직후 취소를 같은 fixture에서 바꿔 실행해, 아직 송신하지 않은 요청의 upstream attempt가 0인지 확인한다.

**가져오지 않을 것:** prefix를 알고 있는 요청만 계속 먼저 보내면 cold 요청이 손해를 본다. 현재 v0.1의 root RR+root FIFO를 LPM/HRRN으로 대체할 근거는 확보하지 않았다. prefix namespace 관련 필드와 body를 보존하는 것은 유효하나 게이트웨이가 그 적중을 보장하지 않는다.

## 3. LMCache: HTTP 응답 캐시가 아닌 엔진 KV 전송 계층

주 경로는 `LMCacheMPConnector`와 multiprocess server다. `vllm_v1_adapter.py`의 abort 회귀는 별도 integration 경로로 표시한다. MP와 기존 V1 구현을 하나의 호출 체인으로 섞지 않았다.

| 단계 | 실제 경로와 관찰 | 고정 소스 |
|---|---|---|
| engine admission hook | vLLM scheduler가 `get_num_new_matched_tokens`를 호출한다. lookup 미완료는 `(None, True)`, miss는 `(0, False)`다. 로컬·LMCache 적중 차이만 load하고 full-prompt hit에서도 마지막 token을 남긴다. | [lmcache/integration/vllm/lmcache_mp_connector.py:977–990,1048–1090](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/lmcache/integration/vllm/lmcache_mp_connector.py#L1048) |
| 할당 후 상태 | async load로 같은 요청의 `update_state_after_alloc`이 두 번 올 수 있다. 전체 block 목록을 매번 append하면 중복되므로 이미 추적한 개수 이후 block만 추가한다. | [lmcache_mp_connector.py:1111–1147](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/lmcache/integration/vllm/lmcache_mp_connector.py#L1111) |
| prefix key·lookup | 완성 chunk의 hash는 이전 prefix hash와 token IDs를 묶어 계산한다. server lookup은 모델/layout를 확인하고 storage prefetch job을 등록한다. 모델 GPU context가 없으면 해당 lookup을 early-exit로 기록한다. | [lmcache/v1/multiprocess/token_hasher.py:183–230](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/lmcache/v1/multiprocess/token_hasher.py#L183), [lmcache/v1/multiprocess/modules/lookup.py:146–204](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/lmcache/v1/multiprocess/modules/lookup.py#L146) |
| KV 이동 | worker hook는 실제 paged KV buffer와 GPU completion event를 사용해 batched retrieve/store를 제출한다. server transfer는 CPU↔GPU block IDs와 IPC event를 받고 복사 후 callback으로 read lock을 반환한다. | [lmcache_mp_connector.py:742–784,829–861](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/lmcache/integration/vllm/lmcache_mp_connector.py#L742), [modules/lmcache_driven_transfer.py:1039–1072,1287–1316,1480–1503](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/lmcache/v1/multiprocess/modules/lmcache_driven_transfer.py#L1287) |
| 요청 종료와 전송 완료 | `request_finished`는 tracker를 정리하고 END_SESSION 뒤 lookup state를 지운다. 일반 경로는 True를 반환해 block 해제를 늦추며 worker `get_finished`가 save/receive 완료 ID를 돌려준다. lazy offload는 별도 block 소유 방식으로 False를 반환한다. | [lmcache_mp_connector.py:1250–1300,892–913](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/lmcache/integration/vllm/lmcache_mp_connector.py#L1250) |
| 늦은 lookup 정리 | adapter의 `end_session`은 미수신 lookup ACK와 제출한 status 조회를 drain한 뒤 서버에 END_SESSION을 보낸다. 순서가 뒤집히면 늦은 handler가 세션을 다시 만들 수 있다는 계약이다. cleanup은 pending/unacked/status/result 등 request별 자료를 제거한다. | [lmcache/integration/vllm/vllm_multi_process_adapter.py:1001–1012,1068–1105](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/lmcache/integration/vllm/vllm_multi_process_adapter.py#L1068) |
| 공유 lock·관측 | `free_lookup_locks`는 prefetch가 실제로 잠근 hit chunk/group과 reader 수만 반환한다. END_SESSION은 session 제거와 request-end event를 발행한다. 이 token/event 회계는 KV 이동·적중 관측이며 upstream RPM/TPM 정산이 아니다. | [modules/lookup.py:462–529](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/lmcache/v1/multiprocess/modules/lookup.py#L462) |

LMCache는 여러 요청의 최종 답변을 저장해 돌려주는 서비스가 아니다. 여기서 prefix key는 전체 이전 token 문맥을 반영하고 값은 모델 내부 KV다. README에는 비-prefix 재사용/CacheBlend도 있지만, 그 기능까지 이번 MP prefix 경로의 결과로 일반화하지 않았다. model/layout/worker/GPU event와 엔진 hook이 필요하므로 회사 API base URL만 지정한 Rust proxy가 이 계층을 흉내 낼 수 없다.

**읽은 upstream 테스트:**

- [tests/v1/multiprocess/test_free_locks.py:141–163](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/tests/v1/multiprocess/test_free_locks.py#L141)는 lookup lock 해제가 storage `finish_read_prefetched`로 전달되는지 검사한다. [232–244행](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/tests/v1/multiprocess/test_free_locks.py#L232)은 4개 chunk 중 2개만 hit했을 때 실제 잠긴 2개만 반환하는지 검사한다. 남의 공유 자원을 반환하지 않는 불변식이다.
- [tests/v1/test_v1_adapter_request_finished_scheduler_role.py:174–226](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/tests/v1/test_v1_adapter_request_finished_scheduler_role.py#L174)는 **별도 V1 adapter**에서 storage engine이 없어도 scheduler가 소유한 async lookup 취소는 실행되고, 둘 다 있으면 각각 취소하는지 검사한다. 구현은 [vllm_v1_adapter.py:1866–1903](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/lmcache/integration/vllm/vllm_v1_adapter.py#L1866)에 있다. 한 cleanup 대상의 부재가 다른 대상의 정리를 막지 않아야 한다.

**우리 설계에 적용할 불변식:** request별 실제 획득한 body permit, queue membership, execution slot, quota reservation을 구분하고 획득한 것만 한 번 반환한다. 첫 요청이 취소된 뒤 늦은 usage가 도착해도 다음 요청의 장부를 건드리지 않는다. terminal 이전에 주고받던 관측 이벤트가 끝난 요청을 다시 runnable로 만들지 않는다. upstream 시도 ID와 logical client request 식별 힌트를 섞지 않는다.

**가져오지 않을 것:** LMCache의 cache miss → engine recompute는 동일 엔진 실행 경로 안의 복구다. HTTP POST 실패 → 재요청과는 중복 작업 위험이 다르다. 이 cache 복구를 근거로 모호한 API 오류를 자동 replay하지 않는다. MP 서버·storage tier·Torch·CUDA 연동을 설치 없이 동작해야 하는 gateway의 필수 런타임으로 추가하지 않는다.

## 4. llm-d Router: 대기 입장과 endpoint 배치를 나눈 요청 라우터

[README.md:9–17,57–64](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/README.md#L9)에 따라 canonical 프로젝트명은 **llm-d Router**다. 이전 이름인 Inference Scheduler와, EPP 내부의 Request Scheduler를 구분한다. Router는 proxy+EPP, EPP는 endpoint 결정 컴포넌트다.

| 단계 | 실제 경로와 관찰 | 고정 소스 |
|---|---|---|
| admission 이전/이후 경계 | Director가 fairness ID/priority를 정한 뒤 `Admit`을 호출한다. 통과 후 endpoint candidates·screeners·data producers·admission plugins를 거쳐 `scheduler.Schedule`로 배치한다. queue admission 성공은 모델 실행 완료가 아니다. | [pkg/epp/requestcontrol/director.go:245–287](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/requestcontrol/director.go#L245) |
| flow queue | FlowControlAdmissionController가 request byte size·fairness ID·priority·TTL을 넣어 `EnqueueAndWait`한다. controller는 queue count/bytes를 쌍으로 증감하고 분배·대기 수명 동안 flow lease를 유지한다. cancel/TTL/shutdown/dispatch 중 하나가 finalization을 결정한다. | [pkg/epp/requestcontrol/admission.go:159–207](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/requestcontrol/admission.go#L159), [pkg/epp/flowcontrol/controller/controller.go:235–309,418–445](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/flowcontrol/controller/controller.go#L235) |
| 공정 순환·배치 | 같은 priority band의 flow를 정렬하고 cursor 다음부터 비어 있지 않은 queue를 선택하는 RR 구현이 있다. processor는 saturation/usage ceiling을 먼저 검사하고 선택한 queue head를 remove→Finalize로 넘긴다. queue 간 선택과 queue 안 순서가 분리된다. | [pkg/epp/framework/plugins/flowcontrol/fairness/roundrobin/roundrobin.go:93–127](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/framework/plugins/flowcontrol/fairness/roundrobin/roundrobin.go#L93), [pkg/epp/flowcontrol/controller/internal/processor.go:497–538,608–637](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/flowcontrol/controller/internal/processor.go#L497) |
| KV 기반 배치 | precise prefix producer가 token/block index와 engine KV-event adapter, subscriber를 시작한다. 요청 block key의 endpoint별 적중을 조회해 prefix scorer에 전달한다. score는 matched/total blocks 및 선택한 match-length 가중치다. | [pkg/epp/framework/plugins/requestcontrol/dataproducer/preciseprefixcache/producer.go:148–194,287–331](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/framework/plugins/requestcontrol/dataproducer/preciseprefixcache/producer.go#L148), [pkg/epp/framework/plugins/scheduling/scorer/prefix/plugin.go:130–173](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/framework/plugins/scheduling/scorer/prefix/plugin.go#L130) |
| 대기 취소 | context 취소나 TTL은 item을 finalize하고 processor sweep이 남은 큐 항목을 치운다. `sync.Once`가 final outcome·metrics·Done 발행을 보호한다. 여기서 finalized=dispatched인 경우 upstream 실행은 이후 단계다. | [controller.go:418–445](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/flowcontrol/controller/controller.go#L418), [pkg/epp/flowcontrol/controller/internal/item.go:132–181](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/flowcontrol/controller/internal/item.go#L132) |
| 실행 중 퇴출 | evict channel을 한 번 닫고 handler가 Envoy ImmediateResponse(429)를 보낸다. upstream reset을 의도한 경로다. request token/KV를 보존해 재개하는 engine preemption이 아니다. reclamation은 stream 종료 확인 뒤 grace를 둬 abort·KV GC·scrape 지연이 반영될 시간을 준다. | [pkg/epp/flowcontrol/eviction/evictor.go:55–90](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/flowcontrol/eviction/evictor.go#L55), [pkg/epp/handlers/server.go:714–739](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/handlers/server.go#L714), [pkg/epp/flowcontrol/controller/internal/reclamation.go:34–93](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/flowcontrol/controller/internal/reclamation.go#L34) |
| 완료·usage | response chunk parser의 usage를 누적 구조로 merge하고 EOF에서 latency 등을 기록한다. Anthropic의 prompt/output usage가 서로 다른 이벤트에 오는 경우를 명시한다. 정상 종료뿐 아니라 오류/disconnect에서도 cleanup context로 completion hook을 호출한다. | [pkg/epp/handlers/response.go:74–124](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/handlers/response.go#L74), [server.go:409–421,656–668](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/handlers/server.go#L409) |

**선택 기능과 기본값을 구분한다.** [flowcontrol/README.md:35–58](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/flowcontrol/README.md#L35)은 flowControl이 기본 off인 opt-in이라고 설명한다. off 경로는 sheddable 요청에만 saturation 거절을 적용한다([admission.go:89–130](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/requestcontrol/admission.go#L89)). 클론에 코드가 있다는 이유로 설치 기본값에 전부 활성화됐다고 볼 수 없다.

**stale metric 정책도 조건부다.** utilization detector는 각 endpoint의 `max(waitingQueue/threshold, KVusage/threshold)`를 평균한다. 기본 `saturated`에서는 missing/stale가 1.0을 기여하고, `ignore`에서는 제외되며 전부 stale이면 0.0으로 dispatch가 계속된다. Filter가 전부 제거한 경우 원래 endpoint 집합을 반환하는 경로도 있다. 따라서 “llm-d는 모든 stale 상태에서 항상 입장을 막는다”는 결론은 틀리다. 근거: [utilization/detector.go:120–181,203–230](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/framework/plugins/flowcontrol/saturationdetector/utilization/detector.go#L120), [detector_test.go:150–155](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/framework/plugins/flowcontrol/saturationdetector/utilization/detector_test.go#L150). 이 결론은 concurrency detector 등 다른 policy 전체의 성질로 확장하지 않는다.

**공정성과 예측의 구분:** RR은 flow별 다음 입장 기회에 관한 정책이다. 별도 program-aware LAS는 완료 응답의 prompt·completion token에 가중치를 줘 attained service를 축적하고 시간에 따라 decay한다([strategy_las.go:20–21,34–66,168–177](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/framework/plugins/flowcontrol/fairness/program-aware/strategy_las.go#L168)). 이는 quota의 법칙이나 미래 output 예측기가 아니다. 아직 완료되지 않은 긴 요청의 실제 token 비용과 missing usage의 영향은 별도 실험이 필요하다.

**읽은 upstream 테스트:**

- [pkg/epp/flowcontrol/controller/internal/item_test.go:63–155](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/flowcontrol/controller/internal/item_test.go#L63) `TestFlowItem_Finalize_Idempotency`: TTL/cancel/dispatch/capacity reject 순서를 바꿔도 첫 terminal outcome이 유지되는지 검사한다. engine 완료가 아닌 queue lifecycle의 idempotency 테스트다.
- [pkg/epp/flowcontrol/controller/internal/reclamation_test.go:213–232](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/pkg/epp/flowcontrol/controller/internal/reclamation_test.go#L213): confirmation이 없으면 timeout 뒤에도 grace가 끝나야 다음 reclaim gate가 열리고, 이후 늦은 confirmation이 gate를 다시 닫지 않는지 fake clock으로 검사한다. 실제 engine GPU 해제 확인 테스트는 아니다.

**배포 전제:** README의 standalone은 Gateway API 기반 공유 Gateway가 없어도 proxy+EPP로 배치할 수 있다는 뜻이다. 검토한 공식 standalone Helm 경로는 EPP pod, Envoy/Agentgateway proxy, 모델 서버 endpoint를 전제로 하고 chart 문서는 cluster의 CRD 설치를 안내한다([README.md:35–46](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/README.md#L35), [config/charts/README.md:7–28](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/config/charts/README.md#L7)). 임의의 cluster-free custom 배치가 불가능하다고 증명한 것은 아니지만, 한 PC 무권한 binary 하나와 같은 설치 계약은 아니다. [go.mod:1–75](https://github.com/llm-d/llm-d-router/blob/6493998a3735ccf1b07450fe6308c9ec5dd929e1/go.mod#L1)도 Envoy control plane, Kubernetes/Gateway API/controller-runtime을 직접 의존한다.

## 5. 현재 제품으로 옮길 실패 테스트

아래는 **제안한 mock 테스트**이며 이번 조사에서 구현·실행하지 않았다. 기존 승인 명세의 동작을 강화하는 사례다. 제품 성능 비교는 [벤치마크 명세](benchmark-spec.md)의 direct/FIFO/RR baseline, 동일 workload·quota·sampling, 모든 outcome 분모 원칙을 유지한다.

| ID | 소스에서 얻은 위험 | 최소 fixture와 판정 |
|---|---|---|
| E01 | vLLM·LMCache의 완료/자원 해제 분리 | upstream이 첫 SSE를 보낸 뒤 completion gate에서 대기. client socket을 끊어도 bounded-drain 정책의 실행 slot은 유지. gate가 열린 EOF 또는 전체 deadline에 한 번만 반환. queued client의 새 attempt가 먼저 시작하면 실패 |
| E02 | vLLM 늦은 출력, llm-d first finalization | 같은 request에 cancel·EOF·deadline·late usage 순서를 바꾼다. terminal reason은 한 번 확정, 실행 slot·body permit 한 번 반환. admitted/attempt/terminal counters의 관계를 검사 |
| E03 | SGLang chunked abort race | queue head 선정과 upstream 송신 사이에 취소를 넣는다. 송신 전 terminal이면 attempt=0. 이미 송신된 경우 단순 queued cancel처럼 예약을 모두 되돌리지 않는다 |
| E04 | LMCache가 잠근 자원만 반환 | A는 body permit만 획득하고 parse 실패, B는 queue만, C는 실행 slot까지 획득. A/B cleanup으로 C slot 또는 다른 요청 body budget이 증가하면 실패. duplicate terminal은 무해해야 함 |
| E05 | cleanup 뒤 늦은 이벤트가 상태 재생성 | A 취소 뒤 B를 입장시키고 A usage를 늦게 전달한다. A는 runnable로 부활하지 않고 B quota를 덮어쓰지 않음. late-known accounting 허용 여부는 승인 장부 계약대로 한 번만 정산 |
| E06 | root fairness와 비용 fairness 차이 | A root에 child 15개, B/C/D에 1개씩 burst. 등록 root 수와 route는 고정, root RR 순서와 root FIFO 확인. 서로 다른 실제 token 서비스량도 같이 기록하며 동등한 GPU time으로 해석하지 않음 |
| E07 | cache-aware 우회가 cold/큰 요청에 주는 손해 | 큰 요청이 기다리는 동안 작은 요청을 계속 넣고 heavy→light→heavy / heavy→heavy→light를 모두 실행. 승인된 우회/기아 방지 기준을 넘기면 실패. cache-hit 필드만 바꿔도 local TPM 예약이 임의 할인되면 실패 |
| E08 | usage partial/missing, token 계산과 chunk 차이 | Anthropic start/delta를 별도 chunk, 한 event를 여러 TCP chunk, usage 없는 EOF, observer event 초과를 준다. raw bytes 유지, protocol별 usage merge, unknown 비율 기록. SSE chunk 개수를 completion token으로 세면 실패 |
| E09 | 짧은 요청도 실행 중 요청을 선점 못함 | 동시성 1에서 긴 요청을 먼저 upstream에 시작한 뒤 짧은 요청을 넣는다. 긴 요청 완료 전 짧은 요청이 완료된다고 주장하지 않음. 큐 wait 감소와 이미 점유된 실행 slot은 구분 |
| E10 | 거절/퇴출이 재시도를 증폭 | 공유 429 cooldown 중 다른 root와 client retry를 넣는다. 여러 alias도 하나의 group cooldown/RPM/TPM을 사용. timeout·부분 stream·모호한 disconnect를 gateway가 replay하면 실패. 원본 API가 쓰지 않은 quota를 환불했다고 가정하지 않음 |

E01/E08/E10은 실제 loopback TCP fixture가 필요하다. iterator `close()`만으로 socket disconnect와 drain 수명을 입증하지 않는다. mock의 server-side gate가 알려주는 계산 상태는 fixture의 사실이며, 회사 API의 보이지 않는 계산 상태까지 입증하지 않는다.

## 6. HTTP만으로 가능한 것과 불가능한 것

| 기능/주장 | 우리 gateway가 가진 정보 | 추가로 필요한 정보·제어 | 현재 판정 |
|---|---|---|---|
| root별 요청 입장 순환 | 고정 route, enqueue 시각, deadline | 없음 | 구현·mock 검증 가능 |
| 공유 RPM/TPM 보수적 입장 | 설정한 quota 계약, 요청 비용 추정, 응답 usage/header | 외부 소비자의 숨은 요청과 provider 집계 규칙은 관측 불가 | 선택한 계약에서 보장 범위를 한정 |
| 실제 token fairness | 일부 endpoint의 usage | usage 완전성, 아직 완료되지 않은 요청 비용, provider counting semantics | 입장 fairness와 별도로 보고 |
| prefix KV 적중 | body·헤더와 사후 cached-token 필드 일부 | 동일 tokenizer/template/model/adapter/salt, engine block event·eviction 상태 | 적중률·예측 latency 보장 불가 |
| 실행 중 token/GPU 선점 | HTTP socket | engine scheduler hook, token IDs·KV·in-flight kernel/transfer 소유권, resume 계약 | v0.1 지원 밖 |
| socket close 뒤 계산 종료 | local socket 종료 | backend abort 처리와 engine 작업 종료 신호 | source 분석만으로 보장 불가 |
| 엔진별 GPU/메모리 포화 | local PC CPU/RSS와 HTTP 시간 | upstream KV usage·waiting queue·metrics freshness | upstream TTFT를 GPU 부하로 단정 불가 |
| 최종 응답 캐시 | request/response bytes를 저장하면 가능 | tool 상태·scope·sampling·품질·개인정보 정책과 별도 검증 | v0.1 제외, prefix cache 효과와 합산 금지 |

세 종류의 자원을 섞지 않는 것이 핵심이다. **queue/body 공간**은 로컬 소유권으로 정확히 반환할 수 있다. **upstream concurrency slot**은 정한 drain/close 계약에 따라 수명을 관리한다. **provider RPM/TPM**은 시간이 지남에 따라 회복하는 사용량 장부라 작업 취소만으로 되살아나는 메모리 permit이 아니다. 이 구분은 위 엔진의 자원 반환 절차에서 얻은 설계 추론이며, provider의 실제 과금 규칙을 확인한 결과가 아니다.

## 7. 설치·검증·미확인 범위

- 네 저장소의 README와 root manifest를 읽었다. vLLM [pyproject.toml:1–39](https://github.com/vllm-project/vllm/blob/a0844fa6c6d6e1143917a033ad6984e40c212862/pyproject.toml#L1)는 Python/Torch 및 native build 도구를, SGLang [python/pyproject.toml:1–112](https://github.com/sgl-project/sglang/blob/e91c94805747b60d0b8e8012f3647ae82d4ae862/python/pyproject.toml#L1)는 Python/Torch와 CUDA·kernel 계열 의존성을 선언한다. CPU/다양한 hardware backend가 있다는 README 설명과, 선택한 기본 manifest가 가벼운 Rust HTTP proxy가 아니라는 사실을 구분한다. “모든 엔진이 GPU 없이는 실행 불가”라고 결론내리지 않았다.
- LMCache [pyproject.toml:1–55](https://github.com/LMCache/LMCache/blob/588c5366f39cf1fbb21a614074aefdf942573d93/pyproject.toml#L1)는 Python/Torch 및 엔진 확장 설치를 선언하며 이번에 읽은 MP transfer 경로는 GPU block/event를 요구한다. CPU/source-only 등의 다른 설치 옵션을 실행해보지 않았다. llm-d manifest와 chart 전제 역시 문서·소스 확인이며 배포 검증은 아니다.
- 원본 test dependencies를 설치하거나 GPU를 시작하지 않았다. 테스트 수·pass 수·성능 수치를 만들지 않았다. 참조 저장소의 구현 코드도 제품으로 복사하지 않았다.
- 선택한 source SHA는 조사 시점의 checkout이며 배포할 stable version·조합 compatibility matrix가 아니다. vLLM↔LMCache↔SGLang 모든 조합의 동작은 미확인이다. 각 저장소의 공개 API/내부 ABI가 바뀔 수 있어 source 그대로 package를 가져오는 전략은 현재 범위에 맞지 않는다.
- 이번 분석은 모델 품질, 실사용 task goodput, 사내 API abort semantics, 숨은 과금, Windows/Linux native execution, 저사양 PC RSS/CPU, 실제 사용자 수요를 증명하지 않는다.
- 다음 구현 검증은 E01–E10 중 승인된 core chunk에 해당하는 fixture부터 수행한다. engine telemetry가 없는 첫 버전에서는 root RR·bounded queue·단일 quota 장부·보수적 unknown accounting을 먼저 완성한다. cache-aware route나 엔진 선점 도입은 독립 요구와 관측 가능한 이득이 생겼을 때 다시 판단한다.
