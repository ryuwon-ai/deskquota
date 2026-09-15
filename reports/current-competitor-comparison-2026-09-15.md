# 현재 기본 버전과 경쟁 게이트웨이 비교

2026-09-15. DeskQuota `687a5ce`의 일반 release 바이너리를 사용한다. 제품 코드는 변경하지 않았고, 실행 스크립트에 비교할 native 바이너리 경로를 명시하는 옵션만 추가했다. 이번 비교는 같은 Mac의 합성 HTTP/SSE 요청에 한정하며 실제 사내 endpoint·모델·Windows의 결과가 아니다.

## 대상과 검증 범위

| 대상 | 실행한 버전 | 구성 |
|---|---|---|
| DeskQuota | `687a5ce`, binary SHA-256 `7573735f0da96aabbf938a7678f8479ad0a8c591cfd1c0294d85d92f3d1d7e6b` | 기본 RR/actual 정산, cache off, retry 0, 비교용 cancel=close, 일반 release |
| Bifrost | transport `v2.1.1`, source `c193745d2a713e9f58f021d43e138df5eb7e038a` | 무대기는 file-only, 제한 실험은 governance + SQLite config store |
| HiveMind | HEAD `0468db5db9d0526e326b4db26971642301d7deed` | global quota, min/max concurrency 2, retry 0, telemetry DB 없음 |
| LiteLLM | `v1.100.1`, source `1dba17b10ded12ad0021edb453ba2c54e4637928` | single deployment/worker, usage-based-routing-v2, cache/retry off |
| Direct | 공통 mock 직결 | 게이트웨이 없는 기준선 |

HiveMind는 버전 문자열이 0.2.0이어도 stable v0.2.0과 다른 HEAD다. Bifrost GitHub `latest`는 enterprise tag `ent-v2.1.1-base`를 가리켜 OSS transport와 구분했다. 비교 도중 조회한 LiteLLM의 최신 릴리스는 당일 공개된 **v1.101.0**이었다. 아래 수치는 고정한 **v1.100.1**의 결과이며 v1.101.0의 성능으로 확대하지 않는다. [릴리스 재조회](../evidence/current-competitor-comparison-2026-09-15/release-refresh.json), [LiteLLM 릴리스 변경](../evidence/current-competitor-comparison-2026-09-15/litellm-latest-release.json).

기존 clone·venv·바이너리를 재사용했고 고정 SHA와 변경 없음, DeskQuota의 컴파일 소스/manifest 70개와 바이너리 해시를 확인했다. Docker·WSL·새 runtime 서비스는 사용하지 않았다. [사전 확인](../evidence/current-competitor-comparison-2026-09-15/preflight.json), [기존 native 준비](competitor-native-preparation-2026-09-15.md), [Python 준비](competitor-python-preparation-2026-09-15.md).

## 무대기 전달: 현재 관측한 장점

Apple M4 / RAM 32GiB / macOS 26.5.1에서 제품을 하나씩 실행했다. 다섯 번의 실행 순서를 회전시켰으며, 매번 warmup 5건 뒤 연결을 재사용해 100건을 순차 제출했다. 모든 arm이 측정 500/500건을 완료했다. mock 생성 지연은 0ms이고 cache/retry는 껐다. 실제 모델 TTFT나 포화 처리량 실험은 아니다.

표의 지연은 각 실행에서 계산한 percentile의 최소–최대다. 단위는 ms이며, RSS는 gateway PID의 대기 상태 표본이다.

| 대상 | p50 | p95 | p99 | idle RSS, MiB |
|---|---:|---:|---:|---:|
| Direct | 0.150–0.159 | 0.197–0.217 | 0.228–0.412 | — |
| **DeskQuota** | **0.269–0.323** | **0.433–0.556** | **0.569–1.095** | **9.56–9.66** |
| Bifrost file-only | 0.455–0.526 | 0.740–1.133 | 1.259–4.845 | 59.48–60.91 |
| HiveMind HEAD | 0.959–1.010 | 1.175–1.420 | 1.402–3.085 | 50.09–50.41 |
| LiteLLM v1.100.1 | 3.914–4.037 | 4.445–5.174 | 4.663–6.303 | 285.59–285.91 |

