# 경쟁 구현의 장점 적용: JSON 사용량 정산과 터미널 캐시 상태

상태: 구현·명세 검토·품질 검토·macOS 전체 검사·릴리스 빌드·로컬 비교와 원시 결과의 독립 검산 완료.

## 비교와 에이전트 토론

quota 담당, cache 담당, 최소 구현 검토자가 각각 기존 제품과 DeskQuota를 읽고 교차 검토했다. 참조 저장소는 수정하지 않았다. 이 표는 소스 근거이며 경쟁 제품을 새로 실행한 성능 비교가 아니다.

| 제품·고정 버전 | 직접 확인한 구현 | 채택 판단 |
|---|---|---|
| Bifrost transport v2.1.1, `c193745d2a713e9f58f021d43e138df5eb7e038a` | [nonstream usage 추출](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/plugins/governance/main.go#L1518-L1595), [terminal token 정산](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/plugins/governance/tracker.go#L113-L154) | 일반 JSON 응답도 최종 사용량으로 정산하는 패턴 채택 |
| LiteLLM v1.100.1, `1dba17b10ded12ad0021edb453ba2c54e4637928` | [성공 응답의 reported total 반영](https://github.com/BerriAI/litellm/blob/1dba17b10ded12ad0021edb453ba2c54e4637928/litellm/router_strategy/lowest_tpm_rpm_v2.py#L204-L243) | 같은 패턴의 독립 근거. 해당 소스를 provider 전체의 동일 회계 계약으로 일반화하지 않음 |
| HiveMind HEAD, `0468db5db9d0526e326b4db26971642301d7deed` (버전 문자열 0.2.0, stable tag와 구별) | [JSON 완료 뒤 token 장부 갱신](https://github.com/jayluxferro/hivemind/blob/0468db5db9d0526e326b4db26971642301d7deed/src/hivemind/proxy/interceptor.py#L715-L736) | reported usage 반영은 채택. 0과 누락을 함께 취급하는 추정량 대체는 채택하지 않음 |
| Bifrost transport v2.1.1 | [hit 관측](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/plugins/semanticcache/search.go#L410-L442), [miss 관측](https://github.com/maximhq/bifrost/blob/c193745d2a713e9f58f021d43e138df5eb7e038a/plugins/semanticcache/main.go#L699-L714) | 이미 있는 exact-cache snapshot을 CLI에 표시. DB·대시보드·hot-path 계측 추가 없음 |
| LiteLLM 조사용 HEAD, `9a715df212d777bbd43f4cab05731978c708ec63` | [역할 획득 후 이중 조회와 완료 알림](https://github.com/BerriAI/litellm/blob/9a715df212d777bbd43f4cab05731978c708ec63/litellm/proxy/common_utils/cache_coordinator.py#L118-L196) | DB/config read용 패턴. LLM 생성 요청 singleflight 구현 근거로 사용하지 않음 |

2026-09-15에는 [Bifrost governance 공식 문서](https://docs.getbifrost.ai/features/governance/budget-and-limits), [캐시 문서](https://docs.getbifrost.ai/features/semantic-caching), [LiteLLM 제한 문서](https://docs.litellm.ai/docs/proxy/users)도 확인했다. 최신 문서의 기능과 위 고정 소스의 동작은 구별한다.

## 이번 선택

기존 JSON 응답은 usage가 있어도 `usage_unknown`으로 끝났다. 앞선 [사내 피드백 실험](company-feedback-2026-09-15.md)의 JSON 30건은 fixture상 실제 120토큰을 반환했지만 예약 4,130을 유지했다. 이는 개선 가능성을 보여주는 이전 결과이며, 이번 변경의 측정 결과가 아니다.

기존 EOF observer와 quota 장부를 확장한다. JSON과 SSE 관측 버퍼는 동시에 사용하지 않고, 정상 body 전달을 유지한다. 도구 호출·출력 길이 제한 등은 유효한 usage가 있으면 정산할 수 있으나, exact-cache에 재사용할 수 있는 응답인지는 별도로 판정한다. 새 의존성·설정·서버를 추가하지 않는 것이 채택 조건이다.

cache snapshot은 `status --json`에 이미 있으므로 일반 `llmgw status`에서도 켜짐 여부와 hit·eligible miss·메모리 상태를 보여준다. 표시를 통한 설정 확인의 이득이며, 그 자체를 성능 향상으로 세지 않는다.

## 보류한 대안과 반례

admission 직후 캐시를 한 번 더 조회하면 concurrency 대기 중 채워진 결과를 재사용할 수 있다. 기존 미시작 Hold의 Drop이 예약을 취소하므로 새 refund API는 필요 없다. 그러나 RPM=1을 소진한 요청은 admission을 받기 전에 최대 60초 기다린다. 캐시가 채워져도 재조회 지점에 도달하지 못하므로 quota 대기의 핵심 문제를 해결하지 않는다. 조회 통계의 이중 집계 계약도 정해야 한다.

전체 singleflight는 leader 취소·실패·저장 불가 응답, follower deadline과 대기 cap, SSE 첫 출력과 느린 reader 버퍼를 추가로 다뤄야 한다. 실제 동일 키 동시 중복률과 대기 원인부터 확인한 뒤 검토한다. 확인한 Bifrost/LiteLLM 생성 경로에서 직접 채택할 singleflight 근거를 찾지 못했으며, 이를 모든 파일·버전에 기능이 없다는 주장으로 일반화하지 않는다.

## 제공자 제한과의 차이

실제 응답 토큰으로 로컬 예약을 줄이는 것이 upstream 한도를 되돌리는 동작은 아니다. [OpenAI 문서](https://developers.openai.com/api/docs/guides/rate-limits#reduce-the-max_tokens-to-match-the-size-of-your-completions)는 요청 추정량·출력 상한에 기반한 제한을 설명한다. 그런 환경은 명시적 `accounting = "reserved"`가 필요할 수 있다. [Claude 문서](https://platform.claude.com/docs/en/api/rate-limits#cache-aware-itpm)는 ITPM/OTPM을 나누고 대부분 모델의 cache-read 입력을 다르게 취급한다. DeskQuota의 단일 combined TPM은 그 한도를 그대로 재현하지 않는다.

요청을 보내기 전의 바이트 기반 예약 추정은 그대로다. 추정량 자체가 설정한 TPM 한도를 넘으면 첫 요청부터 들어가지 못하므로, 응답 뒤 정산으로 해결되지 않는다. JSON이 256 KiB를 넘거나 usage를 반환하지 않는 서버에서도 이번 환급 이득은 없다. 관측에는 제한된 버퍼와 EOF JSON 파싱 비용이 추가된다.

## 검증 기록

- 변경 전 바이너리: `/Users/ryuwon/Library/Caches/deskquota-round2-2026-09-15/baseline-llmgw`, SHA256 `56bf6dabb4e51f534bd4883d73a0a6efd4cb7049af9acfcefa45fcd62720806a`, 10,164,576 bytes.
- 소스·바이너리 기준선: `evidence/competitor-round2-2026-09-15/baseline.json`.
- 설계: [JSON 정산 계약](../docs/superpowers/specs/2026-09-15-competitor-round2-design.md).
- [비교 probe](../product/scripts/probe_json_accounting.py)는 5회씩 실행 순서를 바꿔 baseline/candidate를 비교한다. cache를 끄고 concurrency=1, TPM=244, 요청당 예약232, 반환 usage4인 JSON 4건 burst를 보낸다. 750ms는 제공자나 gateway deadline이 아니라 **실험 client의 대기 한도**이며, 초과한 client는 RST로 닫고 queue 정리를 확인한다. `reserved`와 usage 누락은 환급 없는 대조군이다. 별도 no-wait arm은 프로세스마다 5회 warmup 뒤 20회 측정하며, 합친100개/arm의 분포는 반복 실행의 기술통계다.
- 변경 전 probe 점검: actual·reserved·missing 각각 1완료/3 client timeout, upstream1회·TPM예약232 유지. no-wait25건 완료. 이는 driver 분기 확인이며 변경 후 결과가 아니다 (`probe-baseline-check.json`, `probe-baseline-controls.json`).

### 직접 실행한 결과

macOS 26.5.1, Apple M4, RAM 32 GiB에서 Rust 1.88.0으로 실행했다. 전체 `--all-targets --all-features` 검사는 **427 통과·0 실패·1 무시**다. 무시한 검사는 profiling용이며, 별도의 107개 관련 검사는 전체 검사에 포함되므로 합산하지 않는다. `fmt --check`, all-target/all-feature clippy와 default-feature release 빌드도 통과했다.

새 바이너리는 **10,165,760 bytes**, SHA256 `7573735f0da96aabbf938a7678f8479ad0a8c591cfd1c0294d85d92f3d1d7e6b`다. 이전보다 **1,184 bytes** 증가했고 의존성·설정 추가는 0개다. 빌드·테스트는 모두 iCloud 밖의 Cargo cache를 사용했다. 실행 파일과 소스 해시는 `final-build-01.json`에 기록했다.

**TPM 기전 비교:** 2026-09-15 04:07:00–04:07:22 UTC, 총40개 순차 실행·370요청(별도 warmup50건 포함)이다. 295건 완료·75건 client timeout, upstream295회, HTTP/protocol error·독립 cancelled outcome·rejected는 0건이다. timeout75건은 client RST로 정리했으며 취소로 중복 집계하지 않았다. 모든 완료 응답 원문이 일치하고, 각 실행 뒤 active·queue·held TPM이 0임을 확인했다.

| 조건, 각 arm5회 반복 | 이전 완료 / 제출 | 개선 후 완료 / 제출 | 각 실행의 최종 로컬 TPM: 이전 → 개선 |
|---|---:|---:|---:|
| Actual, usage 입력3+출력1 | 5/20 (timeout15) | **20/20 (timeout0)** | 232 → **16** |
| Reserved, 같은 usage | 5/20 (timeout15) | 5/20 (timeout15) | 232 → 232 |
| Actual, usage 누락 | 5/20 (timeout15) | 5/20 (timeout15) | 232 → 232 |
| No-wait, warmup 포함 | 125/125 | 125/125 | 5,800 → 100 |

Actual의 새 실행은 네 요청을 실제 합계16토큰으로 정산하고, 기존 실행은 첫 요청의 예약232가 남는다. **이 설정에서 불필요한 로컬 대기가 줄었음을 확인했다.** fixture는 upstream quota를 집행하지 않으므로 제공자의 실제 허용량 증가·429 감소나 일반적인 4배 처리량을 의미하지 않는다. Reserved와 usage 누락 대조군에서 임의 환급이 생기지 않은 것도 함께 확인했다.

No-wait는 각 실행의 warmup5건을 제외한100개/arm의 loopback 전체 응답시간이다. 직접 연결과의 overhead 차분이나 독립100회 실험이 아니다.

| No-wait, ms | p50 | p95 | p99 | 최대 |
|---|---:|---:|---:|---:|
| 이전 | 0.196 | 0.263 | 0.301 | 0.317 |
| 개선 | 0.190 | 0.263 | 0.290 | 0.312 |

합친 표본의 p95는 같은 값으로 반올림되지만, 일부 반복에서는 개선 후 p95가 더 높았다. 이 작은 기술통계로 속도 우위를 확정하지 않는다. 전후 RSS 표본 범위는 no-wait 이전9.641–10.797 MiB, 개선9.641–10.844 MiB다. 각 실행의 CPU time 증분은 `ps` 정밀도에서 0.00초로 표시돼 parser 비용을 구분할 수 없다. CPU 사용량이 0이라는 뜻은 아니다. RSS도 peak가 아니다.

**캐시 회귀 측정:** 50ms 합성 지연(스트리밍에 추가5ms 간격), JSON/SSE 각각30표본, direct→miss→hit 고정 순서로180건을 실행했다.

| 형식·경로, 전체 응답시간 ms | p50 | p95 | p99 |
|---|---:|---:|---:|
| JSON direct | 52.557 | 53.253 | 53.391 |
| JSON miss | 53.174 | 54.236 | 54.378 |
| JSON hit | 0.253 | 0.507 | 1.084 |
| SSE direct | 58.891 | 60.666 | 61.065 |
| SSE miss | 59.214 | 60.036 | 60.055 |
| SSE hit | 0.274 | 0.687 | 1.079 |

gateway120건=miss60+hit60, gateway upstream60회, direct60회이며 실패·취소는 0건이다. 모든 응답 원문과 SSE 의미 검사가 통과했다. **usage known60·unknown0**, 입력180·출력60, TPM240·RPM60으로 실제 정산했고 cache hit는 usage·quota를 추가 차감하지 않았다. cache60entry·18,780bytes, eviction/budget bypass0을 확인했다. RSS는9.656→11.000 MiB 전후 표본, CPU 증분은약6.73초 구간에서0.06초다.

50% hit rate는 의도적으로 반복한 입력의 비율이며 실사용 적중률이 아니다. 표본30개의 p99는 최대값이고, 각 arm의 percentile 차이를 요청별 proxy overhead로 해석하면 안 된다. 이 실험은 기존 캐시의 재검증이며 이번 변경으로 캐시 지연이 더 줄었다는 주장도 아니다.

일반 CLI에서도 `exact cache: disabled; hits: 0; eligible misses: 0; entries: 0; retained: 0/4194304 bytes` 출력을 실제 확인했다. 활성화·누락 상태의 표시와 `n/a` 처리는 제품 검사로 확인했다.

원시 결과는 `paired-01.json`, `cache-01.json`, 집계는 `parent-summary.json`이며 모두 `evidence/competitor-round2-2026-09-15/` 아래에 있다. Windows·저사양 실기기·실제 제공자·task 품질·경쟁 제품 전체 성능 우위는 이번 검사로 증명하지 않는다. 커밋·푸시·릴리스 배포·전역 설치·실제 API 호출은 하지 않았다.

독립 검토자가 원시 행·driver의 원문 비교·70개 소스 해시·두 바이너리·19개 테스트 suite·보고서·두 README를 재검산해 승인했다. 검토 자체는 추가 실행 실험이 아니다. 결과는 `result-audit.json`, 최종 상태는 `final-integrity.json`에 남겼다.
