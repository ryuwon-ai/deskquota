# Native Task 2 구현 전 확인

2026-09-12. Native Task 1 통합 검증 중 부모가 수행한 **읽기 전용 소스 확인**이다.
마법사 구현·실제 터미널 실행 결과가 아니다. Task 1이 수용되기 전에는 제품을
수정하지 않는다. 규범은 [native 명세](../docs/superpowers/specs/2026-09-12-native-setup-design.md)와
[Task 2 계획](../docs/superpowers/plans/2026-09-12-native-experience.md)이다.

## 이미 있는 라이브러리의 경계

로컬 Cargo registry의 `dialoguer-0.12.0` 소스를 직접 읽었다. package는 이미
내려받아져 있으나 현재 제품의 직접 dependency에는 아직 없다.

- `Cargo.toml`의 default features는 editor/password다. 계획대로 default features를
  끄면 사용하지 않는 editor/tempfile·password/zeroize 경로를 요청하지 않는다.
- Select·Confirm·MultiSelect에는 `interact_on_opt`가 있으며 Escape/q 취소를
  `Option::None`으로 반환한다. 별도의 메뉴 렌더러를 처음부터 만들 필요는 없다.
- Input에는 같은 `interact_opt` API가 없다. Select의 취소 계약을 문자열 입력에도
  있다고 가정하지 않는다. 단계별 돌아가기/취소와 Ctrl-C는 실제 terminal fixture로
  확인해야 한다.
- 기본 출력은 stderr terminal이다. 명령 진입점에서 non-TTY를 먼저 판별하고
  setup 필요 안내/exit 2를 반환해야 한다. `Input` 내부의 `not a terminal` 오류만
  의존하면 제품이 요구하는 exit code와 안내가 자동으로 생기지 않는다.
- `interact_text` 주석의 ASCII 설명과 `interact`의 line-input 경로를 구분한다.
  한글/공백 경로 지원은 주석으로 판정하지 말고 실제 선택한 API로 검증한다.

참조한 소스는 `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dialoguer-0.12.0/`
아래 `Cargo.toml`, `src/prompts/{select,confirm,multi_select,input}.rs`다.
프로젝트 소스·lockfile 변경이나 build는 수행하지 않았다.

## 현재 코어와 연결할 때 빠뜨리기 쉬운 부분

| 읽은 코드 | 현재 상태 | Task 2에서 확인할 것 |
|---|---|---|
| `product/src/config.rs` | typed Limit의 Known/Unknown/Unlimited, env reference, model output bound가 있음 | 같은 validator를 재사용하고 0을 known 값으로 만들지 않는다 |
| `product/src/config.rs` | 설정 입력은 deny_unknown_fields, disk load 중심 | 저장 전 preview 검증을 위해 임시 파일을 먼저 만들지 않는다. 취소까지 파일 변화 0이라는 계약을 보존한다 |
| `product/src/transport/upstream.rs` | retry/redirect off, no_proxy, 단일 pooled client | 사내 CA·명시적 proxy는 현재 구현돼 있지 않다. 질문만 저장하고 연결에는 무시하는 경로를 만들지 않는다 |
| `product/src/cli.rs` | Task 1은 explicit config와 lifecycle 중심 | 기본 config 경로, bare command, setup, exit 2/130은 Task 2에서 추가할 영역이다 |
| native 계획 Task 3~5 | client patch와 로그인 등록은 이후 task | 선택·계획됨과 실제 적용·검증됨을 분리하며 아직 구현하지 않은 작업을 성공으로 표시하지 않는다 |

마법사는 가능한 설정을 구성하는 command다. 설정 단계의 discovery나 질문 루프를
서버 요청 경로에 넣지 않는다. upstream protocol은 사용자의 선택과 검사 결과로
표시하며 preset/model 이름만으로 추론해 확정하지 않는다. 별도 생성 smoke를
선택하지 않은 model listing 검사에는 inference 요청이 없어야 한다.

