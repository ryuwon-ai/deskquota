# Backfill 실험 기록

상태: 2026-09-14 이번 탐색 round 완료. **60초5시드 반복에서 짧은 요청의 평균 최종 완료시간이16.3% 감소했다. 처리량과 짧은 요청 p95 개선은 관찰하지 못했다.** 동일 합성 workload의 jitter 반복이며 일반 기본값 승격이나 실제 API 성능 입증은 아니다.

Ponytail의 최소 구현 원칙과 ECC `benchmark-optimization-loop`를 적용한다. 새 런타임이나 학습된 예측기 없이 기존 ledger/queue에 한 가지 후보를 넣는다. [명세](../docs/superpowers/specs/2026-09-13-bounded-backfill-experiment.md)와 [독립 설계 검토](../evidence/backfill-design-review.md)가 범위를 정한다. production default와 배포용 패키지는 그대로 보존한다.

## 비교와 판단 기준

- 공통 Rust HTTP transport의 기존 `benchmark_rr`와 새 `benchmark_backfill`. 둘은 같은 feature binary를 사용하고 정책 선택만 다르다.
- 기존 합성 composite workload, RPM16/TPM6000 fixture 단위, concurrency2, 4roots, 60초 장부창, retry0/cache off. quota 측정은 2개 창을 우선 사용한다. startup60초는 분리한다.
- 모든 ingress의 완료·거절·오류·timeout·취소와 실제 upstream 시도를 포함한다. 고정120초 안의 완료량과 이후 drain을 분리한다. 짧은/긴 요청을 각각 비교한다.
- 첫 pair에서 유의미한 변화가 없거나 긴 요청 완료·timeout이 악화되면 일반 기본값으로 승격하지 않는다. 보호한 head의 조건부 기회 보존은 다른 모든 미래 요청의 최대대기 보장이 아니다.
- 반복 단계는 5seed 이상 및 arm 순서 교차가 필요하다. 기존 composite는 개발 중 이미 살펴본 workload이므로 새로운 holdout이라고 부르지 않는다. cap1은 이득을 기대하지 않는 음성 대조다.
- native mock 지연은 실제 API/GPU 지연이나 정답을 푼 agent task 성능이 아니다. 원래 패키지의9.73MiB와 과거 무대기 latency를 새 후보의 측정치로 재사용하지 않는다.

## 재현 진입점

`python3 -B scripts/run-backfill-experiment.py --self-check`는 비교 분모·binary·config·arm 불일치를 거절하는 검사다. 실제 gateway 실행은 아니다.

```sh
python3 -B scripts/run-backfill-experiment.py \
  --binary /Users/ryuwon/Library/Caches/llmgw-cargo/release/llmgw \
  --reference /Users/ryuwon/Library/Caches/llmgw-cargo/release/examples/bench_gateway \
  --output product/artifacts/backfill-resume-2026-09-14/pilot-cap2 \
  --seeds 1 --windows 2
```

출력 경로가 이미 있으면 실패해 과거 run을 덮어쓰지 않는다. `manifest.json`은 각 raw run의 SHA·binary·harness와 비교값을 연결한다. 런너는 자체2700초 한도를 둔다. CLI가 끝나도 수용 기준 충족 여부는 별도 판정한다.

## 직접 측정: 120초 파일럿

동일한 `bench_gateway` SHA `96a6e569c8cce48b88643dfd786c233f0eccd9bdbd42ab7691513c02c12b3216`, seed1, 40개 요청/arm. 전체 실행508.25초에는 각 프로세스의60초 startup hold와 drain이 들어 있다. 아래 지연은 LLM 생성 속도가 아닌 요청 제출부터 완료까지의 시간이다.

| 항목 | RR | Backfill |
|---|---:|---:|
| 120초 내 완료 / 제출 | 26 / 40 | 26 / 40 |
| 최종 완료 / 취소 | 34 / 6 | 34 / 6 |
| 거절 / 오류 / timeout | 0 / 0 / 0 | 0 / 0 / 0 |
| 실제 upstream 시도 / 추가 retry | 34 / 0 | 34 / 0 |
| 짧은 요청 평균 완료시간, 성공30개 | 32.017초 | 20.458초 |
| 짧은 요청 p95, 성공30개 | 107.044초 | 107.014초 |
| 긴 요청 평균 완료시간, 성공4개 | 60.254초 | 60.255초 |
| 긴 요청 최댓값, 성공4개 | 120.207초 | 120.212초 |
| 전체 성공34개의 p95 | 119.871초 | 107.021초 |

짧은 요청 평균은 이 표본에서36.1% 감소했다. 개별 요청의 감소는 최대71.47초다. 그러나 짧은 요청 p95는 거의 같고 처리량도 같다. 전체 p95만으로 tail 문제가 해결됐다고 말할 수 없다. 긴 요청의 몇 ms 차이를 개선 또는 악화로 판정하지 않는다. 실제 긴 요청은 중단·재실행하지 않는다.

초기 RSS는 RR9,125,888B / 후보9,142,272B, 측정 중 관측 최대는11,075,584B /11,108,352B였다. 각각 초기1개 및 주기 표본의 관찰이며 idle 분포나 최소 PC 사양의 증명은 아니다. arm 시작/종료 시 load average도 raw에 남겼다. 이 환경의 몇 ms 차이를 순수 proxy overhead로 사용하지 않는다.

