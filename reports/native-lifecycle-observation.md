# Native Task 1 크기와 빈 상태 실행 관측

2026-09-12, macOS 26.5.1 arm64. [원시 결과](../evidence/native-task1-parent-footprint.json)와
[검증 스크립트](../scripts/probe-native-footprint.py)를 보존했다.
구현자가 멈춘 final HOLD 실행 파일을 부모가 별도로 3회 실행한 결과다.
명세·품질 수용이나 Windows/Linux 지원 판정은 아니다.

| 항목 | 직접 관측 |
|---|---|
| 실행 파일 | 8,679,872 bytes, 약 8.28 MiB |
| Task 8 측정 실행 파일 대비 크기 | +384,624 bytes, 약 375.6 KiB |
| 빈 상태 `on` 반환까지 | 40.302 / 40.777 / 42.124 ms |
| 빈 상태 `off` 반환까지 | 37.540 / 37.720 / 37.244 ms |
| 유휴 RSS, 회차별 10개 표본 중앙값 | 8.984 / 8.984 / 9.000 MiB |
| 요청·cleanup | data ingress 0, upstream 0; 세 worker PID 부재·임시 경로 제거 확인 |

**`on` 시간은 인증된 서버 readiness를 확인하고 CLI가 반환하는 시간이다.**
이번 config는 known RPM 60, unknown TPM, concurrency 1이었다. `on` 직후
status의 quota startup hold는 약 59,970 ms로 남아 있었다. 따라서 이 측정이
약 40 ms 뒤 inference까지 즉시 실행된다는 뜻은 아니다. quota admission 가능
시점과 프로세스가 켜졌다는 상태를 분리한다.

유휴 RSS는 요청과 장부 항목이 없는 프로세스에서 `ps`로 표본을 수집한 값이다.
최대 RSS·동시 요청 메모리·wizard 메모리·proxy의 추가 TTFT를 측정하지 않았다.
이전 Task 8의 무대기 실험과 설정·측정 방식이 달라 RSS 차이를 기능 추가의
인과 효과로 해석하지 않는다. 파일 크기만 동일한 byte 단위로 직접 비교했다.

표본 3개로 p95/p99나 저사양 PC의 성능을 주장하지 않는다. 기존 source/binary
identity와 원측정 자료는 [읽기 전용 검산](../evidence/native-task1-parent-hold-audit.json)에서
확인했다. 대상 release SHA256은
`09dff35ac5311ac499f4980d2572c013b72cd2f7bbcce4a9fb9a81921134a1ec`다.
후속 리뷰 수정으로 실행 파일이 바뀌면 이 수치는 해당 이전 HOLD의 기록으로
남으며 새 실행 파일의 결과로 자동 승계하지 않는다.
