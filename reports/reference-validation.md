# 기존 구현 동작 검증

검증일: 2026-09-11. 대상은 수정하지 않은 Overlaat `4e3cd0ff02a0456d470d5b18744803a56c7e1ffb`다. **제품 구현·경쟁 성능 비교가 아니라, 설계에 참고할 기존 동작과 테스트의 한계를 확인했다.**

## 결과

| 확인 | 직접 관찰 | 판단 범위 |
|---|---|---|
| 원본 scheduler·queue 테스트 | 선택한 두 파일의 57개 통과, 실패·오류·skip 0 | 해당 테스트의 불변식. 전체 저장소 테스트나 실서비스 인증은 아님 |
| heavy/light 도착 순서 | heavy → light → heavy에서는 light 입장, heavy → heavy → light에서는 잔여 비용 0.25가 있어도 light 대기 | 예약의 기아 방지와 짧은 작업 대기가 충돌함. latency 개선율 측정 아님 |
| TCP, abort=true | upstream의 종료를 허용하기 전에 첫 SSE 데이터 수신. 클라이언트를 닫으면 다음 요청 200, 점유·큐 0 | 이 HTTP 경로에서 streaming·취소 후 반환 확인. 실제 모델 계산 중지는 미확인 |
| TCP, abort=false | 종료 gate를 열기 전에 upstream 연결이 닫히고 다음 요청 완료. 새 프로세스 3회 모두 같음 | 이 환경에서 “upstream 자연 종료까지 drain·점유 유지” 계약이 충족되지 않음 |

TCP probe의 exit code 0은 **관찰 스크립트가 끝났다는 뜻**이다. 특히 abort=false 결과는 정책 수용 조건 통과가 아니다. 원본 코드나 의존성을 수정해 이 결과를 감추지 않았다.

## 환경과 원본 테스트

- macOS 26.5.1 arm64, Apple M4, RAM 32 GiB.
- Python 3.14.6. upstream `uv.lock`을 `--frozen --extra dev --no-install-project`로 설치했다. 환경은 clone 밖 `.venvs/overlaat`에 격리했다.
- pytest 9.1.1, pytest-asyncio 1.4.0, HTTPX 0.28.1, httpcore 1.0.9, Uvicorn 0.49.0, Starlette 1.3.1, FastAPI 0.137.2, AnyIO 4.14.0.
- 원본 테스트: `test_scheduler.py` 35개 + `test_queue_behavior.py` 22개. 기록된 실행은 57 passed in 1.77s이며 성능 벤치마크 값으로 쓰지 않는다.
- [실행 명령·SHA·lock hash](../evidence/reference-tests/overlaat/run.json), [pytest 로그](../evidence/reference-tests/overlaat/pytest.log), [JUnit](../evidence/reference-tests/overlaat/scheduler-and-queue.xml), [TCP 실행별 메타데이터](../evidence/reference-tests/overlaat/tcp-runs.json).

원본 테스트는 fake upstream·ASGI 호출을 쓴다. TCP probe는 127.0.0.1의 임시 포트 두 개에서 Uvicorn h11으로 fixture와 원본 HTTP app을 실행한다. lifespan과 DB writer는 끄고 app의 upstream client를 loopback fixture로 연결했다. 따라서 설치된 전체 서비스의 startup·DB·LiteLLM 연동은 검증하지 않았다. 실제 LLM, 유료 API, 회사 endpoint에는 요청하지 않았다.

## 예약은 arrival order에 민감하다

설정은 budget=1, model cap=4, heavy 비용=0.75, light 비용=0.25, aging=0이다. 비용은 이 스케줄러의 실행 예산이며 TPM이 아니다.

| 순서 | 세 요청이 도착한 직후 | 첫 heavy 해제 뒤 |
|---|---|---|
| heavy-1 → light → heavy-2 | heavy-1 + light 실행, heavy-2 대기 | heavy-2 입장 |
| heavy-1 → heavy-2 → light | heavy-1만 실행, heavy-2 예약, light 대기. 미점유 예산 0.25 | heavy-2 + light 입장 |

원본 [`test_oversized_prompt_does_not_starve_fast_lane`](../references/overlaat/tests/test_queue_behavior.py)의 설명도 같은 제약을 인정한다. 원래 기대와 달리 두 번째 heavy가 먼저 예약하면 light가 막히므로 테스트의 도착 순서를 바꿨다고 명시한다. 따라서 테스트가 통과했다고 “모든 도착 순서에서 fast lane 보장”으로 읽으면 안 된다.

[별도 재현 스크립트](../scripts/probe-overlaat-reservation.py)는 원본 Scheduler를 직접 호출하며 두 순서와 첫 heavy 해제 뒤 상태를 검사한다. [관측 JSON](../evidence/reference-tests/overlaat/reservation-order.json)의 virtual_time은 이벤트 순서 표시다. 실제 소요 시간·GPU 작업은 아니다. `ever_admitted` 역시 현재 실행 중 여부와 다르다.

