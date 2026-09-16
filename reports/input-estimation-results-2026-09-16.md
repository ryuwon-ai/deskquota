# 선택형 입력 토큰 추정: 구현과 검증 결과

2026-09-16. **모델별 BPE 입력 추정을 선택할 수 있도록 구현했다.** TPM 때문에 과도하게 기다리던 합성 요청은 더 많이 완료했지만, 모든 요청이 빨라지는 변경은 아니다. 기본값은 기존 `utf8_bytes`이며 출력 상한·요청 본문·응답 usage·RR/FIFO 정책은 유지했다. 실제 제공자 API나 경쟁 제품을 이번에 다시 측정한 결과는 아니다.

## 직접 검증한 개선

일반 release 바이너리 두 개를 같은 Apple M4/32GiB Mac에서 순차 실행했다. 주요 비교는 동일 입력을 세 번 반복했고 순서는 이전→후보, 후보→이전, 이전→후보로 바꿨다. 후보는 `cl100k_base`와 입력 여유분32를 명시적으로 설정했다. 캐시와 재시도는 껐다. 새 mock의 장부가 비어 있으므로 두 arm 모두 `startup_hold_secs=0`을 사용했다. 실제 사용자의 기본 시작 대기60초를 없앤 효과는 포함하지 않는다.

| 합성 조건 | 이전 → 후보 완료 수, 실행당 | 실패 변화 | 성공 요청 p95, 실행별 범위 |
|---|---:|---|---:|
| TPM 중심 혼합 burst12건 | **10/12 → 12/12** | client timeout2 → 0; upstream429는 양쪽0 | 280.30–384.93 → 255.52–258.87ms |
| 입력 이력이 누적되는 순차6건 | **4/6 → 6/6** | client timeout2 → 0; upstream429는 양쪽0 | 103.15–103.33 → 103.32–103.64ms |
| 기존 RPM16/TPM6000 혼합18건 | 18/18 → 18/18 | 실패0 유지 | 40.633–40.635 → 38.130–38.132초 |

첫 두 조건의 client 대기 한도는 요청당1.5초다. 무한 대기 후의 전체 처리량이 아니라 **이 대기 한도 안에서 완료한 요청 수**가 늘었다. upstream 비용은 별도 `tiktoken0.14.0`으로 확인한 문자열 토큰 수와 명시한 합성 채팅 형식을 사용한다. 후보의 예약이 실제 fixture 비용보다 작은 요청은 없었다. 영어·한국어·반복 문자열을 섞었으며, 순차6건은 미리 정한 이력이 커지는 요청이다. 실제 에이전트의 과제 해결이나 대화 품질을 검증한 것은 아니다.

Burst의 성공 평균은 오히려 **139–166ms → 179–204ms**로 늘었다. 성공 집합이10건과12건으로 다르고 실행 순서도 바뀐다. 공통 성공10건의 p95는 각 쌍에서 줄었지만 모든 요청·평균 지연의 개선을 주장하지 않는다. 순차 호출 역시 실행된 요청의 p95 개선은 관찰하지 못했다. 주된 이득은 불필요한 TPM 대기와 시간 초과 감소다.

## 기존 40초 꼬리 지연과 보정 대조군

기존18건 실험의 첫60초 내 완료는 **16 → 16**, 최종 완료는 **18 → 18**이다. p95 약2.5초 감소가 RPM 한도나 지속 처리량을 늘린 결과는 아니다. 18개 표본의 nearest-rank p95는 최댓값이므로 안정적인 서비스 tail의 통계로 일반화할 수 없다.

이 실험은 실제 모델 tokenizer가 아닌 **원본 JSON 바이트/4**라는 과거 합성 비용 계약을 그대로 유지했다. 기본 여유분32에서는8/18건의 예약이 최종 사용량보다 작았고, 부족분 합계는 실행당419였다. 429가 없었다고 정확한 추정이나 모든 한도 상황에서의 안전성을 입증한 것은 아니다.

이 문제를 분리하려고 같은18건에 **여유분256을 지정한 추가 대조1회**를 실행했다. 모든 요청의 입력 추정이 fixture 입력량 이상이었고, 최종 예약 부족도0이었다. 결과는 **18/18 완료, upstream429/timeout0, p95 38.130초**, 첫 창 완료16건이었다. 따라서 이 고정 trace에서는 과소 예약을 없애도 약2.5초의 대기 감소가 남았다. 이는 단일 보정 검사이며 세 번의 독립 보정 비교나 제공자별 권장 여유분의 근거는 아니다.

