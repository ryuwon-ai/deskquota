# Windows native validation — 2026-09-15

Follow-up: [2026-09-16 native clients, Task Scheduler fixes, and cache findings](windows-followup-and-improvements-2026-09-16.md).
The results below preserve the earlier binary and validation boundary.

**Directly verified:** DeskQuota builds and runs on one Windows 10 x64 laptop
without MSVC, Docker, or WSL. The final native suite passed **402 tests, 0 failed,
2 ignored** across 19 suites. The default-feature release, ZIP installer,
interactive setup, lifecycle, and synthetic HTTP/SSE acceptance also passed.
This is Windows compatibility evidence, not a competitive performance result.

## Environment and reproducibility

| Item | Observed value |
|---|---|
| OS | Windows 10 Education, 10.0.19045, x64, NTFS |
| CPU / RAM | Ryzen 5 5600H, 6 cores / 12 logical processors; 31.85 GiB RAM |
| Account | Existing administrator account; not a fresh standard-user installation |
| Shell | Windows PowerShell 5.1.19041.6456; existing RemoteSigned policy preserved |
| Compiler | Rust 1.88.0 GNU, MinGW-w64 GCC 8.1.0, Binutils 2.30 |
| Build limits | 2 jobs; incremental off; debug info off for dev/test profiles |
| Storage | Dedicated directory under LOCALAPPDATA; no Desktop/OneDrive build output |
| Runtime dependencies | One executable; development tools absent from the tested child PATH |

