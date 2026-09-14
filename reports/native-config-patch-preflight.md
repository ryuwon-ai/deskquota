# Native ConfigPatch 구현 전 확인

2026-09-13. Native Task 2 마법사 구현 중 부모가 수행한 **읽기 전용 코드·라이브러리 확인**이다.
Task 3 구현이나 파일 복원·OS 권한 검증 결과가 아니다. 규범은
[설정 명세 5절](../docs/superpowers/specs/2026-09-12-native-setup-design.md)과
[Native 계획 Task 3](../docs/superpowers/plans/2026-09-12-native-experience.md)다.
제품 수정과 테스트 실행은 해당 Task의 구현 담당자가 맡는다.

## 기존 기능에서 재사용할 것

| 확인한 소스 | 직접 확인한 API/계약 | 구현 시 구분할 점 |
|---|---|---|
| `toml_edit 0.25.15+spec-1.1.0` README, `src/value.rs` | `DocumentMut`, `Value::decor`, `decor_mut`; 주석·공백·상대 순서 보존을 지원 | serde 값만 재직렬화하는 것과 문서 편집은 다르다. 수정한 scalar의 주석도 실제 fixture로 확인한다 |
| 같은 README의 limitations | dotted key 순서와 마지막 newline 부재는 보존하지 않음 | byte 전체가 항상 동일하다고 약속하지 않는다. 변경 없는 저장은 원본 bytes를 유지할 수 있다 |
| `fs4 1.1.0 src/lib.rs`의 `FileExt` | `lock`, `try_lock`, 파일 handle 종료 시 lock 해제; Unix flock/Windows LockFileEx | llmgw끼리의 편집을 직렬화할 수 있지만 외부 editor와 atomic compare-and-swap을 보장하지 않는다 |
| 기존 `lifecycle::platform::{directory,open,read}` | state의 사용자 전용 생성·열기와 소유권 검증 | 일반 client 설정의 권한 보존·원자 교체·복원 API 전체가 이미 있는 것은 아니다 |

확인한 crate는 현재 Cargo.lock에 고정된 기존 dependency다. 부모가 새 package를
추가하거나 다른 버전으로 교체하지 않았다. `toml_edit` registry 디렉터리 이름에는
`+spec-1.1.0` suffix가 있으므로 버전 숫자만으로 로컬 경로를 추측하지 않는다.

## state 보호와 client 파일 보존의 경계

수용된 Native Task 1의 Unix 구현은 새 state directory를 0700, file을 0600으로
만들고 현재 UID·종류·group/other mode bit를 검사한다. `open`은 no-follow 등을
사용하지만 자체적으로 truncate하거나 atomic replace하지 않는다. `directory`는
단일 directory 생성 API다. macOS에서는 확장 ACL 전체를 거절하고 path의 inode와
열린 file의 inode를 대조한다. Windows 구현은 생성 시 사용자 전용 DACL을 적용하고
열린 handle의 소유권·접근 범위를 검증한다.

따라서 기존 state API를 호출했다는 사실만으로 다음이 완료되지는 않는다.

- 기존 client file의 mode/ACL을 보존한 same-directory atomic replacement.
- 읽은 bytes와 적용 직전 bytes의 hash 비교 및 변경 후 parse 확인.
- 다른 editor와의 충돌 표시, 여러 파일 변경 중 부분 완료 기록.
- 반복 connect 뒤 최초 소유 이전 값으로 돌아가는 key 단위 disconnect.

권한을 조용히 강화하거나 ACL을 제거해서 통과시키지 않는다. local data token을
기록할 기존 client file이 다른 사용자에게 공개돼 있으면 명세대로 거절하고 이유를
설명한다. 지원하는 권한 형태의 보존과 지원하지 못하는 형태의 명시적 거절을
구분해야 한다. 현재 state 전용 검사가 모든 client ACL 형태를 지원한다고 단정하지 않는다.

## 작게 유지할 데이터와 검증

ConfigPatch는 승인된 특정 JSON/TOML 파일의 key 편집을 만든다. 범용 변환 엔진이나
이전 버전 호환 경로를 추가하지 않는다. 보호된 before-image는 복구 재료이며,
일반 journal·diff·doctor 출력에 credential 값을 복제하는 저장소가 아니다.
upstream credential, local data token, control token을 서로 다른 synthetic 값으로
검증해야 소유권 경계를 구분할 수 있다.

필수 behavior는 이미 계획에 있다: 원본 보존, 적용 직전 변경 거절, 부분 적용 후
표시, 반복 connect의 소유 정보 유지, 사용자 후속 편집 보존, 신규 파일 내용이
달라졌을 때 삭제하지 않기. 값 비교와 파일 전체 hash의 역할을 분리한다.
unrelated key가 바뀌었다고 복구 가능한 소유 key까지 일괄 포기하거나, 전체 backup을
덮어써 사용자 편집을 잃는 구현은 계약을 충족하지 않는다.

