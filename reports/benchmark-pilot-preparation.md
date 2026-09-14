# 첫 성능 실험 준비

2026-09-12. Task 7 품질 리뷰 중 수행한 **소스 확인·환경 조회·측정 설계**다. 제품 benchmark 실행 결과가 아니다. [벤치마크 명세](benchmark-spec.md)와 [Core Task 8](../docs/superpowers/plans/2026-09-12-gateway-core.md)을 구체화하며 제품 범위를 늘리지 않는다.

## 직접 확인

- [환경 조회](../evidence/benchmark-preflight-task7-review.json): Apple M4, RAM 32GiB, 논리 CPU 10개, Darwin arm64, Python 3.14.6, Rust/Cargo 1.88.0. monotonic/perf_counter가 같은 `mach_absolute_time()` 구현을 보고했다. 현재 프로세스의 RSS·누적 CPU 시간을 `ps`로 조회할 수 있었다. 게이트웨이는 실행하지 않았다.
- 환경 조회 시 load average는 약 5였다. 순간 시스템 부하의 기록이며 특정 프로세스의 CPU 사용량이나 성능 불량을 뜻하지 않는다. 본 실험에서 시각·부하·fixture scheduling lag를 같이 남긴다. 타 사용자 프로세스를 종료해 환경을 정리하지 않는다.
- `product/src/admission/quota.rs`는 60초 rolling window와 known quota의 60초 startup hold를 사용한다. 보관 장부 상한은 8,192개다. HTTP 추정 비용은 **요청 JSON UTF-8 바이트 수 + 출력 예약량**이다. 문자 수/4, 실제 tokenizer 결과와 다르다.
- `Snapshot.retained`는 library API에 존재하지만 현재 인증 status에는 장부 항목 수가 없다. 현재 metrics의 첫 body/output 필드는 횟수이며 시각 또는 latency histogram이 아니다. 이미 존재하는 관측값의 의미를 바꾸어 보고하지 않는다.

## 비교와 측정 경계

| 구분 | 실행 조건 | 해석 |
|---|---|---|
| 결정적 trace | 기존 ManualClock·exact_fixture cost API, FIFO/RR | admission 순서·회계·기아·취소의 정확한 재현. HTTP latency와 합치지 않음 |
| 무대기 전송 | direct와 production binary, 동일 synthetic 응답, queue/quota 대기 없음, 재사용 연결과 첫 연결 구분 | 추가 네트워크·전달 지연의 실측. 의도적인 sleep/gate는 이 표본에 넣지 않음 |
| 제한된 자원 | direct / production RR / bench FIFO / bench RR, 같은 ingress 도착열·mock quota·workload | 짧고 긴 요청의 완료량·대기·실패 trade-off. 출력의 실제 과제 정답률을 뜻하지 않음 |
| 메모리·CPU | gateway의 소유 PID를 별도 관찰, mock/load generator와 분리 | sampled RSS 최고값과 OS가 제공하는 high-water mark를 구분. 전체 자식의 누적 CPU를 gateway 단독 CPU로 세지 않음 |

bench FIFO는 각 root의 head 중 가장 먼저 들어온 요청을 고르고 자원이 부족하면 기다리는 비교 정책이다. RR의 자원 우회·기아 방지 효과가 평가 대상이다. timeout·전체 큐 상한·취소·quota·transport는 동일하게 유지한다. 제품 CLI에 정책 설정을 추가하지 않고 승인된 `bench-harness` example에서만 비교 정책을 선택한다.

HTTP arm에 fixture용 token-cost header를 넣지 않는다. mock은 수신한 실제 request bytes와 공개한 응답 usage로 자기 비용을 계산하고, gateway의 추정량과 따로 기록한다. 비용 모델이 일치하는 경우와 의도적으로 다른 경우를 결과에서 식별한다. direct도 같은 mock quota를 통과해야 한다.

서로 다른 요청 집합의 `p95(proxy) - p95(direct)`는 **두 분포의 p95 차이**다. 이를 요청별 추가 지연의 p95라고 쓰지 않는다. 요청별 차이를 보고하려면 고정 입력의 대응 표본과 두 실행의 순서·짝짓기를 기록한다. 음수 차이도 버리지 않으며 CPU 내부 실행시간으로 해석하지 않는다. 측정 변동이 2ms 예산 판단을 바꾸면 우위 판정을 보류한다.

client 송신 시작→mock 수신까지는 ingress 전달·큐·네트워크가 섞인다. gateway가 queue 진입/입장 시각을 직접 계측하지 않으면 그 차이를 순수 queue wait로 명명하지 않는다. 첫 HTTP byte, 첫 body byte, 첫 유효 output delta, terminal marker, EOF 각각의 관측 지점을 명시한다.

## 실행 전 하네스의 실패 검증

