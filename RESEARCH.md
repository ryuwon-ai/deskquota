# 제한된 LLM 요청을 조정하는 게이트웨이 조사

기준일: 2026-09-16. 목표는 **한 PC의 여러 에이전트가 제한된 LLM 용량을 덜 낭비하고 공정하게 쓰게 하는 최소 게이트웨이**다.

## 현재 결론

유사 구현은 이미 있다. Overlaat의 자원별 예약, PromptForge의 클라이언트 순환 큐, SMG의 선점 전 취소, MindRouter와 obleth의 공정 배분을 확인했다. 적용 대상은 **사내·로컬에 한정하지 않고, 한도나 용량 제한이 있는 LLM 요청 전반**이다. 차별화 가설은 공유 한도 안에서 요청의 낭비와 대기를 줄이는, 설치가 쉬운 네이티브 제품이다. [포지셔닝·독창성 재평가와 31개 저장소 Star](reports/positioning-and-originality-2026-09-15.md)는 2026-09-15의 최신 소개와 기존 구현·실행 근거를 구분한다. 일반적인 성능 우위와 시장 규모는 아직 입증하지 않았다.

## 구현 진행

| 범위 | 현재 상태 |
|---|---|
| HTTP/SSE, RPM/TPM, 공정 큐, 429·취소 | Core Task 1–8 수용. 합성 비교에서 효율 우위는 입증하지 못함 |
| 실험용 Backfill | [동일 바이너리의60초5시드 탐색 반복](reports/backfill-experiment-results.md): 짧은 평균 지연16.3% 감소, p95·처리량 동률. 일반 기본값은 그대로 |
| 경쟁 패턴 적용·실행 비교 | [새 비교와 반례](reports/competitor-comparison-results-2026-09-15.md): CLI 상태 표시·metadata 의미 보존, 추가 의존성0, 관련 검사221개 통과. 독립 입력의 완료율 반례로 기본 RR 유지 |
| 사내 피드백 반영 | [Windows 소스·표준 인증·exact cache·시작 대기](reports/company-feedback-2026-09-15.md): 당시 macOS 420개 검사 통과. 실제 Windows 빌드·실행은 별도 검증 필요 |
| 경쟁 패턴 추가 적용 | [JSON 정산·캐시 상태](reports/competitor-round2-2026-09-15.md): 전체 검사427개·독립 검토 통과. 작은 TPM 합성 조건에서 완료5/20→20/20, 무환급 대조군 유지. 실제 제공자 효율은 별도 검증 |
| 진단 추가 전 일반 release 경쟁 재비교 | [35회 실행·캐시 재확인](reports/current-competitor-comparison-2026-09-15.md): 무대기5반복 p95 0.433–0.556ms·idle9.56–9.66MiB. 한 제한 조건에서 첫 창16/최종18건 완료. 2,680건의 실패·대기와 최신 LiteLLM 버전 미실행 범위 포함 |
| 공유 한도 진단·retry 헤더 캐시 | [현재 변경과 전후 검사](reports/quota-diagnostics-2026-09-15.md): 캐시 제외·예약/usage 차이·보호 요청 병목 표시. 합성 재실행 호출2→1, Vary 대조군 유지. 전체435검사·독립 검토 통과, 지연 우위는 미확정 |
| 선택형 입력 추정·38초 하한 분석 | [BPE 측정과 비용](reports/input-estimation-results-2026-09-16.md), [경쟁 quota 계약 비교](reports/quota-policy-deep-dive-2026-09-16.md): 불필요한 TPM 예약 일부 감소. 동일 strict RPM에서18건을 모두 처리하는38초 하한과 빠른 성공 응답만의2초 수치를 구분 |
| 대기 중 캐시 재사용·429 전달 | [구현·실측·에이전트 토론](reports/efficiency-improvements-and-debate-2026-09-16.md): 동일10건·동시성1에서 호출10→1, RPM1·750ms 마감 내 완료1/10→10/10. 전후5쌍, 고유 요청 대조군 유지. macOS453검사·Windows185관련 검사 통과. 실제 API·저사양 우위는 미확인 |
| 재시도 포함 완료 시간·기본 빌드 경량화 | [새 검증과 설정 선택](reports/workflow-completion-and-lightweight-builds-2026-09-16.md): 연속 보충 RPM fixture에서 같은 바이너리의 로컬 quota 설정 비교5쌍, p95 38.13→2.51초·양쪽90/90완료·각90호출. 실제 rolling 한도의 긴 대기는 유지. 기본 macOS10.20MB·Windows16.38MB, Mac각456검사·Windows기본409/BPE관련236검사 통과. 제공자 정책에 맞춘 기존 설정의 효과이며 일반 성능 우위는 미입증 |
| `on/off/status/restart`, 첫 설정, 파일 변경·복원 | Native Task 1–3 수용 |
| Pi·Claude Code·Codex 연결 | [Native Task 4 수용](evidence/native-task4-accepted.json). 동일 Mac 빌드에서 세 도구의 격리 도구 왕복 확인 |
| 사용자 로그인 자동 시작 | [Native Task 5 수용](evidence/native-task5-accepted.json). 명세·품질 통과, 실제 계정 등록·로그인 미검증 |
| 작은 installer와 최종 메모리·실행 시간 측정 | [Native Task 6 수용](evidence/native-task6-accepted.json). macOS ARM 패키지 설치·재설치와 세 도구 왕복 확인 |
| 자동 압축 호환성 | [macOS·Windows 네이티브 대조](reports/auto-compaction-audit-2026-09-16.md): 일반 요약과 다음 턴 통과. 전용 compact endpoint·opaque 입력·큰 바이트 추정 거절은 재현된 미지원 경계 |
| 실제 Windows/Linux·저사양 PC·사용자 채택 | [Windows 10 x64 GNU 빌드·실행·클라이언트 검사](reports/windows-followup-and-improvements-2026-09-16.md) 진행. Windows 파일 교체의 간헐적 실패 원인, Linux·저사양 성능·사용자 채택은 미검증 |

