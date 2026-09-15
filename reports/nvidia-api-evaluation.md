# NVIDIA hosted API 검증 준비

2026-09-13 사용자가 build.nvidia.com의 무료 제한 API를 실제 검증에 추가하도록 제안했다. 현재 상태는 **공식 자료 확인 및 실험 범위 준비**다. 계정별 키·모델 권한·무료 이용 상태·실제 제한 응답은 아직 확인하지 않았으며 인증 API 호출은 0회다.

2026-09-14 원본 연구 폴더 복구 후 확인: 기존 외부 준비 경로 `/Users/ryuwon/llmgw-nvidia-evaluation`는 현재 없고, `NVIDIA_API_KEY`·`NGC_API_KEY`도 실행 환경에 없다. 준비 도구를 현재 연구 폴더의 [nvidia_smoke.py](../scripts/nvidia_smoke.py)와 [로컬 검사](../scripts/test_nvidia_smoke.py)로 다시 작성했다. 제품 런타임 의존성은 늘어나지 않는다. 공식 FAQ/quickstart는 이번에도 직접 열어 확인했다.

## 실행 준비 도구

```sh
python3 -B scripts/nvidia_smoke.py \
  --model meta/llama-3.1-8b-instruct \
  --output product/artifacts/nvidia-dry-run.json
```

기본은 네트워크 호출0회의 dry-run이다. 키가 준비되면 `--live`를 명시하며 키 값 대신 `--key-env` 이름을 쓴다. 선택적인 gateway 비교는 `--gateway-base http://127.0.0.1:PORT/r/ROOT/v1`으로 지정한다. 별도 gateway key는 필요 없으며, `--key-env`에 지정한 provider 키를 표준 `Authorization: Bearer`로 전송한다. 같은 인증을 상위 API로 전달하려면 gateway의 `upstream.auth.mode = "forward"`를 사용한다. 기존 client 설정을 수정하거나 gateway를 자동 기동하는 도구는 아니다. 비교용 gateway의 목적지·모델·upstream auth·retry 설정은 실제 호출 전에 별도 확인해야 한다.

모델당 direct→gateway→gateway→direct 순서, 최대2모델, 요청 종료 후10초 간격이다. gateway를 지정하지 않으면 direct2회/모델만 한다. redirect·자동 retry·환경 proxy는 사용하지 않는다. 요청당 별도 자식 프로세스에150초 wall deadline과1MiB 수신 한도를 적용하며, 첫 HTTP 오류/429/잘못된 SSE에서 전체 실행을 멈춘다. 저장은 HTTP 상태, 수치형 allowlist 제한 헤더, TTFT/완료시각, SSE/usage 상태, 제한된 모델 ID와 payload hash뿐이다. 키·원문·임의 오류 응답·전체 URL은 저장하지 않는다. 출력은0600의 새 파일만 허용하고 각 요청 전후 진행 상태를 남긴다.

이 상한은 **client 요청 수**이며 gateway의 실제 upstream 시도 수를 인증하지 않는다. 또한 작은 smoke의 cold connection/프로세스 timing으로 p95나 성능 우위를 주장하지 않는다. hosted Chat Completions 검증을 Responses·Anthropic·실제 agent 작업의 검증으로 확대하지 않는다.

## 확인한 계약

