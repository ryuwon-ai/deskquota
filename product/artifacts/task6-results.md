# Core Task 6 구현 검증 — 독립 검토 대기

Task 6 구현·자체 검증을 마쳤다. 소스와 바이너리를 HOLD로 고정하며 명세 검토와
새 품질 검토가 끝나기 전에는 Task 6를 최종 수용 완료로 표시하지 않는다.

## 직접 검증

| 항목 | 관찰 |
|---|---|
| 기준 소스 | Task 5 최종 manifest의 36개 파일 모두 편집 전 SHA-256 일치 |
| 초기 RED | root FIFO, RR, 전체 64개 대기 한도 동작이 실제로 실패. 이어 8회/5초 보호·deadline·원래 age 등의 실패를 보존 |
| 추가 반례 | 정상 16-root RR을 bypass로 잘못 세는 문제, threshold head만 oldest 후보로 고르는 문제, 다른 trigger 취소로 barrier가 사라지는 문제를 RED→GREEN으로 수정 |
| 공정성 suite | 22개. 같은 라이브러리 Admission→Queue→Ledger의 exact-cost 가상 시간 trace와 실제 TCP 검사를 포함 |
| 전체 Rust | Debug 161개, Release 동일 161개 통과. lib 5 / config 19 / fairness 22 / quota 12 / stream 32 / wire 44 / usage 27 |
| 정적 검사 | fmt, locked check all-targets, clippy all-targets `-D warnings` 통과 |
| 실제 TCP | root0 실행 1개+child15개와 root1 backlog가 `first,b1,a1,b2,a2…`로 시작. 임의 session header는 share를 늘리지 않음 |
| Metadata | models와 count_tokens가 서로 다른 root에서도 RPM1을 공유하고 TPM debit0 유지 |
| Overflow·취소 | 총64개 대기 뒤 65번째429 `gateway_queue_full`; RST로 대기64개+미시작active1개 제거 후 upstream0, starts0, admitted cleanup1 |
| 전환 경합 | queue→admitted 뒤 해당 waiter를 poll하기 전 취소해도 joint hold가 정확히 해제되고 upstream start0 |
| 수치 상태 | u64 초과 TPM debt를 손실 없는 decimal string으로 직렬화. estimate mode는 byte proxy이며 exact tokenizer라고 표시하지 않음 |
| 최종 Pi 0.84.2 | 두 바이너리 각각 양성 일반1회·read tool2회 upstream, 음성 gateway-off는0+0회 및 예상 실패 |

음성 검사의 probe exit1/`passed:false`는 의도한 실패이며 양성 통과 개수에 더하지
않았다. 기존 Python 테스트 5개와 probe 구현은 수정하지 않았고 이번에는 재실행하지
않았다. 부모가 준비한 foreground CLI/stream/fairness probe는 이 구현자의 증거에
합산하지 않았으며 HOLD 이후 부모가 별도로 실행한다.

## 구현과 소유권

신규 파일은 `src/admission/queue.rs`, `tests/fairness_contract.rs` 두 개다.
기존 admission/quota, config 검증, server/control/metrics, socket fixture helper,
런타임 계약을 수정했다. 9개 기존 파일 변경 + 2개 신규 파일이며 최종 manifest는
38개 파일이다. 이전 Task 1–5 artifact와 연구·reference·사용자 설정은 보존했다.

Queue가 기존 Ledger의 단일 소유자다. root별 FIFO head를 선택마다 한 번 검사하고,
확정 admission에서 quota+slot을 함께 확보한다. 실제 upstream HTTP 시작에서만 RPM을
확정한다. resource 부족으로 건너뛴 head만 다른 실제 admission 때 bypass를 얻는다.
어느 head든 8회/5초에 도달하면 전체 valid total-capacity-fit head 중 원래 wait가
가장 오래된 하나를 선택하며, 선택한 ticket 자체가 입장/취소/만료될 때까지 유지한다.

대기 ticket의 Drop은 pending 항목 또는 아직 poll하지 않은 reservation을 동일 owner
안에서 정리한다. RAII Hold를 같은 mutex 아래에서 Drop하지 않는다. 원래 enqueue
시각과 queue deadline은 admitted Hold까지 보존하지만 retry는 구현하지 않았다.

Status의 `admission`은 queue/active/등록root별 대기수/blocked reason/barrier root,
estimate·quota mode·accounting과 lossless local sums를 담은 bounded typed JSON이다.
Control 인증 및 Origin 규칙은 그대로다. Native `status --json` CLI는 후속 작업이다.

## 실패 기록과 수정 범위

초기 heavy-light-heavy trace는 provisional Hold 해제 뒤 실행 양보가 빠져 한 번
실패했다. socket capture helper는 원래 첫 요청만 반환하는 함수인데 누적 순서로
사용해 첫 ID를 반복 관찰한 오류가 있었고, 실제 누적 captures helper를 추가했다.
heavy-heavy-light의 최초 trace는 마지막 quota expiry와 120초 queue deadline이 같아
정당하게 만료됐다. enqueue를 1초 뒤로 옮긴 feasible trace에서 실제 두 heavy를
시작·완료시키고 deadline 전에 light까지 완료했다. 실패 로그는 삭제하지 않았다.
기존 control test가 요구한 JSON key 순서도 전체 회귀에서 발견해 보존했다.

## 해석과 미확인

가상 exact-cost 결과는 정책 동작의 증거이고 실제 HTTP는 JSON UTF-8 byte+output
예약 또는 metadata 비용이다. provider의 실제 RPM/TPM 준수, 최적 이용률, 처리량 향상,
일반적인 bounded wait, Windows 실행, 실제 LLM, Task 7 retry/global RSS는 입증하지
않았다. cap1의 실행 중 긴 작업은 선점되지 않는다. 알려진 quota의 실제60초 startup
hold와 예제 설정은 유지했다.

## 고정된 검토 대상

- Debug SHA-256: `bc451f4bdfdac9c1f5c0e1de6200b17092ced8d2ab7871f0cf4e787524a89b56`
- Release SHA-256: `65ca882cf0f299ccc85ba1651500193956dce441951eacdb45e17a231c4c896c`
- 전체 소스·바이너리: [source-manifest.json](task6/source-manifest.json)
- 명령과 raw log 진입점: [commands.md](task6/commands.md)
- Pi 명령·exit·원래 upstream 수: [pi-commands.json](task6/pi-commands.json)