이전 Native Task 6의 macOS ARM 패키지는 별도로 보관한 로컬 산출물이다. 공개 저장소에는 [체크섬](product/artifacts/native-final-integration-package/llmgw-macos-arm64.tar.gz.sha256)과 검증 기록을 남겼다. 당시 Rust 테스트 370개와 첫 저장→연결→종료의 독립 재검토를 통과했다. 동일 패키지의 M4 32GiB 개발 PC 관측은 대기 메모리 9.73MiB(10표본), 시작 36.4–63.1ms·종료 35.6–45.3ms(각 5회)다. [측정 범위와 제한](product/artifacts/native-final-integration-package/acceptance/README.md)을 함께 읽는다. 이 수치는 후속 변경본의 성능이나 저사양·처리량 우위를 입증하지 않는다. 현재 소스의 추가 변경과 검증은 위 최신 보고서를, 실행 방법은 [설치 안내](product/docs/installation.md)를 따른다.

## 합의한 경계

| 항목 | 기준 |
|---|---|
| 실행 위치 | 사용자 PC, Windows/macOS/Linux 네이티브 |
| 설치 제약 | Docker·WSL 필수 의존 금지. 코어는 Rust binary 하나; Python·Node는 개발/검증에서 사용 가능 |
| 핵심 언어 | Rust. 언어 자체를 성능 개선의 증거로 삼지 않음 |
| 적용 환경 | 공유 RPM/TPM·동시성 제한이 있는 상용·무료·사내 API와 로컬 endpoint. 현재 구현은 한 instance의 한 upstream이며 엔진 내부 GPU/KV 제어와 구분 |
| 클라이언트 | Claude Code, Codex, Pi 등의 동일 프로토콜 전달. API 포맷 변환 제외 |
| 비용 | 공개 mock 중심. 추가 유료 API 비용을 기본값으로 발생시키지 않음 |
| 사내 검증 | 사용자가 구두로 전할 결과는 별도 기록. 공개 재현 실험과 합치지 않음 |
| 장기 목표 | 실제 사용·기여 기반의 5,000 stars 이상과 Codex for OSS 지원. 달성 보장 없음 |

## 읽는 순서