- [공식 FAQ](https://docs.api.nvidia.com/nim/docs/product)는 Developer Program 회원에게 prototyping용 hosted NIM endpoint 무료 접근을 안내한다. 무료 시험 endpoint와 유료 서비스/자체 NIM 배포를 섞지 않는다.
- [공식 quickstart](https://docs.api.nvidia.com/nim/docs/api-quickstart)는 API 키가 필요하고 NVIDIA DGX Cloud의 hosted API를 호출한다고 설명한다. 따라서 우리 PC에 Docker·WSL·CUDA·GPU를 설치할 필요가 없다.
- [공식 LLM API 목록](https://docs.api.nvidia.com/nim/re/reference/llm-apis)의 base는 `https://integrate.api.nvidia.com`, 경로는 `/v1/chat/completions`다. `meta/llama-3.1-8b-instruct`와 [모델 페이지](https://build.nvidia.com/meta/llama-3_1-8b-instruct)를 확인했다. 두 번째 모델은 실제 계정에서 접근 가능한 모델로 고정한다.
- hosted service의 [429 공식 안내](https://docs.nvidia.com/rag/2.5.0/troubleshooting.html)는 서비스 제한으로 인한 오류를 설명한다. 이는 특정 LLM 계정의 정확한 RPM/TPM 숫자를 확정하는 문서가 아니다. 온라인의 “40 RPM”을 제품 기본값이나 보장된 계정 quota로 넣지 않는다.

## 작은 실제 호출부터

1. 키는 값 대신 환경변수 이름 또는 1Password 참조 위치로 전달받는다. 현재 사용자 client 설정을 바꾸지 않고 임시 HOME/config와 합성 입력을 사용한다.
2. 첫 smoke는 최대 2모델 × direct/gateway 각 2회 = **최대 8회**, 요청당 `max_tokens=64`, 요청한 출력 상한 합계512. 이 숫자는 공급자가 실제 청구하는 token의 확정 상한이 아니다. 무료 hosted endpoint만 사용하고 오류가 나면 자동 유료 경로 전환 없이 중단한다.
3. 첫 요청은 순차 저속으로 확인한다. HTTP 상태, 유효 SSE content/terminal/usage 존재, 모델 ID, 실제 attempts, timing과 필요한 rate-limit header의 수치만 남긴다. 키·원문 입력·응답은 artifact에 저장하지 않는다. 실패·취소·누락 usage도 모두 기록한다.
4. 실제 429와 Retry-After가 관측되면 그 정보를 이용해 대기·공유 cooldown 동작을 확인한다. 관측되지 않은 429는 재현 성공으로 세지 않는다. 고의로 대량 호출해서 한도를 찾거나 여러 키로 제한을 우회하지 않는다.
5. 이후 반복 비교는 계정의 확인된 무료 범위와 RPM/TPM에 맞춰 별도 작은 budget을 정한다. 같은 모델·입력·동시성·retry budget으로 direct+cap/FIFO/RR/candidate를 교차 실행한다. 실제 provider 혼잡과 quota 공유 범위는 변할 수 있으므로 mock의 정책 인과 실험도 유지한다.

사용자가 별도 확인해준 provider 한도보다 낮게 게이트웨이 quota를 설정하는 실험은 **사용자 설정의 제한을 실제 API 앞에서 시험하는 것**이다. 이를 NVIDIA 원래 quota를 찾아냈거나 우회한 결과로 표현하지 않는다. RPM만 알려졌으면 TPM은 unknown으로 둔다. 현재 backfill 후보는 TPM 여유가 있는 일부 상태에 유리하므로 RPM 병목만으로 모든 효율 가설을 검증할 수 없다.

## 이번에 추가로 발견한 경쟁 사례

[NVIDIA-req-limit-proxy](https://github.com/Tiger-sudo894/NVIDIA-req-limit-proxy)와 [nvidia-api-rate-limiter](https://github.com/imviren/nvidia-api-rate-limiter)는 NVIDIA용 제한 proxy를 공개했다. 9월13일의 README 조사에 이어, 9월14일 두 repo와 HiveMind를 지정 SHA로 실제 clone해 [소스 분석](additional-quota-references-2026-09-14.md)을 추가했다. Tiger의429 대기는 각 thread의 sleep이며 모든 요청의 공유 cooldown과 다르다. imviren의 현재429 경로도 README의 재시도 설명과 다르다. 따라서 README 기능명을 실행 계약으로 그대로 채택하지 않는다. 구체적인 문제와 도구의 존재는 수요 신호이며, 우리의 우위나 신규성 증거는 아니다. 기존27개 manifest와 추가3개 고정 checkout의 근거를 별도로 유지하며 경쟁 성능을 실행한 것은 아니다.

최종 준비 검사: Python 로컬 검사5개 통과, 프로세스 시작 중 중단·종료 직전 Pipe 수신 경쟁에 대한 수정은 독립 재검토 PASS. [실행·소스 증거](../product/artifacts/backfill-resume-2026-09-14/nvidia-preparation.json)에 연결했다. 기본 dry-run의 실제 네트워크 호출은0회이며, 인증된 NVIDIA 실호출 역시 아직0회다.