[기존 원인 분석](latency-tail-root-cause-2026-09-16.md)의 과대 예약 문제는 줄었지만, RPM 만료 대기와 같은 root의 FIFO는 그대로 남는다. 과거 RR/backfill 실험의 약47초 p95 수치를 이번 결과로 교체하지 않는다.

## 실패하는 조건도 보존했다

서버가 입력을 바이트로 차감하는 별도 반례를 세 번 실행했다. 이전은 매번 **1건 완료·1건 client timeout**, 후보는 **1건 완료·1건 upstream429**였다. 같은 성공 개수이며 한 실행에서는 성공한 요청 ID도 달랐다. 빠른 거절을 성능 향상으로 계산하지 않는다.

전체 JSON BPE는 서버의 실제 prompt 형식, 숨겨진 토큰, 별도 입력·출력 한도, 제공자 예약 공식을 알 수 없다. OpenAI-compatible URL만으로 encoding을 선택해서는 안 된다. **`cl100k_base`/`o200k_base`는 명시적 선택이고, 여유분32도 정확도 보장이 아니다.** 맞지 않는 서버에서는 기본 바이트 추정을 유지하거나 서버 계약에 맞춰 설정해야 한다. `llmgw status`의 예약 부족·초과 관측도 함께 확인할 수 있다.

## 경량성과 무대기 지연의 비용

3회 실행마다 warmup5건 후100건을 측정했다. 아래 p95는 실행3개의 측정300건을 합친 기술통계이며 독립 실험300회가 아니다. 작은 요청225건은 약2–2.8KiB JSON, 큰 요청75건은 약128KiB 반복 입력이다. client·loopback·mock 응답 처리를 포함한 지연으로, proxy 코드만의 비용은 아니다.

| known TPM 모드 | 작은 요청 p95 | 큰 요청 p95 | 시작 직후 idle RSS, 3개 표본 |
|---|---:|---:|---:|
| 이전 바이트 모드 | 0.298ms | 0.370ms | 9.66–9.77MiB |
| 새 바이트 모드 | 0.286ms | 0.374ms | 9.83–9.91MiB |
| 새 cl100k | 0.368ms | 1.064ms | 41.72–41.75MiB |
| 새 o200k | 0.357ms | 1.051ms | 76.72–76.80MiB |

작업 대기가 없는 경우 BPE는 계산 비용을 추가한다. 큰 입력에서는 약0.7ms 늘었으며, o200k는 초기 목표였던50MiB idle 예산도 넘는다. 기본 모드의 작은 차이를 속도 향상이나 무회귀 보장으로 해석하지 않는다. unknown TPM에서 BPE를 선택한 경우에는 tokenizer를 준비하지 않았고 idle RSS는9.86–9.94MiB였다.

프로세스 생성부터 첫 control status 성공까지는 바이트 모드 약8–10ms, cl100k 약21ms, o200k 약34–47ms였다. 5ms 간격의 readiness 확인이며 설정 작성·doctor 호출은 제외했다. 서비스 전체 시작 시간이나 첫 LLM 응답 시간과 다르다.

**macOS 실행 파일은 10,188,304 → 59,869,408bytes, 약10.19 → 59.87MB로 커졌다.** 어휘 사전을 내장하므로 이 배포 크기는 BPE를 선택하지 않아도 부담한다. 대신 실행 시 사전 다운로드, Python·Node·Redis, Docker·WSL은 필요 없다. RSS는 시작 직후 표본이며 peak 메모리 측정이 아니다. 저사양 PC의 성능은 검증하지 않았다.

## 구현과 호환성 검증