이 문서에는 새 테스트 결과가 없다. Native Task 2에서 실제로 제공한 저장 API가
수용된 뒤 필요한 경계만 다시 확인하고 Task 3의 RED/GREEN과 별도 리뷰로 검증한다.

## 2026-09-13 추가 확인: Windows 교체 API

현재 설치된 `windows-sys 0.61.2`의 `Win32/Storage/FileSystem/mod.rs:367`에
`ReplaceFileW`가 이미 노출돼 있다. 새 package 추가 전에 검토할 기존 API다.
Microsoft의 [ReplaceFileW 공식 계약](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)을
읽었다. 원본의 DACL 등 속성을 보존하며 교체하는 API지만,
`REPLACEFILE_IGNORE_MERGE_ERRORS`와 `REPLACEFILE_IGNORE_ACL_ERRORS`는 ACL 보존
실패를 무시할 수 있다. 해당 flag로 파일 쓰기를 성공 처리하면 승인된 보존 계약을
충족하지 못한다. `REPLACEFILE_WRITE_THROUGH`도 문서상 지원되지 않는다.

실패 반환을 곧바로 “원본 무변경”으로 표시해서도 안 된다. 공식 문서는 일부 오류에서
원본·교체 파일의 이름이나 속성이 이미 바뀔 수 있다고 설명한다. Task 3의 보호된
before-image·journal·적용 후 재읽기는 native API 선택 후에도 필요하다.
원본·교체·backup은 같은 volume이어야 하며, backup 경로도 사용자 전용 보호를
만족해야 한다. 경로 API 자체가 외부 editor와 hash 기반 CAS를 제공하지는 않는다.

이는 **공식 API와 기존 binding의 확인**이다. Windows 교체·권한·crash 복구를
실행하지 않았고, 이 API를 제품에 추가하지 않았다. Windows 구현 선택은 작은
FFI 경계와 실제 대상 OS fixture로 검증해야 한다.

추가로 같은 공식 문서의 오류 1176은 backup 인수가 없을 때 원래 대상이 사라질 수
있다고 명시한다. 그런 실패 뒤 replacement 임시 파일까지 일괄 삭제하면 복구 재료도
잃는다. Native Task 2 수정본은 이 경우를 단순한 문서상 제한으로만 처리하지 않고,
보호된 원본 복구 파일을 교체 전에 만들고 불명확한 실패 시 유지하도록 보완한다.
이는 구현 지시이며 완료 증거가 아니다. host에서 실패 모형을 검증해도 실제 Windows
API·파일시스템 실행을 증명하지 않는다.

## 2026-09-13 저장 실패 재현 뒤 확인한 기존 구현

고정된 Codex checkout의 `codex-rs/core/src/config/edit.rs:739–777`은 현재 파일을
`DocumentMut`로 읽어 편집하고, 실제 변경이 없으면 쓰지 않는다.
`codex-rs/utils/path-utils/src/lib.rs:144–155`의 `write_atomically`는 같은 directory의
`NamedTempFile`에 전체 내용을 쓴 다음 `persist`한다. 이는 원본을 먼저 truncate하지
않는 작은 저장 패턴의 근거다. 이 짧은 helper 자체에서 우리 명세의 snapshot 충돌 검사,
ACL 보존, 여러 파일 journal까지 제공한다고 해석하지 않는다.

고정된 Pi checkout의 `packages/coding-agent/src/core/settings-manager.ts:263–289`는
기존 파일에 대해 lock을 잡은 뒤 현재 내용을 읽고 변경을 만든다. 다만 쓰기는
`writeFileSync`이므로 해당 부분을 원자 교체나 crash 안전의 근거로 쓰지 않는다.
검증된 제품에서도 필요한 계약을 각각 확인해야 한다.

현재 제품 Cargo.lock에는 `fs4`, `rustix`, `windows-sys`가 있으며 `tempfile`은 없다.
위 패턴을 확인한 것이 dependency 추가 결정은 아니다. Native Task 2의
[독립 명세 리뷰](../evidence/native-task2-spec-review/README.md)가 재현한 기존 config
손상과 stale preview 덮어쓰기는 해당 Task에서 먼저 고친다. 일반 client key 복원과
journal은 Task 3 범위로 유지한다. 이 추가 확인은 읽기 전용이며, 수정본의 안전성은
후속 실행과 재검토 결과로 판단한다.

## 2026-09-13 Pi 파일별 JSON 문법 확인

