# 스트리밍 관찰 계약

확인일: 2026-09-12. Core Task 3의 wire 전달·사용량 관찰·종료 수명에 필요한 범위만 검토했다. 아래는 **공식 문서와 고정 source 근거**이며 제품 테스트 결과나 실제 공급자 TPM 정책의 증거가 아니다. Gateway는 본문을 재작성하지 않고, 제한된 크기의 observer로 메타데이터만 읽는다.

| 경로 | 근거 | 제품에 적용할 계약 |
|---|---|---|
| Chat Completions | [OpenAI SDK schema](https://github.com/openai/openai-python/blob/main/src/openai/types/chat/chat_completion_stream_options_param.py)는 `include_usage`의 마지막 chunk가 빈 `choices`와 전체 사용량을 담고, 중단되면 누락될 수 있음을 명시한다. [Pi 처리 순서](https://github.com/earendil-works/pi/blob/f3c672245d25ef2283ffc0d9cdec8a5482651103/packages/ai/src/api/openai-completions.ts#L550-L580)도 usage를 choices보다 먼저 읽는다. | 빈 choices에서도 usage를 관찰한다. 요청에 include_usage를 강제로 삽입하지 않는다. usage 누락은 Unknown이며 0이나 잔여 TPM 환급으로 바꾸지 않는다. `[DONE]`과 HTTP body EOF는 별도다. |
| Responses | [Codex](https://github.com/openai/codex/blob/944d6fd1ba4baab69dbedd205282dc72ec20abb5/codex-rs/codex-api/src/sse/responses.rs#L472-L507)는 incomplete와 completed를 구분하고, completed 내부 usage를 optional로 처리한다. [Pi](https://github.com/earendil-works/pi/blob/f3c672245d25ef2283ffc0d9cdec8a5482651103/packages/ai/src/api/openai-responses-shared.ts#L741-L752)는 incomplete도 종료 처리하고 failed를 별도 처리한다. | 클라이언트마다 종료 해석이 다르므로 wire 의미를 바꾸지 않는다. `response.completed.response.usage`를 관찰하되, 종료 표식만 보고 실행 슬롯을 반납하지 않는다. failed/incomplete를 성공으로 합치지 않는다. |
| Messages | [Claude 공식 streaming 문서](https://platform.claude.com/docs/en/build-with-claude/streaming)는 message_delta 사용량이 누적값임을 명시한다. [Pi 초기값](https://github.com/earendil-works/pi/blob/f3c672245d25ef2283ffc0d9cdec8a5482651103/packages/ai/src/api/anthropic-messages.ts#L591-L612)과 [갱신](https://github.com/earendil-works/pi/blob/f3c672245d25ef2283ffc0d9cdec8a5482651103/packages/ai/src/api/anthropic-messages.ts#L740-L775)은 start에서 받은 input을 유지하고, delta에 존재하는 필드만 대입한다. | output 1→7→12는 12다. 20으로 합산하지 않는다. cache read/create 항목을 input과 구분하며, 공급자별 quota 차감 방식을 자동 추론하지 않는다. message_stop도 body EOF와 별도로 관찰한다. |

공식 웹 페이지와 OpenAI SDK의 main 링크는 조회 당시 자료이며 고정 clone이 아니다. Pi·Codex 링크는 [clone manifest](../evidence/repositories.json)의 SHA에 고정했다. Pi의 비용 표시용 `missing || 0`은 사용량 예약·정산 계약에 채택하지 않는다. [Pi Responses 집계](https://github.com/earendil-works/pi/blob/f3c672245d25ef2283ffc0d9cdec8a5482651103/packages/ai/src/api/openai-responses-shared.ts#L552-L576)와 [Chat 집계](https://github.com/earendil-works/pi/blob/f3c672245d25ef2283ffc0d9cdec8a5482651103/packages/ai/src/api/openai-completions.ts#L1509-L1547)도 실제 TPM 제한 정책을 증명하지 않는다.

## SSE framing과 관찰 한계

[WHATWG 표준](https://html.spec.whatwg.org/multipage/server-sent-events.html)에 따라 UTF-8, 최초 BOM, CR/LF/CRLF, 여러 data 행, colon 뒤 한 칸, comment와 빈 행을 처리한다. EOF 전에 빈 행으로 끝나지 않은 이벤트는 완성된 이벤트로 간주하지 않는다. 이 규칙은 wire를 정규화하라는 뜻이 아니다. 수신 bytes를 그대로 전달하며 observer만 framing을 해석한다. Browser 재연결·Last-Event-ID 동작은 구현 범위에 넣지 않는다.

실행 계약은 다음과 같다.

- `first_body_byte`, `first_observed_output_delta`, `terminal_marker`, `response_body_eof`를 구분한다. 첫 ping·role·빈 delta는 출력 생성의 증거가 아니다. 출력 delta도 gateway에서 관측한 시점이며 엔진 내부 첫 토큰 시간이 아니다.
- 단일 SSE 이벤트의 observer 보관 한도는 256 KiB다. 과대 이벤트나 신뢰할 수 없는 framing/usage는 Unknown으로 남기고 전달을 계속한다. 압축·비 SSE 본문을 사용량 추출 목적으로 해제하거나 전부 모으지 않는다.
- 알려지지 않은 새 이벤트·comment·ping은 그대로 통과시킨다. 무관한 이벤트 하나 때문에 이후 정상 usage를 버릴 필요는 없다. 누적값 감소·타입 오류·누락 등 최종 사용량의 신뢰성 문제는 보수적으로 처리한다.
- downstream 전달 큐는 bytes 기준으로 제한한다. bounded item 수만으로는 chunk 크기나 backing allocation의 한도가 보장되지 않는다. 실제 peak RSS는 별도 자원 실험에서 측정한다.
- 기본 drain은 소비자가 떠난 뒤 전달할 bytes를 버리면서 upstream body 종료까지 실행 슬롯을 유지한다. 명시적 close는 HTTP 요청을 닫는다. 어느 경우도 엔진 계산 중단을 증명하지 않는다.
- 단절 검증은 RST·연결 오류와 정상적인 request write-half-close를 구분한다. 응답을 아직 쓰지 않은 상태의 FIN만으로 상대 애플리케이션이 읽기까지 포기했는지 알 수 있다고 가정하지 않는다. 무응답 upstream 사례의 즉시 감지는 RST로 증명하고, 모든 TCP 종료 방식으로 일반화하지 않는다.
- 정상 종료·실패·시간 초과·강제 종료가 경합해도 terminal cleanup과 슬롯 반환은 한 번이다. 사용량 이벤트 도착이 이 수명 계약을 앞당기지 않는다.

## 제품 검증으로 연결할 반례

| 합성 입력/상태 | 기대 관찰 |
|---|---|
| usage가 있는 빈 choices chunk 뒤 `[DONE]`, EOF gate 유지 | usage를 읽고 terminal을 관찰해도 실행 슬롯 유지 |
| Messages start input=20/output=1, delta output=7, delta output=12, stop | output=12; input=20 유지; cache 항목은 별도 |
| 누락·음수·문자열 usage 또는 빈 행 없는 마지막 JSON | 최종 사용량을 0으로 만들지 않음 |
| UTF-8/CRLF가 TCP read 경계에서 잘림; comment·멀티 data 행 | observer framing 정상, 전달 bytes 동일 |
| 과대 이벤트·단일 거대 comment·느린 reader | observer/전달 큐 한도 유지, wire 조용한 손실 없음 |
| 첫 이벤트 후 downstream 완전 단절, upstream은 계속 무응답 | 새 chunk 없이도 단절 감지; 정책에 따라 drain/close, 정확한 슬롯 수명 |
| headers를 아직 못 받은 worker에서 consumer가 떠남 | 시작한 요청은 handler 수명과 분리; 기본 drain 계약 유지 |
| stop과 EOF·deadline 동시 발생 | 신규 입장 중단, bounded drain, cleanup 한 번, worker 잔존 없음 |

이 표는 테스트 수용 조건이다. 통과 여부는 [구현 진행 기록](../evidence/implementation-progress.json)과 제품 raw 로그에서 따로 확인한다.
