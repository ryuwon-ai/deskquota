<p align="center">
  <img src="assets/deskquota-hero.png" alt="DeskQuota — 내 자리에서 관리하는 LLM 사용 한도" width="100%">
</p>

<p align="center"><strong>쓰던 도구 그대로. LLM 한도는 함께 관리하세요.</strong></p>
<p align="center">같은 LLM 엔드포인트를 사용하는 에이전트와 스크립트를 위한 가벼운 로컬 게이트웨이.</p>
<p align="center"><a href="README.md">English</a> · 한국어</p>
<p align="center"><a href="#시작하기">시작하기</a> · <a href="#쓰던-도구-연결하기">도구 연결</a> · <a href="#한도-설정하기">한도 설정</a></p>

---

한 에이전트는 코드를 작성하고, 다른 에이전트는 리뷰합니다.
뒤에서는 스크립트가 재시도합니다. 모두 같은 사용 한도를 나눠 쓰지만,
서로 얼마나 쓰고 있는지는 알지 못합니다.

**DeskQuota는 이 요청들을 한곳에서 조율합니다.** 내 PC에서 실행되어
기존 도구와 사내·외부 API 또는 로컬 LLM 엔드포인트를 연결합니다.
설정한 한도 안에서 요청 순서를 정하고, 조건에 맞는 동일 응답을 재사용하며,
상위 서버가 바쁘거나 장애가 났을 때 대기와 복구를 함께 관리합니다.

**Rust 실행 파일 하나. 실행에 Docker, WSL, Python, Node, 데이터베이스가 필요하지 않습니다.**
터미널 명령어는 `llmgw`입니다.

```text
Claude Code ─┐
Codex ───────┼──► 내 PC의 DeskQuota ──► LLM 엔드포인트
Pi / 스크립트 ┘     한도 · 대기열 · 캐시 · 복구
```

## 어떤 일을 하나요?

| 이런 상황에서 | DeskQuota가 하는 일 |
|---|---|
| 여러 도구가 같은 한도를 쓸 때 | 이 인스턴스를 지나는 요청의 RPM·TPM·동시성을 함께 관리합니다. |
| 한 작업이 대기열을 채울 때 | 설정한 클라이언트별 대기열에 차례를 주고, 특정 요청이 계속 밀리지 않도록 보호합니다. |
| 캐시 가능한 동일 요청이 반복될 때 | 완성된 응답을 재사용합니다. 동시에 들어온 중복 요청도 하나의 결과를 기다릴 수 있습니다. |
| 제공자가 `429`를 반환할 때 | 유효한 재시도·한도 갱신 안내를 읽고, 대기 중인 요청이 함께 기다리게 합니다. |
| 상위 서버가 반복해서 실패할 때 | 서킷브레이커로 요청을 제한하고, 보호 범위마다 요청 하나로 복구를 확인합니다. |
| 스트리밍 응답이 들어올 때 | 응답 조각을 전달하고, 지원되는 최종 usage로 토큰 예약량을 정산합니다. |

## 시작하기

**초기 프리뷰입니다.** 이 README는 현재 소스의 동작을 설명합니다.
공개된 `v0.1.0-preview.1` 패키지에는 캐시·복구 개선 일부가 아직 없습니다.
최신 구현을 사용하려면 소스에서 빌드하세요.

### 실행 파일 설치

[프리뷰 릴리스](https://github.com/ryuwon-ai/deskquota/releases/tag/v0.1.0-preview.1)에서
압축 파일, 설치 스크립트, 각각의 `.sha256` 파일을 받으세요.
네 파일을 오프라인 PC로 옮겨 설치할 수도 있습니다.

| 운영체제 | 압축 파일 | 설치 스크립트 |
|---|---|---|
| macOS · Apple silicon | `llmgw-macos-arm64.tar.gz` | `install.sh` |
| Windows · x64 | `llmgw-windows-x64.zip` | `install.ps1` |

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

## 한도 설정하기

| 설정 | 의미 |
|---|---|
| RPM·TPM에 숫자 지정 | 최근 60초 기준의 로컬 사용 한도를 적용합니다. |
| `unknown` | 로컬 한도를 추가하지 않고 제공자가 제한하도록 둡니다. |
| `unlimited` | 로컬 한도를 적용하지 않겠다고 명시합니다. |

동시성 제한과 공유 대기는 계속 적용됩니다. 제공자가 한도를 연속으로 보충한다면
최근 60초 기준과 동작이 다를 수 있습니다. 숫자 한도는 로컬 예산을 지키고 싶을 때
설정하세요. 다른 PC가 같은 할당량을 얼마나 썼는지까지 파악하지는 못합니다.

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

## 알아둘 한계

- 인스턴스 하나는 엔드포인트 하나와 자신을 지나는 요청을 관리합니다.
  제공자의 한도를 늘리거나, 모든 외부 소비를 관측하거나, 진행 중인 생성을
  일시 정지했다가 재개할 수는 없습니다.
- 기다리면 작업을 완료할 기회가 늘 수 있지만 지연도 길어집니다.
  이미 호출 간격을 잘 조절하는 단일 클라이언트라면 이점이 작을 수 있습니다.
- 실제 클라이언트의 자동 압축을 검사했지만 `/responses/compact`는 지원하지
  않습니다. TPM 한도를 지정하면 해석할 수 없는 compaction 입력과 예산을 넘는
  입력 추정치를 거절합니다.
- macOS·Windows 네이티브 패키징 워크플로가 있습니다. 현재 Windows 클라이언트의
  안정성, 장시간 작업, 저사양 PC에는 추가 검증이 필요합니다.
  모든 환경의 속도 향상이나 429 완전 제거를 보장하지 않습니다.

## 개발

`product/`에서 실행합니다.

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tests -p 'test_*.py'
```

테스트는 합성 데이터를 사용합니다. 빌드 결과는 iCloud Desktop 같은 동기화 폴더
밖에 두세요. 조사 노트, 원시 측정값, 생성 데이터, 로컬 인증정보는 Git에서
제외하고 제품 소스·회귀 테스트·CI는 버전 관리합니다.

문제를 제보할 때는 클라이언트 버전, 민감정보를 지운 설정, 기대한 동작과
최소 재현 방법을 함께 남겨 주세요. 키, 사내 URL, 프롬프트, 비공개 소스 코드는
공유 자료에 포함하지 마세요.

## 라이선스

[MIT](LICENSE-MIT) 또는 [Apache-2.0](LICENSE-APACHE) 중 선택해 사용할 수 있습니다.
