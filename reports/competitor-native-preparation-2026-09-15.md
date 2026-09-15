# 경쟁 gateway 네이티브 실행 준비

2026-09-15. 기존 `references/`의 고정 체크아웃은 유지하고, 실행용 안정 배포 태그를 iCloud 밖 `/Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/native/`에 별도로 clone했다. 이 문서는 **실행 준비와 HTTP smoke 결과**이며 성능 비교 결과가 아니다.

## 직접 확인한 결과

| 대상 | 실행용 고정 소스 | 이번 확인 |
|---|---|---|
| Bifrost HTTP transport | `transports/v2.1.1`, `c193745d2a713e9f58f021d43e138df5eb7e038a` | macOS ARM64 소스 빌드 성공, localhost 시작·SSE 내용·종료·usage·429 단일 시도 확인 |
| PromptForge | `promptforge-workshop-v0.2.0`, `5df5802e0e9f4e3a6333c543f13ca66f84b3f7bc` | 소스 clone, 공식 macOS ARM64 Workshop archive 다운로드 및 공식 SHA-256 일치. headless gateway 실행은 아직 안 함 |

[소스 manifest](../evidence/reference-native-2026-09-15/sources.json), [다운로드](../evidence/reference-native-2026-09-15/downloads.json), [준비 상태](../evidence/reference-native-2026-09-15/preparation-summary.json).

Bifrost 공식 다운로드는 GET도 HTTP 403이었다. GitHub의 HTTP transport 안정 릴리스에는 바이너리 asset이 없고, 공식 npm wrapper 1.6.3도 같은 CDN URL을 사용한다. 기존 인증을 변경하지 않았다. Go가 설치되어 있지 않아 공식 Go 1.27.0 ARM64 archive를 작업 캐시에만 풀고 공개 SHA-256을 확인했다. 전역 설치는 없다.

