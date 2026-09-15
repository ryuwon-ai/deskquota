# HiveMind · LiteLLM native 비교 실행 준비

2026-09-15. 상태: **별도 고정 clone, 격리 설치, 실제 loopback JSON/SSE/429 smoke 완료**. 성능 benchmark·실제 LLM 호출·제품 수정은 이 작업에 포함하지 않았다. 기존 `references/` clone은 변경하지 않았다.

## 고정한 대상

| 대상 | 실행/조회 버전 | 정확한 소스 SHA | 결정 |
|---|---|---|---|
| HiveMind stable | [v0.2.0](https://github.com/jayluxferro/hivemind/releases/tag/v0.2.0), 2026-04-18 | `d659a253ff1af1dfefed4cd865007991ab570011` | clone·별도 설치. stable CLI에 RPM/TPM override가 없어 이번 제한 비교 실행 대상으로 선정하지 않음 |
| HiveMind current HEAD | 조회한 main, 패키지 표기는 여전히 0.2.0 | `0468db5db9d0526e326b4db26971642301d7deed` | 실제 smoke 완료. **stable release와 구분** |
| LiteLLM stable | [v1.100.1](https://github.com/BerriAI/litellm/releases/tag/v1.100.1), 2026-09-10 | `1dba17b10ded12ad0021edb453ba2c54e4637928` | 해당 PyPI proxy distribution 실행, stable source 별도 clone |

원본 GitHub latest-release 응답, HEAD 조회, clone remote/HEAD/상태, 설치 의존성 전체는 [evidence 폴더](../evidence/reference-python-2026-09-15/)와 [manifest](../evidence/reference-python-2026-09-15/manifest.json)에 있다. LiteLLM 설치본의 router/limiter/header 처리와 HiveMind 설치본의 4개 핵심 파일은 clone 원본과 SHA-256 일치했다([7개 검사](../evidence/reference-python-2026-09-15/installed-source-check.json)). 전체 배포 파일의 동등성을 검사했다는 뜻은 아니다.

모든 clone·venv·임시 runtime 설정은 `/Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/python/`에 있다. 이미 설치된 CPython 3.12.12와 uv를 사용했고 전역 패키지를 설치하지 않았다. 런타임 환경은 HiveMind HEAD 39개, LiteLLM proxy 107개 distribution을 고정했다. 이 개수는 메모리나 지연 측정값이 아니다. 설치된 Redis/DB client 패키지와 실제 Redis/DB 서버 의존성을 혼동하지 않는다. 두 smoke 모두 별도 DB·Redis·Docker·WSL 없이 실행했다.

## 그대로 실행할 명령

저장소 루트에서 loopback mock을 먼저 실행한다. 아래 두 실행기는 각각 자기 venv Python을 사용해야 한다. 외부 upstream 옵션은 없으며 mock 포트만 지정한다.

```sh
rtk /Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/python/hivemind-head-venv/bin/python evidence/reference-python-2026-09-15/hivemind-head-launch.py --listen-port 18765 --mock-port 18764 --rpm 16 --tpm 6000
```

```sh
rtk /Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/python/litellm-venv/bin/python evidence/reference-python-2026-09-15/litellm-launch.py --listen-port 18766 --mock-port 18764 --rpm 16 --tpm 6000
```

무대기 arm에서는 두 실행기에 `--rpm 1000000 --tpm 1000000000`을 지정한다. **limiter를 끈 설정이 아니라 해당 workload에서 구속하지 않을 높은 한도**다. 내부 scheduler를 교체하거나 라이브러리 코드를 patch하지 않는다. HiveMind의 `None`은 provider 기본값으로 대체되고 `0`은 이 구현에서 안전한 비활성화 값이 아니다.

- client endpoint: `/v1/chat/completions`; body `model: synthetic`, `stream: true`, 원래 `max_tokens` 유지.
- HiveMind: `max_concurrency=2`, `min_concurrency=2`, `max_retries=0`, `rate_limit_scope=global`, `max_rate_wait_s=120`. AIMD floor를 2로 고정하여 다른 arm과 실행 슬롯 cap을 맞춘다. 이것은 기본 AIMD 모드의 평가가 아니다.
- HiveMind root/session 식별: `x-hivemind-agent-id: <root>`. `global`은 모든 root의 rate-limit 장부를 공유한다. 이 header를 넣었다고 DeskQuota의 root RR와 같은 정책이 되는 것은 아니다.
- LiteLLM: 단일 deployment `model_name: synthetic`, provider adapter `openai/synthetic`, upstream의 model은 `synthetic`으로 유지. `usage-based-routing-v2`, model `max_parallel_requests=2`, router `default_max_parallel_requests=2`, `enable_pre_call_checks=true`; router/SDK/LiteLLM retry 모두 0, cache false, worker 1.
- LiteLLM header 전달: `general_settings.forward_client_headers_to_llm_api: true`. 설정 전에는 `x-benchmark-ingress`가 사라졌다. [실패 전 상태](../evidence/reference-python-2026-09-15/litellm-before-header-forwarding-smoke.json)를 보존했다.
- LiteLLM launch가 가격표 원격 조회·telemetry를 끄는 환경과 CLI `--telemetry False`를 고정한다. 실제 provider credential을 읽거나 사용하지 않으며 API key는 `synthetic-loopback-only` fixture 문자열이다.
- header `x-benchmark-ingress`와 `x-hivemind-agent-id`가 두 arm에서 실제 mock에 도착했다. LiteLLM은 JSON/SSE를 OpenAI adapter로 다시 구성하므로 wire byte equality를 기대하면 안 된다. 메시지/출력 한도/fixture 비용/응답 의미는 별도로 검사한다.

## 직접 검증한 범위

[HiveMind 결과](../evidence/reference-python-2026-09-15/hivemind-head-smoke.json), [LiteLLM 결과](../evidence/reference-python-2026-09-15/litellm-smoke.json): 최종 각 3개 사례에서 JSON 200, SSE 200 + terminal marker, nonstream 429 전파를 확인했다. 사례별 upstream 시도는 정확히 1회였다. HiveMind `/_stats`는 global scope, RPM16/TPM6000, concurrency2, retry0을 표시했다. LiteLLM concurrency/quota는 설정과 해당 limiter/세마포어 소스가 근거이며 부하 상태에서 cap의 동작을 입증한 것은 아니다.

재실행:

```sh
rtk python3 evidence/reference-python-2026-09-15/smoke.py hivemind-head
rtk python3 evidence/reference-python-2026-09-15/smoke.py litellm
```

이 smoke는 18764–18766 포트를 사용하고 자신의 process group과 mock만 종료한다. 최종 검사에서 해당 포트 3개가 모두 닫혔다. [검증 결과와 실행기 해시](../evidence/reference-python-2026-09-15/preparation-verification.json)를 보존했다. SSE 수신 확인은 provider 완료 전 첫 token 도착 시점의 검증이나 TTFT benchmark를 대신하지 않는다. stream 429·취소·장시간 quota 포화는 부모의 공통 비교 harness에서 별도 확인해야 한다.

## README와 코드가 달랐던 점

HiveMind HEAD README는 telemetry를 opt-in으로 설명하지만 [실제 CLI config 생성](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/src/hivemind/cli_args.py#L279-L287)은 `--telemetry-dsn`과 환경 변수가 없으면 로컬 Postgres DSN을 기본값으로 넣는다. [proxy startup](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/src/hivemind/proxy/server.py#L225-L242)은 값이 있으면 연결을 시도한다.

실행기는 지원되는 `HiveMindConfig`와 `run_proxy` 진입점을 사용하고 `telemetry_dsn=None`을 명시한다. 기본 standalone builder는 request-log DB 객체를 만들지 않는다([builder](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/src/hivemind/proxy/server.py#L492-L541)). 따라서 이 비교 구성에서 DB 연결을 요구하지 않는다. 기본 CLI 그대로 실행한 결과라고 쓰면 안 된다.

## 한도가 같아도 회계는 같지 않다

| 경계 | DeskQuota 현재 계약 | HiveMind HEAD 비교 구성 | LiteLLM v1.100.1 비교 구성 |
|---|---|---|---|
| quota 공유 단위 | 설정 upstream을 통과한 root 전체 | 명시적 global | 단일 model deployment, 단일 worker의 로컬 cache |
| 시간창 | rolling 60초, known quota startup hold | rolling 60초, 시작 시 빈 장부 | UTC `HH-MM` 키로 구분한 분 단위 |
| TPM admission | 요청 입력 추정 + 출력 예약량을 원자적으로 검사 | 이미 완료한 token 장부가 한도에 도달하면 대기; 새 출력 예약 없음 | 완료 TPM + 입력 token 추정으로 deployment 필터; RPM pre-call 검사 |
| 완료 처리 | reserved 또는 actual 정산 모드와 늦은 usage 처리 | 보고 usage, 없으면 추정 사용량을 완료 시 기록 | 성공 callback의 reported total token으로 TPM 갱신 |
| 한도 도달 | bounded queue·긴 head 보호·deadline | 슬롯 획득 뒤 rate wait, max-rate-wait | 해당 구성은 rate-limit rejection 경로; 별도 beta scheduler를 활성화하지 않음 |
| 모델/응답 | passthrough | passthrough | OpenAI adapter가 JSON/SSE를 재구성 |

HiveMind [rate limiter](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/src/hivemind/scheduler/rate_limiter.py#L256-L290)와 [완료 사용량](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/src/hivemind/proxy/interceptor.py#L715-L736), LiteLLM [RPM 검사와 TPM 완료 갱신](https://github.com/BerriAI/litellm/blob/1dba17b10ded12ad0021edb453ba2c54e4637928/litellm/router_strategy/lowest_tpm_rpm_v2.py#L135-L242), [입력량 기반 필터](https://github.com/BerriAI/litellm/blob/1dba17b10ded12ad0021edb453ba2c54e4637928/litellm/router_strategy/lowest_tpm_rpm_v2.py#L317-L340), [concurrency semaphore](https://github.com/BerriAI/litellm/blob/1dba17b10ded12ad0021edb453ba2c54e4637928/litellm/router_utils/client_initalization_utils.py#L16-L34)가 이 표의 소스 근거다.

HiveMind의 완료 실제 usage 회계는 DeskQuota `accounting=actual`과도 동일하지 않다. DeskQuota는 입장 전에 예약하고 나중에 조정한다. 이번 HiveMind smoke는 성공 응답 2개의 total token이 각각 10이었지만 장부에는 21을 기록했다. usage가 없는 실패 응답에도 입력 추정을 사용하는 경로가 있어 생긴 차이이며, 실제 quota 비용은 mock의 canonical 기록으로 별도 집계해야 한다.

## 부모 비교 harness에 넘기는 주의점

1. upstream quota 비용을 원래 fixture의 입력/출력 값으로 고정한다. LiteLLM 재직렬화 바이트 수나 각 제품 token 추정으로 upstream 한도 자체를 바꾸지 않는다.
2. metadata를 생성과 별도 분류한다. LiteLLM `/v1/models`는 configured catalog이며 HiveMind/DeskQuota의 upstream 전달 모델 목록과 동일한 호출이 아니다. 첫 완제품 비교는 생성 전용이 안전하다.
3. client retry0, gateway/SDK retry0을 설정하고 wire attempt도 실제 센다. 429를 빠르게 반환하는 arm과 오랫동안 대기하는 arm은 모든 제출 요청의 terminal outcome·고정 시간 내 완료·drain을 함께 표시한다.
4. DeskQuota known quota startup hold를 warmup에서 다루는 방법과 LiteLLM UTC 분 경계를 기록한다. 서로 다른 내부 회계를 scheduler 단독 비교로 설명하지 않는다.
5. root identity, 취소/긴 요청의 완료율, raw usage, upstream 재시도량을 함께 평가한다. 이 준비 완료가 경쟁 성능 우위, 저사양 우위, Windows 실행, 실제 모델 품질을 증명하지 않는다.
