# UniBoost 추가 검증과 HTTP quota gateway 적용 경계

확인일: 2026-09-14 KST. [기존 문헌 검토](literature-review.md)를 먼저 읽고, **Beyond Prediction: Tail-Aware Scheduling for LLM Inference** 한 편만 추가 검증했다. 문서·원문 조회이며 제품 성능 측정이나 논문 재현이 아니다. 기존 OSDI/NSDI/MLSys 조사보다 모두 늦게 발표된 논문이라는 뜻도 아니다.

## 게재 상태와 확인 수준

**직접 검증:** Microsoft 공식 페이지의 실제 연결 대상은 ICML 포스터와 arXiv PDF다. ICML 공식 포스터 페이지도 제목·저자와 ICML 2026 등록을 제공하므로 학회 채택을 확인했다. arXiv는 2026-06-16 제출된 v1이며 학회명을 기재한다. 읽은 파일은 arXiv 사본이다. 학회 채택과 동료심사 의견 원문 열람을 구분한다. [Microsoft](https://www.microsoft.com/en-us/research/publication/beyond-prediction-tail-aware-scheduling-for-llm-inference/), [ICML 공식 포스터](https://icml.cc/virtual/2026/poster/63644), [arXiv 이력](https://arxiv.org/abs/2606.18431).

**미확인:** ICML에서 연결한 OpenReview는 브라우저 검증 화면이어서 리뷰·결정문을 읽지 못했다. 별도 PMLR 출판 페이지도 검증하지 않았다. 저자 프로젝트의 `Code` 링크는 조회 당시 실제 `href="#"`였다. 이 경로에서 공개 구현을 확보하지 못했으며, 다른 곳에도 코드가 없다고 단정하지 않는다. [OpenReview](https://openreview.net/forum?id=V40F5O5N85), [저자 프로젝트](https://yl3469.github.io/uniboost-icml26/).

## 논문이 실제로 바꾸는 것

**원문 근거:** UNIBOOST는 요청별 출력 길이를 예측하지 않고 도착 시각에서 진행량 기반 boost를 빼 우선순위를 만든다. prefill/decode를 같은 순위로 비교하며, MEMGUARD는 기하급수적인 토큰 경계에서만 재선점을 검토한다. γ-Ada는 P95–P99 응답시간의 꼬리 기울기를 이용한다. KV 사용량·victim 선택·토큰 단위 실행 제어가 필요하다. M/G/1·지수모멘트 가정의 증명은 실제 GPU 연속 배칭 전체의 무조건적인 기아 방지 보장이 아니다. [원문 §3, Appendix A.1–A.2, 인쇄 5–7·12–13쪽](https://arxiv.org/pdf/2606.18431).

**저자 실험:** 주 실험 장비는 8×A100 80GB, 96 vCPU, 호스트 메모리 248GB다. Llama-3-8B·CodeLlama-34B·Qwen-72B와 Azure·ShareGPT·s1k, 10,000개 요청을 사용했다. prefix/radix cache는 끈 조건이다. 비교군은 Sarathi, 완벽한 길이를 제공한 LTR+/TRAIL+, 이상화한 LAS/MLFQ+이므로 원본 저장소 제품 간 비교로 해석하지 않는다. [원문 §4, 7쪽](https://arxiv.org/pdf/2606.18431).

Table 2의 Llama-8B·reasoning 70%/Azure 30% 조건에서 TRAIL+ 대비 결과는 다음과 같다. 지연 개선율의 분모는 baseline이다. **이 값은 저자 보고이며 우리 재현값이 아니다.** [원문 §4.1·Table 2, 7–8쪽](https://arxiv.org/pdf/2606.18431).

| 지표 | UniBoost 변화 |
|---|---:|
| P99 전체 응답시간 | 35.1% 감소 |
| 평균 전체 응답시간 | 19.1% 증가 |
| P99 첫 토큰 시간 | 34.0% 감소 |
| 평균 토큰 간 시간 | 25.3% 증가 |
| 토큰 처리량 | 1.2% 증가 |

저자 웹 설명에는 평균과 꼬리가 함께 개선된다는 일반화도 있지만 위 표는 평균 악화를 명시한다. 결과 해석은 표의 조건·분모를 우선했다. [저자 프로젝트 Results와 Table 2](https://yl3469.github.io/uniboost-icml26/).

## 우리 제품에 대한 판단과 다음 행동

아래는 **설계 추론·제안**이며 논문이 우리 제품에서 검증한 결과가 아니다.

| 구분 | 적용 판단 |
|---|---|
| 지금 활용할 근거 | 평균·완료량만 보면 긴 요청의 손해를 숨길 수 있다. root별 대기시간, P95/P99, 가장 오래 기다린 요청, deadline 내 완료 비율을 함께 판단한다. |
| 시작 전 대기열에서 검증할 것 | quota 때문에 막힌 요청의 예정 입장과 슬롯을 보호하면서 남는 용량에 다른 root의 요청을 넣는 제한적 우회다. 작은 요청만 끝없이 우선하지 않도록 큰 요청의 지연 악화를 함께 기각 기준으로 둔다. 현재 backfill 후보의 성능은 별도 실행 결과로 판정한다. |
| 그대로 옮길 수 없는 것 | HTTP 프록시는 upstream의 KV 소유권·token step 실행·정확한 남은 시간을 제어하지 않는다. SSE 중단 후 재요청은 상태를 보존하는 선점/재개와 다르며, 이미 소모한 토큰·RPM을 회수한다고 가정할 수 없다. MEMGUARD의 토큰 경계나 보장을 HTTP 요청에 붙이지 않는다. |
| 당장 넣지 않을 것 | γ 자동 튜닝, 출력 길이 예측 모델, 엔진별 KV 어댑터. 이 논문 하나로 네이티브 quota core에 추가 의존성과 정책 조절값을 늘릴 근거는 부족하다. |

다음 실험에서는 같은 요청·도착 시각·quota·서비스시간으로 정책을 비교하고, 성공·429·거절·취소·deadline 초과를 모두 ingress 분모에 남긴다. 가벼운 요청의 개선만으로 채택하지 않고 긴 요청의 완료량·지연, 추가 upstream 시도, 메모리·CPU 비용까지 함께 본다. 이 제안은 기존 측정 계약에 맞춘 것이며 이번 조사에서 새로운 하네스나 측정은 실행하지 않았다.

현재 근거로 주장 가능한 것은 **2026년 학회 연구도 긴/짧은 작업 사이의 꼬리 지연과 선점 비용을 명시적으로 다룬다**는 점이다. 우리 gateway의 논문급 기여·경쟁 우위·저사양 효율은 아직 이 자료로 입증되지 않는다. 원문 자체에 이미 우선순위 조절과 기아 방지 계열 선행 연구가 있으므로 이 개념만으로 새로움을 주장해서도 안 된다.

조회 URL·PDF 해시·확인 범위·접근 실패는 [기계 판독 증거](../evidence/recent-scheduling-evidence-2026-09-14.json)에 기록했다. 제품·기존 manifest·참조 checkout은 수정하지 않았다.
