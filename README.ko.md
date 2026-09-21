<p align="center">
  <img src="assets/deskquota-hero.png" alt="DeskQuota — 내 자리에서 관리하는 LLM 사용 한도" width="100%">
</p>

<p align="center"><strong>같은 LLM 한도로, 더 많은 일을.</strong></p>
<p align="center">쓰던 도구는 그대로. 한도 조율은 DeskQuota에 맡기세요.</p>
<p align="center">
  <a href="https://github.com/ryuwon-ai/deskquota/actions/workflows/native.yml"><img src="https://github.com/ryuwon-ai/deskquota/actions/workflows/native.yml/badge.svg?branch=develop" alt="네이티브 패키지 CI"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue" alt="MIT 또는 Apache-2.0 라이선스"></a>
</p>
<p align="center"><a href="README.md">English</a> · 한국어</p>
<p align="center"><a href="#왜-deskquota인가요">왜 DeskQuota인가요?</a> · <a href="#숫자로-보는-성능">성능</a> · <a href="#시작하기">시작하기</a> · <a href="#쓰던-도구-연결하기">도구 연결</a> · <a href="#한도-설정하기">설정</a></p>

---

**DeskQuota는 제한된 LLM 자원을 더 알뜰하게 쓰기 위한 가벼운 게이트웨이입니다.**
Claude Code, Codex, Pi와 직접 만든 스크립트의 요청 스케줄링, exact cache,
토큰 정산, 장애 복구를 한곳에서 관리합니다.

내 PC에서 실행하고 도구의 주소를 localhost로 연결하면 됩니다.
외부 API, 사내 엔드포인트, 로컬 LLM 어디에든 적용할 수 있습니다.
인스턴스 하나가 엔드포인트 하나와 자신을 지나는 요청을 조율합니다.

**Rust 실행 파일 하나면 충분합니다. Docker, WSL, Python, Node, Redis나 데이터베이스를 따로 운영할 필요가 없습니다.**
`llmgw setup`으로 설정하고, `llmgw on`으로 켜고, `llmgw off`로 끄세요.

## 왜 DeskQuota인가요?

| 장점 | 달라지는 점 |
|---|---|
| **한도를 필요한 작업에 쓰세요** | 여러 도구의 요청을 하나의 RPM·TPM·동시성 예산으로 조율합니다. |
| **같은 답을 다시 생성하지 마세요** | Exact TTL 캐시를 켜면 조건에 맞는 동일 요청을 서버 재호출 없이 처리합니다. 동시에 들어온 중복 요청도 하나의 결과를 공유할 수 있습니다. |
| **여러 에이전트에 차례를 나누세요** | 설정한 클라이언트별 대기열에 공정하게 차례를 배분하고, 기다리는 요청이 계속 뒤로 밀리지 않도록 보호합니다. |
| **재시도도 함께 조율하세요** | `429` 대기 시간, 제공자의 재시도·한도 갱신 안내, 서킷브레이커를 함께 적용해 여러 호출자의 복구 시점을 조율합니다. |
| **쓰지 않은 토큰 예약량을 돌려받으세요** | 스트리밍을 포함한 지원 응답의 최종 usage로 실제 사용량을 정산하고, 남은 예약량을 해제합니다. |
| **익숙한 작업 방식을 유지하세요** | 표준 API 인증, 클라이언트 연결 프로필, 네이티브 명령어를 제공합니다. 별도 데이터 토큰 헤더를 붙일 필요가 없습니다. |

DeskQuota의 강점은 **여러 도구의 한도를 함께 관리하면서도 로컬 유틸리티처럼 가볍게 운영할 수 있다는 것**입니다.
스케줄링·캐시·복구를 한 프로세스에서 처리하고, 대기열과 메모리 캐시의 크기를 제한합니다.

## 숫자로 보는 성능

| 유휴 워커 메모리 | 로컬 HTTP 왕복 p95 | 동일 요청 10개의 서버 호출 수 |
| :---: | :---: | :---: |
| **9.56 MiB** | **0.45 ms** | **10 → 1회** |

**서버 호출은 90% 절감하고, 동일 요청 묶음은 9.0배 빠르게 완료했습니다.**
Exact cache를 켜면 동일 요청 10개의 전체 완료 시간이 **1,035 ms에서 114 ms로**
줄었습니다. 캐시를 끈 설정과 켠 설정 모두 5회 반복에서 **50/50건을 완료**했습니다.

