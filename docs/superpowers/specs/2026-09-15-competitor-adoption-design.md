# DeskQuota 경쟁 패턴 적용과 실행 비교

사용자 승인: 2026-09-15 “오케이 적용해 그리고 해당 repo들도 들고와서 비교해봐”. 직전 [교차 토론](../../../reports/competitor-adoption-debate-2026-09-15.md)의 권고를 구현·검증한다. 기존 core/native 명세와 안전 계약을 유지한다. 새로운 provider 계약이나 캐시·라우팅 플랫폼은 포함하지 않는다.

## 제품 변경

1. 일반 `status` 출력에 이미 존재하는 admission 대표 차단 사유, 보호 root, quota mode/capacity/debit/held, accounting/estimate mode를 표시한다. JSON 상태 계약과 gateway 정책은 바꾸지 않는다. 전체 큐의 대표 사유이지 요청별 정확한 원인/ETA가 아님을 표현한다. unknown/unlimited를 잔량 0으로 바꾸지 않는다.
2. 실제 Models·CountTokens의 비생성 비용과 TPM unknown/unlimited 생성 비용을 명확히 구분한다. 기존 `RequestCost`에 최소한의 구분을 더하고 ledger entry까지 보존한다. 진짜 metadata는 정산에서 양의 TPM debit으로 변하지 않는다. 일반 zero fixture/생성 요청의 늦은 usage 차감은 유지한다.
3. 기존 실험 backfill에서 증명된 metadata만 zero-cost 모호성 검사에서 제외한다. candidate와 기존 active/unstarted entry를 모두 처리한다. RPM, execution slot, root FIFO, cooldown, deadline, bounded queue/ledger, head의 조건부 입장 기회는 그대로 보호한다. 로컬 catalog로 upstream 응답을 대신하지 않는다.
4. 현재 RR와 backfill의 독립 workload·120초 반복·full ledger 비용을 비교해 기본값 승격 여부를 판단한다. 이번 작은 타입 변경만으로 backfill을 기본값으로 바꾸지 않는다. 이득 없는 조건과 반례는 보존한다.

## 비교

기존 pinned clone은 보존한다. Bifrost·PromptForge·HiveMind·LiteLLM의 현재 release/SHA와 실행 조건을 별도 캐시 디렉터리에 고정한다. 직접 실행 가능한 비교군은 최소 하나 이상 실제 동일 local mock에 연결한다. 가능하면 native Bifrost와 가장 가까운 HiveMind를 둘 다 실행한다. 실행되지 않은 레포는 정적 비교로 명시하고 실패 원인·정확한 명령을 남긴다.

같은 요청 의미, 모델, 원문 body, 전체 retry budget, upstream 제한, deadline, 종료 분모를 사용한다. 제공자가 payload나 모델명/usage를 바꾸면 기록하고 admission 정책 단독 비교라고 주장하지 않는다. 경쟁자가 동일 예약 계약을 표현하지 못해도 의도적으로 불리한 값을 넣지 않는다. 기능적으로 지원하는 최선의 작은 설정과 원래 계약을 보고한다. 캐시는 끄고 실제 provider/API credential은 사용하지 않는다.

## 검증과 합격

- 작은 결정적 검사: metadata 분류, unknown generation 구분, 늦은 positive usage, 누적 metadata backfill, cap1/RPM 부족/같은-root FIFO, queued/started 취소, deadline/cooldown 및 큰 요청 기회 보존.
- HTTP 검사: metadata 원본 응답/권한 오류 전달과 실제 upstream 시도, generation 정상 stream, 기존 Pi/client 흐름의 영향 범위.
- 성능: metadata와 짧은/긴 생성 분리, 고정 창 완료, 모든 terminal 결과, upstream 시도, queue/total latency, CPU/RSS. 성공 지연은 drain 포함 여부를 명시한다. warmup/startup hold는 측정 창 밖으로 분리하되 cold start 비용도 숨기지 않는다.
- 자원: 모든 Rust 출력과 비교 runtime은 iCloud 밖. 측정 arm은 순차 실행하고 빌드/설치와 겹치지 않는다. 최대 장부8192·roots16의 비용을 빈 장부와 구분한다.
- RR보다 악화되는 workload가 있으면 보고하고 기본 승격하지 않는다. 실제 LLM 정답 작업·Windows·저사양·경쟁사 보편 우위로 일반화하지 않는다.

제품 의존성 추가 0. 변경은 기존 checkout에서 진행해 보존된 연구/fixture 경로를 유지한다. 다른 에이전트는 서로 다른 파일만 소유하며 성능 실행은 부모가 직렬화한다. 커밋·푸시·릴리스는 이 구현 요청에 포함하지 않는다.