[원시 run과 manifest](../product/artifacts/backfill-resume-2026-09-14/pilot-cap2/manifest.json), [독립 사후 audit](../product/artifacts/backfill-resume-2026-09-14/pilot-audit.log), [소스·검사·리뷰](../product/artifacts/backfill-resume-2026-09-14/review-and-source-check.json)를 보존했다. 80개 ingress 모두 정확히 한 번 종결되고, 양쪽 실제 시도 수와 선언 설정 재구성·관측 quota/accounting/root가 맞았다. example은 lifecycle `identity: null`이므로 전체 runtime 설정 지문을 인증했다는 뜻은 아니다. cap/cancel/retry 전체 설정은 생성 TOML·실행 소스 근거와 구별한다.

## 후속 측정 범위

45분 전체 round wall budget 안에서 cap1 음성 대조1쌍과 seed2–6의 교차 순서5쌍을60초 측정창으로 실행했다. startup과 종료 후 drain은 계속 포함해 관찰한다. 창마다 같은20개 요청 구성을 사용하며, 120초 파일럿40개와60초 반복20개는 별도 집계한다. 평균은60초 내 완료만이 아니라 **측정창 뒤 drain까지 포함한 최종 완료시간**이며, 모든 ID의 terminal 결과와 성공 집합의 동일성을 먼저 확인한다. 이 jitter 반복을 새로운 workload·holdout 또는120초5쌍으로 부르지 않는다. 새 holdout과120초5쌍은 실행하지 않았고 완료로 세지 않는다.

## 실제 API 후속

사용자 제안에 따라 [NVIDIA hosted API 소량 검증](nvidia-api-evaluation.md)을 추가했다. 키와 계정 무료 범위가 확인되기 전에는 실제 API 호출을 하지 않는다. 실제 API는 모델 응답·혼잡·제한 계약의 확인에 쓰며, 모의 정책 실험과 합쳐 단일 개선률로 보고하지 않는다.

## 직접 측정: 60초5시드 반복과 cap1 대조

| 시드 / 실행 순서 | 짧은 성공 요청 평균 RR | Backfill | 평균 감소 |
|---|---:|---:|---:|
| 2 / RR→Backfill | 14.718초 | 12.322초 | 16.28% |
| 3 / Backfill→RR | 14.718초 | 12.321초 | 16.29% |
| 4 / RR→Backfill | 14.722초 | 12.323초 | 16.30% |
| 5 / Backfill→RR | 14.721초 | 12.325초 | 16.28% |
| 6 / RR→Backfill | 14.721초 | 12.324초 | 16.28% |

각 arm은 총100개 제출,85개 최종 완료,15개 취소,60초 창 내 완료55개였다. 거절·오류·timeout·추가 retry는 모두0이며 실제 upstream85회/arm이다. 모든 ID의 최종 결과가 양쪽에서 같았다.

짧은 요청 성공75개/arm의 평균은14.720초→12.323초다. 그중15개가1초 이상 빨라졌고,1초 이상 느려진 짧은 요청은0개였다. 나머지 모두가16.3%씩 빨라졌다는 뜻은 아니다. 짧은 p95는 양쪽 약47.0초로 거의 같았다. 긴 요청 성공10개/arm의 시드별 평균 차이는−0.97~+2.57ms였고, 완료·취소 수는 동일했다. 다섯 시드는0–10ms 도착 jitter를 바꾼 반복이지 다섯 종류의 사내 workload가 아니다.

cap1 음성 대조에서는20개 제출/arm,17개 완료·3개 취소,60초 내11개 완료였다. 짧은 평균14.859초→14.860초로, 유의미한 이득을 관찰하지 못했다.

[반복 원시 manifest](../product/artifacts/backfill-resume-2026-09-14/repeat-cap2-window1/manifest.json), [전체 반복 audit](../product/artifacts/backfill-resume-2026-09-14/repeat-audit.json), [계산 가능한 통계](../product/artifacts/backfill-resume-2026-09-14/repeat-descriptive.json), [cap1 통계](../product/artifacts/backfill-resume-2026-09-14/negative-cap1-descriptive.json), [최종 검증](../product/artifacts/backfill-resume-2026-09-14/final-verification.json)을 연결한다. `audit.py`와 `summarize.py`는 해당 artifact 폴더에 보존했다.

전체14개 native run의320개 ingress가 모두 종결됐고 실제 upstream 시도272회를 확인했다. benchmark 실행 시간 합계는2112.73초, 최종 round 확인까지 wall 시간은2599.64초로2700초 한도 안이다. 모든 소유 worker를 정리했고 소스102개·기존 배포 패키지의 SHA도 유지됐다.

## 판정과 다음 단계

후보를 **추가 검증할 가치가 있는 실험 코드로 보존**한다. 일반 실행에는 RR을 유지한다. 이 결과는 quota 때문에 묶인 일부 작은 요청의 입장 시점을 앞당기는 근거다. LLM 생성 속도, 전체 처리량, 모든 root의 tail 개선, 저사양 환경·경쟁 제품 우위의 증거는 아니다.

다음 수용 조건은 이미 본 workload와 다른 holdout,120초5쌍의 별도 반복, 전체 장부에서의 scan 비용 확인이다. NVIDIA는 먼저 준비된 소량 도구로 모델·SSE·usage·제한 응답을 확인해야 한다. 키 위치가 아직 전달되지 않아 실제 인증 호출은0회다. 이 단계들을 통과하기 전에는 기본값 승격·논문급 성과·OSS 채택을 주장하지 않는다.

[추가3개 경쟁 구현의 고정 소스 분석](additional-quota-references-2026-09-14.md)과 [ICML2026 UniBoost 검토](recent-scheduling-evidence-2026-09-14.md)를 함께 반영했다. OS식 스케줄링이라는 큰 개념의 선행 작업이 있고, 엔진의 token/KV 선점은 HTTP gateway의 요청 대기와 다른 제어 범위다.
