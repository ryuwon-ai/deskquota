# NVIDIA hosted API 검증 준비

2026-09-13 사용자가 build.nvidia.com의 무료 제한 API를 실제 검증에 추가하도록 제안했다. 현재 상태는 **공식 자료 확인 및 실험 범위 준비**다. 계정별 키·모델 권한·무료 이용 상태·실제 제한 응답은 아직 확인하지 않았으며 인증 API 호출은 0회다.

2026-09-14 원본 연구 폴더 복구 후 확인: 기존 외부 준비 경로 `/Users/ryuwon/llmgw-nvidia-evaluation`는 현재 없고, `NVIDIA_API_KEY`·`NGC_API_KEY`도 실행 환경에 없다. 준비 도구를 현재 연구 폴더의 [nvidia_smoke.py](../scripts/nvidia_smoke.py)와 [로컬 검사](../scripts/test_nvidia_smoke.py)로 다시 작성했다. 제품 런타임 의존성은 늘어나지 않는다. 공식 FAQ/quickstart는 이번에도 직접 열어 확인했다.

## 실행 준비 도구

```sh
python3 -B scripts/nvidia_smoke.py \
  --model SELECTED_MODEL_ID \
  --output product/artifacts/nvidia-dry-run.json
```

기본은 네트워크 호출0회의 dry-run이다. `SELECTED_MODEL_ID`는 현재 계정의 무료 접근을 확인한 실제 모델 ID로 바꿔야 한다. `--model`은 필수이며 기본 모델이나 다른 모델로의 자동 전환은 없다. 키가 준비되면 `--live`를 명시하며 키 값 대신 `--key-env` 이름을 쓴다. 선택적인 gateway 비교는 `--gateway-base http://127.0.0.1:PORT/r/ROOT/v1`으로 지정한다. 별도 gateway key는 필요 없으며, `--key-env`에 지정한 provider 키를 표준 `Authorization: Bearer`로 전송한다. 같은 인증을 상위 API로 전달하려면 gateway의 `upstream.auth.mode = "forward"`를 사용한다. 기존 client 설정을 수정하거나 gateway를 자동 기동하는 도구는 아니다. 비교용 gateway의 목적지·모델·upstream auth·retry 설정은 실제 호출 전에 별도 확인해야 한다.

모델당 direct→gateway→gateway→direct 순서, 최대2모델, 요청 종료 후10초 간격이다. gateway를 지정하지 않으면 direct2회/모델만 한다. redirect·자동 retry·환경 proxy는 사용하지 않는다. 요청당 별도 자식 프로세스에150초 wall deadline과1MiB 수신 한도를 적용하며, 첫 HTTP 오류/429/잘못된 SSE에서 전체 실행을 멈춘다. 저장은 HTTP 상태, 수치형 allowlist 제한 헤더, TTFT/완료시각, SSE/usage 상태, 제한된 모델 ID와 payload hash뿐이다. 키·원문·임의 오류 응답·전체 URL은 저장하지 않는다. 출력은0600의 새 파일만 허용하고 각 요청 전후 진행 상태를 남긴다.

이 상한은 **client 요청 수**이며 gateway의 실제 upstream 시도 수를 인증하지 않는다. 또한 작은 smoke의 cold connection/프로세스 timing으로 p95나 성능 우위를 주장하지 않는다. hosted Chat Completions 검증을 Responses·Anthropic·실제 agent 작업의 검증으로 확대하지 않는다.

## 확인한 계약

