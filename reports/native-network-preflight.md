# 사내 CA·프록시 구현 전 확인

2026-09-12. Accounting 하네스 구현 중 부모가 수행한 **읽기 전용 소스 검토**다.
제품 네트워크 설정을 구현하거나 실제 사내 endpoint에 접속한 결과가 아니다.
기준은 [native 설정 명세](../docs/superpowers/specs/2026-09-12-native-setup-design.md)의
명시적 upstream proxy·CA, loopback 우회, TLS 검증 유지다.

현재 `product/Cargo.toml`은 reqwest **0.13.5**, default features off,
`rustls`, `stream`, `http2`를 사용한다. `UpstreamClient::new`는 `.no_proxy()`와
retry/redirect off, 연결 풀을 이미 설정한다. 사용자 지정 proxy·CA를 전달하는
필드는 아직 없으며, 마법사에서 묻기만 하고 저장된 선택을 무시해서는 안 된다.

## 기존 dependency로 제공되는 경계

로컬 registry의 고정 버전 소스
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reqwest-0.13.5/`를 읽었다.

| 소스/API | 확인한 의미 | 구현 시 적용할 점 |
|---|---|---|
| `src/async_impl/client.rs:1423`, `ClientBuilder::proxy` | 명시적 proxy를 추가하고 system proxy 자동 사용을 끔 | 별도 system-proxy feature나 환경 자동 탐색을 켤 필요가 없음 |
| 같은 파일 `:1436`, `ClientBuilder::no_proxy` | 등록한 proxy도 모두 제거함 | 명시적 proxy를 추가한 **뒤** 호출하면 선택을 지워버림. builder 순서 검증 필요 |
| `src/proxy.rs:361`, `Proxy::no_proxy` | 개별 proxy의 제외 목록 지정 | 명시적 upstream proxy에도 loopback 제외를 붙이고 실제 fixture로 확인 |
| 같은 파일 `:487–509`, `NoProxy::from_string` | IPv4/IPv6와 CIDR, domain 규칙을 지원하는 lazy parser | `from_env`를 사용하지 않고 정해진 loopback 예외를 적용. 모든 host를 제외하는 `*`는 사용하지 않음 |
| `src/tls.rs:217`, `Certificate::from_pem_bundle` | PEM bundle에서 certificate 목록을 읽음 | 직접 PEM parser나 TLS stack을 만들 필요 없음. 빈/잘못된 bundle은 명확히 거절 |
| `src/async_impl/client.rs:1877`, `tls_certs_merge` | 지정 roots를 native/built-in roots에 더함 | 기존 신뢰를 유지하면서 사용자가 지정한 CA만 추가하는 경우의 API |
| 같은 파일 `:1902`, `tls_certs_only` | native/built-in roots를 제외하고 지정 roots만 사용 | merge와 같은 의미가 아님. 필요 없는 별도 설정 선택지를 만들지 않음 |
| 같은 파일 `:1911`, `add_root_certificate` | 문서상 deprecated | 새 구현은 현재 API 사용 |

현재 선택한 rustls 경로는 roots 추가가 없으면 platform verifier를 만들고,
추가 roots가 있으면 `Verifier::new_with_extra_roots`로 전달한다
(`src/async_impl/client.rs:750–777`). 이 소스 연결은 실제 Windows/macOS/Linux의
회사 trust store가 모두 동작한다는 실행 증거가 아니다. 사용자가 지정한 CA 파일과
OS trust 경계를 각각 확인해야 한다.

## 다음 구현의 최소 검증

- 설정이 없을 때 기존 pooled client의 동작과 loopback control 경로가 유지되는지 확인한다.
- 명시적 proxy를 넣고도 loopback upstream/control 요청이 proxy fixture에 닿지 않는지
  실제 socket으로 확인한다. 환경의 proxy 값에만 기대어 PASS로 표시하지 않는다.
- 비 loopback 이름을 대상으로 하는 요청은 소유한 local proxy fixture에서 처리하도록
  만들어, 인터넷 접속 없이 설정이 실제 적용됨을 확인한다. fixture가 외부로 relay하지 않는다.
- 임시 CA와 서버 certificate로 신뢰 성공, 잘못된 CA/hostname 거절을 확인한다.
  검증 비활성 옵션, 시스템 CA 설치나 사용자 전역 proxy 변경으로 통과시키지 않는다.
- CA 파일·auth reference 오류는 비밀값을 출력하지 않고 설정 문제로 표시한다.
  CA·proxy의 parse/load는 구성 시 수행하며 요청마다 파일을 읽는 경로를 만들지 않는다.
- 기본 doctor는 offline을 유지한다. model GET과 유료일 수 있는 generation smoke는
  별도 선택이며 성공 상태도 구분한다.

위 항목은 앞으로 수행할 수용 검사다. 새 dependency 추가·Rust 수정·빌드·runtime
probe는 이 조사에서 수행하지 않았다. Accounting 비교의 소스·binary 고정이 끝난
뒤 Native Task 2에서 승인된 범위에 맞춰 연결한다.