1. [전체 참조 목록](reports/reference-catalog.md): 기존27개와 [추가3개 quota proxy](reports/additional-quota-references-2026-09-14.md)의 clone·고정 SHA·분석 진입점. [초기9개 비교](reports/repository-analysis.md)는 이전 조사 기록
2. [논문·기업 자료 검토](reports/literature-review.md)와 [ICML 2026 UniBoost 추가 검토](reports/recent-scheduling-evidence-2026-09-14.md): 실험 조건·tradeoff·HTTP gateway 적용 경계
3. [수요·제품 방향](reports/demand-and-positioning.md): 실제 사례와 아직 검증되지 않은 가설
4. [검토한 코어 설계](docs/superpowers/specs/2026-09-12-local-quota-gateway-design.md)와 [네이티브 설정 설계](docs/superpowers/specs/2026-09-12-native-setup-design.md): quota·공정 큐와 setup/on/off·모델/파일 변경 고지
5. [벤치마크 명세](reports/benchmark-spec.md): 개선을 입증하거나 기각할 실험
6. [작업 하네스](HARNESS.md): 반복 작업, 검증, 재개 규칙
7. [기존 구현 동작 검증](reports/reference-validation.md): 원본 테스트 57개, 예약 도착 순서, 실제 TCP 취소의 차이
8. [Pi 클라이언트 검증](reports/client-integration-validation.md): 실제 CLI의 재시도 중첩·Retry-After·세션 헤더
9. [Hermes 설치·연동 조사](reports/native-setup-research.md): 추가 clone 1개, 최신 Codex/Claude 설정과 OS 로그인 실행. [Native lifecycle 사전 점검](reports/native-lifecycle-preflight.md)은 기존 dependency와 config별 소유 identity의 연결 범위를 기록
10. [코어 구현 계획](docs/superpowers/plans/2026-09-12-gateway-core.md)과 [네이티브 경험 구현 계획](docs/superpowers/plans/2026-09-12-native-experience.md): 두 명세·네 chunk 리뷰 완료
11. [스트리밍 관찰 계약](reports/stream-protocol-contracts.md): Pi·Codex와 공식 프로토콜의 usage·framing·종료 경계를 Core Task 3 수용 조건으로 연결
12. [NVIDIA 소량 API 검증 준비](reports/nvidia-api-evaluation.md): dry-run 기본 도구·로컬 검사 완료, 키 위치와 실제 무료 계정 검증 대기
13. [경쟁 구현 교차 토론](reports/competitor-adoption-debate-2026-09-15.md)과 [적용·실행 비교](reports/competitor-comparison-results-2026-09-15.md): 고정 Bifrost·HiveMind·LiteLLM 비교, PromptForge 소스 검토, 독립 입력의 반례와 metadata 기전 검사. 이전 패키지의 수치와 새 실험 바이너리 수치를 구분한다.
14. [사내 피드백 반영](reports/company-feedback-2026-09-15.md)과 [JSON 정산 후속](reports/competitor-round2-2026-09-15.md): 표준 인증·native Windows 소스 수정·bounded exact cache, 일반 JSON 사용량 정산의 범위와 별도 실행 근거.

## 직접 확인한 것과 남은 것

