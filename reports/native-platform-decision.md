# Native Task 1 Windows 보호 경계 결정

2026-09-12. **구현 결정이며 Windows 동작 검증 결과가 아니다.**

Windows에서도 Docker/WSL 없이 사용자 PC에서 실행한다는 요구를 유지한다.
기존 Cargo의 전역 `unsafe_code = forbid`는 프로젝트 내부 구현 정책이며,
승인된 core/native 명세와 AGENTS.md에는 해당 금지 조항이 없다. 파일 생성 시
보호와 기존 파일 검사에 필요한 Windows API를 여러 safe wrapper의 raw 포인터
변환으로 연결하는 것보다, 공식 `windows-sys`의 좁은 호출 경계를 직접 검토하는
편이 단순하다고 판단했다. [후보 조사](native-lifecycle-preflight.md)의 API 존재만으로
전체 보호 기능이 제공된다고 가정하지 않았다.

부모는 구현자의 구체적 API·크기 제안을 확인한 뒤 다음 범위를 허용했다.

- 이미 lockfile에 있는 `windows-sys 0.61.2`를 Windows target의 필요한 feature로
  사용한다. 새 supervisor, GUI library, 권한 우회용 shell script는 추가하지 않는다.
- 나머지 제품은 `unsafe_code = deny`를 유지하고 Windows 파일 보호 모듈 하나에만
  명시적 예외를 둔다. 각 FFI 호출은 포인터·길이·정렬·handle 수명·실패 시 정리의
  근거를 설명한다. 리뷰는 이 예외 자체와 모든 호출 경계를 포함한다.
- 현재 사용자 SID와 보호된 DACL을 사용해 state/token을 생성한다. 기존 파일은
  열린 handle의 owner/DACL과 reparse·종류를 검사한다. 불충분한 기존 권한은
  자동 수정하지 않는다. 보호 검사에 실패하면 토큰 내용을 기록하지 않는다.
- 기존 Unix의 safe library 기반 경로와 HTTP/SSE hot path에는 unsafe 예외를
  확장하지 않는다. 최소 Windows Rust target의 타입 검사와 실제 Windows의
  ACL·일반 사용자·시작/종료 실행은 서로 다른 검증 단계로 기록한다.

공식 [CreateDirectoryW 문서](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createdirectoryw)는
생성 시 보안 descriptor 적용과 파일시스템의 ACL 지원 조건을 명시한다.
따라서 생성 성공만으로 private state라고 판단하지 않고, 보호를 확인한 이후에만
secret 파일의 내용을 쓴다.
[GetTokenInformation](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation)의
반환 길이와 필요한 query 권한을 확인하며,
[GetSecurityInfo](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo)가
반환하는 owner/DACL은 소유 descriptor buffer와 함께 유지하고 LocalFree로 정리한다.
이 문서 확인은 우리 FFI 코드의 안전성이나 OS 실행 성공을 대신하지 않는다.

이 결정은 사용자에게 새로운 권한 작업을 요청한 것이 아니다. 현재 사용자 파일,
OS 자동 시작, 관리자 권한, Git/release는 변경하지 않는다. 실제 제품 변경량과
dependency 증가, 테스트 결과는 Native Task 1의 HOLD 및 독립 리뷰에서 확정한다.

## macOS 확장 ACL과 Linux POSIX ACL

부모는 임시 synthetic 파일에 `0600`을 적용한 뒤 `everyone allow read` 확장 ACL을
추가할 수 있음을 직접 확인했다. 모드·소유 UID와 ACL 목록만 확인했으며 다른
사용자의 실제 읽기는 실행하지 않았다. 제품을 실행하지 않은 이 관측은
[별도 증거](../evidence/native-task1-parent-acl-observation.json)에 기록했고 임시
파일은 정리했다. 구현자는 이어서 실제 lifecycle fixture로 상속 ACL이 있는
state directory의 잘못된 허용을 재현하고 수정 중이다.

이 경계에는 macOS 전용 `exacl 0.13.0`의 safe API를 사용해 비어 있지 않은 확장
ACL을 보수적으로 거절하는 구현을 선택했다. 기존 ACL을 수정하지 않으며 token
내용을 쓰기 전에 검사한다. path 기반 ACL 조회와 열린 file의 device/inode 대조가
어떤 정체성을 확인하는지 독립 리뷰에 포함한다. 임의의 동시 파일 교체·ACL 변경에
대한 완전한 atomic 보장을 주장하지 않는다.

Linux에는 libacl 런타임 의존성을 추가하지 않는다.
[Linux acl(5)](https://man7.org/linux/man-pages/man5/acl.5.html)는 named user/group
허용의 상한인 ACL_MASK가 group mode bits에 대응하고, 기본 ACL 상속 시에도 생성
mode가 접근 권한을 제한한다고 설명한다. 소유자 검사와 group/other bits 0이라는
현재 조건의 판단 근거다. 이것은 Linux POSIX ACL 계약에 대한 문서 근거이며,
Linux에서 직접 실행한 결과나 모든 네트워크·비 POSIX 파일시스템에 대한 보증이 아니다.
