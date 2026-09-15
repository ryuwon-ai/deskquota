# 자동 압축 호환성 검증 — 2026-09-16

**일반적인 클라이언트 요약 방식은 동작한다. 다만 모든 압축 방식을 투명하게 지원하는 것은 아니다.** 설치된 Pi·Codex·Claude Code에서 자동 압축을 실제로 유발하고, 요약이 다음 요청에 포함되는 것까지 직접 연결과 DeskQuota 경유로 대조했다. 별도로 OpenAI 전용 압축 경로와 TPM 정책에서 거절되는 요청도 재현했다.

이 검증은 `0b01a1f`의 런타임을 대상으로 한다. 제품 동작은 바꾸지 않고 재현 스크립트, 결과, 문서를 추가했다. 이 체크포인트의 macOS 전체 Rust 검사는 438개 통과·0개 실패·1개 제외, Clippy는 경고 없이 통과했다. 이 수치는 아래 네이티브 압축 검증과 별개다.

## 직접 검증

| 클라이언트 | 환경 | 직접 연결 | 게이트웨이 경유 | 압축 증거 |
|---|---|---|---|---|
| Pi 0.84.2 | macOS ARM64 | 통과 | 통과 | `compaction_start/end`, 이유 `threshold`, 다음 요청에 요약 포함 |
| Codex 0.154.0 | macOS ARM64 | 통과 | 통과 | 세션의 `compacted` 항목 1개, 재개 후 요약을 포함한 응답 완료 |
| Claude Code 2.1.175 | macOS ARM64 | 통과 | 통과 | `system/compact_boundary`, `trigger=auto`, 이후 응답 완료 |
| Claude Code 2.1.76 | Windows 10 x64 | 통과 | 통과 | `system/compact_boundary`, `trigger=auto`, 이후 응답 완료 |

Pi와 Codex는 각 연결 방식에서 상위 서버 호출 3회, Claude는 5회였다. 마지막 요청에 fixture가 반환한 요약 표시가 포함되고, 후속 응답 표시가 클라이언트 출력에 나타나는 것을 검사했다. 자동 압축을 명령으로 수동 호출하지 않았다.

모두 **합성 loopback 상위 서버**를 사용했다. 일반 턴 하나의 응답에 입력 usage 120,000을 넣어 임계값을 넘겼으며, 실제로 120,000토큰의 대화를 생성한 것은 아니다. 따라서 요약 품질, 실제 모델의 긴 문맥 처리 능력, 장시간 작업 안정성이나 성능 우위는 입증하지 않는다.

게이트웨이 조건은 RPM 18, TPM 450,000, 동시 요청 한도 3, `accounting = "actual"`, exact cache 활성화다. 빠른 격리 검사를 위해 `startup_hold_secs = 0`으로 설정했다. 기본 60초 시작 대기나 동시 포화 부하는 이 압축 검사에 포함되지 않았다.

클라이언트의 문맥 한도와 압축 임계값은 검사 환경에만 지정했다.

- Pi: context 128,000, reserve 16,384, keepRecentTokens 1, compaction 활성화.
- Codex: context 128,000, auto-compact 임계값 100,000, `llmgw` custom provider, Responses HTTP.
- Claude: compact window 128,000, 임계 비율 25%, 세 번의 일반 턴 이후 압축 유발.

실제 사용자 설정·토큰·대화는 사용하지 않았다. 임시 HOME·클라이언트 설정·작업 디렉터리에서 실행하고 종료했다. 프로필 파일을 적용하는 `connect/disconnect`를 이번 검사에서 다시 실행한 것은 아니다. 특히 macOS Claude 2.1.175 결과가 현재 `connect`의 2.1.76 버전 제한을 변경하지 않는다.

### 캐시와 usage

동일한 짧은 Chat Completions 스트리밍 요청을 두 번 보내 상위 서버 호출 **1회**를 확인했다. 캐시 적중 응답은 첫 응답과 바이트 단위로 같았으며 `prompt_tokens = 120000`도 유지됐다. macOS와 Windows에서 모두 확인했다.

캐시 적중이 상위 서버 quota를 소비하지 않는 것과 클라이언트가 대화 문맥을 계산하는 것은 서로 다르다. 적중 응답의 usage를 0으로 바꾸면 압축 판단을 흐릴 수 있지만, 현재 구현은 그렇게 하지 않는다. 도구·상태가 포함된 호출 및 `context_management` 같은 미허용 캐시 필드는 캐시 대상에서 제외된다. 후자는 [캐시 정책 코드](../product/src/cache.rs)의 근거다.