Bifrost는 `GOMAXPROCS=2`, `go build -p 2`, 15분 제한으로 **100.15초에 빌드 종료 0**을 기록했다. 코드 자체는 바꾸지 않았다. UI는 upstream의 [load-test 방식](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/.github/workflows/scripts/load-test.sh#L247-L262)에 맞춰 placeholder만 포함했다. 따라서 공식 전체 UI 배포 바이너리와 동일하다고 표현하지 않는다.

- 바이너리: `/Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/native/bifrost-http-source`
- 크기: 112,742,994 bytes. 이 크기는 RSS가 아니다.
- SHA-256: `e1a89310241d2c59ce9075dd60d2725272737288697c7c20ca32b640b27bab43`
- [전체 빌드 명령·제한·결과](../evidence/reference-native-2026-09-15/bifrost-build.json), [바이너리·의존성 메타데이터](../evidence/reference-native-2026-09-15/bifrost-binary.json)

## Bifrost 실행 설정과 실제 wire 차이

완성된 [config.json](../evidence/reference-native-2026-09-15/bifrost-config.json)을 새 app directory로 복사하고 `providers.openai.network_config.base_url`의 mock port만 맞춘다. **base_url에 `/v1`을 넣지 않는다.** Bifrost가 `/v1/chat/completions`와 `/v1/models`를 붙인다. pricing/model parameter JSON 경로는 현재 작업 캐시를 가리킨다.

```sh
rtk /Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/native/bifrost-http-source \
  -app-dir /Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/native/bifrost-app \
  -host 127.0.0.1 -port 18772 -log-level warn
```

| 항목 | 설정·관찰 |
|---|---|
| client endpoint | `POST /openai/v1/chat/completions`, `model="synthetic"` 그대로 수용 |
| ingress 추적 | `x-bf-eh-x-benchmark-ingress`를 보내면 upstream에는 `x-benchmark-ingress`로 전달. allowlist에도 명시 |
| 재시도 | provider `max_retries=0`. 정상적인 429 응답에 실제 upstream 시도 1회 확인 |
| 실행/대기 | provider concurrency 2, buffer 64; `drop_excess_requests=true`로 channel 밖 무한 대기 방지. runtime 포화·cap 검증은 후속 비교 preflight에서 해야 함 |
| cache·관측 | semantic cache/telemetry plugin 명시적 off, content/request logging off |
| DB | `config_store.enabled=false`, `logs_store.enabled=false`. smoke app directory에는 config.json만 생성됨 |
| bootstrap 인터넷 | pricing/model parameters는 각 `{}`의 local file URL, MCP 및 live-model background refresh 0 |
| 자동 모델 조회 | background refresh 0이어도 시작 시 **별도의 GET /v1/models 2회** 실제 관찰. ingress가 없는 control traffic으로 별도 집계해야 함 |
| 입력 변환 | `max_tokens=16` → `max_completion_tokens=16`, `stream_options.include_usage=true` 추가. 본문 125 → 176 bytes. 메시지 의미와 한도를 유지하지만 bytes가 같지는 않음 |
| 출력 변환 | 일반 chunk에 `usage:null`; 마지막 유효 usage object는 한 번. object usage `{prompt_tokens:32,completion_tokens:16,total_tokens:48}` 확인 |

소스 근거: [provider schema](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/transports/config.schema.json), [시작 옵션](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/transports/bifrost-http/main.go#L90-L110), [telemetry off 처리](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/transports/bifrost-http/server/plugins.go#L197-L214).

**최소 file-only 구성의 경계:** governance를 생략하면 관련 plugin/auth의 config store 부재 로그가 나오지만 서버와 위 inference smoke는 동작한다. 빈 `governance:{}`를 추가하면 이 태그에서는 governance handler가 config store를 요구해 시작에 실패했다. [실패 기록](../evidence/reference-native-2026-09-15/bifrost-smoke-governance-empty-failure.json)을 보존했다. 따라서 이 arm에 **Bifrost 자체 RPM/TPM governance가 작동한다고 주장하지 않는다.** 공통 mock 제공자의 동일 제한 아래 큐/전달을 비교하는 baseline이다. Bifrost quota 정책 자체를 비교하려면 별도 검증된 governance 구성이 필요하다.

## smoke와 비교 하네스의 수정점

[재실행 가능한 smoke](../evidence/reference-native-2026-09-15/smoke_bifrost.py), [실제 기록](../evidence/reference-native-2026-09-15/bifrost-smoke.json).

최종 smoke는 정상 생성 1회, 고정 429 1회, bootstrap 모델 조회 2회만 수행했다. 합성 내용·`[DONE]`·usage object·429 무재시도·DB 파일 부재를 assertion으로 확인했고, 자기 프로세스와 listener는 종료했다. 부하·latency 우위·memory 예산 통과를 측정하지 않았다.

기존 `benchmark_http.Mock`을 먼저 연결했으나 untagged startup catalog 호출과 `max_completion_tokens`를 처리하지 못해 fixture 예외가 났다. 이는 Bifrost 성능 실패가 아니다. [초기 fixture 실패](../evidence/reference-native-2026-09-15/bifrost-initial-smoke-failure.json)를 남기고 bounded stdlib mock으로 wire를 확인했다.

공정한 완제품 비교에는 다음이 필요하다.

1. mock은 generation payload의 두 출력 한도 필드를 이해하고, 의미가 같은 한도인지 검사한다. quota 비용은 **같은 사전 fixture 비용**을 사용해야 JSON 재직렬화로 Bifrost만 불리해지지 않는다.
2. bootstrap `/models`를 지원하고 별도로 계수한다. cold-start 비용에서는 제외하지 않으며, steady-state workload는 bootstrap을 끝내고 동일 mock quota 상태에서 시작한다.
3. SSE usage `null`은 유효 usage object가 아니다. 기존 reader의 `duplicate_usage` 판정은 이 표준 표현에 맞춰 수정해야 한다. 모든 arm에 같은 해석을 적용한다.
4. 사용한 정상 SSE는 role, content, `finish_reason:"stop"`, usage, `[DONE]` 순서다. 최소한 이 정상 형식과 중간 stream 종료를 분리해 확인한다. 일부 필드의 필수 여부를 이 smoke만으로 일반화하지 않는다.
5. source-supported concurrency/queue 설정을 실제 포화 fixture로 확인한 후 성능을 잰다. local limit 없는 Bifrost 최소 구성을 모든 gateway 기능의 최고 성능으로 간주하지 않는다.

## PromptForge의 가능한 최소 실행

[공식 gateway README](https://github.com/cppalliance/promptforge/blob/5df5802e0e9f4e3a6333c543f13ca66f84b3f7bc/crates/promptforge-gateway/README.md)는 `--no-default-features` 빌드가 local inference, web search, config UI를 제외하고 Node 없이 가능하다고 명시한다. gateway는 `serve <config> --profile benchmark`로 실행한다. [준비한 TOML](../evidence/reference-native-2026-09-15/promptforge-config.toml)은 remote dominion concurrency2/queue64/RR, synthetic 단일 모델만 사용한다.

```sh
rtk cargo build --locked --release -p promptforge-gateway --no-default-features
rtk promptforge-gateway serve promptforge-config.toml --profile benchmark
```

위 명령은 **이번에 실행하지 않았다**. Rust 빌드 시 `CARGO_TARGET_DIR`는 iCloud 밖 전용 작업 캐시에 설정해야 한다. 기존 product와 경쟁 코드를 동시에 빌드하지 않는다. 소스의 [dominion 설정](https://github.com/cppalliance/promptforge/blob/5df5802e0e9f4e3a6333c543f13ca66f84b3f7bc/crates/promptforge-gateway-config/src/config.rs#L274-L315)과 [기존 integration fixture](https://github.com/cppalliance/promptforge/blob/5df5802e0e9f4e3a6333c543f13ca66f84b3f7bc/crates/promptforge-gateway/tests/it/support.rs#L382-L419)를 참고했다. `X-PromptForge-Client`는 공정성 ID이고 임의 ingress 추적 헤더를 그대로 보존한다는 뜻은 아니다. 실제 헤더 보존·retry·SSE는 실행 검증이 남았다.

다운로드한 Workshop archive는 12,144,532 bytes, SHA-256 `b1fc3370eff1f83d60d407e2eadf03d9871ba93ed74208be9f0e52da1c57e042`로 공식 GitHub digest와 일치한다. 안에는 Workshop GUI 실행 파일 하나가 있으며 별도 headless gateway 바이너리는 없다. GUI를 사용자 환경에 설치하거나 실행하지 않았다. signature 파일을 별도로 검증한 것은 아니므로 서명 검증 완료라고 표현하지 않는다.

이번 준비에서 실제 LLM API 호출·모델 다운로드·Docker/WSL·전역 설치·git commit/push는 없었다. 기존 연구용 clone은 수정하지 않았으며, 비교용 clone의 tracked source도 변경하지 않았다.

## 공정한 quota 비교용 추가 구성

limiter를 끈 Bifrost만 DeskQuota의 quota 정책과 비교하는 것은 충분하지 않다. 부모 검토 요청에 따라 **[공유 RPM16/TPM6000 quota 구성](../evidence/reference-native-2026-09-15/bifrost-quota-config.json)**을 별도로 준비했다. provider `openai` 전체에 rate-limit ID 하나를 연결해 synthetic 모델의 모든 root가 같은 한도를 공유한다. concurrency2, queue64, retry0, cache/telemetry/logs off는 그대로다.

이 태그의 무수정 HTTP server는 file-only governance를 켜면 시작할 수 없다. 근거는 [server.go2353–2357](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/transports/bifrost-http/server/server.go#L2353-L2357)의 무조건 governance handler 생성과 [nil config-store 거절](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/transports/bifrost-http/handlers/governance.go#L253-L259)이며, 위의 실제 부팅 실패와 일치한다. plugin만의 in-memory 지원을 전체 HTTP 서버 지원으로 확대 해석하면 안 된다.

추가 quota 구성은 **같은 네이티브 프로세스 안의 SQLite config store**만 사용한다. 외부 DB 서비스는 필요 없다. 새 run마다 별도 app directory와 DB path를 지정해야 이전 사용량이 다음 run에 섞이지 않는다. pricing/model 자료는 여전히 local file이며 logs store는 끈다. 이 변형은 다음의 제한된 runtime preflight까지 수행했다. 본 실험의 RPM16/TPM6000 처리량·공정성 검증은 아직 남는다.

Bifrost 제한은 `60s` 주기 counter와 관측 usage·성공 완료를 사용하는 정책이다. [tracker.go112–154](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/plugins/governance/tracker.go#L112-L154)는 미완료 요청의 최대 output을 DeskQuota처럼 선예약하지 않으며 성공 request count를 terminal에서 증가시킨다. 동일 RPM/TPM 숫자가 동일 회계 계약을 의미하지 않는다. 실제 preflight에서 provider-wide 거절과 counter 증가를 확인한 다음, 모든 arm에 동일한 외부 mock 제한을 걸어 완료·대기·오류·upstream 시도를 비교해야 한다. 이 불일치는 비교 결과에서 함께 공개한다.

### SQLite quota 구성의 직접 smoke

`python3 evidence/reference-native-2026-09-15/smoke_bifrost.py --quota`로 **새 app directory의 SQLite 구성**을 시작하고, provider의 RPM만 1로 낮춘 작은 검사까지 수행했다. 첫 생성은 200/정상 SSE, 두 번째 생성은 gateway에서 429였다. 두 번째 요청은 mock upstream에 도착하지 않았으며 생성 upstream 시도는 총 1회였다. 시작 시 catalog GET 2회는 별도였다. [원시 결과](../evidence/reference-native-2026-09-15/bifrost-quota-smoke.json).

공유 provider 제한의 실제 거절을 확인한 것이며 TPM6000의 모든 경계나 동시성 경쟁을 검증한 것은 아니다. SQLite `config.db`, `config.db-wal`, `config.db-shm`을 생성했고, 로그 DB는 없었다. 따라서 no-wait/file-only와 quota/SQLite의 RSS·시작 비용을 같은 프로필로 합치지 않는다.

본 실험은 `bifrost-quota-config.json`의 `config_store.config.path`에 들어 있는 `<RUN_APP_DIR>/config.db`를 해당 run의 절대 경로로 교체해야 한다. smoke 스크립트도 매번 이 값을 새 디렉터리로 교체한다. config의 `governance.rate_limits[0]`는 TPM6000/60s, RPM16/60s이고 `governance.providers[0]`는 `name:"openai", rate_limit_id:"fixture-shared-quota"`이다. 변경된 두 숫자와 실제 정책을 결과 manifest에 함께 기록한다.