- submitted request ID와 terminal outcome 집합이 일치해야 한다. 완료·거절·오류·timeout·취소는 배타적인 결과이며 pending을 삭제해 분모를 맞추지 않는다.
- ingress ID와 upstream attempt ID를 분리한다. retry는 새 성공 작업이 아니며, 시도 중복·알 수 없는 ID·누락을 self-check가 거절해야 한다.
- 완료는 HTTP200만이 아니라 예상 synthetic 응답과 종료 조건까지 충족해야 한다. 프레임 오류·응답 없는 EOF·잘린 SSE는 성공 latency 표본에 포함하지 않는다.
- 취소된 ingress가 drain 정책 때문에 upstream에서 끝나는 경우 두 결과를 따로 기록한다. 최종 snapshot과 mock 시도 기록으로 실행 중 요청의 정리 여부를 확인한다.
- raw payload·URL 인증값·token은 결과에 저장하지 않는다. fixture/config의 비밀값 없는 구조와 hash, binary hash, seed, 시간과 count만 보관한다.

## 반복과 비용

먼저 짧은 기능 점검으로 하네스가 실패를 감지하는지 확인한 뒤, 승인된 5 seed·5 quota window 실험을 실행한다. 본 실험은 실제 60초 window를 사용한다. 가상 시간을 쓴 결과를 생산 binary의 실시간 결과로 부르지 않는다.

4 arm × 5 seed × 5 window × 60초는 순차 실행 시 **측정 구간만 최소 100분**이다. 별도 workload마다 반복하면 더 길어진다. 이는 실행 시간 산술이며 실제 진행 기록이 아니다. startup hold·warm-up·drain 시간은 별도로 잡고 결과를 합치지 않는다. arm 순서는 seed별로 바꾸되 같은 PC에서 경쟁 arm을 동시에 돌려 생긴 경합을 성능 차이로 쓰지 않는다. 작은 pilot을 full matrix 완료로 표시하지 않는다.

startup hold를 코드에서 줄이거나 known quota를 unlimited로 바꾸어 실행 시간을 단축하지 않는다. warm-up이 실제 요청 예산을 쓰면 해당 시도와 측정 시작 시 장부 상태를 기록한다. 짧은 요청만 유리해지고 긴 요청의 완료율이 내려가면 그 손해까지 판단한다.

## 경쟁 제품 후보

[조회 기록](../evidence/benchmark-release-candidates-2026-09-12.json)에 설치 전 metadata만 남겼다. GitHub 익명 API는 403 rate limit으로 실패해 공식 release 페이지에서 확인했다.

- [Bifrost HTTP transports/v2.1.1](https://github.com/maximhq/bifrost/releases/tag/transports%2Fv2.1.1)은 HTTP 배포 후보이다. 플러그인별 release와 transport release를 혼동하지 않는다. 실행 binary·체크섬·최소 config 고정은 아직 하지 않았다.
- [LiteLLM v1.100.1](https://github.com/BerriAI/litellm/releases/tag/v1.100.1)은 조회 시 Latest였고 [PyPI version metadata](https://pypi.org/pypi/litellm/1.100.1/json)에 ARM64 macOS wheel이 있다. wheel SHA를 기록했지만 다운로드·설치·의존성 resolve·실행은 하지 않았다.
- [LiteLLM v1.99.1](https://github.com/BerriAI/litellm/releases/tag/v1.99.1)은 공식 설명이 Docker-only이므로 Git tag만 보고 같은 버전의 native Python 패키지가 있다고 가정하면 안 된다.

후보 조회는 최종 stable baseline 채택이나 호환성 검증이 아니다. 첫 공통 transport pilot 이후 네이티브 실행 가능 여부와 최소 설정을 확인하고 별도 비교 run으로 추가한다.

### 네이티브 배포 경로 추가 확인

Bifrost의 [공식 설치 문서](https://docs.getbifrost.ai/quickstart/gateway/setting-up)는 NPX가 native binary를 실행하는 경로와 file-only config를 설명한다. Redis·Docker가 필수인 구성으로만 비교하지 않는다. config store와 logs store를 끈 최소 설정을 별도 후보로 삼되, 채택할 stable binary에서 동작을 확인해야 한다.

[배포 metadata 조회](../evidence/benchmark-bifrost-native-distribution.json)에서 stable commit의 NPX source를 읽어 다운로드 URL 규칙을 확인했다. GitHub release에는 source archive 링크만 보였고 native binary는 별도 배포 도메인을 사용한다. 해당 v2.1.1의 macOS arm64·Windows amd64·Linux amd64 URL에 대한 HEAD 요청은 이 환경에서 모두 HTTP403이었다. **배포 파일이 없거나 해당 OS를 지원하지 않는다는 뜻은 아니다.** binary 다운로드·checksum·버전 출력·실행 확인은 하지 않았고, 이 실패를 Bifrost 성능 결과에 포함하지 않는다.
