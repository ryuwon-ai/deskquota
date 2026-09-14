# 증거를 남기는 작업 루프

사용자가 요청한 하네스 엔지니어링을 이 작업에 적용한다. 목표·근거·진행 상태·검증 명령을 작업 폴더에 남겨 다음 세션이 실제 상태에서 이어가게 한다. 현재 스레드의 지속 목표를 사용하며 별도 유료 에이전트 서비스나 반복 예약 작업은 만들지 않는다.

설계 근거는 OpenAI의 [Harness engineering, 2026-02-11](https://openai.com/index/harness-engineering/)과 Anthropic의 [Harness design, 2026-03-24](https://www.anthropic.com/engineering/harness-design-long-running-apps)다. 여기서 채택한 것은 작은 진입 문서, 확인 가능한 산출물, 생성과 검증의 분리, 실패에 따른 수정이다. 해당 기업의 개발 속도·비용 수치를 우리 성과로 사용하지 않는다.

## 한 번의 루프

1. `README.md`와 `evidence/work-items.json`을 읽고 현재 파일·프로세스·Git 상태를 확인한다.
2. 미완료 항목 하나에서 검증 가능한 가설을 고른다. 예: “긴 요청이 도착했을 때 짧은 요청만 계속 우회시키면 기아가 생긴다.”
3. 변경 전에 판단 기준과 반례를 기록한다. 제품 구현 단계에서는 실패하는 재현 시나리오를 먼저 확보한다.
4. 최소 변경 또는 자료 조회를 수행한다. 출처를 읽는 작업과 성능을 실행해서 입증하는 작업을 구분한다.
5. 같은 입력·자원·제한·시간 예산으로 비교하고 실패·취소·시간 초과도 결과에 포함한다.
6. 결과에 따라 채택·수정·기각한다. 확인되지 않은 가설을 완료 처리하지 않는다.
7. `evidence/loop-log.jsonl`에 시도, 관찰, 증거 경로, 다음 행동을 추가한다. `work-items.json` 상태를 갱신한다.

## 검증 명령과 의미

```sh
cd /Users/ryuwon/Desktop/ryuwon-project/llm-gateway-research
python3 scripts/research.py verify
python3 scripts/research.py status
python3 scripts/verify-source-links.py --output evidence/source-audit-verification.json
```

`verify`는 27개 체크아웃의 SHA·remote·변경 유무, PDF 해시, 작업 항목의 증거 파일 존재 여부를 확인한다. 내용을 자동으로 참이라고 판정하지 않는다. `status`는 미완료 항목을 숨기지 않는다. `verify-source-links.py`는 고정 SHA의 source 파일과 행 범위가 실제 존재하는지 확인하며 해석의 정확성을 자동 판정하지 않는다. 제품 테스트 명령은 구현 계획과 함께 추가하며, 아직 존재하지 않는 테스트를 통과한 것으로 표시하지 않는다.

[기존 구현 동작 probe](reports/reference-validation.md)는 원본 Scheduler와 HTTP app을 호출한다. 이 결과도 제품 benchmark와 구분한다. 관찰 스크립트의 exit code 0과 정책 수용 조건 충족을 별도 기록한다. 예를 들어 TCP no-abort probe는 정상 종료했지만 기대한 drain·점유 유지 동작은 충족하지 않았다. 조사 도구 자체의 오류는 원본 제품의 실패로 세지 않는다.

[Pi client probe](reports/client-integration-validation.md)는 설치된 CLI와 loopback fixture를 사용한다. clone 버전과 실행 버전을 구분하고, 일치한다고 추정하지 않는다. 선택한 사례 수, 실제 HTTP 시도 수, 의도한 오류 종료, 성공 출력, 실험하지 않은 프로토콜·OS를 함께 기록한다. 원시 prompt·credentials 대신 필요한 구조와 설정만 저장한다.

[제품 Pi E2E](product/scripts/probe_pi.py)는 실제 설치된 Pi→foreground gateway→합성 upstream을 연결한다. 제품 디렉터리에서 `python3 scripts/probe_pi.py --binary target/debug/llmgw --output artifacts/manual/pi-e2e.json`을 실행한다. `--gateway-off`는 gateway 시작만 생략하고 같은 Pi JSON 모드·입력·성공 조건을 유지하는 음성 대조다. 예상 결과는 probe exit 1, assistant 실패 이벤트, 출력 없음, upstream 요청 0회이며 이 결과를 양성 통과 수에 더하지 않는다. Pi JSON 모드의 exit 0만으로 성공을 판정하지 않는다. 실제 wire 경로·헤더, read 도구의 정확한 호출·결과·다음 요청까지 검사한다. 임시 상태·합성 파일과 자신의 자식 프로세스만 사용하며 원문을 artifact에 기록하지 않는다. 단계별 `--output` 디렉터리를 사용해 과거 검증 결과와 실패 로그를 덮어쓰지 않는다. 예시의 `artifacts/manual/`은 수동 실행용이며 Task 4·5의 보존된 증거와 별개다. 현재 리뷰 상태와 해당 단계의 실행 결과 경로는 [구현 기록](evidence/implementation-progress.json)에 따른다.

[제품 CLI smoke](evidence/product-cli-smoke.json)는 별도 foreground process와 임시 state 파일, loopback upstream을 사용한다. `python3 scripts/probe-product-cli.py --binary product/target/debug/llmgw --output evidence/product-cli-smoke.json`으로 재현한다. binary hash를 기록하고 URI·원문 body·인증·control 종료를 검사한다. 엔진이나 실제 agent 실행, 지연 성능 결과가 아니다. 이는 현재 개발 실행의 preprovisioned token 파일 계약이며 향후 설치 마법사 완료를 뜻하지 않는다. 일반 CLI·stream 수명 probe의 임시 quota는 RPM unlimited·TPM unknown으로 명시해 quota 대기와 분리한다. 제품 예제의 known RPM 설정과 실제 60초 재시작 보류 정책은 바꾸지 않는다.

`scripts/probe-product-stream.py`는 foreground binary에 대해 terminal marker 이후·headers 이전의 실제 RST 단절과 upstream EOF gate를 사용한다. 두 번째 요청이 upstream에 도착하는 순서로 실행 슬롯 수명을 검사한다. EOF gate를 열지 않고 stop을 보내 10초 grace 뒤 process exit·HTTP close가 발생하는 사례도 검사한다. 이 과정의 관찰 시간은 latency benchmark가 아니며 library 내부 child-task join 순서를 증명하지 않는다. `python3 scripts/probe-product-stream.py --binary product/target/debug/llmgw --output evidence/product-stream-smoke.json`으로 실행하며, 실패도 결과 파일에 남긴다. [fixture 자체 검사](evidence/product-stream-fixture-self-check.json)는 gateway 없이 gate와 RST 관찰만 검증한 별도 증거다. 제품 검증은 실제 실행 결과가 생기기 전까지 완료로 세지 않는다. FIN-only/half-close, 느린 reader와 메모리 한도 검증은 제품 socket 테스트가 별도로 맡는다.

`scripts/probe-product-fairness.py`는 별도 foreground binary에서 두 root의 FIFO·라운드로빈, metadata의 동일 큐 사용, queued RST 취소, 전체 대기 64개 한도와 취소 후 재입장을 검사한다. upstream 첫 요청을 gate로 붙잡고 인증된 control status로 실제 큐 진입을 확인한 뒤 순서를 관찰한다. 성공뿐 아니라 거절·취소를 모두 ingress 분모에 포함한다. `python3 scripts/probe-product-fairness.py --binary product/target/debug/llmgw --output evidence/product-fairness-task6-debug.json`으로 실행하며, 제품 소스와 binary가 HOLD된 뒤에만 검증한다. 임시 quota는 RPM unlimited·TPM unknown이므로 exact token cost의 heavy/light 기아 방지 trace를 대신하지 않는다. `--self-check --output evidence/product-fairness-fixture-self-check.json`은 fixture gate와 시도 집합 판정만 검사하며 gateway 실행·성능 증거가 아니다.

## 진행·대기·정체 구분

단계의 held source를 다음 편집 이후에도 재현하려면 `python3 scripts/archive-product-source.py --manifest evidence/product-task6-pre-review-source-manifest.json --output evidence/product-task6-held-source.tar.gz`처럼 별도 파일로 보관한다. manifest와 일치한 source bytes만 묶고 archive 내용을 다시 읽어 검증하며 기존 archive는 덮어쓰지 않는다. binary·실행 상태는 포함하지 않는다. Task 6부터 이 묶음을 남기며 이전 단계에 보존하지 않은 source까지 복구 가능하다고 주장하지 않는다. Git commit 또는 build provenance를 대신하는 증거는 아니다.

리뷰할 소스를 고정한 뒤 `scripts/fingerprint-product.py`로 파일 해시를 기록한다. 예를 들어 `python3 scripts/fingerprint-product.py --phase core-task3-review --output evidence/product-task3-source-manifest.json --probe evidence/product-stream-smoke.json --probe evidence/product-cli-task3-smoke.json`은 두 probe의 성공 여부와 실제 실행 binary 해시도 비교한다. 캡처 중 파일·binary가 바뀌면 실패한다. 이 결과는 해당 소스가 binary로 빌드됐다는 증명이 아니므로 마지막 변경 이후의 build/test 기록을 함께 남긴다. Git 저장소가 없는 현재 단계에서도 리뷰 대상을 명확히 하기 위한 도구다.

| 판정 | 필요한 관찰 | 다음 행동 |
|---|---|---|
| 진행 | 코드·자료·실험 결과가 다음 판단을 바꿈 | 증거를 남기고 다음 항목 수행 |
| 대기 | 살아 있는 정확한 프로세스/세션/작업 핸들을 확인 | 같은 핸들을 다시 조회, 중복 실행 금지 |
| 정체 | 상태 설명만 반복하거나 결과 없는 재시도 | 원인을 재확인하고 다른 안전한 검증 수행 |
| 외부 입력 필요 | 설계의 미승인 선택, 접근 불가능한 실제 환경 등 | 의존하지 않는 조사 계속, 필요한 질문 하나만 제시 |

연속 실패는 재시도 횟수를 늘리는 근거가 아니다. 무엇이 부족한지 분류한다: 잘못된 가설 / 도구 부족 / 실험 부정확 / 실제 환경 부재 / 사용자 결정 필요. 검증 기준을 낮춰 통과시키지 않는다.

## Task 7 독립 재시도 probe

`scripts/probe-product-retry.py`는 별도 foreground binary에서 기본 retry 0, 다른 root·metadata의 공유 cooldown, opt-in ms 대기, 반복·충돌 timing header의 최대 대기, 영구·모호·압축·과대 오류의 원문 전달, 최대 추가 1회를 검사한다. `python3 scripts/probe-product-retry.py --binary product/target/debug/llmgw --output evidence/product-retry-task7-debug.json`으로 실행하며, 구현자의 최종 source/binary HOLD 이후에만 제품을 실행한다. 모든 ingress의 HTTP 결과와 실제 upstream 시도를 별도로 기록한다. 임시 Unlimited RPM·Unknown TPM이므로 known quota 회계·정확한 provider TPM 검증을 대신하지 않는다. 대기 하한 관찰은 성능 비교가 아니다.

`--self-check --output evidence/product-retry-fixture-self-check-v3.json`의 기록은 합성 upstream 응답 순서와 누락·추가 시도 판정만 검증한 것이다. `gateway_executed: false`를 제품 통과로 세지 않는다. 출력 파일이 이미 있으면 덮어쓰지 않고 실패하므로 재검증 시 새 단계 이름을 사용한다. 현재 제품 실행·리뷰 상태는 `evidence/implementation-progress.json`에 따른다.

## 완료 조건

조사 완료, 구현 완료, 성능 입증, 사용자 채택, 5,000 stars, OSS 선정은 별개다. 저장소 파일이 생겼거나 단위 테스트가 통과했다고 뒤의 항목을 완료로 승격하지 않는다. 설계 승인 뒤에는 brainstorming 스킬의 명세 리뷰 단계와 writing-plans 절차를 따른다. 커밋·공개 게시·지원서 제출은 각각 승인된 범위에서만 수행한다.
