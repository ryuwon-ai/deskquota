# 참조 저장소 전체 목록과 분석 진입점

2026-09-12 기준 **27개 저장소를 실제 clone**했다. 이전 10개에 16개를 추가했고, 성능 하네스 준비 중 Bifrost의 공식 benchmark 저장소 1개를 더 분석했다. 모델 weight·submodule·실제 사용자의 benchmark prompt dataset은 내려받아 실행하지 않았다. 추가 clone은 shallow·blob filter·필요한 sparse source checkout으로 관련 코드와 테스트를 펼쳤다. 전체 파일 전수 감사나 27개 제품 실행 완료라는 뜻은 아니다.

2026-09-14에 **HiveMind와 NVIDIA용 quota proxy2개를 추가로 고정 clone**했다. [3개 소스 분석](additional-quota-references-2026-09-14.md)과 [별도 manifest](../evidence/additional-quota-references-2026-09-14.json)에 실제 HEAD·remote·clean·35곳의 소스 근거를 기록했다. 기존27개 manifest를 재작성하지 않았으며 `research.py verify`의 참조 검사는 여전히 기존27개를 대상으로 한다. 추가3개도 실행 성능 검증은 아니다.

선정 범위는 직접 대안 gateway, 실제 연결할 agent/local engine, 최근 논문의 공개 구현, 검토한 기업 기술자료의 router다. 모든 LLM 관련 저장소를 끝없이 수집하지 않는다. 추가 모듈/trace는 구체적인 테스트 필요가 생기면 출처와 해당 버전을 별도로 고정한다.

## 클론과 검토 대상