우리 설계에는 두 순서 모두를 반례로 넣는다. 작은 요청의 무제한 우회와 큰 요청의 즉시 예약 어느 쪽도 기본 정답으로 정하지 않는다. 큰 요청의 완료율과 최대 대기, 작은 요청 p95, 유휴 예산을 함께 비교해야 한다.

## ASGI 테스트와 실제 소켓 취소는 다르다

설치한 HTTPX 0.28.1의 `httpx/_transports/asgi.py`는 `body_parts`에 응답 body를 모으고 app 실행이 끝난 뒤 합친 stream을 반환한다. 따라서 이 transport만으로 첫 SSE 데이터가 upstream 완료 전에 도착했는지, 실제 TCP disconnect가 전파됐는지 검증할 수 없다.

원본 [`test_client_abandoned_no_abort_holds_slot_until_upstream_drains`](../references/overlaat/tests/test_queue_behavior.py)는 `_forward`를 직접 부르고 반환 iterator를 `aclose()`한다. `GeneratorExit`를 넣는 fake stream 검사다. 실제 Uvicorn에서 응답을 읽는 task가 취소되는 경로와 구별해야 한다.

[TCP probe](../scripts/probe-overlaat-tcp.py)는 다음 순서를 만든다.

1. cap=1인 원본 proxy로 첫 요청을 보낸다. fixture는 SSE 데이터 하나를 보낸 뒤 별도 gate에서 멈춘다.
2. 첫 요청이 아직 열린 상태에서 두 번째 요청이 큐에 들어간 것을 확인한다.
3. 첫 client response를 닫는다.
4. abort=false에서는 gate를 닫아 둔 채 다음 요청 완료를 최대 0.25초 관찰하고 상태를 저장한다. 이후 gate를 열어 정리한다. 이 시간은 관측 상한이며 latency 목표가 아니다.

[abort=true](../evidence/reference-tests/overlaat/tcp-disconnect.json)는 예상대로 첫 연결을 닫고 슬롯을 반환했다. [abort=false](../evidence/reference-tests/overlaat/tcp-no-abort.json)도 gate가 닫힌 동안 `upstream_fixture_closed=true`, `next_completed=true`, `used=0`, `queue_depth=0`을 기록했다. [반복 2](../evidence/reference-tests/overlaat/tcp-no-abort-repeat-2.json)와 [반복 3](../evidence/reference-tests/overlaat/tcp-no-abort-repeat-3.json)도 같았다. 따라서 이 fixture의 자연 EOF까지 기다리지는 않았다.

**코드 근거로 추론한 경로:** Uvicorn h11은 ASGI spec 2.3을 전달한다. Starlette는 이 경로에서 disconnect 시 stream task를 취소하고, httpcore의 body iterator는 `BaseException` 발생 시 연결을 닫는다. 이 때문에 원본 `finally`의 drain이 이미 종료된 iterator를 만날 수 있다. 이 설명은 소스 추론이며 예외 stack 전체를 계측한 인과 증명은 아니다.

직접 확인한 것은 HTTP 연결과 gateway 점유 상태다. fixture는 모델이 아니므로 실제 엔진이 계속 계산하는지, 몇 token을 낭비하는지, 다른 OS·HTTP/2·ASGI 서버에서도 같은지는 미확인이다. 특정 고정 버전의 정책 검증 공백으로 보고하며 전체 제품의 안정성을 일반화하지 않는다.

## 조사 스크립트의 실패도 보관한다

예약 probe의 첫 실행은 조사자가 `Scheduler.budget` 메서드를 숫자처럼 사용해 실패했다. 원본 API를 확인하고 호출부만 수정했다.

TCP probe의 초기 버전은 `anext(response.aiter_lines())`에 만든 임시 iterator를 유지하지 않았다. 의도한 close 지점 전에 stream이 종료되어 queue 확인이 실패했다. [진단 로그](../evidence/reference-tests/overlaat/tcp-probe-harness-failure.log)를 남겼다. iterator를 변수에 유지하는 한 가지 수정 후 첫 연결을 열어 둔 채 queue_depth=1을 확인했다. 이 초기 실패는 upstream 결함으로 집계하지 않는다.

## 재현과 다음 실험

설치된 격리 환경을 사용하는 명령이다. 작업 위치는 연구 폴더다.

```sh
PYTHONDONTWRITEBYTECODE=1 .venvs/overlaat/bin/python scripts/probe-overlaat-reservation.py
PYTHONDONTWRITEBYTECODE=1 .venvs/overlaat/bin/python scripts/probe-overlaat-tcp.py --abort-on-disconnect true
PYTHONDONTWRITEBYTECODE=1 .venvs/overlaat/bin/python scripts/probe-overlaat-tcp.py --abort-on-disconnect false
```

첫 제품의 검증에는 정상 EOF, 큐 취소, 첫 body 전 취소, SSE 사이 취소, 느린 downstream, timeout 경합을 각각 넣는다. no-abort 엔진의 자원 점유는 독립적으로 검증해야 한다. cancellation 신호와 자원 소유권의 수명을 분리한다는 설계 원칙을 가져오되, 경쟁 구현의 수정이나 제품 코드는 이번 조사에서 작성하지 않았다.