- [공식 FAQ](https://docs.api.nvidia.com/nim/docs/product)는 Developer Program 회원에게 prototyping용 hosted NIM endpoint 무료 접근을 안내한다. 무료 시험 endpoint와 유료 서비스/자체 NIM 배포를 섞지 않는다.
- [공식 quickstart](https://docs.api.nvidia.com/nim/docs/api-quickstart)는 API 키가 필요하고 NVIDIA DGX Cloud의 hosted API를 호출한다고 설명한다. 따라서 우리 PC에 Docker·WSL·CUDA·GPU를 설치할 필요가 없다.
- [공식 LLM API 목록](https://docs.api.nvidia.com/nim/re/reference/llm-apis)의 base는 `https://integrate.api.nvidia.com`, 경로는 `/v1/chat/completions`다. `meta/llama-3.1-8b-instruct`와 [모델 페이지](https://build.nvidia.com/meta/llama-3_1-8b-instruct)를 확인했다. 두 번째 모델은 실제 계정에서 접근 가능한 모델로 고정한다.
- hosted service의 [429 공식 안내](https://docs.nvidia.com/rag/2.5.0/troubleshooting.html)는 서비스 제한으로 인한 오류를 설명한다. 이는 특정 LLM 계정의 정확한 RPM/TPM 숫자를 확정하는 문서가 아니다. 온라인의 “40 RPM”을 제품 기본값이나 보장된 계정 quota로 넣지 않는다.

2026-09-16 재확인에서 기존 예시인 [Llama 3.1 8B Instruct](https://build.nvidia.com/meta/llama-3_1-8b-instruct)의 무료 endpoint는 **Deprecated**로 표시됐다. 과거에 페이지가 존재했다는 확인을 현재 무료 호출 가능 근거로 사용하지 않는다. 따라서 smoke의 기본 모델을 제거하고 두 도구 모두 명시적인 모델 선택을 요구한다. 새 모델도 account 접근과 thinking/output 상한 계약을 확인한 뒤 선택한다.

## 작은 실제 호출부터

1. 키는 값 대신 환경변수 이름 또는 1Password 참조 위치로 전달받는다. 현재 사용자 client 설정을 바꾸지 않고 임시 HOME/config와 합성 입력을 사용한다.
2. 첫 smoke는 최대 2모델 × direct/gateway 각 2회 = **최대 8회**, 요청당 `max_tokens=64`, 요청한 출력 상한 합계512. 이 숫자는 공급자가 실제 청구하는 token의 확정 상한이 아니다. 무료 hosted endpoint만 사용하고 오류가 나면 자동 유료 경로 전환 없이 중단한다.
3. 첫 요청은 순차 저속으로 확인한다. HTTP 상태, 유효 SSE content/terminal/usage 존재, 모델 ID, 실제 attempts, timing과 필요한 rate-limit header의 수치만 남긴다. 키·원문 입력·응답은 artifact에 저장하지 않는다. 실패·취소·누락 usage도 모두 기록한다.
4. 실제 429와 Retry-After가 관측되면 그 정보를 이용해 대기·공유 cooldown 동작을 확인한다. 관측되지 않은 429는 재현 성공으로 세지 않는다. 고의로 대량 호출해서 한도를 찾거나 여러 키로 제한을 우회하지 않는다.
5. 이후 반복 비교는 계정의 확인된 무료 범위와 RPM/TPM에 맞춰 별도 작은 budget을 정한다. 같은 모델·입력·동시성·retry budget으로 direct+cap/FIFO/RR/candidate를 교차 실행한다. 실제 provider 혼잡과 quota 공유 범위는 변할 수 있으므로 mock의 정책 인과 실험도 유지한다.

사용자가 별도 확인해준 provider 한도보다 낮게 게이트웨이 quota를 설정하는 실험은 **사용자 설정의 제한을 실제 API 앞에서 시험하는 것**이다. 이를 NVIDIA 원래 quota를 찾아냈거나 우회한 결과로 표현하지 않는다. RPM만 알려졌으면 TPM은 unknown으로 둔다. 현재 backfill 후보는 TPM 여유가 있는 일부 상태에 유리하므로 RPM 병목만으로 모든 효율 가설을 검증할 수 없다.

## 이번에 추가로 발견한 경쟁 사례

[NVIDIA-req-limit-proxy](https://github.com/Tiger-sudo894/NVIDIA-req-limit-proxy)와 [nvidia-api-rate-limiter](https://github.com/imviren/nvidia-api-rate-limiter)는 NVIDIA용 제한 proxy를 공개했다. 9월13일의 README 조사에 이어, 9월14일 두 repo와 HiveMind를 지정 SHA로 실제 clone해 [소스 분석](additional-quota-references-2026-09-14.md)을 추가했다. Tiger의429 대기는 각 thread의 sleep이며 모든 요청의 공유 cooldown과 다르다. imviren의 현재429 경로도 README의 재시도 설명과 다르다. 따라서 README 기능명을 실행 계약으로 그대로 채택하지 않는다. 구체적인 문제와 도구의 존재는 수요 신호이며, 우리의 우위나 신규성 증거는 아니다. 기존27개 manifest와 추가3개 고정 checkout의 근거를 별도로 유지하며 경쟁 성능을 실행한 것은 아니다.

2026-09-14 당시 준비 검사: Python 로컬 검사5개 통과, 프로세스 시작 중 중단·종료 직전 Pipe 수신 경쟁에 대한 수정은 독립 재검토 PASS. [실행·소스 증거](../product/artifacts/backfill-resume-2026-09-14/nvidia-preparation.json)에 연결했다. 기본 dry-run의 실제 네트워크 호출은0회이며, 인증된 NVIDIA 실호출 역시 아직0회다.

## 2026-09-16: 정답을 확인하는 작은 작업 비교 준비

[nvidia_workflows.py](../scripts/nvidia_workflows.py)는 첫 smoke 이후 사용할 **고정 과제/API probe**다. 설치된 Pi·Codex·Claude의 실행, 도구 호출, 저장소 수정이나 자동 압축까지 검증한 것으로 표현하지 않는다. 이 도구를 추가하면서 실행한 것은 로컬 검사이며, 아직 이 도구의 NVIDIA 실측 결과는 없다.

```sh
python3 -B scripts/nvidia_workflows.py \
  --model SELECTED_MODEL_ID \
  --gateway-base http://127.0.0.1:4141/r/eval/v1 \
  --output product/artifacts/nvidia-workflows-dry.json
```

기본은 네트워크 호출 없는 dry-run이다. 실제 계정에서 접근 가능한 모델을 선택하고, 무료 이용 상태와 예산을 확인한 뒤 `--live --gateway-retries-disabled`를 추가한다. 키는 `--key-env` 이름으로만 지정한다. `--gateway-first`는 gateway→direct 순서로 바꾸므로, 허용된 예산 안에서 순서를 뒤집은 별도 실행을 할 수 있다. 첫 실행만으로 평균·p95나 경쟁 우위를 주장하지 않는다.

| arm별 과제 | 호출 수 | 완료 조건 |
|---|---:|---|
| 코드 수정 | 1 | 공개 toy 함수의 잘못된 `max`를 `min`으로 수정한 전체 함수의 AST가 고정 정답과 일치 |
| 병렬 리뷰 | 2, 동시 실행 | 서로 다른 공개 snippet의 경계 오류와 사용자별 cache 격리 누락을 각각 지정 JSON으로 보고 |
| 분류 재실행 | 2, 순차 실행 | 완전히 같은 요청 본문을 재사용하고 두 응답 모두 고정 분류 정답과 일치 |

총 상한은 **1모델 × 2arm × 5요청 = 10 client 요청**이다. 요청마다 `max_tokens=384`, 요청한 출력 상한 합계3,840이며, 고정 요청 본문은 각각4KiB 이하다. 이는 입력 token·추론 token·공급자 청구액의 확정 상한이 아니다. 응답은 기존 smoke의1MiB 한도와150초 자식 프로세스 wall deadline을 그대로 사용한다. 전체 네트워크 작업 예산은900초이며, 기한 후에는 자식 종료·join 정리 시간이 추가될 수 있다. 과제 사이10초 쉬고, 이 인위적인 간격은 개별 과제 완료 시간에서 제외한다. classifier의 동일 요청2회 사이에는 인위적인 pause를 넣지 않는다.

실제 호출 전에는 **별도 임시 gateway**의 목적지가 NVIDIA hosted API인지, `upstream.auth.mode="forward"`, `retry_transient_429=false`, `startup_hold_secs=0`, 동시성3인지 확인한다. 한도를 모르면 RPM/TPM을 `unknown`으로 두며, 이는 별도 개인 한도를 집행하지 않는 선택임을 기록한다. classifier cache 실험에는 `[cache]`의 `ttl_secs=300`, `max_history=3`을 사용하고 빈 cache에서 시작한다. 기존 작업용 gateway·client 설정을 바꾸거나 공유하면 이 비교 조건을 충족하지 않는다.

`--gateway-retries-disabled`는 사용자가 확인한 설정의 표시이며, probe가 실행 중 gateway의 설정과 실제 upstream 수를 인증하는 기능은 아니다. **10회는 client 상한**이며 retry 없는 별도 gateway라는 조건에서만 upstream도 최대10회다. 실제 upstream 시도 수와 cache hit 수는 gateway의 실행 전후 counter 또는 별도 관측으로 확인해야 한다. 결과의 `upstream_attempts`는 이 도구만으로 확인할 수 없어 `null`이다. cache hit의 usage는 이전 응답의 전달 값이므로 모든 응답 usage를 합쳐 새 공급자 소비량으로 계산하지 않는다.

HTTP·stream·timeout 오류가 나면 새로운 과제를 시작하지 않는다. 병렬 리뷰에서 이미 시작한 다른1회는 그 deadline까지 정리하고 결과에 포함한다. Ctrl-C나 probe 예외는 executor가 기다리기 전에 공유 취소 신호를 보내며, transport가 최대0.1초 poll 간격으로 신호를 확인해 자신의 자식 프로세스를 종료한다. 정답 불일치는 `task_failed`로 남기며, 자동 수정·재질문·client retry는 하지 않는다. 최종 완료 분모에는 실패한 과제도 남긴다. classifier가 빨라졌다는 이유만으로 cache hit라고 판정하지 않는다.

응답 내용은 한도 내 자식 프로세스 메모리에서만 검사하고 파일·로그로 보내지 않는다. 모델이 반환한 코드를 실행하거나 설치하지 않으며, 고정 AST 정답 검사는 의미가 같은 다른 구현을 실패로 판정할 수 있는 좁은 평가다. JSON에는 과제 식별자, payload hash, HTTP/SSE/usage 상태, 개별 요청·과제 시간과 정답 판정만 저장한다. cold client process와 새 HTTP connection 비용이 포함되므로 일반 SDK의 재사용 connection 성능과 구분한다.

로컬 직접 검증: `cd scripts && python3 -B -m unittest test_nvidia_smoke test_nvidia_workflows`의 **12개 검사 통과**. dry-run 무호출, 배타적0600 결과 파일, 두 리뷰의 실제 동시 진입, 동일 분류 payload, HTTP429 뒤 추가 과제 중단, 정답 실패 분모, deadline·중단 보존, 병렬 중단 시 sibling 취소, 실제 loopback SSE에서 내용의 비저장 판정을 검사했다. 실제 provider 429·quota·cache 절약·설치된 agent 작업 성능은 이 로컬 통과로 입증되지 않는다.

## 설치된 Pi의 read/edit 작업 준비

[nvidia_pi.py](../scripts/nvidia_pi.py)는 **설치된 Pi 0.84.2 CLI와 기본 read/edit 도구**를 실행한다. 임시 작업 폴더의 공개 `fixture.py`를 읽고 수정한 뒤 마지막 응답까지 확인한다. 고정 과제/API probe와 달리 실제 agent의 도구 실행·다음 요청을 포함하지만, 아직 NVIDIA를 사용한 실행 결과는 없다.

```sh
python3 -B scripts/nvidia_pi.py --self-check --output product/artifacts/nvidia-pi-check.json
```

옵션 없는 기본은 dry-run이며, 위 `--self-check`는 실제 Pi와 **loopback fixture만** 실행한다. 실호출에는 명시적인 `--live --model SELECTED_MODEL_ID --gateway-base http://127.0.0.1:PORT/r/ROOT/v1 --gateway-retries-disabled`가 필요하다. 계정의 무료 접근을 확인한 모델만 고른다. `--disable-template-thinking`은 해당 모델이 `chat_template_kwargs.enable_thinking=false`를 지원함을 확인했을 때만 추가한다. 이 설정은 generic 모델 자동 탐지나 fallback이 아니다.

arm마다 최대3요청, 요청당 `max_tokens=384`, 본문16KiB, Pi 프로세스150초 deadline을 적용한다. direct/gateway 합계 최대6 client 요청이며 실패하면 다음 arm을 실행하지 않는다. gateway의 실제 시도 수는 별도 counter가 필요하다. 매 live 실행 전 같은 guard로 아래 로컬 검사를 다시 통과해야 한다.

| 직접 실행한 로컬 검사 | 관측 HTTP 요청 | 판정 |
|---|---:|---|
| native read → edit → 마지막 응답 | 3 | 도구2개 성공, 최종 파일 AST 정답 일치 |
| 허용 파일 밖 접근 | 1 | 도구 실행 전 차단 |
| 반복 tool loop | 3 | 네 번째 요청 전 프로세스 종료 |
| guard 누락 | 0 | provider 인증 불가로 종료 |
| HTTP429 | 1 | agent/provider 재시도 없음 |

Pi는 일반 extension 오류를 기록한 뒤 계속할 수 있다. 따라서 guard가 요청·도구 제한을 등록하고 상태 파일을 만든 뒤에만 process 내부에서 인증 환경변수를 활성화한다. guard가 누락되면 실제 키를 담은 요청을 보낼 수 없고, 요청 상한에서는 오류를 던지는 대신 프로세스를 종료한다. `read/edit`는 정확히 그 임시 파일에만 허용하며 bash·write·외부 파일 접근은 열지 않는다. 생성된 Python은 AST로 비교할 뿐 실행하지 않는다.

원시 stdout/stderr는 버리고, guard는 수치 usage·HTTP 상태·도구 성공·종료 상태만 남긴다. Pi SDK의 usage0은 provider usage 누락일 수도 있으므로 정확한 소비량으로 단정하지 않는다. 사용자 HOME·설정·session·확장·자동 압축은 이 과제의 임시 환경에서 분리한다. 이 검사로 사용자 기존 설정이나 실제 NVIDIA의 token·429 계약까지 검증했다고 표현하지 않는다.

현재 `cd scripts && python3 -B -m unittest test_nvidia_smoke test_nvidia_workflows test_nvidia_pi`의 **14개 검사 통과**. 그중 Pi 검사는 위5개 native 시나리오를 실제 실행한다. 별도 `--self-check`도 통과했으며, hosted API 요청은0회다.

현재 준비 결과: [실제 Pi의 오프라인 5개 제어 실험](../evidence/native-preview-2026-09-16/nvidia-pi-offline.json), [고정 과제 dry-run](../evidence/native-preview-2026-09-16/nvidia-workflows-dry.json), [검증 상태](../evidence/native-preview-2026-09-16/nvidia-readiness.json). 이 기록의 NVIDIA hosted 호출은 모두 0회다.
