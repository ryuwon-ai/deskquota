# Native 배포·크기 측정 사전 확인

2026-09-13. Native Task 2 수정 중 부모가 수행한 읽기 전용 확인이다.
배포 profile 변경이나 새 성능 측정을 수행하지 않았다. 적용·검증은 승인된
[Native 계획 Task 6](../docs/superpowers/plans/2026-09-12-native-experience.md)의 범위다.

## 현재 확인한 것

초기 Native Task 2 실행 파일은 9,287,392B다. SHA와 동적 라이브러리 목록은
[정적 검사 기록](../evidence/native-task2-parent-linkage.json)에 있다.
Mac Security/CoreFoundation, libiconv, libSystem에 연결된다. 이 파일은 명세 리뷰에서
수정이 요구된 초기 빌드이며 제품 수용 또는 다른 OS 실행의 증거가 아니다.

현재 `product/Cargo.toml`에는 별도 release profile이 없다. Rust toolchain은 1.88.0에
고정돼 있다. 위 파일 크기만으로 현재 RSS·startup·요청 지연을 추정하지 않는다.
기존 core의 무대기 측정과 Native Task 1의 lifecycle 관측도 각기 다른 source/binary의
증거로 보존한다. Task 2 수정으로 그 결과가 자동 갱신되지 않는다.

## 기존 제품에서 확인한 배포 패턴

고정 Codex checkout `944d6fd1ba4baab69dbedd205282dc72ec20abb5`의
`codex-rs/Cargo.toml:595–604`는 release에 thin LTO와 codegen-units 4를 지정한다.
패키징에서 별도 디버그 심볼을 보관하고 strip하기 전까지 release binary의 심볼을
유지한다고 설명한다. 작은 배포 파일과 문제 분석용 심볼을 분리하는 기존 패턴의
근거이며, 우리 gateway에 같은 설정이 빠르거나 작다는 실험 결과가 아니다.

Task 6에서 profile이나 strip을 바꾼다면 최종 배포 파일의 SHA·toolchain·lock SHA를
기록하고 그 파일로 설치·on/off smoke를 수행해야 한다. 심볼을 제거하기 전 파일의
크기를 배포 크기로 표시하거나, 파일 크기 감소를 응답 지연 개선으로 표시하지 않는다.
크기를 이유로 TLS 보호·진단 근거·동작 중인 프로토콜을 제거하지 않는다.

## 필요한 실행 증거

- 설치 명령 peak RSS와 상주 worker idle RSS를 구분한다.
- 최종 native artifact로 setup/on/off와 인증된 readiness를 확인한다.
- runtime 없는 계정의 offline archive 실행은 별도 검증이다. 개발 PC에 Python과
  Node가 설치돼 있다는 사실이나 동적 링크 목록만으로 그 검증을 대체하지 않는다.
- 확보하지 못한 OS·아키텍처는 미검증으로 남긴다. Mac 결과를 Windows/Linux 지원
  완료로 옮겨 적지 않는다.
- 공개 archive URL·서명·release는 실제 존재하는 산출물과 승인된 게시 단계가 생긴
  뒤에만 안내한다. 현재는 fixture 서버와 offline archive 검사만 준비한다.

현재 제품 예산은 무대기 proxy 추가 p95 2ms 미만과 idle RSS 50MiB 미만이다.
[벤치마크 명세](benchmark-spec.md)의 잠정 예산이며 파일 크기와 별개다.
이번 문서는 profile을 선택하거나 해당 예산 달성을 새로 주장하지 않는다.

## Native Task 2 명세 수용본 정적 확인

후속 F8 수정 release `c890b6a4…`는 **9,316,432B**이며,
[해당 파일의 정적 검사](../evidence/native-task2-f8-parent-linkage.json)에서도
Security/CoreFoundation, libiconv, libSystem 네 시스템 라이브러리가 표시됐다.
초기 빌드의 기록은 별도로 보존한다. 이번에도 binary를 실행하지 않았으며,
개발 도구가 없는 계정에서의 설치·실행이나 RSS·지연 측정을 대체하지 않는다.