기존 config를 다시 설정할 때 defaults만 보존하고 여러 root/model·retry·accounting
설정을 조용히 초기화하지 않는지도 검증한다. 새 resource의 pending intent를
보관할 필요가 있으면 최종 동작에 필요한 최소 정보만 둔다. 기존 client 값이나
인증정보를 새 전용 저장소에 불필요하게 복제하지 않는다.

Task 1에서 수용할 protected-file API와 source identity는 HOLD 이후 확정된다.
이 기록의 소스 상태는 당시 관찰이며, Task 2 구현 전에 수용된 baseline을 기준으로
다시 필요한 경계만 확인한다. Task 1 수용 뒤 확인한 기존 reqwest의 CA·proxy API와
builder 순서·loopback 검증 조건은 [네트워크 사전 점검](native-network-preflight.md)에 둔다.

## 2026-09-13 추가 확인: 공정성 안내와 저장 전 검증

수용된 코어 명세 5절의 공정성 단위는 **등록한 route의 root**다. 같은 Pi 설정을
공유하는 독립 세션과 그 자식 agent는 같은 root 예산을 공유한다. 별도 root 주소를
설정하지 않은 세션들을 자동으로 식별하거나 각각 공정하게 배분한다고 안내하지
않는다. 마법사가 보여주는 연결 주소와 이후 client adapter의 root 선택에서도 이
범위를 설명한다. 이번 benchmark의 네 root는 명시적으로 나눈 fixture이며 실제
여러 agent가 자동으로 분리된다는 검증이 아니다.

현재 `Config`의 디스크 로더는 먼저 파일을 canonicalize한 뒤 TOML을 파싱하지만,
내부 validator 자체는 파일이나 네트워크를 읽지 않는다. 저장 전 검증은 이 검증을
재사용할 수 있도록 좁은 API를 제공하는 방식으로 연결한다. 취소 전 preview를
만들기 위해 config/state/token 파일을 미리 만들 필요는 없다.

현재 auth validator는 env **이름과 헤더 형식**을 검사한다. 해당 값이 지금 shell에
있는지, 지정 upstream에서 인증되는지, 다음 로그인 worker에서도 보이는지는 다른
확인이다. 저장만은 설정 보관 결과, 저장 후 시작은 로컬 worker readiness 결과,
network/inference 선택은 별도 upstream 검사 결과로 보고한다. offline doctor의
정상 config 판정을 인증·추론 성공으로 표시하지 않는다.

이번 추가 확인도 코드와 명세를 읽은 결과다. 마법사와 client 설정 변경은 아직
구현하거나 실행하지 않았으며, quota 실험 중 제품 파일과 binary는 수정하지 않았다.

## 2026-09-13 추가 확인: 구현 중 resolve된 console

Native Task 2가 dependency를 추가한 뒤의 Cargo.lock에는 `console 0.16.6`이
기록돼 있다. 부모가 로컬 `console-0.16.6/src/unix_term.rs:343–365`와
Dialoguer `src/prompts/input.rs:328`을 읽었다. Unix `read_single_key(false)`는
Ctrl-C 입력을 받으면 terminal flags를 복원한 뒤 SIGINT를 발생시킨다.
Dialoguer text Input은 `Term::read_key`를 사용한다. 이 부분은 기존 라이브러리의
처리 경로이며 새로운 terminal/signal renderer가 필요하다는 근거가 아니다.

실제 PTY 검사는 구현 담당자가 별도로 수행한다. Unix signal 종료를 Python이
`returncode=-2`로 표시한 결과와 셸의 130, 명시적 Cancel 메뉴가 반환한 exit 130을
구분해 기록해야 한다. 소스 확인만으로 Ctrl-C·terminal 복구·한글 입력이 실제로
통과했다고 표시하지 않는다. 이 추가 기록에서 부모는 product를 빌드하거나
실행하지 않았다.