## 거절이 재현된 경계

다음 결과는 macOS와 Windows에서 동일했다. 실패 요청은 게이트웨이가 거절해 상위 서버에 도달하지 않았고, 같은 요청을 합성 상위 서버에 직접 보내면 200이었다. 합성 서버의 200은 실제 제공자가 해당 요청이나 토큰 수를 허용한다는 증거가 아니다.

| 요청 | TPM 450,000 설정 | TPM unknown | 의미 |
|---|---|---|---|
| `/responses/compact` | 404 `route_not_found` | 동일 | 전용 압축 endpoint 미지원 |
| `/responses`의 `type=compaction` + `encrypted_content` 입력 | 400 `unsupported_multimodal_estimate` | 200, 응답 바이트 보존 | known TPM의 입력 형태 검사에서 암호화 압축 항목을 거절 |
| 약 460 KB 텍스트를 담은 Messages 요청 | 400 `estimate_exceeds_budget` | 200, 응답 바이트 보존 | JSON 바이트 수와 출력 예약 합계가 450,000을 넘으면 실제 토큰 수를 알기 전에 거절 |

마지막 조건은 압축 요청에도 적용된다. `accounting = "actual"`은 **완료 후** 정산이므로, 입장 전에 거절된 요청을 해결하지 못한다. 실제 토큰 수보다 보수적인 바이트 추정 때문에 유효한 긴 대화가 거절될 가능성은 있으나, 이 검사에서는 실제 tokenizer/제공자로 그 차이를 측정하지 않았다. `unknown`은 해당 TPM 검사를 끄므로 일반적인 해결책으로 권장하지 않는다.

반대로 다음 전달 경로는 양쪽 OS에서 직접 통과했다.

- 허용된 root의 `/messages/count_tokens`: 200, 응답 바이트 보존.
- Messages 및 Responses의 `context_management` 필드가 포함된 텍스트 요청: 200, 응답 바이트 보존. 실제 제공자의 서버 측 압축 실행까지 검증한 것은 아니다.
- 네이티브 Claude 호출의 `anthropic-beta`, `anthropic-version` 헤더가 상위 서버에 도달함.

