# 공유 한도 진단과 재시도 헤더 캐시 개선

2026-09-15. 한도가 있는 LLM 요청을 조정하는 경량 게이트웨이라는 방향을 첫 번째 작은 변경으로 구현했다. Windows 실기 검증은 사용자가 PC를 연결한 뒤 진행한다. 이번에는 새 의존성·DB·설정·스케줄러를 추가하지 않았으며 커밋·푸시·릴리스·외부 LLM 호출도 하지 않았다.

## 적용한 동작

| 변경 | 사용자가 확인할 수 있는 것 | 유지한 경계 |
|---|---|---|
| 캐시 정책 평가 수와 제외 이유 | 캐시를 고려한 요청 중 hit·eligible miss·제외의 비중 | 캐시 비활성 및 ingress 검증 실패는 분모 밖. 원문·사용자별 레이블 저장 없음 |
| 예약량과 관측 usage 비교 | known TPM의 종료된 upstream 시도 수, usage 누락, 알려진 표본의 예약·관측·초과 예약·부족 예약 합계 | 토큰 예측 정확도나 제공자 잔량이 아님. 기존 actual/reserved 정산 그대로 |
| 보호 중인 요청의 실제 병목 표시 | 일반적인 보호 barrier 문구 대신 RPM·TPM·실행 슬롯 등의 대표 원인 | nonempty queue의 cooldown 우선, empty queue는 원인 없음. 선택·기아 보호·wake 정책 그대로 |
| 재시도 횟수 헤더로 인한 miss 제거 | `x-stainless-retry-count`만 다른 동일 요청의 결과 재사용 | 인증·root·전체 URL/query·다른 유효 헤더·본문 바이트 구분 유지. 원본 응답의 해당 `Vary`는 저장 거부 |

캐시 제외 이유는 `request_cache_control → size → endpoint → tools_state → history → unsupported_shape` 순서로 하나만 센다. 평가와 조회가 끝난 상태에서는 `considered = hits + misses + sum(bypasses)`다. 응답 저장 거부와 capture 예산 부족은 요청 적격성과 별개다. 모든 누적 수치는 프로세스 재시작 시 초기화된다.

예약 비교는 기존 `Ledger::finish`가 시작된 생성 시도에 대해 한 번 기록한다. 재시도는 별도 시도이며, cache hit·metadata·시작 전 취소·TPM unknown/unlimited는 제외한다. 유효한 0 usage는 알려진 값이고, 누락·잘못된 합산·overflow는 unknown이다. 토큰 누계는 u128 포화 덧셈과 JSON 십진 문자열을 사용한다. 기존 장부의 차감량을 바꾸는 기능은 아니다.

[명세](../docs/superpowers/specs/2026-09-15-quota-diagnostics-design.md) · [런타임 계약](../product/docs/runtime-contract.md) · [영문 README](../README.md) · [한국어 README](../README.ko.md).

## 직접 실행한 검증

기존 코드에서 retry 헤더 변경 miss, 원본 `Vary` 저장 거부 누락, 진단 필드 부재와 일반 barrier 표시를 실패로 재현한 뒤 수정했다. [구현자의 red/green 기록](../evidence/quota-diagnostics-2026-09-15/impl-results.json).

- 전체 Rust 검사: **435 통과, 0 실패**, 별도 명시적 profiling 1개 제외. 19개 suite 로그를 다시 합산했다.
- Rust 1.88의 fmt, Clippy `-D warnings`, 일반 release 빌드 통과. `bench-harness`는 전체 검사에만 포함했으며 측정 실행 파일은 기본 기능으로 빌드했다.
- 계획·명세·코드 품질 독립 리뷰 승인. 리뷰는 코드 검사이며 실행 증거와 구분한다.
- 71개 소스·manifest 해시를 고정하고 빌드·측정 후 동일함을 확인했다. Cargo 출력과 보관 바이너리는 Desktop/iCloud 밖에 있다.

[검사 로그 목록](../evidence/quota-diagnostics-2026-09-15/validation-01.json) · [Clippy·release 로그](../evidence/quota-diagnostics-2026-09-15/build-01.json) · [리뷰](../evidence/quota-diagnostics-2026-09-15/reviews.json) · [재집계](../evidence/quota-diagnostics-2026-09-15/audit.json).

### 이전·수정 실행 파일의 캐시 동작 비교

각 조건에서 본문을 고정하고 재시도 헤더 값만 0→1로 바꿨다. 실제 SDK 재시도나 실제 모델 호출이 아닌 loopback 합성 요청이다.

| 2개 요청의 조건 | 이전 upstream 호출 | 수정 후 upstream 호출 |
|---|---:|---:|
| JSON, 일반 재사용 | 2 | 1 |
| SSE, 일반 재사용 | 2 | 1 |
| JSON, 해당 헤더를 명시한 `Vary` | 2 | 2 |
| SSE, 해당 헤더를 명시한 `Vary` | 2 | 2 |

