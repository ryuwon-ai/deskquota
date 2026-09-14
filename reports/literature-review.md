# 논문과 기업 기술자료 검토

조사 기준일: 2026-09-11. 최근 6개월을 우선으로 OSDI 2026, NSDI 2026, MLSys 2026의 공식 proceedings를 확인했다. 공정성의 기초는 오래된 문헌을 별도 표시한다. **발표일, 실험 수행 시점, 비교 구현 버전, 현재 적용 가능성은 각각 다른 값**이다. 학회 게재가 우리 환경에서의 재현을 의미하지 않는다.

검토 깊이: A = 공식 학회 게재와 원문 방법·평가 조건 검토, B = 공식 기업 문서/기술 글 검토, C = 초록·메타데이터 검토. A도 이 PC에서 GPU 실험을 재현했다는 뜻은 아니다.

## 우선 읽은 2026년 학회 논문

| 문헌 | 신뢰 근거·검토 깊이 | 주요 기법 | 적용 층 |
|---|---|---|---|
| [Simple Is Better / LMetric](https://www.usenix.org/conference/osdi26/presentation/zhang-dingyan), OSDI 2026, 7월 | 공식 proceedings, SJTU·Alibaba, A | 새 prefill token 수 × instance batch size로 cache와 load balance 결합 | 서버 telemetry가 있는 다중 instance 라우터 |
| [PLA-Serve](https://proceedings.mlsys.org/paper_files/paper/2026/hash/bbb7506579431a85861a05fff048d3e1-Abstract-Conference.html), MLSys 2026 | 공식 proceedings, A. PDF 제목·방법명은 LAPS | 긴/짧은 prefill 분리, 짧은 batch 대기와 CUDA Graph | 엔진·prefill pipeline 변경 |
| [FastServe](https://www.usenix.org/conference/nsdi26/presentation/wu-bingyang), NSDI 2026, 5월 | 공식 proceedings, PKU, A | token iteration 선점, skip-join MLFQ, GPU/host KV 이동 | 추론 엔진 변경 |

### LMetric: 단순한 지표도 관측 가능한 환경이 전제다

원문 §4.1의 실험 장비는 H20 16개이며 router는 160 Xeon cores·1 TB DRAM이다. 모델은 Qwen2-7B/Qwen3-30B, chatbot·agent·coder traces를 사용한다. §6에서는 비교 정책을 공통 Rust router에 다시 구현해 정책과 구현 언어의 영향을 분리하려 했다. §7은 기본 적용 대상을 같은 모델·동종 GPU pool의 PD-colocation으로 제한한다.

논문은 KV locality와 부하를 함께 보는 단순한 정책의 가치를 보여준다. **사내 API URL 하나에는 per-worker batch size·KV hit 정보가 없으므로 수식을 바로 계산할 수 없다.** 단일 PC proxy의 RPM/TPM admission과도 목적이 다르다. 우리에게 가져올 원칙은 “관측 가능한 작은 지표를 쓰고, 정책과 언어 이득을 분리해 측정한다”다.

공개 [router 코드](https://github.com/blitz-serving/blitz-router)와 [trace 출처](https://github.com/alibaba-edu/qwen-bailian-usagetraces-anon)가 원문에 기재돼 있다. 코드 실행·trace 재생은 미실시다. 저자의 성능 개선 수치를 우리 제품의 예상 개선율로 사용하지 않는다.

### LAPS: 짧은 입력 보호와 실행 엔진 변경을 구분한다

§3과 appendix는 dual queue와 temporal/spatial prefill 분리를 설명한다. artifact는 SGLang v0.4.6.post5 기반이며, 8×H200·200 GB 이상 host memory·CUDA·Mooncake를 요구한다. Appendix의 빠른 평가 workload는 `max_new_tokens=1`로 prefill을 분리해 본다. 긴 tool 호출과 여러 turn을 완주하는 agent task의 완료 시간과는 다른 지표다.

짧은 요청이 긴 prefill 때문에 막히는 현상은 우리 문제와 맞닿는다. 그러나 게이트웨이가 request body를 여러 번 보내는 것은 chunked prefill이 아니다. **short/long 보호 가설과 측정 방법만 참고하고 CUDA batching을 v0.1로 가져오지 않는다.** 최신 SGLang에는 원문 개발 당시 없던 최적화가 추가됐음을 appendix도 인정한다. 최신 baseline과 다시 비교해야 한다.

### FastServe: CPU context switch와 가장 가까우나 proxy로 구현할 수 없다

원문 §4는 token iteration마다 일시 중단할 수 있는 scheduler와 KV 상태 관리, §5는 Python scheduler 및 C++/CUDA 실행 엔진, §6은 A100 실험을 설명한다. 비교 baseline은 **vLLM v0.6.1**, FasterTransformer v5.3이다. NSDI 2026 게재일을 최신 vLLM 상대 우위로 해석하면 안 된다. 같은 FastServe 엔진의 FCFS baseline도 따로 있어 실행 구현의 기여를 일부 분리한다.

우리 gateway가 가진 것은 HTTP 연결이지 GPU KV 상태가 아니다. 연결을 끊고 요청을 다시 보내면 이미 사용한 token·prefill을 다시 지불할 수 있고 응답도 달라질 수 있다. **실행 중 요청의 투명한 선점은 제외하고 다음 요청/turn의 admission 순서를 조절**한다. 엔진 adapter가 실제 pause/resume 계약을 제공할 때 별도 설계 대상으로 검토한다.

## 보조 논문: 신뢰 수준을 섞지 않는다

| 문헌 | 확인 상태 | 지금 사용할 범위 |
|---|---|---|
| [VTC, Fairness in Serving Large Language Models](https://www.usenix.org/conference/osdi24/presentation/sheng), OSDI 2024 | 공식 게재 확인, 이번 작업은 초록 중심 C | 요청 개수보다 input/output 서비스 비용을 고려한다는 기초. continuous batching의 fairness bound를 요청 단위 proxy에 전용하지 않음 |
| [DLPM, Locality-aware Fair Scheduling](https://arxiv.org/abs/2501.14312), 2025-01-24 | 저자·arXiv 확인, 별도 게재처는 이번 검색에서 확인 못함, C | prefix locality와 fairness가 충돌한다는 가설. 기초 보조 문헌 |
| [SMetric](https://arxiv.org/abs/2607.08565), 2026-07-09 | 최근 preprint, 학회 게재 미확인, C | session 첫 요청과 후속 요청을 달리 라우팅하는 가설. global KV store·다중 instance 조건이므로 v0.1 필수 알고리즘으로 채택하지 않음 |

유명 저자가 썼다는 이유만으로 preprint를 심사 완료 문헌으로 취급하지 않는다. 반대로 최신이라는 이유로 실험 코드·조건을 확인하지 못한 논문을 핵심 설계 근거로 삼지 않는다.

## 기업 자료에서 바로 가져올 수 있는 것

| 자료 | 확인 내용 | 우리 작업에 적용 |
|---|---|---|
| [NVIDIA Dynamo agentic inference, 2026-04-17](https://developer.nvidia.com/blog/full-stack-optimizations-for-agentic-inference-with-nvidia-dynamo/) | harness의 priority/출력 길이 힌트, router와 engine의 분리, KV 위치 정보 | 힌트가 없을 때 추측으로 작업 중요도를 결정하지 않음. vendor 글의 shipped 기능·실험 제안은 구분 |
| [Anthropic infrastructure noise, 2026-02-05](https://www.anthropic.com/engineering/infrastructure-noise) | 같은 model/task라도 자원·timeout에 따라 agent 평가 결과가 달라짐 | agent CPU/RAM·tool timeout·gateway queue timeout을 함께 고정. M4 결과를 저사양 Windows 성능으로 표현하지 않음 |
| [OpenAI harness engineering, 2026-02-11](https://openai.com/index/harness-engineering/) | repo에 발견 가능한 근거와 평가 경로를 두는 개발 방법 | 작은 AGENTS, 작업 상태, source pin, 실행 가능한 조사 검증 명령 |
| [Anthropic harness design, 2026-03-24](https://www.anthropic.com/engineering/harness-design-long-running-apps) | 생성/평가 분리, 세션 간 산출물, 구성요소를 하나씩 제거하며 영향 확인 | 한 번에 알고리즘·캐시·모델을 모두 바꾸지 않고 ablation. 복잡한 harness의 자체 비용도 측정 |

## API 제한은 논문보다 공식 계약을 우선한다

[Claude rate limits](https://platform.claude.com/docs/en/api/rate-limits)는 RPM·ITPM·OTPM을 나누고, 현재 문서는 OTPM에 `max_tokens` 대신 실제 생성량을 센다고 설명한다. 대부분 모델에서 cached input의 ITPM 처리도 일반 input과 다르다. spend-cap 429에는 `retry-after`가 없고 재시도로 해결되지 않는 경우가 있다.

[Azure quota](https://learn.microsoft.com/en-us/azure/foundry-classic/openai/how-to/quota?view=foundry-classic)는 prompt·max_tokens·best_of를 포함한 추정 사용량으로 admission 시 제한하고, RPM은 1초/10초 같은 짧은 구간에서도 검사할 수 있다고 설명한다. **응답 usage에서 적게 썼다고 upstream의 admission 예산이 즉시 환급된다는 가정은 위험하다.**

[Ollama FAQ](https://docs.ollama.com/faq)는 내부 bounded queue와 초과 시 503을 명시한다. “Ollama는 큐가 없다”는 전제는 배제한다. 병렬 context가 메모리를 더 쓰므로 앞단의 동시성 상한은 backend 설정과 맞춰야 한다. FAQ 안의 일부 GPU 버전 문구는 오래됐으므로 실제 엔진 버전은 후속 실험에서 별도 확인한다.

## 검증 결과와 후속 조건

- PDF 3편은 [manifest](../papers/manifest.json)에 URL·SHA-256·조회 시각을 남겼다. 방법론·baseline·artifact 페이지는 텍스트와 렌더링으로 확인했다.
- 논문 수치 재현, 최신 엔진 비교, 단일 PC에서의 개선율은 미검증이다.
- 설계 확정 또는 비교 실험 직전에 관련 repo의 최근 30일 변경, 공식 API 제한 문서, paper artifact baseline을 다시 확인한다. 날짜 확인만으로 재검증을 대체하지 않는다.
- 다음 판정: 요청 admission만으로 얻는 이득이 단순 FIFO/round robin 대비 실용적인지 공개 mock에서 먼저 확인한다.

## 2026-09-12: 공개 artifact를 clone해 대조한 후속 결과

[Blitz·LAPS·FastServe source 감사](paper-artifact-source-deep-dive.md)에서 논문 제목·게재일과 실제 checkout의 branch·함수·dependency·benchmark를 대조했다. LAPS main의 문서상 waiting-window 항목은 선택한 runtime 코드에서 확인되지 않았고, 일부 benchmark의 LAPS arm은 CUDA Graph 옵션만 켠다. FastServe artifact README는 NSDI26 논문을 직접 지칭하지만 외부 SwiftTransformer와 사전 환경의 완전한 버전 대응은 미확인이다.

Blitz HTTP backend의 abort 함수는 no-op이며, TLA liveness 가정의 engine ACK를 실제 transport가 지원하는지는 별개다. LAPS timing 함수는 `resp.json()` 완료까지 기다린다. 이를 **원본 함수+합성 loopback1회**로 [확인](../evidence/reference-tests/laps/metric-boundary.json)했다. 기존 논문의 게재 신뢰도와 source/metric 해석의 적합성을 따로 판단한다.

NVIDIA Dynamo는 [별도 source 감사](dynamo-source-deep-dive.md)에서 attempt별 소유권과 공통 cleanup budget을 확인했다. 채택할 것은 작은 수명 관리 원칙이며, GPU/KV/분산 router 전체를 설치 의존성으로 가져오지 않는다.

## 2026-09-13: 이번 개선의 선행 연구와 차별화 경계

OSDI 2026 [LMetric 공식 게재 페이지](https://www.usenix.org/conference/osdi26/presentation/zhang-dingyan)와 NSDI 2026 [FastServe 공식 게재 페이지](https://www.usenix.org/conference/nsdi26/presentation/wu-bingyang)를 다시 확인했다. 전자는 작은 관측 지표를 조합하는 설계 근거, 후자는 엔진 선점과 HTTP 입장 제어를 구별하는 근거로 유지한다. 이번에 논문의 실험을 재실행한 것은 아니다.

새로 발견한 [Scheduling the Unschedulable](https://arxiv.org/html/2604.06970v1)은 2026-04-08 arXiv v1, Zhihu SotaLab 저자들의 black-box API 입장 제어 연구다. 원문 §3–5를 읽었다. 출력 길이 예측을 전제로 DRR 배분·실행 가능 요청 선택·과부하 제어를 나눈다. 주 비교는 혼잡 모형의 mock이며, 실제 API 표본은 지연 보정용이다. [메타데이터](https://arxiv.org/abs/2604.06970)에 심사 학회는 표시되지 않고 코드는 요청 시 제공이라고 적혀 있다. 따라서 **최근 인접 선행 연구로 기록하되 심사 완료 핵심 근거나 재현한 baseline으로 취급하지 않는다.** “API 앞에서 요청을 공정하게 배분한다” 자체는 새 기여로 주장할 수 없다.

[Slurm 공식 문서](https://slurm.schedmd.com/sched_config.html)의 backfill은 높은 우선순위 작업의 예상 시작 시점을 보존하며 실행 시간 추정에 의존한다. 이번 [실험 명세](../docs/superpowers/specs/2026-09-13-bounded-backfill-experiment.md)는 **실행 슬롯 하나를 실제로 남기고, 알려진 다음 RPM/TPM 만료 시점의 여유만 쓰는 제한적 변형**이다. 실행 시간 예측기와 미래 이벤트 전체 탐색을 추가하지 않는다. 새 알고리즘이라는 주장보다, 예측 없는 좁은 조건에서 어떤 낭비를 줄이고 어떤 기회를 포기하는지 재현하는 것이 현재 검증 목표다.

Stanford CS244C의 [agent DAG scheduling 프로젝트](https://www.scs.stanford.edu/26wi-cs244c/proj/dag_sched_agent.pdf)도 검색에서 발견했으나 본문 재조회가 timeout되어 이번 설계 근거에 넣지 않았다. 대학 도메인의 수업 결과물을 학회 심사 논문으로 승격하지 않는다. 공개 경쟁 제품 실행, 실제 작업 정답률, 장기·저사양 검증은 별도 미완료다.