이번 다섯 반복에서는 DeskQuota의 p95와 메모리가 세 비교 구성보다 낮았다. 다만 기능 범위와 켜 놓은 limiter가 다르다. Bifrost 무대기 구성은 governance off, Python 구성은 limiter를 높은 비구속 한도로 켰고, DeskQuota는 unlimited다. 언어만 바꾼 실험도, 모든 설정의 최적 성능 비교도 아니다. Direct의 p95를 다른 arm의 p95에서 빼서 요청별 추가 지연이라고 부르지 않는다.

CPU 누적값의 100건당 증분은 DeskQuota 0.01–0.02초, Bifrost 0.04–0.05초, HiveMind 0.07–0.08초, LiteLLM 0.38–0.39초였다. `ps`의 낮은 해상도 때문에 CPU 효율 배수로 홍보하지 않는다. 메모리 표본 역시 전체 process tree나 연속 peak 측정이 아니다. LiteLLM의 client-visible SSE usage는 이 요청 옵션에서 missing 500건이었지만, 응답 내용·종료 검사는 모두 통과했다. usage 누락을 성공 실패나 실제 사용량 0으로 취급하지 않는다.

## 제한 실험의 방법

이전 비교와 같은 seed101의 생성 요청 18개를 사용한다. 짧고 긴 입력, 출력 예약량과 합성 처리 시간을 섞어 32초 동안 제출한다. root 4개, upstream 실행 동시성 2, rolling 60초 RPM16/합성 TPM6000이다. 첫 60초 완료 수와 이후 drain을 포함한 최종 완료를 나누고, 각 요청의 최대 125초 timeout까지 결과를 남긴다. DeskQuota 자체 queue deadline은 120초다. startup hold 60초는 측정 창 전에 기다렸으며 cold start 결과에 포함하지 않았다.

변환된 JSON 크기로 경쟁 제품을 불리하게 만들지 않도록 upstream은 원래 입력에서 정한 비용을 차감한다. `byte_reserved`는 원본 JSON bytes + 출력 예약량, `quarter_actual`은 각 항을 4로 나누고 올림한 비용이다. 동일 입력에서 두 계약을 모두 검사한다. 각 제품의 자체 quota 창·추정·거절·대기 정책은 그대로 두므로 완제품 구성 비교다.

각 비용 계약/arm은 한 번의 탐색 실행이다. 작은 성공 표본의 p95는 사실상 최대값이라 안정적인 tail 추정이 아니다. LiteLLM의 UTC minute bucket 위상을 맞추지 않았고, HiveMind는 기본 per-agent/AIMD 모드를 고정 동시성/global로 바꾼 비교 구성이다. 이 결과로 그 기본 모드의 우열을 판단하지 않는다.

사용자가 구두로 알려준 **실제 한도 20RPM/500k TPM, 운영 설정 18RPM/450k TPM/동시성3**은 별도 환경 정보다. 위 스트레스 실험에 그 숫자를 적용했다고 주장하지 않는다. 사내 요청·주소·인증 정보는 사용하지 않았다.

## 제한 실험 결과

`첫 창/최종`은 첫 60초 완료 수와 drain 포함 완료 수다. 각 행의 분모는 18건이며, p95는 성공한 응답만의 값이다. 거절과 timeout을 함께 보아야 한다.

| 예약량과 실제 비용이 같은 `byte_reserved` | 첫 창/최종 완료 | 거절 | timeout | 성공 p95, 초 | upstream 시도 |
|---|---:|---:|---:|---:|---:|
| Direct | 7/7 | 11 | 0 | 0.405 | 18 |
| **DeskQuota actual** | **7/16** | **0** | **2** | **116.198** | **16** |
| Bifrost | 7/7 | 11 | 0 | 0.411 | 18 |
| HiveMind | 6/13 | 5 | 0 | 51.452 | 18 |
| LiteLLM v1.100.1 | 7/7 | 11 | 0 | 0.640 | 15 |

이 조건에서 DeskQuota는 요청을 보관했다가 이후 창에서 더 많이 완료했다. 첫 창의 처리량은 늘지 않았고, 두 요청은 끝내 timeout됐다. 최종 완료율 개선을 한도 증가나 사용자 체감 지연 개선으로 표현하면 안 된다.