OpenAI는 [서버 측 압축과 독립 `/responses/compact` 호출](https://developers.openai.com/api/docs/guides/compaction)을 별도 방식으로 문서화한다. 이번 Codex custom-provider 검증은 일반 `/responses`로 요약했다. 이 성공으로 전용 API나 OpenAI 기본 제공자의 압축 경로까지 지원한다고 판단하면 안 된다.

## 코드·공식 문서로 확인한 내용

1. **사용자의 자동 압축 설정을 끄지 않는다.** Pi 어댑터는 모델 항목과 기본 선택을, Claude는 gateway 관련 env를, Codex는 별도 provider profile을 설정한다. 압축 비활성화 키는 쓰지 않는다. 기존 `probe_pi.py`의 `compaction.enabled=false`는 해당 격리 테스트 설정이며, 기존 기본 왕복 검사가 압축 호환성을 검증하지 못했던 이유다. 새 검사에서는 활성화했다.
2. **문맥 메타데이터는 여전히 부족하다.** Pi 어댑터는 미확인 기본값 128,000/16,384를 쓰고 경고한다. 현재 CLI는 `ModelMetadata.context_window`를 전달하지 않는다. 실제 모델이 더 작으면 압축 시점이 늦을 수 있고, 더 크면 일찍 압축할 수 있다. Codex는 [문맥 한도·압축 임계값 설정](https://developers.openai.com/codex/config-reference)을 지원하지만 게이트웨이는 이를 검증해 채워 주지 않는다. TPM은 분당 처리 예산이며 모델 문맥 한도와 같지 않다.
3. **토큰 카운트 경로는 있지만 root 허용이 필요하다.** 현재 `connect claude`의 새 route에는 `messages`만 추가된다. 필요한 경우 해당 root의 `endpoints`에 `messages/count_tokens`를 명시해야 한다. 최신 [Anthropic gateway 문서](https://code.claude.com/docs/en/llm-gateway-protocol)는 카운트 endpoint를 선택 사항으로 설명하며, 없으면 클라이언트가 문자 수 기반 추정을 사용한다고 밝힌다. 누락 자체를 자동 압축 전체 실패로 단정하지 않는다.
4. **큐는 압축 의미를 구분하지 않는다.** 압축용 요약도 같은 root의 FIFO·quota·cooldown을 적용받는다. 현재 120초 capacity wait 제한이나 클라이언트 timeout과 충돌하는지는 장시간 혼합 부하에서 별도 검증해야 한다. 요약 요청을 자동으로 무제한 통과시키거나 quota를 우회하지 않는다.

소스 경로: [Pi 프로필](../product/src/clients/pi.rs), [Claude 프로필](../product/src/clients/claude.rs), [Codex 프로필](../product/src/clients/codex.rs), [CLI 연결](../product/src/cli.rs), [경로 처리](../product/src/protocol/mod.rs), [입장 전 입력 검사](../product/src/protocol/request.rs), [서버 제한](../product/src/server.rs).

Claude의 검사 전용 임계값은 [공식 환경 변수 문서](https://code.claude.com/docs/en/env-vars)를 참조했다. Pi는 설치된 0.84.2의 `dist/core/agent-session.js`와 `dist/core/compaction/compaction.js`를 확인했다. 별도 Codex 참조 clone의 `944d6fd1ba4baab69dbedd205282dc72ec20abb5`는 custom provider와 OpenAI provider의 압축 경로 차이를 설명하는 참고 자료이며, 설치된 0.154.0과 같은 소스라고 간주하지 않았다. 설치 버전의 동작 결론은 네이티브 실행에 근거한다.

## 다음 개선 순서

| 우선순위 | 변경 후보 | 완료 기준 |
|---|---|---|
| 1 | 모델의 context window와 출력 한도를 사용자가 확인해 연결 설정에 반영 | 작은 문맥·큰 문맥 모델 모두 올바른 시점에 압축하고 기존 설정을 임의로 덮어쓰지 않음 |
| 1 | Responses 전용 압축 경로 및 opaque state의 quota 계약 설계 | 전용 endpoint와 압축 후 다음 턴 모두 통과하며, 추정 불가능한 비용을 몰래 무료 처리하지 않음 |
| 1 | 긴 요청의 입장 전 토큰 추정 개선 | 실제 tokenizer 또는 제공자 count 결과로 과도한 거절 감소와 quota 초과 위험을 함께 측정 |
| 2 | 포화 큐에서 압축 후 작업 복구 검증 | 압축 성공·실패·대기 시간·취소·다른 root의 굶주림을 함께 기록 |

압축을 게이트웨이가 별도로 재구현하거나, LLM으로 압축 요청을 분류하는 기능은 추가하지 않았다. 먼저 클라이언트의 기존 방식을 정확히 통과시키고, 이 문서의 재현된 경계를 해결하는 것이 현재 범위에서 작고 검증 가능한 개선이다.

## 재현과 증거

```sh
cd product
CARGO_TARGET_DIR="$HOME/Library/Caches/llmgw-cargo" cargo build --release --locked --bin llmgw
python3 scripts/probe_compaction.py \
  --binary "$HOME/Library/Caches/llmgw-cargo/release/llmgw" \
  --output /tmp/deskquota-compaction.json
```

설치된 CLI를 PATH에서 선택하고 버전을 결과에 기록한다. 특정 클라이언트만 검사하려면 `--clients claude`처럼 지정한다. macOS 주 결과는 2.1.175 경로를 PATH 앞에 두고 실행했다. Windows는 Python 3.11과 기존 GNU release 실행 파일을 사용했으며 추가 Rust 빌드나 전역 설치는 하지 않았다. Windows 결과를 재현할 때는 Windows 경로의 실행 파일을 `--binary`로 전달한다.

최종 기록: [macOS JSON](../evidence/auto-compaction-2026-09-16/macos.json), [Windows JSON](../evidence/auto-compaction-2026-09-16/windows.json), [파일 해시](../evidence/auto-compaction-2026-09-16/manifest.json), [재현 스크립트](../product/scripts/probe_compaction.py).

JSON의 protocol `passed`는 **알려진 400/404를 포함한 현재 동작이 예상대로 재현됐다는 뜻**이며, 모든 압축 방식의 지원 판정이 아니다. 첫 검사에서는 작은 Pi fixture 문맥값, 기본 startup hold, 짧은 Claude 대화로 인해 검사 조건을 수정했다. Windows에서는 Python cp949 해독 오류를 UTF-8 지정으로 수정했고, 테스트가 직접 만든 state ACL이 제품의 보호 계약을 충족하지 않아 기존 `on/off`가 state를 만들도록 바꿨다. 제품의 ACL 검사는 완화하지 않았다.