각 바이너리에서 8개 응답 모두 200이고 원문 바이트가 일치했다. 실제 upstream 시도는 8→6, 최종 로컬 TPM은 32→24였다. 수정 후에는 `considered=8`, hit 2, miss 6, store 2였으며 usage 표본은 실제 시도와 같은 6개다. `Vary` 대조군은 eligible miss이지만 저장되지 않는다. hit를 새 usage나 새 upstream 시도로 세지 않았다. 첫 probe 실행의 `tpm_debit` 키 오타는 드라이버 오류로 보존했고, 기존 필드인 `tpm_debited`로 고친 동일 스크립트로 양쪽을 검사했다.

[이전 결과](../evidence/quota-diagnostics-2026-09-15/retry-baseline.json) · [수정 결과](../evidence/quota-diagnostics-2026-09-15/retry-candidate.json) · [probe](../evidence/quota-diagnostics-2026-09-15/probe_cache_retry.py).

## 지연과 가벼움의 확인 범위

기존 `benchmark_cache.py`를 재사용했다. 각 실행은 JSON/SSE × direct/miss/hit × 30개, 총 180개 관찰이며 gateway 요청은 120개다. upstream에 50ms 지연을 넣고 절반을 의도적으로 반복한다. 처음 수정본 검사에서 hit p95는 JSON 0.559ms, SSE 1.193ms였고, SSE가 이전 기록보다 높아 추가 비교 전에 [실행 계획](../evidence/quota-diagnostics-2026-09-15/latency-recheck-plan.json)을 고정했다.

동일한 보관 바이너리로 3쌍을 순차 실행했다. 순서는 이전→수정, 수정→이전, 이전→수정이다.

| 쌍 | JSON hit p95 이전 → 수정 | SSE hit p95 이전 → 수정 |
|---|---:|---:|
| 1 | 0.940 → 0.620ms | 0.897 → 0.581ms |
| 2 | 0.981 → 0.934ms | 0.734 → 1.050ms |
| 3 | 0.577 → 1.384ms | 0.769 → 0.839ms |

**일관된 지연 개선을 입증하지 못했다. 진단 비용이 0이라거나 회귀가 없다고 확정할 결과도 아니다.** 실행 간 편차가 있고 각 실행 안의 direct→miss→hit 순서는 고정이며 warmup이 없다. 30개 표본의 p99는 최댓값과 같다. p50/p95/p99와 각 실행의 자원 표본은 [재집계](../evidence/quota-diagnostics-2026-09-15/audit.json)에 보존했다. 좋은 쌍만 골라 속도 향상으로 주장하지 않는다.

최초 검사와 재비교 6회는 모두 180/180 응답·바이트 검사와 SSE 의미 검사를 통과했다. 실행마다 gateway hit 60, miss 60, upstream 60, 실패·취소 0이었다. 의도적 50% hit이며 회사의 실제 적중률이 아니다. 실제 제공자의 429 감소나 고정 시간 내 처리량을 측정하지 않았다.

재비교에서 유휴 RSS는 이전 **9.672–9.719MiB**, 수정 **9.688–9.719MiB**였다. 요청 후 RSS는 이전 **11.063–11.109MiB**, 수정 **11.141–11.203MiB**였다. 약 6.7초 관찰 구간의 프로세스 CPU 증분은 이전 0.06초, 수정 0.06–0.07초다. RSS는 전후 표본이며 peak가 아니고, CPU에는 제어 요청과 전체 worker 동작이 포함된다. 이 Mac의 결과를 저사양 PC 보장으로 옮길 수 없다.

일반 release 크기는 **10,165,760 → 10,183,456 bytes**, 증가분 **17,696 bytes, 약 17.3KiB**다. 새 패키지 의존성은 없다.

- 이전 SHA-256: `7573735f0da96aabbf938a7678f8479ad0a8c591cfd1c0294d85d92f3d1d7e6b`
- 수정 SHA-256: `24a0042abe812a3620affcc02dd091a8558ea92b231ed23facc812bc94cb7712`

## 남은 확인

Windows PC 연결 후, 제공한 실행 파일로 MSVC·Docker·WSL 없이 설치→setup→클라이언트 연결→on/status/off→복원을 확인한다. Tailscale나 시스템 설정은 이번 작업에서 변경하지 않았다. Linux·실제 SDK 재시도·회사 및 외부 API·저사양 검증은 별도로 남는다.

다음 최적화는 새 진단에서 실제 과대 예약·제외 원인을 확보한 뒤 선택한다. 동시 중복 합치기, 토크나이저 추가, 자동 모델 라우팅, 새로운 스케줄러는 넣지 않았다. 이번 결과가 입증하는 것은 좁은 재사용 동작과 상태 진단이며 새 알고리즘·시장 수요·범용 성능 우위가 아니다.