| 실제 비용이 예약보다 작은 `quarter_actual` | 첫 창/최종 완료 | 거절 | timeout | 성공 p95, 초 | upstream 시도 |
|---|---:|---:|---:|---:|---:|
| Direct | 16/16 | 2 | 0 | 2.505 | 18 |
| **DeskQuota actual** | **16/18** | **0** | **0** | **40.634** | **18** |
| Bifrost | 16/16 | 2 | 0 | 2.511 | 16 |
| HiveMind | 16/17 | 1 | 0 | 40.278 | 18 |
| LiteLLM v1.100.1 | 15/15 | 3 | 0 | 2.522 | 15 |

DeskQuota의 actual 정산은 이 조건에서 과대 예약을 남기지 않았고, RPM 때문에 남은 요청은 다음 창에서 완료했다. 첫 창 완료는 Direct·Bifrost·HiveMind와 같은 16건이다. 성공 집합이 다르므로 p95만 비교해 가장 빠르거나 느리다고 순위를 매기지 않는다.

거절 위치도 달랐다. `quarter_actual`의 Bifrost 2건과 LiteLLM 3건은 upstream에 도착하기 전에 거절됐고, HiveMind 1건은 mock이 거절했다. `byte_reserved`의 LiteLLM은 gateway에서 3건, mock에서 8건이 거절됐다. 다른 제품에도 대기·재시도·라우팅 설정이 있으므로 이 최소 비교 구성의 결과를 제품 전체의 한계라고 단정하지 않는다.