네이티브 `on` / `off`는 각각 **39 ms / 35 ms**였습니다.
시작·종료를 3회 반복한 중앙값입니다.

<details>
<summary>벤치마크 환경과 측정 기준</summary>

- **빌드·환경:** 2026년 9월 21일, Apple M4, 메모리 32 GiB, macOS 26.5.1.
  Rust 1.88의 기본 기능 release 빌드이며, 소스
  [`03f4009`](https://github.com/ryuwon-ai/deskquota/commit/03f4009085c6ef8fceea84c07fb69f26118c3de3)의
  [네이티브 CI 패키지](https://github.com/ryuwon-ai/deskquota/actions/runs/35546518243)를 사용했습니다.
- **HTTP 지연:** 직접 연결과 게이트웨이 연결을 번갈아 5쌍 측정했습니다.
  회당 워밍업 5건 뒤 SSE 요청 100건을 순차 전송하고 연결을 재사용했습니다.
  캐시는 끄고 한도 대기는 없앴으며, 두 경로 모두 **500/500건을 완료**했습니다.
  회차별 p95 5개의 중앙값은 **직접 연결 0.22 ms, DeskQuota 경유 0.45 ms**입니다.
  클라이언트와 로컬 HTTP 모의 서버를 포함한 왕복 시간이며, 서버에 생성 지연을
  추가하지 않았습니다.
- **Exact cache:** 같은 실행 파일에서 캐시를 끈 설정과 켠 설정을 번갈아 5쌍
  측정했습니다. 회당 동일 JSON 요청 10건, 빈 캐시, 동시성 1, 충분한 한도를
  사용하고 시작 대기는 껐습니다. 모의 서버는 요청 묶음이 대기열에 들어올 때까지
  응답을 보류한 뒤, 실제 서버 호출마다 **100 ms의 고정 처리 시간**을 적용합니다.
  완료 시간은 첫 요청 제출부터 마지막 응답 검증까지이며, 5회 중앙값입니다.
  전체 서버 호출은 **캐시를 끄면 50회, 켜면 5회**였습니다.
- **메모리·시작·종료:** 유휴 RSS는 워커 3개에서 수집한 30개 표본의 중앙값입니다.
  시작·종료 시간은 CLI 수명 주기를 측정한 값입니다. 설정된 한도 보호 대기 시간은
  프로세스 시작 시간과 별개로 적용됩니다.

</details>

## 이런 작업에 잘 맞습니다

- **코딩·리뷰·자동화를 동시에 돌릴 때.** 같은 사용 한도를 나누는 클라이언트별
  대기열에 차례를 배분합니다.
- **분류기·스크립트·짧은 요청을 반복 실행할 때.** 조건에 맞는 동일 답변을
  캐시에서 가져와 서버 왕복과 한도 소모를 줄입니다.
- **한도가 있는 API나 사내 엔드포인트를 쓸 때.** 도구들이 함께 사용할
  RPM·TPM·동시성을 정하고, 제공자가 요구한 대기를 한곳에서 관리합니다.
- **여러 작업을 하는 PC에서 로컬 모델을 돌릴 때.** 동시 요청 수를 제한하고,
  초과 요청은 모델 서버에 보내기 전에 대기시킵니다.

```text
Claude Code ─┐
Codex ───────┼──► 내 PC의 DeskQuota ──► LLM 엔드포인트
Pi / 스크립트 ┘     한도 · 대기열 · 캐시 · 복구
```

## 시작하기

### 실행 파일 설치

[프리뷰 릴리스](https://github.com/ryuwon-ai/deskquota/releases/tag/v0.1.0-preview.1)에서
압축 파일, 설치 스크립트, 각각의 `.sha256` 파일을 받으세요.
네 파일을 오프라인 PC로 옮겨 설치할 수도 있습니다.

| 운영체제 | 압축 파일 | 설치 스크립트 |
|---|---|---|
| macOS · Apple silicon | `llmgw-macos-arm64.tar.gz` | `install.sh` |
| Windows · x64 | `llmgw-windows-x64.zip` | `install.ps1` |

배포 패키지 버전은 `v0.1.0-preview.1`입니다. 이 문서에서 소개하는 최신
캐시·복구 기능은 [소스에서 빌드](#소스에서-빌드)해 사용할 수 있습니다.

<details>
<summary>macOS 설치</summary>

다운로드한 네 파일이 있는 디렉터리에서 실행하세요.

```sh
shasum -a 256 -c install.sh.sha256 &&
sh install.sh --archive ./llmgw-macos-arm64.tar.gz \
  --checksum-manifest ./llmgw-macos-arm64.tar.gz.sha256 \
  --install-dir "$HOME/.local/bin" --path-action preview
```

</details>

<details>
<summary>Windows 설치</summary>

다운로드한 네 파일이 있는 디렉터리에서 PowerShell을 열고 실행하세요.

```powershell
$expected = ((Get-Content ./install.ps1.sha256 -Raw).Trim() -split '\s+')[0]
if ((Get-FileHash ./install.ps1 -Algorithm SHA256).Hash -ne $expected) { throw 'Installer checksum mismatch' }
& ./install.ps1 -Archive ./llmgw-windows-x64.zip `
  -ChecksumManifest ./llmgw-windows-x64.zip.sha256 `
  -InstallDir "$env:LOCALAPPDATA\Programs\llmgw\bin" -PathAction Preview
```

</details>

설치 스크립트는 압축 파일을 검증하고 PATH 설정 방법을 출력합니다.
셸 프로필이나 레지스트리의 PATH를 직접 바꾸지는 않습니다. 안내된 디렉터리를
사용자 PATH에 추가한 뒤 새 터미널을 열거나, 실행 파일의 전체 경로로 실행하세요.
패키지는 정식 서명이 없는 프리뷰이므로 OS와 회사의 설치 정책을 따라야 합니다.
Linux·Intel Mac 패키지는 아직 제공하지 않습니다.

### 소스에서 빌드

Rust 1.88과 해당 운영체제의 컴파일러·링커가 필요합니다. macOS에서는:

```sh
git clone https://github.com/ryuwon-ai/deskquota.git
cd deskquota/product
CARGO_TARGET_DIR="$HOME/.cache/deskquota/cargo" cargo install --locked --path .
```

Cargo의 실행 파일 디렉터리가 PATH에 있어야 합니다. Windows 소스 빌드에는
MSVC 또는 GNU 툴체인이 필요하지만, 미리 빌드된 패키지를 실행할 때는 필요 없습니다.
토크나이저 기반 입력 추정이 필요할 때만 `--features bpe`를 추가하세요.
이 기능은 메모리를 더 사용합니다.

### 한 번 설정하고, 필요할 때 켜세요

```sh
llmgw setup
llmgw on
llmgw status
llmgw off
```

설정 마법사에서 엔드포인트, API 형식, 모델, 인증 방식, RPM·TPM·동시성,
exact cache 사용 여부를 정합니다. 저장 전 변경 내용을 미리 보여 줍니다.
설정 파일이 없는 상태에서 `llmgw`만 실행해도 마법사가 열립니다.

| 명령어 | 용도 |
|---|---|
| `llmgw setup` | 설정을 만들거나 수정합니다. |
| `llmgw on` / `llmgw off` | 백그라운드 게이트웨이를 켜고 끕니다. |
| `llmgw status` | 대기열, 사용량 정산, 캐시와 오류를 확인합니다. |
| `llmgw doctor` | 외부 서버 호출 없이 로컬 설정을 검사합니다. |
| `llmgw restart` | 저장한 설정을 실행 중인 게이트웨이에 적용합니다. |
| `llmgw autostart on` / `llmgw autostart off` | 로그인 시 자동 실행 설정을 미리 봅니다. 출력된 해시로 변경을 적용합니다. |

## 쓰던 도구 연결하기

상위 서버는 클라이언트가 사용하는 API 형식을 지원해야 합니다.
DeskQuota는 Chat Completions·Messages·HTTP Responses를 전달하며,
서로 다른 API 형식을 변환하지 않습니다.

| 자동 설정 프로필 | API | 허용하는 클라이언트 버전 |
|---|---|---|
| Pi | Chat Completions | 0.84.2 |
| Claude Code | Messages | 2.1.76 |
| Codex | Responses over HTTP | 0.154.0 |

자동 설정은 클라이언트 버전을 검사합니다. 다른 도구나 버전은 base URL을 직접
설정하고, 필요한 프로토콜과 모델 기능이 동작하는지 확인하세요.
옵션은 `llmgw connect --help`에서 볼 수 있습니다. 설정한 root와 모델을 연결하려면:

```sh
llmgw connect pi --root pi-work --model YOUR_MODEL_ID
# 미리보기를 확인한 뒤 같은 명령에 --apply-hash 출력된_해시를 추가하세요.
llmgw disconnect pi
```

**연결하면 클라이언트의 base URL과 설정 파일이 바뀝니다.** 미리보기에는 Pi의
`models.json`, Claude의 `settings.json`, Codex의 `config.toml`처럼 실제로 수정할
파일과 항목이 표시됩니다. 기존 연결과 모델 목록·기능이 다르게 보일 수 있습니다.
게이트웨이를 꺼도 연결 설정은 남습니다. `disconnect`는 이후 별도로 변경하지 않은
관리 대상 값을 복원합니다.

Forward 모드는 일반적인 `Authorization` 또는 `x-api-key` 인증을 전달하며,
별도의 데이터 토큰은 요구하지 않습니다. 게이트웨이는 loopback에만 바인딩하고,
시작·종료 등의 제어에는 별도 인증을 씁니다. 채팅 구독이나 브라우저 로그인만으로
API 인증이 연결되지는 않습니다.

<details>
<summary>프로토콜과 컨텍스트 압축 지원 범위</summary>

Chat Completions·Messages·HTTP Responses는 원래 API 형식으로 전달합니다.
모델 기능은 연결한 서버가 제공합니다. `/responses/compact`는 지원하지 않으며,
TPM 한도를 지정하면 해석할 수 없는 compaction 입력과 예산을 넘는 입력 추정치를
거절합니다.

</details>

## 한도 설정하기

| 설정 | 의미 |
|---|---|
| RPM·TPM에 숫자 지정 | 최근 60초 기준의 로컬 사용 한도를 적용합니다. |
| `unknown` | 로컬 한도를 추가하지 않고 제공자가 제한하도록 둡니다. |
| `unlimited` | 로컬 한도를 적용하지 않겠다고 명시합니다. |

동시성 제한과 공유 대기는 계속 적용됩니다. 제공자가 한도를 연속으로 보충한다면
최근 60초 기준과 동작이 다를 수 있습니다. 숫자 한도는 로컬 예산을 지키고 싶을 때
설정하세요. 다른 PC가 같은 할당량을 얼마나 썼는지까지 파악하지는 못합니다.

<details>
<summary>토큰 정산·캐시·복구 세부 설정</summary>

- **토큰 정산:** 새 설정의 기본값은 `actual`입니다. 유효한 최종 usage로 예약량을
  보정하며, usage가 없으면 예약을 유지합니다. 입력 추정은 근사치입니다.
  제공자가 예약량을 한도 기준으로 삼는다면 `reserved`를 선택하세요.
- **Exact cache:** 기본값은 꺼짐입니다. 마법사에서 켜거나 `[cache]`에
  `ttl_secs = 300`, `max_history = 3`을 설정하세요. 응답 데이터는 최대 4 MiB까지
  보관합니다. 조건에 맞는 동일 텍스트 요청에 이전의 완성된 답변과 usage를
  재사용합니다. 도구 호출·상태 참조 요청·미완성 응답은 제외합니다.
- **재시도:** 게이트웨이 내부 재시도는 기본적으로 꺼져 있습니다. 최상위 설정의
  `retry_transient_429 = true`는 출력 전 재시도가 허용되는 요청에 최대 1회를
  추가합니다. 클라이언트의 재시도 횟수와 마감 시간도 함께 고려해야 합니다.
  대기 안내가 없는 긴 제한은 이를 넘어설 수 있습니다.
  스트리밍 시작 후에는 자동으로 재전송하지 않습니다.
- **재시작 대기:** `startup_hold_secs`는 기본 60초이며 0–3600초로 설정합니다.
  재시작 직후 이미 사용한 한도를 다시 쓰지 않도록 알려진 한도 구간을 보호합니다.
  0이면 이 대기를 끕니다.

설정 파일을 수정한 뒤에는 `llmgw restart`로 적용하세요.

</details>

## 기여하기

새 클라이언트 연결 프로필, 버그 수정, 핵심 기능 개선을 환영합니다.
재현 가능한 문제나 더 편하게 만들고 싶은 작업 흐름을
[이슈](https://github.com/ryuwon-ai/deskquota/issues)로 남겨 주세요.

`product/`에서 실행합니다.

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tests -p 'test_*.py'
```

문제를 제보할 때는 클라이언트 버전, 민감정보를 지운 설정, 기대한 동작과
최소 재현 방법을 함께 남겨 주세요. 키, 사내 URL, 프롬프트, 비공개 소스 코드는
공유 자료에 포함하지 마세요.

## 라이선스

[MIT](LICENSE-MIT) 또는 [Apache-2.0](LICENSE-APACHE) 중 선택해 사용할 수 있습니다.
