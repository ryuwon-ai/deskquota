# Native Task 3 검토 입력

2026-09-13. 처음에는 부모의 읽기 전용 코드 확인으로 작성했다. 아래 잠금 항목은
이후 직접 반례를 확인했으며, 후속 절에 그 결과와 수정 상태를 구분한다.

## 같은 client resource의 잠금 식별

현재 `product/src/config_patch/storage.rs`의 `cooperating_lock_path`는 resource
경로 문자열을 hash하고 `std::env::temp_dir()` 아래에 잠금을 둔다. 다른 journal
directory를 사용하는 테스트는 통과했다. 그러나 서로 다른 TMPDIR/TEMP를 가진
프로세스와 동일 파일의 경로 별칭까지 같은 잠금을 쓰는지는 그 테스트로 확정되지 않는다.

검토자는 preview/apply 경로에서 별도로 경로를 정규화하거나 제약하는지 확인하고,
필요하면 동일한 임시 client fixture에 대한 두 독립 프로세스로 판정한다. 계약은
llmgw 편집끼리의 직렬화이며, 비협조적 외부 editor와의 마지막 hash 검사/rename
사이에 filesystem CAS를 보장하라는 요청은 아니다. 부모는 제품을 실행하지 않았다.

## 검증 범위

Task 3는 typed adapter가 사용할 ConfigPatch library다. connect/disconnect CLI,
실제 Pi/Claude/Codex 실행, 자동 시작과 설치는 후속 Task다. 현재 Mac 실행 결과와
Windows static/shared-helper failure model을 분리하며 실제 Windows 지원을
추정하지 않는다. 기존 공개 측정 파일과 reference clone은 수정하지 않는다.

## 초기 HOLD 뒤 직접 확인한 반례

[기록한 library driver](native-task3-parent-lock-probe/probe.rs)를 기존 Task 3 rlib에
링크해 격리된 임시 파일에 실행했다. `client/settings.json`과
`client/../client/settings.json`의 canonical path는 같지만, 생성된 잠금 경로는
달랐다. 첫 경로의 잠금을 잡고 있는 동안 두 번째 경로의 실제 `apply`가 성공해
파일을 변경했다. 같은 문자열 경로도 별도 child TMPDIR에 따라 잠금이 달라졌다.
[결과와 rlib 해시](native-task3-parent-lock-probe/result.json)에 보관한다.
소스 79개·고정된 실행 파일의 해시는 보존했고, probe의 임시 파일은 정리했다.

이는 llmgw끼리의 잠금 계약 위반으로 판정했다. 새 독립 SPEC 전 같은 구현자가
`product/artifacts/native-task3/lock-fix/`와 별도 build target에서 수정한다.
초기 HOLD의 전체 283개 테스트 통과만으로 이 경계를 충족했다고 표시하지 않는다.

## 기존 파일 옆 잠금 패턴

설치된 Pi 0.84.2 `dist/core/settings-manager.js:55`는
`proper-lockfile.lockSync(path, { realpath: false })`를 사용한다. 함께 설치된
`proper-lockfile/README.md:29–30`와 `lib/lockfile.js`는 대상 파일 옆의 `.lock`
경로를 사용한다. 라이브러리 기본 realpath 옵션은 기존 파일을 요구하며, Pi는
이를 끈다. 따라서 Pi가 모든 별칭을 canonicalize한다는 근거로 쓰지 않는다.
Task 3에서 참고할 것은 프로세스 temp directory와 무관한 파일 옆 잠금 경로다.
Node의 mkdir/mtime heartbeat를 Rust 구현에 새로 추가하는 결정은 아니다.