- **Core Task 7은 명세·품질 재검토 후 완료했다.** 64개 동시 writer 검사 누락을 보완했고, 잘린 429 본문이 응답 없는 연결 종료가 되는 문제를 기존 502 경로로 수정했다. [품질 재검토](evidence/task7-quality-review/rereview/README.md)는 원래 반례와 인접 회귀를 디버그·최적화 빌드에서 각각 통과했다.
- Core Task 7의 당시 최종 Rust 테스트는 **두 빌드 각각 188개 통과**했다. [직접 실행한 native probe](evidence/product-task7-quality-parent-verification.json)는 각 빌드에서 21개 ingress·24개 upstream 시도와 모든 HTTP 결과를 확인했고, release streaming·debug CLI도 통과했다. Pi 최종 positive/negative 결과와 script·template·binary 해시를 [대조](evidence/product-task7-quality-pi-audit.json)했다. 이전 실패·수정 기록은 보존한다.
- **Core Task 8은 전체 40run 측정·부모 검산·독립 명세 및 [품질 재검토](evidence/task8-quality-review/rereview/README.md) 후 수용했다.** 하네스의 재개 검증·시작 취소 정리 문제 두 건을 수정하고 원래 반례로 재검증했다. [결과 보고서](reports/benchmark-pilot-results.md)에 본 측정 4,000건·별도 warmup100건과 모든 실패를 기록했다. 무대기 제품 RR 추가 p95는 0.1985–0.3480ms, idle RSS는 8.625–8.6875MiB로 잠정 예산 안이었다. 빈 장부·이 M4 Mac의 결과다. quota의 300초 내 완료 합계는 direct328/제품 RR285/FIFO285/비교용 RR284로 성능 우위를 확인하지 못했다. 이후 완료와 추가 대기는 따로 표시했다. [quota 계약 후속 분석](reports/quota-contract-followup.md)과 아래 정산 방식 비교로 이어갔으며, 다른 성능 가설은 별도 검증이 필요하다.
- **Native Task 1 명세·품질 통과:** `on/off/status/restart`, config별 lifetime lock과 인증된 identity를 구현했다. 설정 편집 중 on 안내와 restart 경합을 수정하고 [명세 재검토](evidence/native-task1-spec-review/rereview/README.md)·[품질 재검토](evidence/native-task1-quality-review/rereview/README.md)를 통과했다. 당시 수정본은 lifecycle 테스트 Debug·Release 각각 **25개**, CLI 1회와 fmt/check/clippy를 통과했다. 품질 검토자는 경합 3건을 독립 재현해 정상 동작과 worker 보존을 확인했다. [당시 HOLD](product/artifacts/native-task1/quality-fix/README.md)의 실행 파일은 8,697,040B다. 이전 211개 전체 테스트와 [Mac 3회 유휴 측정](reports/native-lifecycle-observation.md)은 해당 이전 소스의 증거로 보존한다. `target/native`로 원측정 파일을 보존했고 현재 사용자 설정·자동 시작 등록은 변경하지 않았다. 현재 완료 범위는 위 진행 표를 따른다.
- **정산 비교 실험 수용 완료:** [계획](docs/superpowers/plans/2026-09-12-accounting-ablation.md)에 따라 같은 ordinary RR binary의 `reserved`/`actual` 정산만 바꿔 10회·5쌍·1,000건을 측정했다. [결과 보고서](reports/accounting-ablation-results.md)는 [독립 검산](evidence/accounting-ablation-parent-audit.json), [명세 재검토](evidence/accounting-task2-spec-review/rereview/README.md), [품질 검토](evidence/accounting-task2-quality-review/README.md)를 통과했다. 같은 300초 완료는 **285/290**, 이후 drain 포함 최종 완료는 **397/410**으로 구분한다. 최종 timeout은 25/15지만 긴 요청은 **15/15**로 같고, 감소는 짧은 요청에서 발생했다. 다섯 seed는 한 합성 fixture의 timing jitter 반복이며 실제 agent·저사양·경쟁 제품 우위의 근거가 아니다. Rust 실행 경로와 기본 정산 정책은 그대로다. 측정 파일·실행 파일은 이후 네이티브 기능 개발과 분리해 보존한다.
- **Native Task 2 첫 설정 마법사 수용:** 기존 설정·주석 보존, 저장만 하기, 변경 없는 재실행과 CA 오류 시 기존 worker 보존을 확인했다. [구현본](product/artifacts/native-task2/quality-fix/README.md)은 당시 전체 Rust264개를 통과했고, [독립 품질 재검토](evidence/native-task2-quality-review/rereview/README.md)에서 추가79개·PTY10회를 확인했다.
- **Native Task 3 파일 변경·복원 수용:** [최종 구현본](product/artifacts/native-task3/nonfinite-fix/README.md)의 당시 전체 Rust313개와 독립 QUALITY10개·SPEC4개를 확인했다. 사용자 편집·재시도·부분 실패·파일 별칭 잠금을 보완했고 복원할 수 없는 소유 TOML 날짜·비유한 값은 쓰기 전에 거절한다. [품질](evidence/native-task3-quality-review/rereview-q3/README.md)·[최종 명세](evidence/native-task3-spec-review/final-confirmation/README.md).
- **Native Task 4 클라이언트 연결 수용:** [최종 구현본](product/artifacts/native-task4/quality-fix/README.md)은 동일 release에서 전체 Rust339개·release 연동24개·Python7개 및 설치된 Pi·Claude Code·Codex의 격리 도구 왕복을 통과했다. [품질 재검토](evidence/native-task4-quality-review/rereview-q1-q2/README.md)는 독립7개·PTY2회, [최종 명세 확인](evidence/native-task4-spec-review/final-confirmation/README.md)은 독립8개와 보존 파일1,245개 무변경을 확인했다. Claude 공유 파일 검사, 확인 후 readiness, 연결 해제 뒤 오래된 계획의 소유권을 보완했다. [부모 대조](evidence/native-task4-quality-fix-parent-hold-check.json)는 실행 파일·소스·기존 증거의 일치를 확인했다. 실제 Windows/Linux와 Codex catalog listing, 새 성능 수치는 미검증이다. 이전 한 번의 Mac ACL 오류는 정확한 원인 미확인으로 보존한다.
- 현재 사용자 설정·자동 시작 등록은 바꾸지 않았다. 단계별 실패·수정·원측정과 초기 [기록 작성 사고](product/artifacts/native-task2/execution-incident.json)는 그대로 보존한다.
- 27개 참고 저장소를 clone하고 SHA·브랜치·시각·sparse 범위를 [기록](evidence/repositories.json)했다. 기존 26개에 공식 benchmark 저장소 1개를 추가했다. 분석은 gateway·engine·local client·논문 artifact·Dynamo·측정 하네스로 나눴다. [전체 목록과 근거](reports/reference-catalog.md).
- 2026년 OSDI·NSDI·MLSys 논문 3편의 PDF를 내려받아 원문과 실험 조건을 검토했다. [출처·해시](papers/manifest.json)를 보관한다. 다른 논문과 기업 글은 검토 깊이를 따로 표시했다.
- Overlaat의 선택한 원본 테스트 57개를 실행했다. 별도 probe로 예약의 도착 순서 의존성과 TCP 취소 시 no-abort 정책의 검증 공백을 확인했다. 이는 경쟁 성능 비교가 아니다.
- 설치된 Pi 0.84.2를 격리한 설정과 loopback fixture로 실행해 8개 사례를 확인했다. 기본 429 재시도, 두 retry 계층의 중첩, 불완전한 stream 뒤 재요청, session header 옵션을 관찰했다. 실제 LLM 또는 우리 gateway와의 호환 검증은 아니다.
- `product/`의 Core Task 1/2는 당시 Rust 설정·HTTP 전달 테스트 **50개**와 명세·품질 재검토를 통과했다. 실제 `llmgw run` binary의 요청 1회와 control stop 종료도 [검증](evidence/product-cli-smoke.json)했다.
- Core Task 3의 SSE 수명·취소·backpressure는 명세·품질 리뷰와 수정 후 완료했다. 동일한 **83개 테스트를 디버그·최적화 빌드 각각** 통과했고, [최적화 CLI의 수명 검사 4개](evidence/product-stream-task3-final-release.json)도 통과했다. [명세 재검토](evidence/task3-spec-rereview/README.md)와 [품질 재검토](evidence/task3-quality-rereview/README.md)에 반례·수정·검증을 보관한다.
- Core Task 4도 완료했다. 설치된 Pi 0.84.2로 일반 응답과 임시 파일 read 도구 왕복을 확인했고([실행 결과](product/artifacts/pi-e2e.json)), [명세 리뷰](evidence/task4-spec-review/README.md)·[품질 재검토](evidence/task4-quality-rereview/README.md)를 통과했다.
- Core Task 5의 RPM·TPM 장부는 **명세·품질 재검토 후 완료**했다. 초기 Actual 정산 오류는 수정해 [명세 재검토](evidence/task5-spec-rereview/README.md)를 통과했고, [별도 품질 리뷰](evidence/task5-quality-review/README.md)가 찾은 `message_stop` 뒤 usage 문제도 좁은 순서 검사로 수정했다. 수정본의 **138개 Rust 테스트를 두 빌드에서 각각** 통과했고, Pi·CLI 왕복도 다시 확인했다. [품질 재검토](evidence/task5-quality-rereview/README.md)는 원래 반례와 동시 취소·8,192개 장부 항목을 포함한 독립 테스트 7개를 각 빌드에서 통과했다. [구현 기록](evidence/implementation-progress.json)에 실패·수정·검증 단계를 보관한다.
- Core Task 6의 root별 FIFO·라운드로빈·기아 방지 큐는 **명세·품질 리뷰 후 완료**이다. 정상 16개 root 순환의 잘못된 우회 집계와 barrier 선택·유지 반례를 수정했다. 전체 **161개 Rust 테스트를 두 빌드에서 각각** 통과했다. [독립 foreground probe](scripts/probe-product-fairness.py)도 [디버그](evidence/product-fairness-task6-debug.json)·[최적화](evidence/product-fairness-task6-release.json)에서 각각 77개 ingress의 완료·취소·거절, FIFO/RR, 64개 한도와 재입장을 확인했다. Pi와 stream 수명 검사도 현재 binary에서 통과했다. [명세 리뷰](evidence/task6-spec-review/README.md)의 독립 8개 검사도 두 빌드에서 통과했다. [품질 리뷰](evidence/task6-quality-review/README.md)의 독립 경합·타이머 검사 3개도 두 빌드에서 통과했다. 이후 Core Task 7 검증과 Task 8 합성 측정 상태는 위 항목을 따른다. 실제 provider quota 정확도·실제 LLM·Windows 실행은 아직 검증하지 않았다.
- 사용자 범위 승인과 네이티브 UX 추가 요구를 반영한 두 명세 및 네 구현 chunk의 리뷰를 완료했다. [리뷰 기록](evidence/design-plan-review.json). 승인된 계획에 따라 mock을 통한 첫 엔드투엔드 경로를 구현하고 있다. 별도 범위 재승인을 기다리지 않는다.

`python3 scripts/research.py verify`는 조사 산출물의 무결성만 확인한다. `python3 scripts/research.py status`로 미완료 작업을 확인한다.
