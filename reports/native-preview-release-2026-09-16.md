# 네이티브 사전 릴리스 검증

대상은 기본 경량 빌드의 macOS ARM64·Windows x64 패키지다. 런타임 기능을
추가하지 않고 기존 설치 도구와 검증 코드를 재사용했다. NVIDIA 실측은
[별도 평가 기록](nvidia-api-evaluation.md)에서 관리한다.

[v0.1.0-preview.1](https://github.com/ryuwon-ai/deskquota/releases/tag/v0.1.0-preview.1)을
공개했다. 빌드 소스는 `c838cdb24a8eb813a3a7c21ce1daf0e5cb1bf990`이며,
[최종 CI](https://github.com/ryuwon-ai/deskquota/actions/runs/35059510506)의
두 플랫폼 작업이 모두 통과했다.

## 구현과 확인 기준

- Rust 1.88.0, `Cargo.lock`, 기본 기능 빌드. BPE 사전은 포함하지 않는다.
- GitHub Actions의 macOS 14 ARM64·Windows 2022 x64 네이티브 러너를 사용한다.
  Windows는 MSVC와 정적 CRT로 빌드하며, 사용자 PC의 컴파일러 설치와 무관하다.
- 아카이브는 실행 파일과 안내 문서 4개만 포함한다. 설치 스크립트와 각
  파일의 SHA-256 목록을 별도 배포한다. 설치 시 PATH는 미리보기만 출력한다.
- 정확한 CI 아카이브를 설치하고 원본 실행 파일의 해시와 대조한 뒤 런타임을
  실행한다. Windows는 DLL 목록과 개발 도구를 PATH에서 뺀 실행도 검사한다.
- CI 토큰은 읽기 전용이며 공개 작업을 자동 수행하지 않는다. 통과한 실행의
  산출물을 내려받아 검사한 다음 같은 커밋에 고정한 사전 릴리스로 공개한다.

## 현재 직접 검증

- CI의 macOS 기본 Rust 검사 443개, Windows 409개가 통과했다. Windows의
  profiling 1개는 ignored이며 성공 수에 포함하지 않는다.
- Windows 설치 probe에서 PowerShell7 → Python → PowerShell5의 모듈 경로
  상속으로 `Get-FileHash`를 찾지 못하는 문제가 실제 stderr에서 확인됐다.
  검사 도구의 PS5 자식 환경에서만 `PSModulePath`를 제외했다. 이는
  [Microsoft가 문서화한 처리](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_psmodulepath)이며
  시스템 환경변수·PowerShell 실행 정책·설치 프로그램은 변경하지 않았다.
- GitHub Windows 러너의 System32에는 Docker도 있어, System32만 남기는
  검사로는 개발 도구 부재를 확인할 수 없었다. 게이트웨이 자식 프로세스의
  PATH를 빈 문자열로 바꾸고, 검사에 필요한 Windows 도구만 절대 경로로
  호출하도록 수정했다. 호스트의 PATH와 설치된 도구는 그대로 유지한다.
- 패키징 4개·설치 및 검사 도구 14개 Python 테스트를 로컬에서 통과했다.
- [Windows 10 실기기의 최종 파일·빈 PATH 검사](../evidence/native-preview-2026-09-16/windows-lab-final.json):
  CI MSVC 실행 파일 9,557,504bytes를 정확한 ZIP으로 설치했다. 설치·재설치·
  잘못된 체크섬 거부·PATH 미변경·on/status/off가 통과했다. 개발 도구를
  PATH에서 제외한 상태에서도 합성 JSON/SSE 82요청이 상위 2호출·캐시 80회 적중,
  실패 0으로 완료됐고 actual RPM 2/TPM 46이 기록됐다.
- [최종 Windows DLL 목록](../evidence/native-preview-2026-09-16/windows-ci-dependencies.txt)은
  Windows 시스템 DLL뿐이며 별도 VC/GNU 런타임 DLL이 없다. 이 PC에 모든
  개발 패키지가 제거됐다는 뜻은 아니다.
- [Mac 후보의 재시도 지시 11개 검사](../evidence/native-preview-2026-09-16/macos-candidate-retry.json)도 통과했다.

## 공개 파일 대조

| 대상 | 압축 파일 크기 | 실행 파일 크기 |
|---|---:|---:|
| macOS ARM64 | 4,296,163 bytes | 10,214,128 bytes |
| Windows x64 MSVC | 3,878,922 bytes | 9,557,504 bytes |

[CI 검사와 파일 식별값](../evidence/native-preview-2026-09-16/ci-acceptance.json)을
보존했다. macOS 최종 실행 파일은 이 Mac에서 검사한 후보와 바이트 단위로
동일했다. Windows 최종 파일은 이전 후보와 해시가 달라, 그 최종 ZIP을
Windows 10 실기기에 별도 설치하고 전체 설치·실행 검사를 다시 통과했다.

[공개 후 재다운로드 기록](../evidence/native-preview-2026-09-16/public-readback.json):
패키지 2개·설치 도구 2개·각 체크섬 4개가 모두 CI 산출물과 바이트 단위로
일치했다. 체크섬, 태그의 소스 커밋, 공개 prerelease 상태를 확인했다.
인증정보 없는 HTTP 요청으로 릴리스 메타데이터와 체크섬 파일도 받았다.
개발 폴더의 빌드 결과나 실패한 CI 산출물을 대신 배포하지 않았다.

## 한계

Apple Developer ID·공증 및 Windows Authenticode 서명은 없다. 해시는 파일의
무결성 검사이며 게시자 인증을 대신하지 않는다. 새 PC의 Gatekeeper·SmartScreen·
회사 보안 정책 통과는 별도 확인이 필요하다. 보안 설정 우회는 제공하지 않는다.
Intel macOS·Linux·BPE 바이너리는 이번 배포 대상에 포함하지 않는다.
설치 및 합성 HTTP 확인을 실 API 성능이나 사용자 채택의 증거로 해석하지 않는다.

## 근거

- [배포 워크플로](../.github/workflows/native.yml)
- [설치 안내](../product/docs/installation.md)
- [GitHub 공식 네이티브 러너 목록](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [설계](../docs/superpowers/specs/2026-09-16-native-preview-release.md)
- [구현 계획](../docs/superpowers/plans/2026-09-16-native-preview-release.md)