Pi 설정 전체를 같은 JSON 문법으로 가정하지 않는다. 고정 checkout의
`packages/coding-agent/src/core/settings-manager.ts:419`와 설치된 0.84.2
`dist/core/settings-manager.js:181`은 settings를 일반 `JSON.parse`로 읽는다.
반면 모델 파일은 고정 `core/model-config.ts:267`, 설치된
`dist/core/model-config.js:225`에서 `stripJsonComments` 이후 `JSON.parse`로 읽는다.
따라서 `models.json`의 유효한 주석을 삭제하거나, 지원되지 않는 다른 JSON 확장까지
받아들이는 편집기는 적합하지 않다. 설치된 CLI를 실행한 것이 아닌 코드 확인이다.

현재 제품 lockfile과 로컬 Cargo registry에는 JSONC 편집 crate가 없다. 기존
serde_json은 일반 JSON 값 검사에 사용할 수 있지만, 주석을 유지하는 편집 API로
간주하지 않는다. [jsonc-parser 공식 문서](https://docs.rs/jsonc-parser/0.33.1/jsonc_parser/)의
0.33.1은 `cst` 기능으로 주석을 포함한 문서의 필드 편집을 제공한다. 기본 parser는
여러 확장을 허용하므로 `ParseOptions`를 실제 client 문법에 맞게 제한해야 한다.
일반 JSON은 모든 확장을 끄고, Pi models는 주석만 별도로 허용하는 식이다.

이는 Task 3 후보 API 조사이며 채택·설치·제품 변경을 수행하지 않았다. 실제 도입 전
Rust 1.88 호환, 필요한 feature와 전이 dependency, 중복 key·주석·문자열 escape·필드
삭제/복원의 동작을 확인한다. 임의 JSON 편집 framework를 만들지 않고 필요한
클라이언트 파일의 보존 계약에 맞는 최소 API만 사용한다.

추가로 [0.33.1 tag의 Cargo.toml](https://raw.githubusercontent.com/dprint/jsonc-parser/0.33.1/Cargo.toml)을
확인했다. `cst = []`이고 나열된 일반 dependency는 모두 optional이다. 따라서
주석 편집을 위해 별도 background service가 필요한 구조는 아니다. 다만 edition은
2024이며 `rust-version` 선언이 없어 manifest만으로 Rust 1.88 호환을 확정할 수 없다.
이 확인에서도 crate를 설치하거나 build하지 않았다. 선택한다면 Task 3의 격리된
빌드에서 실제 toolchain 호환과 필요한 feature 조합을 검증해야 한다.

Task 3 진행 중 registry에 내려받은 0.33.1 소스를 읽어 추가 경계를 확인했다.
`src/scanner.rs:335–353`은 ParseOptions와 별도로 VT/FF 및 Unicode whitespace를
건너뛴다. `src/string.rs:160–182`도 escape되지 않은 control 문자를 별도 거절하는
분기가 없어 보인다. 따라서 모든 확장 옵션을 껐다는 사실만으로 `JSON.parse`와
같은 문법을 보장하지 않는다. 이는 부모의 소스 확인이며 실행 반례는 아직 아니다.
필요하면 CST 편집과 기존 `serde_json`의 엄격한 검증을 함께 사용한다. JSONC의
주석 제거 범위도 라이브러리가 분석한 정보를 사용하고 별도 lexer를 만들지 않는다.

## Native Task 2 수용 뒤의 재사용 경계

최종 수용본 `3495daf5…` manifest의 `setup/persist.rs`에는 private
`read_regular`, `verify_snapshot`, `write_protected_file`과 Windows 복구 helper가
있다. [명세 재검토](../evidence/native-task2-spec-review/rereview-f8/README.md)와
[최종 품질 재검토](../evidence/native-task2-quality-review/rereview/README.md)를
통과했다. source archive와 manifest는 `product/artifacts/native-task2/quality-fix/`에
보존하며, Task 3는 이 최종 소스를 기준으로 한다. 이는 Task 2의 수용이며 client
transaction이나 실제 Windows 실행의 수용은 아니다.

Task 3에서 이 저장 경로를 재사용한다면 설정 마법사와 client transaction이 공통으로
필요한 파일 보존 부분만 좁게 분리한다. setup draft·pending intent·worker 시작 판단을
일반 client 파일 쓰기로 끌어오지 않는다. 특히 setup 저장은 기존 mode/ACL을 보존할
수 있지만, local data token을 넣는 client 파일에는 **쓰기 전 사용자 전용 접근 검사**가
추가로 필요하다. 단순히 기존 함수의 visibility를 넓히는 것으로 이 계약이 충족되지
않는다. before-image·소유 key·journal·부분 복구는 Task 3에서 검증할 별도 책임이다.
