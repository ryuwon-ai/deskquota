# Gateway 소스 심층 검토: 작은 Rust 코어에 가져올 것과 버릴 것

검토일: 2026-09-12. 목적은 한 PC의 코딩 에이전트들이 부족한 공급자 RPM/TPM을 공유하는 네이티브 Rust gateway의 구현 근거를 찾는 것이다. 프로토콜 변환·관리 콘솔·전체 프롬프트 저장을 제품 범위에 추가하는 조사가 아니다.

**가장 직접적인 근거는 TensorZero의 예약/정산 분리, Otari의 한 번만 정산하는 상태 전이, 공통 HTTP client 재사용이다. 반대로 detached task와 unbounded channel, 전체 stream 수집, SDK·gateway·client의 중첩 재시도는 그대로 채택하지 않는다.** 스트리밍 응답을 빨리 반환한다는 사실과 메모리가 응답 길이에 비례하지 않는다는 사실은 다르다.

## 검토 범위와 증거 수준

| 저장소 | 직접 확인한 HEAD / 소스 기준 | 라이선스·상태와 해석 |
|---|---|---|
| TensorZero | `62eb8f63e8ec62018d70420dbf1a8c5d1c026315` | root `LICENSE:1` Apache-2.0. 조사 manifest는 archived=true, commit 2026-06-04를 기록. 유지보수 중인 제품 추천이 아니라 고정된 구현 사례로 사용 |
| Helicone ai-gateway | `9649b27bdc9fb0907d359e899894102a15f3a085` | root `LICENSE:1` GPL-3.0. `README.md:8` Apache badge 및 `Cargo.toml:19` Apache-2.0와 불일치. manifest의 GPL-3.0 표기와 실제 LICENSE를 기록하고 코드 재사용은 하지 않음. commit 2025-11-20, archived=false만으로 활발한 유지보수를 뜻하지 않음 |
| Portkey gateway | `669825cbe89ee51569918b8f78a9db486fd69dd4` | root `LICENSE:1` MIT, `package.json:3` 1.15.2. `README.md:7`은 별도 2.0.0 pre-release branch를 안내. 이 분석은 main의 해당 SHA이며 2.0 또는 hosted enterprise 전체의 분석이 아님 |
| any-llm | `9b3448ff8ef7953877758f6b618720cee59a74e8` | root LICENSE Apache-2.0, `pyproject.toml:6` 이름은 any-llm-sdk. HTTP gateway가 아닌 Python provider SDK. `README.md:107-109`는 gateway 기능을 별도 Otari로 안내 |
| Otari, 추가 한정 검토 | `706269043c755720de82c07057671cbef0a365ae` | root LICENSE Apache-2.0. 실제 gateway의 admission/정산/stream 취소/의존성만 추가 검토. 모든 endpoint·routing·관리 기능은 분석하지 않음 |

체크아웃 출처·commit 날짜·수집 시각·sparse 범위는 [repositories.json](/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/evidence/repositories.json)에 고정했다. 위 HEAD와 clean status는 각각의 실제 하위 Git 저장소에서 조회했다. 라이선스는 파일 표기 확인이며 배포·파생물 법률 판단이 아니다.

- **직접 검증:** 위 다섯 체크아웃의 HEAD·clean 상태, 파일 존재, 아래 함수·분기·테스트 본문과 manifest 내용.
- **코드/문서 근거:** 아래 호출 경로, 정산 정책, 의존성·설치 대상. 해당 SHA의 소스를 읽은 결과다.
- **추론:** 우리 제품에 대한 채택/기각, 누락된 fixture 제안, 느린 소비자에서의 메모리 증가 가능성. 측정 결과가 아니다.
- **미확인:** 이 보고서의 원본 테스트는 실행하지 않았다. 빌드·패키지 설치·Docker·실제 모델 호출·성능 비교·TCP 단절 실험도 하지 않았다. 원본 테스트가 존재한다는 사실을 통과나 운영 증명으로 쓰지 않는다.

아래 `파일:행`은 각 절에 명시한 reference 루트 기준이며 그 절의 전체 SHA에 고정된다. README의 처리량·지연·메모리 홍보 수치는 결론에서 사용하지 않았다.

## 1. TensorZero