- 모델 설정: `input_estimator`와 `input_token_overhead`. bytes/0이 기본이며 BPE의 기본 여유분은32다. 허용하지 않는 이름·음수·TOML 범위 초과·계산 overflow를 거절한다.
- known TPM일 때만 선택한 공유 사전을 listen 전에 준비한다. 상태 확인·설정 파싱·doctor는 사전을 준비하지 않는다. 원본 JSON을 세며 output cap과 fallback은 변경하지 않는다.
- setup에서 기존 모델 선택을 보존하고 첫 모델을 수정해도 다른 모델·route·주석은 유지한다. 상태 화면과 영문/한국어 문서에 추정 방식과 한계를 표시했다.
- macOS 전체 Rust 검사 **444통과·0실패·1명시적 제외**. 마지막 prompt 경계값 수정 후 관련 검사 **93통과**, Clippy `-D warnings`, fmt, 일반 release 빌드 통과. 두 검사 수를 더해 고유 테스트 수로 표시하지 않는다.
- native setup의 취소·뒤로·저장·시작·복원/정리 등 **8개 시나리오 통과**.
- `cl100k_base`를 켠 native **Pi0.84.2, Codex0.154.0, Claude Code2.1.63**에서 direct/gateway 각각 자동 요약과 다음 턴이 통과했다. 프로토콜·캐시 점검을 포함한9개 상위 사례 모두 통과. 합성 usage120k로 요약을 유도했으며 실제120k 대화 품질 검증은 아니다. Claude는 이 probe의 직접 설정이며 `connect` 프로필 전체 호환 인증과 다르다.
- 같은 probe에서 약460KB 반복 텍스트를 BPE 모드로 전달하고 응답 원문을 보존했다. `/responses/compact` 미지원과 known TPM의 opaque compaction 거절은 그대로다.
- `windows-labtop`의 기존 Rust1.88 GNU/WinLibs 환경에서 **155통과·0실패·1명시적 제외**, release 빌드 및 BPE wire/설정 검증 통과. MSVC·Docker·WSL을 새로 설치하지 않았다. 최종 Windows 실행 파일 SHA는 `2f9cb06e446d3deafae972c8629d88821be498e8c77a19a5986925e1bd96e5c8`이다. Windows 성능 비교와 전체 테스트 재실행은 이번 범위에 포함하지 않았다.

Windows 소스 archive의 재현 가능한 epoch mtime 때문에 Cargo가 수정 전 산출물을 재사용한 중간 검사는 최종 검증에서 제외했다. 소스 해시 확인 후 mtime을 갱신하여 실제 재컴파일했고, 추가 회귀 테스트 이름과 변경된 실행 파일 SHA까지 확인했다. 해시가 일치하는 소스를 배치했다는 사실만으로 새 코드가 빌드됐다고 판단하지 않았다.

## 재현과 증거

```sh
python3 product/scripts/benchmark_input_estimation.py --self-check
python3 product/scripts/benchmark_input_estimation.py \
  --binary /path/to/llmgw --encoding cl100k_base --profile burst \
  --output /tmp/deskquota-burst.json
python3 product/scripts/benchmark_input_estimation.py \
  --binary /path/to/llmgw --encoding cl100k_base --input-overhead 256 \
  --profile original18 --seed 101 --output /tmp/deskquota-calibrated.json
```

`legacy`는 새 설정 필드를 모르는 이전 비교 바이너리용이다. 새 제품에서 기존 동작을 비교하려면 `utf8_bytes`를 사용한다. 벤치마크 실행에는 Python3.11+가 필요하지만 제품 실행에는 필요 없다.

- [원시 결과를 재검산하는 스크립트](../evidence/input-estimation-2026-09-16/analyze.py), [주요48회 집계](../evidence/input-estimation-2026-09-16/summary.json), [보정 대조1회](../evidence/input-estimation-2026-09-16/calibrated-original18-candidate.json)
- [독립 입력 비용 확인](../evidence/input-estimation-2026-09-16/independent-cost-preflight.json), [최종 macOS 바이너리](../evidence/input-estimation-2026-09-16/final-binary.json), [최종 검증 명령](../evidence/input-estimation-2026-09-16/final-validation.json)
- [자동 요약 검증](../evidence/input-estimation-2026-09-16/compaction-cl100k.json), [native setup](../evidence/input-estimation-2026-09-16/native-setup.json), [Windows 재컴파일 검증](../evidence/input-estimation-2026-09-16/windows/input-estimation-final-rebuilt.json)
- [독립 검토 요약](../evidence/input-estimation-2026-09-16/reviews.json), [최종 소스·검증 기록](../evidence/input-estimation-2026-09-16/acceptance.json)
- [설계와 대안](../docs/superpowers/specs/2026-09-16-input-estimation-design.md), [구현 계획](../docs/superpowers/plans/2026-09-16-input-estimation.md), [고정 의존성](../product/Cargo.lock)

주요48회는 **2,748건 제출 = 완료2,730 + client timeout15 + upstream429 3**이며, warmup120건을 포함한다. 보정 대조18건과 예비 실행은 별도다. 저장된 모든 요청 결과·본문·usage·upstream 시도·설정/바이너리 SHA·정리 상태를 독립 재검산했다. 실험을 통과했다는 표시는 결과가 온전하다는 뜻이며, 실패 반례까지 성능 개선으로 인정한다는 뜻이 아니다.

선택형 기능으로 수용한다. 실제 회사/외부 API의 제한 계약 검증, 다양한 arrival trace, 장시간 운영, 경쟁 제품과 동일 과제의 완료 시간 비교는 남아 있다. 기본값 변경이나 scheduler 확장의 근거로 삼지 않는다. 이번 작업에서 커밋·푸시는 하지 않았다.