LiteLLM `quarter_actual` 로그에는 `current_rpm: 15`, `rpm_limit: 16`과 deployment 제외가 남았다. 고정 버전의 후보 선택 조건은 `rpm_dict[item] + 1 >= _deployment_rpm`이다. 이 실행이 이전 비교의 16건보다 한 건 적었던 결과는 그대로 보존한다. 최신 v1.101.0 소스에도 같은 식이 있지만 최신 runtime을 실행한 것은 아니다. [해당 실행 로그](../evidence/current-competitor-comparison-2026-09-15/quota-quarter_actual-litellm/gateway.log), [고정 소스 조건](https://github.com/BerriAI/litellm/blob/1dba17b10ded12ad0021edb453ba2c54e4637928/litellm/router_strategy/lowest_tpm_rpm_v2.py#L339), [최신 소스 조회](../evidence/current-competitor-comparison-2026-09-15/litellm-rpm-boundary-source.json).

quota 실행의 idle RSS는 DeskQuota 9.59–9.61MiB, Bifrost 81.45–82.70MiB, HiveMind 50.28–50.52MiB, LiteLLM 285.92–286.02MiB였다. Bifrost는 이때 SQLite governance 구성을 사용하므로 file-only 무대기 메모리와 구분한다.

## 현재 캐시 재확인

경쟁 제품의 캐시는 앞의 비교에서 모두 껐다. 다음은 **DeskQuota 자체의 별도 direct/miss/hit 검사**이며 경쟁 캐시 속도 순위가 아니다. 같은 binary, 50ms 지연의 mock, JSON/SSE 각각 각 arm 30건, 순서는 direct→miss→hit로 고정했다. 이 실험은 warmup을 두지 않았고 50% 적중을 의도적으로 만들었다.

| 경로 | JSON p95, ms | SSE p95, ms |
|---|---:|---:|
| Direct | 53.259 | 60.547 |
| Cache miss | 54.138 | 60.667 |
| **Cache hit** | **0.599** | **0.715** |

gateway 요청 120건 중 60건이 hit여서 upstream 호출 60회를 피했다. 별도 direct 60건까지 총 180건의 body와 완료 의미를 검증했고 실패·취소는 없었다. 이 50%를 실사용 적중률로 제시하지 않는다. 캐시 데이터 4MiB 제한은 전체 프로세스 메모리 제한이 아니며, 이번 전후 RSS 표본은 9.69→11.03MiB였다. [원시 캐시 결과](../evidence/current-competitor-comparison-2026-09-15/cache.json).

## 기능과 비용의 차이

아래는 실행 성능과 별개의 코드·공식 문서 근거다.

| 항목 | DeskQuota | Bifrost | LiteLLM | HiveMind HEAD |
|---|---|---|---|---|
| 주요 범위 | 개인 PC의 한 endpoint와 여러 root | 다중 provider gateway, routing, governance | 다중 provider/API 변환, routing, budget, 운영 기능 | 여러 코딩 agent의 quota·backpressure·MCP |
| 실행 형태 | Rust 실행 파일 하나 | Go 실행 파일, 기능에 따라 저장소 추가 | Python 환경, 단일 worker 최소 구성 가능 | Python 환경, 이번 구성은 DB 없이 실행 |
| 완료 usage | JSON/SSE로 예약량 정산 | 완료 usage 기반 governance counter | reported usage 기반 limiter/callback | reported usage + 누락 시 추정 경로 |
| 혼잡 대응 | root RR/FIFO, 예약을 포함한 입장 검사, bounded queue | governance 제한·provider concurrency/queue | deployment limiter·routing·설정 가능한 retry/fallback | agent별 quota, AIMD, fair-share governor |
| 응답 캐시 | 작은 exact memory cache | exact/semantic cache, vector-store backend | exact local memory/disk/Redis 등, semantic 옵션 | 조사한 proxy 경로의 cache 기능은 provider prompt-cache 관측 |
| 오류 재시도 | 기본0, 제한된 transient429만 opt-in 최대1회 | 폭넓은 retry/fallback 정책 | 폭넓은 retry/fallback 정책 | provider별 429/5xx retry |
| 사용 경험 | setup/on/off, client 설정 preview/restore, 선택적 로그인 시작 | CLI/Web UI와 운영 설정 | CLI/config/UI와 여러 연동 | IDE 설정 생성, proxy/MCP, agent 자동 식별 |

Bifrost exact mode는 임베딩이 필요 없지만 저장소는 필요하다. DeskQuota의 작은 내장 캐시는 이 구성과 비교하면 설치·실행 요소를 줄인다. 그러나 **LiteLLM도 `cache_params.type: local`을 지원**하므로 외부 Redis가 필요 없다는 이유만으로 독점 기능이라 할 수 없다. [Bifrost 공식 캐시 문서](https://docs.getbifrost.ai/features/semantic-caching), [고정 버전의 store 요구](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/plugins/semanticcache/main.go#L249), [LiteLLM 공식 캐시 문서](https://docs.litellm.ai/docs/proxy/caching), [고정 버전 local backend](https://github.com/BerriAI/litellm/blob/1dba17b10ded12ad0021edb453ba2c54e4637928/litellm/caching/caching.py#L232).

HiveMind는 우리와 가장 가까운 범위의 경쟁자다. 명시적 agent header 외에도 세션 metadata 등으로 agent를 구분하고, AIMD와 fair-share를 제공한다. 현재 DeskQuota의 정적 root 구분·동시성보다 자동 조정 기능이 넓다. 이 기능이 항상 더 좋은 성능을 낸다는 뜻은 아니다. [고정 README의 식별·공정 분배 정책](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/README.md#L106), [provider별 retry](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/src/hivemind/proxy/retry.py#L16).

## 사용자 환경에 대한 해석

**현재 확인한 경쟁력은 작은 상주 비용과 낮은 전달 지연이다.** exact cache와 actual 정산의 존재 자체는 새로운 알고리즘이 아니다. Rust 실행 파일에 제한 관리·캐시·설정 복원을 작게 묶었다는 제품상의 장점으로 평가해야 한다.

캐시가 적중하면 네트워크 추론을 건너뛰는 효과가 전달 경로의 1ms 미만 절감보다 클 수 있다. 그러나 현재 DeskQuota는 기본 history 3, 요청 32KiB, 텍스트 전용이며 tools·상태형 요청을 제외한다. 매번 달라지는 코딩 agent 대화에서 사내 별도 gateway의 40–60% 적중률을 그대로 기대할 근거는 없다. 모든 effective header도 키에 들어가므로 요청마다 달라지는 header는 재사용을 줄일 수 있다. 적중률 분모는 캐시 검사 대상이며 전체 요청과 구분한다.

구체적으로 이번 LiteLLM 환경의 OpenAI Python SDK는 `x-stainless-retry-count`를 재시도 횟수로 설정한다. DeskQuota는 그 헤더를 그대로 캐시 키에 넣는다. 따라서 본문이 같아도 그 값이 달라진 호출은 서로 다른 키가 된다는 **코드상 제약**이 있다. 실제 사내 클라이언트가 그 헤더를 보내는지, 이 이유로 얼마나 miss하는지는 측정하지 않았다. 인증·실제 의미가 있는 옵션을 무시하는 방식으로 적중률을 부풀리면 안 된다. [설치 SDK 버전·해시·관찰 행](../evidence/current-competitor-comparison-2026-09-15/cache-key-sdk-source.json).

개선 우선순위는 다음과 같다.

1. **Windows 배포와 실행 검증.** 회사 PC에서 Build Tools 없이 받은 실행 파일을 바로 쓰는 경로가 아직 가장 큰 제품 공백이다. 소스의 Windows API 수정과 실제 Windows 실행은 구분해야 한다.
2. **실제 요청의 cache bypass와 예약 오차 확인.** 원문을 저장하지 않고 제외 이유·추정 대비 actual·대기 원인을 집계해 병목을 골라야 한다. 동시 중복이 확인돼야 요청 합치기의 복잡도를 정당화할 수 있다.
3. **provider 제한 계약에 맞는 입장 추정.** 완료 usage 정산은 이미 보내는 요청의 큰 예약을 사전에 줄이는 기능이 아니다. 실제 비용보다 큰 예약 때문에 대기하는지부터 확인해야 한다.

현재는 다중 provider 운영 플랫폼이나 검증된 Windows 배포 도구를 대체했다고 말할 수 없다. 단일 PC·단일 endpoint·낮은 상주 비용이라는 범위에서 경쟁할 근거를 쌓는 단계다.

## 감사와 재현

35회 비교에서 측정 요청 2,680건은 **2,632완료 + 46거절 + 2timeout**으로 전부 종료됐다. retry·실제 취소·protocol 오류는 0이었다. 취소 예정 요청이 100ms 이전에 끝났으므로 이 workload가 취소 기능을 검증했다고 표현하지 않는다. warmup 125건, 시작 시 모델 조회 14건과 별도 캐시 실험 180건은 이 분모에서 제외했다. 조건별 결과를 합친 총 성공률로 경쟁 순위를 만들지 않는다.

요청 ID·동일 입력·upstream 시도·종료 상태·각 요청군의 percentile·root별 최대 종료 지연을 원시 기록에서 재계산했다. 시작과 끝의 제품 소스·binary·하네스 해시가 일치했다. Python 경쟁 환경도 핵심 파일 7개와 distribution 버전 39/107개가 준비 당시와 같았다. 전체 설치 파일의 모든 바이트를 검증한 것은 아니다.

- [35회 실행 순서와 정확한 명령](../evidence/current-competitor-comparison-2026-09-15/schedule.json)
- [원시 결과 재계산 및 판정](../evidence/current-competitor-comparison-2026-09-15/audit.json)
- [실행별 세부 지표](../evidence/current-competitor-comparison-2026-09-15/summary.json)
- [설치된 비교 구현 재확인](../evidence/current-competitor-comparison-2026-09-15/installed-source-check.json)

```sh
python3 evidence/current-competitor-comparison-2026-09-15/audit.py
python3 scripts/compare-native-gateways.py --arm deskquota_native_actual \
  --native-binary /path/to/llmgw --profile generation --seed 101 \
  --cost-contract quarter_actual --cap 2 --output /tmp/deskquota-new-comparison
```

이미 있는 출력 폴더를 덮어쓰지 않는다. 기존 비교 스크립트는 고정한 외부 runtime의 로컬 경로를 사용하므로 다른 PC에서는 준비 보고서에 따라 그 환경부터 구성해야 한다. 이번 작업은 제품 기능·의존성을 추가하지 않았고 새 commit/push도 수행하지 않았다.