Rust was provisioned into the dedicated test directory with no PATH changes.
The existing MinGW installation supplied the compiler, assembler, and `dlltool`.
This records the available toolchain, not a recommendation to install old GCC.
The build used `AWS_LC_SYS_PREBUILT_NASM=1`, an explicit external GCC linker,
and `RUSTFLAGS=-C link-self-contained=no -C dlltool=C:/mingw64/bin/dlltool.exe`.
Prebuilt NASM objects are supported for this non-FIPS x64 dependency build.
[AWS-LC Windows requirements](https://aws.github.io/aws-lc-rs/requirements/windows.html)

```powershell
cargo +1.88.0-x86_64-pc-windows-gnu test --locked --all-targets --no-fail-fast --features bench-harness --target x86_64-pc-windows-gnu -- --test-threads=2
cargo +1.88.0-x86_64-pc-windows-gnu build --locked --release --bin llmgw --target x86_64-pc-windows-gnu
```

The tested snapshot includes uncommitted changes on top of repository commit
`687a5ce443bc612e3cc07f973ffd8df4c36655b9`; it is not that commit's release.
The initial 107-file source transfer and subsequent changed files were checked
by SHA-256 on Windows. The [test record](../evidence/windows-native-2026-09-15/native-tests.json)
records the final tested source hashes. A later module-header comment and
documentation updates do not alter the built executable.

## Defects found and fixed

| Native failure | Root cause and final change |
|---|---|
| Captured `llmgw on` never returned | Background workers inherited the invoking process's pipe handles. Clear inheritance on valid standard handles before detached spawn. Rust 1.88's Windows process implementation otherwise passes handle inheritance to CreateProcess. |
| Reset cancellation tests failed on Windows | Mio exposes a reset through closed readiness rather than its ERROR interest. Use Winsock `FD_CLOSE` and an OS threadpool wait; retain graceful send-half-close behavior. No polling or thread per connection. |
| Newly created state/client directories failed ACL validation | Creating all ancestors with default inherited permissions conflicted with protected-state requirements. Create only missing ancestors through the existing protected-directory helper; preserve and validate existing state. |
| Setup/config patch failed during flush | Windows file flushing requires a writable handle. Open the file for writing when syncing after metadata restoration. |
| Cross-platform autostart template checks failed | Validate absolute paths against the target platform rather than the build host. |
| Test cleanup and fixtures failed | Several fixtures protected files only on Unix. Reuse production Windows protection in fixtures; account for native Windows lexical path normalization. |

The handle-inheritance explanation was checked against [Rust 1.88 Windows process source](https://raw.githubusercontent.com/rust-lang/rust/1.88.0/library/std/src/sys/process/windows.rs).
The reset diagnosis was checked against installed Mio 1.2.3 source and the
[Winsock FD_CLOSE contract](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-wsaeventselect).
The wait owns its socket, event, and callback context; destruction disarms the
wait and joins callbacks before releasing those resources, following
[SetThreadpoolWait](https://learn.microsoft.com/en-us/windows/win32/api/threadpoolapiset/nf-threadpoolapiset-setthreadpoolwait)
and [WaitForThreadpoolWaitCallbacks](https://learn.microsoft.com/en-us/windows/win32/api/threadpoolapiset/nf-threadpoolapiset-waitforthreadpoolwaitcallbacks).
The implementation adds a feature to the existing `windows-sys` dependency;
it adds no crate or runtime service.

Two full Windows runs passed after the functional fixes; the final run also
had no compiler warnings. The ignored tests are a profiling-only case and a
symlink fixture requiring Developer Mode or symlink privilege. Neither is
counted as passed. macOS all-target tests and Clippy with `-D warnings` also
passed. Windows Clippy was not run.

## Release and installation

| Check | Result |
|---|---|
| Default-feature release build | Passed; 243.53 seconds on this host |
| Executable size | 16,344,206 bytes, about 16.34 MB / 15.59 MiB |
| SHA-256 | `2e0f0258a23ebaa30ed05359fad8a33c7da46c437937ee9689586ca46a031126` |
| ZIP installation and reinstall | Passed, including a destination containing a space |
| Wrong checksum | Rejected; existing executable remained byte-for-byte intact |
| PATH registry | User and machine values unchanged |
| Runtime child PATH | Windows/System32 and Windows only; cargo, rustc, gcc, Node, Python, Docker unresolved |
| Lifecycle | `on`, `status`, `off`, repeated `off`, and stopped-state readback passed |
| DLL import inspection | Only Windows system DLL names observed; no MinGW libgcc/libwinpthread or MSVC redistributable DLL names |

The release harness reused a `test-compile` stage label in its raw result;
the recorded command and build stderr identify a release build. The normalized
[release record](../evidence/windows-native-2026-09-15/release-build.json) preserves
this discrepancy. No public release was created. A restricted PATH test on an
existing developer account does not prove clean-machine deployment.

## Interactive setup and actual quota settings

The real Windows console wizard was driven over SSH PTY. It saved a synthetic
corporate preset with forwarded Authorization, one manual model, RPM **18**,
TPM **450,000**, concurrency **3**, startup hold **60 seconds**, actual usage
accounting, and exact cache with TTL **300 seconds** / at most **3 messages**.
Model listing was skipped and the synthetic upstream was never called.

Save only completed, offline `doctor` passed, and a subsequent `on/status/off`
cycle using that saved configuration passed. Status observed a startup hold of
**59,931 ms**, RPM capacity 18 and TPM capacity 450,000. Autostart remained
disabled and Pi/Claude/Codex profiles remained unconnected. See the
[interactive setup record](../evidence/windows-native-2026-09-15/interactive-setup.json).

## Synthetic cache and resource check

The reusable [acceptance script](../product/scripts/verify_windows.py) runs a
Python stdlib loopback fixture with a deliberate **100 ms response delay**.
Each format makes one cold request followed by 40 identical sequential requests
over a reused HTTP connection. There is no separate warm-up, randomized
competitor arm, or multi-run confidence interval. Times measure complete response
receipt, not time to first model token.

| Format | Cold request | Cache hit p50 | Cache hit p95 | Cache hit p99 | Hit samples |
|---|---:|---:|---:|---:|---:|
| JSON | 103.792 ms | 0.165 ms | 0.194 ms | 0.273 ms | 40 |
| SSE | 100.911 ms | 0.181 ms | 0.229 ms | 0.246 ms | 40 |

**82 submitted / 82 completed / 80 cache hits / 2 upstream attempts / 0 failures.**
Both upstream calls received standard Authorization and no `X-LLMGW-token`.
The responses' usage settled to **RPM debit 2 / TPM debit 46**, matching
20 input + 3 output tokens for each upstream call. Cache hits did not debit
another upstream request. This deliberately repetitive workload cannot predict
real conversation hit rates; its p99 is the largest of only 40 samples.
It does not measure competitive throughput, task goodput, or provider latency.

| Snapshot | Working set | Private bytes | Threads | Handles | Cumulative CPU |
|---|---:|---:|---:|---:|---:|
| Before traffic | 8,925,184 | 2,056,192 | 17 | 103 | 0.015625 s |
| 64 idle connections, start | 9,945,088 | 3,600,384 | 17 | 363 | 0.015625 s |
| Same connections after 2 s | 9,945,088 | 3,600,384 | 17 | 363 | 0.015625 s |
| After replay traffic | 10,539,008 | 2,863,104 | 19 | 117 | 0.015625 s |

The sampled working set was **8.51–10.05 MiB**. Creating 64 idle connections did
not increase the sampled thread count. No CPU-time increment was observed over
the two-second idle interval at the Windows counter's resolution; this is not
a claim of zero CPU use or a long-duration leak test. Full numbers and the
acceptance script hash are in the [acceptance record](../evidence/windows-native-2026-09-15/release-acceptance.json).

## Remaining validation and final remote state

- Fresh non-admin account, Windows 11/ARM64, localized paths, signing/SmartScreen,
  actual login startup, sleep/resume, and lower-resource machines remain open.
- Native profile/patch tests passed; installed Windows Pi, Claude Code, and Codex
  inference/tool flows have not been exercised.
- Real company CA/proxy and provider quotas were not tested here. No real LLM
  endpoint or paid API was called. Linux execution remains unverified.
- Test workers and builds were stopped. The requested SSH service remains running
  with Manual startup; its new firewall rule is restricted to the current Mac's
  Tailscale address. The dedicated restricted key's temporary PTY permission was
  removed after setup. Test files and toolchain remain under LOCALAPPDATA.

The Windows pass closes the native-build and basic runtime uncertainty for this
host. Broader platform support and performance superiority need their own
evidence; the overall native/low-resource validation work item remains open.