| 저장소 | 층 | 선택한 검토 내용 | 고정 SHA·브랜치 | 분석 |
|---|---|---|---|---|
| [overlaat](https://github.com/tdamsma/overlaat) | API quota | 다자원 예약, 순서 의존성, TCP 취소 | [4e3cd0ff02a0](https://github.com/tdamsma/overlaat/tree/4e3cd0ff02a0456d470d5b18744803a56c7e1ffb) · `main` | [보고서](reference-validation.md) |
| [promptforge](https://github.com/cppalliance/promptforge) | API quota | 클라이언트 순환 큐·설치 | [da1456b545f6](https://github.com/cppalliance/promptforge/tree/da1456b545f655f3743c4b76b099689fc70aa7b4) · `master` | [보고서](repository-analysis.md) |
| [obleth-gateway](https://github.com/thediymaker/obleth-gateway) | API quota | 공정 스케줄러·BSL 경계 | [afffd566fdaa](https://github.com/thediymaker/obleth-gateway/tree/afffd566fdaaa5966197472259f204693b919dde) · `main` | [보고서](repository-analysis.md) |
| [litellm](https://github.com/BerriAI/litellm) | 범용 gateway | 라우팅·retry·cache·외부 서비스 경계 | [9a715df212d7](https://github.com/BerriAI/litellm/tree/9a715df212d777bbd43f4cab05731978c708ec63) · `litellm_internal_staging` | [보고서](repository-analysis.md) |
| [bifrost](https://github.com/maximhq/bifrost) | 범용 gateway | Go 요청 경로·pool·plugin 비용 | [1b23416533e0](https://github.com/maximhq/bifrost/tree/1b23416533e0a8b36192f70b175f9babbd6308f1) · `dev` | [보고서](repository-analysis.md) |
| [smg](https://github.com/smg-project/smg) | inference router | 입장·취소·모델 엔진 경계 | [07c8e82968de](https://github.com/smg-project/smg/tree/07c8e82968de0b5c70e2c92b5e8349cdec30079a) · `main` | [보고서](repository-analysis.md) |
| [llama-swap](https://github.com/mostlygeek/llama-swap) | 로컬 lifecycle | 모델 프로세스 관리·native UX | [41ec321b6216](https://github.com/mostlygeek/llama-swap/tree/41ec321b6216d838488b2a7d936274ed227c0c5e) · `main` | [보고서](repository-analysis.md) |
| [MindRouter](https://github.com/ui-insight/MindRouter) | 공정 배분 | 사용자별 quota·queue | [f909d4324a29](https://github.com/ui-insight/MindRouter/tree/f909d4324a29bf8ed2547e35039143f82b4988be) · `main` | [보고서](repository-analysis.md) |
| [pi](https://github.com/earendil-works/pi) | agent client | retry 두 계층·세션 헤더·tool protocol | [f3c672245d25](https://github.com/earendil-works/pi/tree/f3c672245d25ef2283ffc0d9cdec8a5482651103) · `main` | [보고서](client-integration-validation.md) |
| [hermes-agent](https://github.com/NousResearch/hermes-agent) | native UX | 설치·onboarding·agent 연동 | [3b45681c25a8](https://github.com/NousResearch/hermes-agent/tree/3b45681c25a880477a2a806cdebe91d2f1bfe9ce) · `main` | [보고서](native-setup-research.md) |
| [codex](https://github.com/openai/codex) | agent client | Responses·retry·모델 catalog | [944d6fd1ba4b](https://github.com/openai/codex/tree/944d6fd1ba4baab69dbedd205282dc72ec20abb5) · `main` | [보고서](local-client-source-deep-dive.md) |
| [ollama](https://github.com/ollama/ollama) | 로컬 engine | 모델 큐·semaphore·503·취소 | [53fed2611281](https://github.com/ollama/ollama/tree/53fed26112817f7c55f664efb9e3f65f06cab7db) · `main` | [보고서](local-client-source-deep-dive.md) |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) | 로컬 engine | slot·prefix cache·취소·resume session | [d3146f2b56c2](https://github.com/ggml-org/llama.cpp/tree/d3146f2b56c2db4711ac8391871c9e529d1946d7) · `master` | [보고서](local-client-source-deep-dive.md) |
| [vllm](https://github.com/vllm-project/vllm) | engine | 토큰 budget·KV 선점·remote abort | [a0844fa6c6d6](https://github.com/vllm-project/vllm/tree/a0844fa6c6d6e1143917a033ad6984e40c212862) · `main` | [보고서](engine-source-deep-dive.md) |
| [sglang](https://github.com/sgl-project/sglang) | engine | HRRN·RadixCache·decode 취소 | [e91c94805747](https://github.com/sgl-project/sglang/tree/e91c94805747b60d0b8e8012f3647ae82d4ae862) · `main` | [보고서](engine-source-deep-dive.md) |
| [LMCache](https://github.com/LMCache/LMCache) | KV layer | 전송 완료·lookup·공유 lock 반환 | [588c5366f39c](https://github.com/LMCache/LMCache/tree/588c5366f39cf1fbb21a614074aefdf942573d93) · `dev` | [보고서](engine-source-deep-dive.md) |
| [tensorzero](https://github.com/tensorzero/tensorzero) | Rust gateway | 예약·usage 정산·drain·메모리 | [62eb8f63e8ec](https://github.com/tensorzero/tensorzero/tree/62eb8f63e8ec62018d70420dbf1a8c5d1c026315) · `main` | [보고서](gateway-source-deep-dive.md) |
| [portkey-gateway](https://github.com/Portkey-AI/gateway) | Node gateway | retry·stream buffer·timeout 경계 | [669825cbe89e](https://github.com/Portkey-AI/gateway/tree/669825cbe89ee51569918b8f78a9db486fd69dd4) · `main` | [보고서](gateway-source-deep-dive.md) |
| [any-llm](https://github.com/mozilla-ai/any-llm) | Python SDK | provider client·SDK retry 경계 | [9b3448ff8ef7](https://github.com/mozilla-ai/any-llm/tree/9b3448ff8ef7953877758f6b618720cee59a74e8) · `main` | [보고서](gateway-source-deep-dive.md) |
| [llm-d-router](https://github.com/llm-d/llm-d-router) | 분산 router | EPP flowControl·RR·eviction | [6493998a3735](https://github.com/llm-d/llm-d-router/tree/6493998a3735ccf1b07450fe6308c9ec5dd929e1) · `main` | [보고서](engine-source-deep-dive.md) |
| [helicone-ai-gateway](https://github.com/Helicone/ai-gateway) | Rust gateway | GCRA·SSE·logger tee·license 불일치 | [9649b27bdc9f](https://github.com/Helicone/ai-gateway/tree/9649b27bdc9fb0907d359e899894102a15f3a085) · `main` | [보고서](gateway-source-deep-dive.md) |
| [otari](https://github.com/mozilla-ai/otari) | Python gateway | 원자 예약·terminal once·Windows 제외 | [706269043c75](https://github.com/mozilla-ai/otari/tree/706269043c755720de82c07057671cbef0a365ae) · `main` | [보고서](gateway-source-deep-dive.md) |
| [blitz-router](https://github.com/blitz-serving/blitz-router) | OSDI26 artifact | LMetric와 patched engine telemetry | [a005ceb3a668](https://github.com/blitz-serving/blitz-router/tree/a005ceb3a6689d30514bd27e45ede0db933d0fea) · `lmetric/camera-ready` | [보고서](paper-artifact-source-deep-dive.md) |
| [LAPS](https://github.com/Jianshu-She/LAPS) | MLSys26 artifact | short/long prefill·artifact 버전 차이 | [aa29962d5012](https://github.com/Jianshu-She/LAPS/tree/aa29962d501236ecd2a0c7a8470c5977df3fe057) · `main` | [보고서](paper-artifact-source-deep-dive.md) |
| [FastServe](https://github.com/LLMServe/FastServe) | NSDI26 관련 artifact | MLFQ·token 선점·KV swap | [187a2742bd3e](https://github.com/LLMServe/FastServe/tree/187a2742bd3eb8514d8609d80ad98e105621feed) · `main` | [보고서](paper-artifact-source-deep-dive.md) |
| [dynamo](https://github.com/ai-dynamo/dynamo) | vendor router | 취소 소유권·attempt identity·cleanup budget | [4b72f79cb97f](https://github.com/ai-dynamo/dynamo/tree/4b72f79cb97f144377a5df1ac392b5b72e6dab3e) · `main` | [보고서](dynamo-source-deep-dive.md) |
| [bifrost-benchmarking](https://github.com/maximhq/bifrost-benchmarking) | 측정 하네스 | quota mock·header latency·완료 분모·RSS sampling | [2c416fb234bf](https://github.com/maximhq/bifrost-benchmarking/tree/2c416fb234bf4abac25cf61f4470fc2528362afd) · `main` | [보고서](benchmark-reference-audit.md) |

정확한 remote·전체 SHA·commit 날짜·조회 시각·sparse 경로는 [manifest](../evidence/repositories.json)에 있다. 신규 GitHub metadata는 [1차 추가 후보](../evidence/additional-reference-candidates.json), [Otari](../evidence/otari-metadata.json), [논문·기업 구현](../evidence/paper-vendor-references.json)에 기록했다. clone 로그는 `evidence/clone-logs/`다. main/dev의 최신 source를 봤다는 것과 안정 release를 실행했다는 것은 다르다.

## 이번 분석이 바꾼 판단

| 이전에 쉽게 할 수 있는 추정 | 소스에서 확인한 차이 | 선택 |
|---|---|---|
| HTTP를 끊으면 추론과 quota 소비가 모두 끝남 | 엔진 전송/KV 정리는 지연될 수 있음. llama.cpp session header가 있으면 계속 생성 | 기본 bounded drain, 실제 API 취소 계약 검증 전 close 최적화 금지 |
| stream API이면 메모리가 고정 | unbounded channel·전체 chunk Vec·logging tee·구분자 없는 frame buffer 존재 | 전달·관찰 byte 상한, 느린 client/큰 frame fixture 필수 |
| 모델 URL만 로컬로 바꾸면 모든 도구 연동 | Codex Responses 제약, catalog schema·provider identity cache가 따로 존재 | protocol과 모델 표시 고지, 격리 profile 검증 |
| 클라이언트가 Retry-After를 준수 | 일부 client/HTTP loop는 local backoff, SDK/stream retry 계층도 별도 | 전체 wire attempt 계측, shared quota cooldown은 gateway가 통제 |
| 캐시가 있으면 같은 cache 기능 | 응답 cache·provider cached tokens·KV prefix·CPU/GPU 이동은 다른 층 | v0.1 응답/semantic cache 제외, body·prefix 의미 보존 |
| 최신 학회면 최신 engine 상대 우위 | 게재일과 code baseline·artifact branch·의존성 시점이 다름 | 논문 개선 배수 재사용 금지, source와 runtime 대응 명시 |

## 유지보수와 출처 주의점

- TensorZero는 조회 당시 archived였다. 역사적 구현 분석 대상이며 현행 유지보수 추천으로 쓰지 않는다.
- Helicone ai-gateway는 README/Cargo와 root LICENSE 표기가 불일치한다. root GPL-3.0 사실을 기록하고 코드 복사를 하지 않는다.
- `llm-d-inference-scheduler`는 현재 `llm-d-router`로 redirect되어 canonical 이름으로 clone했다.
- any-llm은 SDK이며 실제 gateway는 Otari다. Otari는 별도로 clone했고 source manifest상 Windows 배포를 제외한다.
- Blitz Router/FastServe/Dynamo의 GitHub license 자동 분류가 비거나 NOASSERTION이어도 파일을 별도 확인한다. 파일별·artifact 라이선스와 데이터 조건을 임의로 통합하지 않는다.
- paper artifact 내부의 문서와 실제 실행 코드가 다를 수 있다. sparse checkout에서 보이지 않는 파일은 Git tree로 먼저 확인하고 저장소에 없다고 단정하지 않는다.

## 검증 수준

`python3 scripts/research.py verify`는 27개 HEAD·remote·clean 상태, PDF3개의 해시, 작업 증거 파일을 검사한다. 원본 동작을 실행한 것은 기존 Overlaat 선택 테스트57개와 별도 예약/TCP probe, 설치된 Pi의 synthetic loopback8개 사례다. 추가16개와 benchmark 저장소1개의 선택 source를 분석했다. 이 중 LAPS timing 함수1개는 별도 합성 loopback probe로 측정 경계를 확인했다. GPU·LLM·OS 실행이나 성능 우위 증명은 아니다.

제품 구현은 [승인된 코어 계획](../docs/superpowers/plans/2026-09-12-gateway-core.md)대로 계속한다. 레퍼런스 기능을 모두 넣지 않고 공통 HTTP client, 원자적 입장, root 공정 큐, bounded streaming, 정확한 terminal 소유권부터 완성한다.