소스 루트: [references/tensorzero](/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/references/tensorzero). 고정 [commit](https://github.com/tensorzero/tensorzero/tree/62eb8f63e8ec62018d70420dbf1a8c5d1c026315).

| 구간 | 실제 호출·분기 | 고정 소스 근거 |
|---|---|---|
| ingress | Axum `/inference`와 OpenAI 호환 routes 등록 → `inference_handler`가 공유 AppState의 client/cache/rate manager를 받음 → `inference` → JSON 또는 SSE response | `crates/gateway/src/routes/external.rs:24-46`; `crates/tensorzero-core/src/endpoints/inference.rs:208-265` |
| 요청·재시도 | variant의 `retry_collecting_errors` → `model_config.infer_stream`. 기본 retry 횟수는 0. 활성화하면 retryable 오류만 exponential backoff+jitter. model은 provider 순회 및 첫 chunk 확인을 수행하므로 variant retry와 provider fallback은 서로 다른 층 | `crates/tensorzero-core/src/variant/mod.rs:1008-1041`; `crates/tensorzero-core/src/utils/retries.rs:40-105`; `crates/tensorzero-core/src/model.rs:1028-1140` |
| cache·예약 | streaming provider 요청은 cache read부터 수행. hit면 provider 호출/예약을 건너뜀. miss면 provider inference가 tickets를 선소비. 반환 후 wrapper가 실제 usage로 정산 | `crates/tensorzero-core/src/model.rs:805-871,2789-2797,3038`; `crates/tensorzero-core/src/rate_limiting/rate_limiting_manager.rs:143-188` |
| HTTP transport | `TensorzeroHttpClient`가 reqwest client들을 공유. HTTP/2 동시 stream 제한 회피용 waterfall은 client당 임계값 100으로 별도 연결을 늘리는 정책 | `crates/tensorzero-http/src/lib.rs:115-169`; `crates/tensorzero-http/src/lib.rs:754-803` |
| SSE 수신 | request send → HTTP status/content-type 검사 → `SseStream::from_byte_stream(response.bytes_stream())`. 이 자체에는 재접속 retry loop가 없음 | `crates/reqwest-sse-stream/src/lib.rs:72-120,131-151` |
| timeout | 첫 chunk까지의 TTFT deadline과 전체 deadline 중 빠른 것을 적용하고, 첫 chunk 이후에는 남은 전체 deadline을 stream에 적용 | `crates/tensorzero-core/src/model.rs:1077-1140` |
| 종료·취소 소유권 | provider wrapper의 별도 task가 upstream을 계속 읽고 unbounded channel로 전달. receiver가 사라져도 send 오류를 무시하고 drain하여 사용량 정산. fatal body error면 읽기를 멈춤 | `crates/tensorzero-core/src/model.rs:1312-1326,1336-1391` |
| 전달과 저장 | provider wrapper는 cache/content capture 시 chunk를 모음. 별도로 endpoint `create_stream`은 전달하면서도 모든 chunk clone을 Vec에 저장하고 끝에서 aggregation·observability write | `crates/tensorzero-core/src/model.rs:1288-1309`; `crates/tensorzero-core/src/endpoints/inference.rs:1232-1295,1434-1486` |
| usage | 단일 stream의 필드별 누적 최대값을 취함. OpenAI 마지막 usage, Anthropic 부분 누적 usage를 구분하는 설명이 있음. 필드가 끝까지 없으면 None 유지 | `crates/tensorzero-core/src/inference/types/usage.rs:54-103` |

예약과 실제 사용량의 구분은 작은 gateway에도 유효하다. `crates/tensorzero-core/src/rate_limiting/rate_limiting_manager.rs:286-307`은 실제가 예약을 넘으면 추가 소비하고, 적으면 **Exact만 환급**하며 UnderEstimate는 환급하지 않는다. `crates/tensorzero-core/src/model.rs:1338-1352`는 에러 또는 누락된 사용량을 UnderEstimate로 넘긴다. 공급자 usage가 없거나 stream이 끊긴 것을 0 사용으로 기록하지 않는 설계 근거다.

다만 `crates/tensorzero-core/src/rate_limiting/mod.rs:842-848`의 입력 추정은 `text.len()/2`다. Rust `str.len()`은 UTF-8 바이트 수이므로 주석의 “2 characters”와 동일한 단위가 아니다. 주석 자체도 hard bound가 아니라고 밝힌다. 한국어·코드·tool schema·이미지를 모두 엄밀한 TPM 상한으로 계산해 준다고 해석하면 안 된다. 실제로 `crates/tensorzero-core/src/inference/types/mod.rs:998-1003`는 tool_config를 추정 입력에서 제외하고 TODO를 남긴다.

운영 무게도 구분해야 한다. `crates/tensorzero-core/src/rate_limiting/rate_limiting_manager.rs:60-118`은 활성 규칙에 Valkey 또는 Postgres backend를 요구하며 둘 다 없으면 오류다. `crates/tensorzero-core/src/cache.rs:54-120`의 cache는 ClickHouse/Valkey 경로다. 전체 제품의 저장·관측 구조를 복제하면 로컬 quota 조정기보다 훨씬 큰 제품이 된다.

**채택:** 예약 영수증과 실제 정산 분리, 사용량 미확인 상태, retry 기본 0, 연결 재사용, TTFT/전체 timeout 구분, 중간 transport 오류를 fatal로 분류. **기각:** 현재 요구에 불필요한 H2 waterfall pool, unbounded channel, prompt/response 전문 보관, DB 서비스 필수 quota, 프로토콜 재구성.

| 읽은 원본 테스트 | 실제 assertion과 한계 |
|---|---|
| `crates/tensorzero-core/tests/e2e/streaming_errors.rs:352-461` | 로컬 TLS H2 server가 첫 body 이후 RST_STREAM을 보내게 하고 실제 reqwest body 오류를 얻어 production error converter의 FatalStreamError 분류를 검사. 전체 gateway TCP 취소/메모리/정산을 한 번에 검사하는 테스트는 아님. e2e feature·TLS 설정이 필요하며 미실행 |
| `crates/tensorzero-core/src/rate_limiting/rate_limiting_manager.rs:586-620` | 100 예약 후 50 actual 반환 경로가 Ok인지 검사. 이 테스트 본문의 assertion만으로 refund 차액·중복 정산·경합 안전까지 입증하지 못함 |
| `crates/tensorzero-python/tests/test_drop_stream.py:39-85` | embedded sync/async stream을 일부 읽고 GC 시 미완료 경고가 나는지 검사. 실제 HTTP disconnect 후 upstream 생명주기와 동일한 테스트가 아님 |

## 2. Helicone ai-gateway

소스 루트: [references/helicone-ai-gateway](/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/references/helicone-ai-gateway). 고정 [commit](https://github.com/Helicone/ai-gateway/tree/9649b27bdc9fb0907d359e899894102a15f3a085).

| 구간 | 실제 호출·분기 | 고정 소스 근거 |
|---|---|---|
| ingress | Hyper service → MetaRouter에서 `/ai`, router, direct proxy 분기. router별 cache/rate limit/request context/strategy stack을 구성 | `ai-gateway/src/router/meta.rs:105-165`; `ai-gateway/src/router/service.rs:45-85` |
| HTTP client | Dispatcher 생성 때 provider client 생성·보관. reqwest connect timeout/total timeout/tcp_nodelay 설정. request마다 새 client를 만드는 함수 구조가 아님 | `ai-gateway/src/dispatcher/service.rs:67-88`; `ai-gateway/src/dispatcher/client.rs:181-226` |
| retry | sync는 응답 Result를 보고 retry, stream은 `dispatch_stream` 초기화 실패를 backon으로 retry. retry config가 있으면 기본 max-retries는 2이며 전체 gateway의 무조건 기본값이라는 뜻은 아님 | `ai-gateway/src/dispatcher/service.rs:723-846,849-910`; `ai-gateway/src/config/retry.rs:76-85` |
| SSE | reqwest-eventsource 생성 → 첫 event를 await → detached task가 후속 이벤트를 unbounded channel에 넣음. 첫 event는 콘텐츠 chunk가 아닌 Open 이벤트일 수 있음 | `ai-gateway/src/dispatcher/client.rs:161-177,245-315` |
| 종료·취소 | `[DONE]` 또는 StreamEnded에서 break. receiver drop은 다음 `tx.send` 실패 때 감지해 break 후 EventSource.close. 조용한 upstream에서 receiver 종료를 동시에 기다리는 select는 이 loop에 없음 | `ai-gateway/src/dispatcher/client.rs:270-308` |
| 로깅 tee | 사용자 response body가 poll될 때 각 Bytes clone을 두 번째 unbounded channel로 전달. LoggerService는 이 body를 collect해 S3/sidecar logging 경로로 넘김 | `ai-gateway/src/types/body.rs:45-67`; `ai-gateway/src/logger/service.rs:83-110` |
| 로컬 rate limit | per_api_key GCRA의 capacity/refill frequency, InMemory 또는 Redis backend. 이 경로에서 요청 전 TPM 추정·실제 token 정산은 확인되지 않음 | `ai-gateway/src/config/rate_limit.rs:39-58,120-124,177-179`; `ai-gateway/src/middleware/rate_limit/service.rs:114-121` |
| provider 429 | response headers의 Retry-After를 숫자 초 또는 HTTP-date로 읽어 rate-limit monitor에 전달 | `ai-gateway/src/dispatcher/service.rs:477-497,955-981` |
| cache | InMemory/Redis. cacheable miss response는 `body.collect()` 후 cache에 put하고 response 재생성. streaming time-to-first-byte가 동일하다고 가정하면 안 됨 | `ai-gateway/src/config/cache.rs:31-57`; `ai-gateway/src/middleware/cache/service.rs:414-467` |

취소에는 두 개의 채널이 있다. upstream SSE task는 소비자보다 앞서 읽을 수 있고, logger tee는 사용자 response poll에 종속된다. TensorZero처럼 receiver 오류를 무시하며 끝까지 정산을 보장하는 정책과 다르다. 조용한 upstream에서 disconnect 직후 task가 언제 종료되는지는 실제 socket fixture로 별도 검증해야 한다.

`ai-gateway/src/types/body.rs:51-52`는 상위 concurrency/body size 제한을 이유로 unbounded channel이 괜찮다고 주석을 단다. 그러나 이번에 읽은 app/dispatcher 및 관련 소스 검색에서는 그 주장을 뒷받침하는 response byte hard cap을 확인하지 못했다. 주석만으로 메모리 상한이 보장된다고 보고하지 않는다. logger는 전체 response를 모으므로 streaming API라는 사실만으로 고정 메모리를 뜻하지 않는다.

`Cargo.toml`의 직접 의존성에는 reqwest/tower/governor 외 Redis, SQLx Postgres, AWS Bedrock, telemetry가 포함된다(`ai-gateway/Cargo.toml:18,39-89`). Postgres 연결은 `ai-gateway/src/app.rs:200-209`에서 cloud 모드에 한정되어 sidecar에 무조건 DB가 필요하다고 단정할 수 없다. cache는 메모리 backend도 있고 기본 capacity 설정은 256 MiB지만 실제 RSS나 강제 필요 메모리를 측정한 것이 아니다.

**채택:** 유지되는 provider client, 설정과 transport timeout 분리, Retry-After 두 표기 처리, middleware 역할 분리. **기각:** GPL/Apache 불일치 코드의 복사, 메모리 무제한 channel, logger 전문 저장, request limiter를 TPM 공유 scheduler로 오인, 상위 backon과 SSE library의 retry를 검증 없이 함께 켜기. reqwest-eventsource 내부 재접속 횟수는 이 clone의 소스만으로 확정하지 않았다.

| 읽은 원본 테스트 | 실제 assertion과 한계 |
|---|---|
| `ai-gateway/tests/rate_limit.rs:61-131` | 첫 3개 성공, 4번째 429, Retry-After 존재, 500ms 뒤 재개를 검사. per-user 요청 rate limit 사례이며 RPM+TPM 원자 예약·공정 큐 시험은 아님 |
| `ai-gateway/tests/retries.rs:20-73` | mock upstream 500을 3회 준비하고 최종500/응답 body를 소비. 이 본문은 streaming disconnect 또는 숨은 EventSource retry를 검사하지 않음. mock harness·logging stubs가 필요하며 미실행 |

## 3. Portkey gateway

소스 루트: [references/portkey-gateway](/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/references/portkey-gateway). 고정 [commit](https://github.com/Portkey-AI/gateway/tree/669825cbe89ee51569918b8f78a9db486fd69dd4).

| 구간 | 실제 호출·분기 | 고정 소스 근거 |
|---|---|---|
| ingress | Hono `/v1/chat/completions` → requestValidator → chatCompletionsHandler → JSON body/헤더 config 해석 → `tryTargetsRecursively` | `src/index.ts:133-147`; `src/handlers/chatCompletionsHandler.ts:16-30` |
| provider 호출 | target config 상속·hooks·request mapping → cache lookup → `recursiveAfterRequestHookHandler` → `retryRequest` → fetch | `src/handlers/handlerUtils.ts:366-444,630-635,1209-1225` |
| retry 예산 | async-retry 횟수와 status 설정. after-request hook이 retryable 결과를 만들면 남은 횟수에서 이미 소비한 횟수를 빼고 재귀. 두 구조가 있다고 무조건 곱셈이라고 단정하면 안 되지만 외부 SDK/client retry는 별개 | `src/handlers/retryHandler.ts:84-184`; `src/handlers/handlerUtils.ts:1258-1289` |
| 429 대기 | 설정 시 Retry-After 또는 ms 헤더를 찾고 `parseInt`; 전체 retry 대기 budget을 넘으면 중지. HTTP-date를 직접 해석하는 분기는 없음 | `src/handlers/retryHandler.ts:108-148` |
| timeout·취소 | fetch용 AbortController를 만들지만 fetch가 response headers를 반환하면 timer 해제. 생성한 signal은 custom requestHandler 인자로 전달되지 않음. 일반 fetch options에도 ingress request signal 연결이 보이지 않음 | `src/handlers/retryHandler.ts:4-25`; `src/handlers/handlerUtils.ts:169-192` |
| stream | responseHandler가 성공 stream 분기 → TextDecoder stream mode로 frame 경계까지 부분 buffer → TransformStream writer에 await write | `src/handlers/responseHandlers.ts:71-105`; `src/handlers/streamHandler.ts:140-178,318-389` |
| stream 오류·종료 | loop 예외를 log하고 finally에서 writer.close 시도. 해당 경로에 reader.cancel/releaseLock 또는 upstream AbortController 소유 연결은 없음 | `src/handlers/streamHandler.ts:356-389` |
| cache | root config가 cache=true일 때 memory middleware 연결. body+URL 해시, TTL, stream 저장은 명시적으로 제외. 기본 conf는 false | `src/index.ts:108-110`; `src/middlewares/cache/index.ts:14-25,44-50,60-80`; `conf.json` |
| quota·usage | RedisRateLimiter 유틸은 존재하지만 `rg`로 src 전체를 조사한 결과 정의 파일 외 호출처 없음. stream usage를 provider format으로 전달하는 것과 공유 quota 정산은 별개 | `src/shared/services/cache/utils/rateLimiter.ts:84-188`; `src/handlers/services/logsService.ts:299-306` |

범용 gateway가 커지는 원인을 보여 준다. request builder, provider mapping, hooks, retry, after-hook retry, cache와 log response clone이 한 요청에 관여한다. 현재 제품은 같은 프로토콜을 전달하므로 그 변환·훅 계층을 가져오지 않고 upstream 시도 1회의 수명과 quota receipt를 먼저 완성하는 편이 단순하다.

stream forwarding은 await writer.write로 전달 압력을 받지만, SSE 구분자가 끝없이 오지 않으면 `buffer`는 계속 늘 수 있다. 또한 `src/handlers/services/logsService.ts:302`의 Response.clone은 본문을 tee할 수 있다. 이 clone의 소비 속도·해제 여부를 측정하지 않고 streaming memory가 제한된다고 주장하지 않는다. 현재 Node log handler는 stream이면 placeholder를 사용한다(`src/middlewares/log/index.ts:121-132`); 이것만으로 clone의 전체 생명주기가 증명되지는 않는다.

`package.json:43-58`은 Node/Hono/async-retry/ioredis 등의 기본 dependencies를 보여 준다. Redis 라이브러리가 설치된다는 사실과 Redis 서버를 반드시 운영한다는 사실은 다르다. `README.md`의 npx 실행은 네이티브 UX 참고가 되지만 Rust binary 단독 배포와 동일한 패키징이 아니다. fetch 내부 연결 풀의 실제 재사용 설정은 이 소스에서 확정하지 못했다.

**채택:** retry 시도 수 노출, retry 전체 대기 budget, streaming cache 제외처럼 기능별 지원 경계를 명시하기. **기각:** provider 변환·hook 재귀, timeout을 전체 streaming timeout으로 오인, Retry-After의 숫자 전용 처리, 사용되지 않는 rate-limit 유틸을 운영 기능으로 세기.

| 읽은 원본 테스트 | 실제 assertion과 한계 |
|---|---|
| `tests/integration/src/handlers/tryPost.test.ts:25-36` | 200, body 존재, ReadableStream 타입만 검사. 첫 chunk·마지막 usage·DONE·중간 단절을 소비해서 확인하지 않음 |
| 동일 파일 `:214-244` | 잘못된 credential에 설정한 retry 결과 header=-1/401, 500ms request timeout 결과408 검사. 정확한 wire 시도 수나 header 이후 idle timeout은 검증하지 않음 |
| 동일 파일 `:544-573` | streaming cache 시험은 `it.skip`. 이를 지원 입증으로 세지 않음 |
| `tests/integration/src/handlers/requestBuilder.ts:5-31` | 테스트 fixture가 `.creds.json`과 실제 provider 설정을 사용. 원본 integration은 여기서 실행하지 않음. 로컬 credential 파일은 열지 않음 |

## 4. any-llm: gateway와 구분해야 하는 provider SDK

소스 루트: [references/any-llm](/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/references/any-llm). 고정 [commit](https://github.com/mozilla-ai/any-llm/tree/9b3448ff8ef7953877758f6b618720cee59a74e8).

| 구간 | 실제 경로·존재 여부 | 고정 소스 근거 |
|---|---|---|
| ingress | HTTP server가 아닌 Python `acompletion()` 진입. provider/model 해석 후 AnyLLM.create → llm.acompletion | `src/any_llm/api.py:227-265`; `README.md:107-109,217` |
| client 수명 | 객체 생성 시 `_init_client` → AsyncOpenAI. functional API는 매 호출 create; 같은 provider 객체 또는 주입한 http_client 재사용과 수명이 다름 | `src/any_llm/any_llm.py:169-175`; `src/any_llm/providers/openai/base.py:179-184` |
| request | CompletionParams 검증/정규화 → `_acompletion` → SDK `.parse` 또는 `.create` | `src/any_llm/any_llm.py:764-814`; `src/any_llm/providers/openai/base.py:200-228` |
| stream | SDK AsyncStream을 async iterator로 순회하고 chunk를 정규화해 yield. 전체 response 수집은 이 wrapper에 없음 | `src/any_llm/providers/openai/base.py:129-147,186-197` |
| retry·취소 | AsyncOpenAI에 kwargs 전달. 이 경로는 max_retries를 명시적으로 0으로 만들지 않음. 정확한 SDK 기본 retry 수/connection pool은 선택된 SDK 버전에 종속. wrapper iterator 안에 명시적 `finally: response.close()`는 없음 | `src/any_llm/providers/openai/base.py:179-197`; `pyproject.toml:13` |
| rate·cache·usage | 429 예외 변환에서 Retry-After 원문 보존. provider cached-token 필드와 prompt cache key 지원은 provider 기능이며 로컬 응답 cache·공정 quota 큐가 아님 | `src/any_llm/utils/exception_handler.py:204-224,265-270`; `tests/integration/test_cached_tokens.py:86-114` |

**채택:** SDK 어댑터와 gateway 역할 분리, dependency를 통한 HTTP client 주입, unknown usage 유지, Retry-After 원문 보존. **기각:** functional wrapper 호출을 공유 connection pool로 추정하기, SDK retry를 확인하지 않고 바깥 gateway retry 추가, 이 저장소를 로컬 RPM/TPM scheduler 구현 근거로 인용하기.

`pyproject.toml:9-18`은 Python≥3.11, pydantic/OpenAI/Anthropic/httpx 등을 기본 요구한다. optional provider 묶음을 줄일 수 있어도 Rust gateway 코어에 필요한 의존성은 아니다. 이 분석은 Python client fixture 및 provider SDK의 숨은 동작을 점검하는 데 직접 적용된다.

| 읽은 원본 테스트 | 실제 assertion과 한계 |
|---|---|
| `tests/unit/providers/test_openai_base_provider.py:32-67` | 주입한 httpx.MockTransport로 request 1회와 prompt_cache_key 전달 경로를 검사, finally에서 주입 client를 close. 실제 TCP 연결 재사용 검증은 아님 |
| `tests/unit/test_exception_handler.py:285-317` | 429의 숫자 Retry-After, HTTP-date, 누락 None을 확인. 실제 대기·재시도 시각을 검사하지 않음 |
| `tests/integration/test_streaming.py:15-60` 및 `tests/integration/test_cached_tokens.py:86-114` | provider iterator를 소비하고 내용/usage cached_tokens를 검사. API key 또는 local provider가 필요하며 skip 분기가 있음. 이 보고서에서는 미실행 |

## 5. Otari: 실제 gateway의 admission·정산만 추가 확인

소스 루트: [references/otari](/Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research/references/otari). 고정 [commit](https://github.com/mozilla-ai/otari/tree/706269043c755720de82c07057671cbef0a365ae).

| 구간 | 실제 코드 | 고정 소스 근거 |
|---|---|---|
| ingress·admission | FastAPI chat route → shared request context에서 user RPM 확인·가격과 budget 예약 → provider adapter | `src/gateway/api/routes/chat.py:361-374,466-518`; `src/gateway/api/routes/_pipeline.py:1658,1810`; `src/gateway/rate_limit.py:37-73` |
| quota 축 | user별 sliding-window RPM은 monotonic clock과 deque. 금액·토큰·요청 예산 hold는 SQL conditional UPDATE의 AND guards로 함께 수용/거부 | `src/gateway/rate_limit.py:22-73`; `src/gateway/services/budget_service.py:680-715` |
| provider | chat adapter가 any-llm의 functional acompletion을 호출하고 stream_options.include_usage를 보충 | `src/gateway/api/routes/chat.py:226-243` |
| stream retry 경계 | 첫 chunk까지 실패/timeout이면 attempt를 바꾸지만 첫 chunk가 확보되면 고정. 그 이후 오류는 client로 전달 | `src/gateway/streaming.py:354-448` |
| stream 메모리 | 일반 chunk는 즉시 yield. hybrid cost 정산을 위해 terminal suffix만 보관하며 최대4 chunks 초과면 flush하고 다시 전송 | `src/gateway/streaming.py:27-32,219-266` |
| 정산 once | reservation ledger의 `WHERE status=ACTIVE` UPDATE에 성공한 호출만 terminal 전이와 counter 조정. claim과 release는 같은 transaction에서 완료 | `src/gateway/services/budget_reservation_ledger.py:239-270` |
| 정상·미완료 | generator는 complete/no-usage/error/incomplete callback 구분. CancelledError 및 finally에서도 incomplete 정산 진입. standalone disconnect는 tool 비용을 정산하거나 reservation을 refund | `src/gateway/streaming.py:301-351`; `src/gateway/api/routes/_pipeline.py:3713-3741` |
| usage 누락 | 정상 stream에 usage가 없으면 allow_free/estimate/fail 정책 분기. estimate/fail은 token estimate 유지. provider가 보고하지 않은 값을 actual0으로 혼동하지 않는 명시적 정책이 있음 | `src/gateway/api/routes/_pipeline.py:3613-3668` |

여기의 user token budget은 공급자 API key별 TPM pacing과 동일하지 않다. 여러 에이전트에게 공급자 창을 나눠 주는 공정 scheduler까지 이 코드가 제공한다고 주장하지 않는다. 또한 standalone의 **disconnect refund를 공급자 token quota의 재사용 허가로 옮기면 안 된다.** HTTP 취소 후에도 공급자가 이미 소비했거나 계속 처리할 수 있기 때문이다. 우리 제품은 dispatch 전 취소와 dispatch 후 미확인 사용량을 분리하고, 후자는 보수적 예약 유지/정산 정책으로 다뤄야 한다.

terminal buffer를 4 chunks로 제한하는 것은 전문 수집을 피하는 좋은 패턴이나 byte 상한은 아니다. 우리 제품의 SSE usage observer에는 개별 frame byte 상한이 추가로 필요하다. Otari의 protocol 변환·hybrid inline cost 첨부 자체는 현재 코어 범위에 필요하지 않다.

원본 테스트 `tests/unit/test_streaming_generator.py:439-476`는 keepalive 대기 중 generator.aclose를 호출하여 pending upstream await 취소와 incomplete callback 1회를 검사한다. 실제 TCP disconnect 시험은 아니다. `tests/integration/test_streaming_precommit_refund.py:71-99`는 upstream 첫 chunk 이전 실패를 mock하고502·spend0·reserved0·error log를 검사한다. 두 테스트 모두 읽었지만 실행하지 않았다.

패키징의 한계가 명확하다. `pyproject.toml:13-16`은 Python≥3.13과 any-llm-sdk[all]을 요구하고 FastAPI/SQLAlchemy/Postgres·SQLite drivers/MCP/PDF·문서 처리/auth 등을 기본 dependencies에 포함한다. `pyproject.toml:137-145`는 Windows를 배포 대상으로 삼지 않고 uv resolution도 Linux/macOS로 제한한다. `README.md`는 SQLite 단독과 Compose Postgres 전체 stack을 구분한다. Docker가 논리적 필수라는 뜻은 아니지만 Windows/macOS/Linux Rust binary 하나라는 우리 설치 계약을 충족하는 사례는 아니다.

**채택:** 한 예약에 하나의 terminal 상태 전이, 여러 자원의 원자적 admission, 첫 chunk 전후 retry 경계, keepalive 대기 task 정리, terminal 일부만 buffer. **기각:** SQL/멀티테넌트/tool/UI 계층 복제, disconnect를 무료 token 소비로 취급, 네이티브 Windows 지원의 증거로 사용.

## 로컬 Rust 구현으로 연결할 수용 조건

아래는 소스에서 얻은 **제안**이며 이 보고서에서 통과시킨 테스트 목록이 아니다. 숫자는 임의 성능 목표 대신 관찰 가능한 불변식을 사용한다.

| 우선순위 | 독립 fixture | 요구 결과 | 근거·반례 |
|---|---|---|---|
| P0 | 동일 credential/quota group에 여러 client가 동시에 RPM과 TPM 경계 요청 | 모든 자원 충족 시만 1회 dispatch; 하나가 모자라면 다른 자원을 선소비하지 않음 | TensorZero tickets, Otari AND-guard. user RPM와 provider TPM를 혼동하지 않음 |
| P0 | queue 중 disconnect, headers 전 disconnect, stream 중 disconnect를 각각 발생 | 첫 경우 upstream0회. dispatch 후에는 작업 owner가 정책대로 종료/정산하고 permit을 1회만 반환 | TensorZero drain vs Helicone next-send 감지 vs Otari incomplete refund |
| P0 | disconnect + deadline + upstream EOF 동시 발생 | receipt의 terminal 전이는 한 번. double refund·negative inflight·남은 hold 누락 없음 | Otari ACTIVE→terminal ledger |
| P0 | 첫 chunk 일부 뒤 socket reset 또는 EOF, 마지막 usage 없음 | 완료로 오인하지 않음; partial usage/unknown을 구분; 자동 replay로 중복 생성하지 않음 | TensorZero H2 body 오류 시험, Portkey stream 타입만 확인하는 테스트 공백 |
| P0 | gateway retry0, client retry0/활성, SDK retry0/활성 조합 | 사용자 작업 수·gateway ingress 수·실제 upstream 시도 수를 분리 집계. 모든 wire attempt가 quota 소비에 반영 | TensorZero variant/provider 층, Helicone backon/EventSource, Any-LLM SDK |
| P0 | upstream 429의 초/HTTP-date/ms/누락/잘못된 Retry-After와 여러 client 동시 재요청 | 원문 전달과 해석을 구분하고 해당 quota group cooldown만 적용. 과거 날짜·overflow·반올림 경계를 명시 | Helicone 두 형식 해석, Any-LLM 원문 보존, Portkey parseInt 한계 |
| P0 | HTTP headers 즉시 전송 후 첫 token 지연, 이후 chunk 간 긴 공백 | connect/TTFT/idle/total deadline을 혼동하지 않음; headers 성공으로 전체 timeout이 무효화되지 않음 | TensorZero deadline 분리, Portkey fetchWithTimeout timer 해제 |
| P0 | 느린 client + 빠른 긴 stream, 소비자 완전 정지, SSE 구분자 없는 거대 frame | 전달 채널·프레임·observer 메모리의 byte 상한 관찰. 전체 출력 크기에 비례한 clone/Vec 누적 없음 | TensorZero/Helicone unbounded, Portkey partial frame buffer, Otari chunk 수 상한 |
| P0 | OpenAI 마지막 usage, Anthropic 시작 input+누적 output, usage0, usage 누락, 부분 usage | 누적치를 더해 이중 계산하지 않음. None과0 구분. 공급자 cache token을 자체 cache hit와 구분 | TensorZero usage aggregator, Any-LLM cached-token 시험 |
| P1 | 같은 upstream에 순차·동시 요청 후 재사용 관찰 | 단일 shared reqwest client 수명을 유지하고 TCP connection 수를 관찰. H2 waterfall 없이 요구 충족 여부 판정 | TensorZero 고QPS 특화 pool과 Any-LLM functional client 생성의 차이 |
| P1 | no-abort 처리 중 shutdown 및 provider 영구 무응답 | 정산 소유 task가 유실되지 않고 drain deadline 이후 unknown으로 마무리. 종료가 무한 대기하지 않음 | TensorZero TaskTracker는 소유권 근거이지 무한 drain의 정당화가 아님 |
| P1 | 프로세스 재시작 직전 공급자 사용량 발생 | 로컬 상태 초기화가 upstream rate window 초기화로 간주되지 않음. 필요 상태 또는 보수적 cooldown을 검증 | 외부 quota와 로컬 in-memory limiter의 수명 차이 |

## 구현 선택의 경계

현재 요구에는 공유 reqwest client, byte 제한을 둔 streaming 전달, 작고 명시적인 usage observer, 단일 quota admission/정산 상태 기계가 적합하다. 응답 캐시·semantic cache·프로토콜 변환·provider 자동 교체는 요청 의미와 token 소비를 바꾸므로 현 단계의 경량 코어로 가져오지 않는다.

이 결론은 소스 수준 판단이다. 어떤 언어가 더 빠른지, 이 설계가 기존 gateway보다 더 많은 사용자 작업을 완료하는지, Windows에서 배포 가능한지는 별도의 mock workload·실제 client·OS 실행 결과로 판단해야 한다.
