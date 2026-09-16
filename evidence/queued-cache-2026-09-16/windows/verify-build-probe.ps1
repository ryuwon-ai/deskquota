$dqRoot=Join-Path $env:LOCALAPPDATA 'deskquota-windows-lab-20260915'
$run=Join-Path $dqRoot 'queued-cache-20260916'
$env:CARGO_HOME=Join-Path $dqRoot 'cargo'
$env:RUSTUP_HOME=Join-Path $dqRoot 'rustup'
$env:CARGO_TARGET_DIR=Join-Path $dqRoot 'target'
$env:CARGO_BUILD_JOBS='2'
$env:CARGO_INCREMENTAL='0'
$env:CARGO_PROFILE_DEV_DEBUG='0'
$env:CARGO_PROFILE_TEST_DEBUG='0'
$env:AWS_LC_SYS_PREBUILT_NASM='1'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER='C:/mingw64/bin/gcc.exe'
$env:RUSTFLAGS='-C link-self-contained=no -C dlltool=C:/mingw64/bin/dlltool.exe'
$env:PATH=(Join-Path $env:CARGO_HOME 'bin')+';C:/mingw64/bin;'+$env:PATH
$archive=Join-Path $run 'source-final.tar.gz'
$sha='7540fa494fdfd881f3b58c6526855cd96971aeee1f1d92e5ca322b260ccc669e'
if ((Get-FileHash $archive -Algorithm SHA256).Hash.ToLower() -ne $sha) {throw 'source archive mismatch'}
$source=Join-Path $run 'source'
New-Item -ItemType Directory -Path $source | Out-Null
& tar.exe -xzf $archive -C $source
if ($LASTEXITCODE -ne 0) {throw 'source extraction failed'}
$manifest=Get-Content (Join-Path $source 'source-manifest.json') -Raw | ConvertFrom-Json
foreach ($file in $manifest.files) {
 if ((Get-FileHash (Join-Path $source $file.path) -Algorithm SHA256).Hash.ToLower() -ne $file.sha256) {throw 'source file mismatch'}
}
# Cargo uses mtimes; only after all content hashes pass do we force fresh source compilation.
foreach ($file in $manifest.files) {(Get-Item (Join-Path $source $file.path)).LastWriteTimeUtc=[datetime]::UtcNow}
$probe=Join-Path $run 'probe-rejection-head.py'
if ((Get-FileHash $probe -Algorithm SHA256).Hash.ToLower() -ne '40697d0183060f2531cf37e5efe8d9d6061d7294b178d5488e88dbfa697adf92') {throw 'probe source mismatch'}
New-Item -ItemType Directory -Path (Join-Path $source 'scripts') | Out-Null
Copy-Item $probe (Join-Path $source 'scripts/probe-rejection-head.py')
$result=[ordered]@{stage='native-queued-cache-and-rejection-head';platform=[Environment]::OSVersion.VersionString;source_sha256=$sha;source_files=$manifest.files.Count;verified_source_files=$manifest.files.Count;source_mtimes_refreshed=$true;toolchain='1.88.0-x86_64-pc-windows-gnu';build_jobs=2;probe_source_sha256=(Get-FileHash $probe -Algorithm SHA256).Hash.ToLower();wrapper_sha256=(Get-FileHash (Join-Path $run 'windows-probe-wrapper.py') -Algorithm SHA256).Hash.ToLower()}
$result | ConvertTo-Json | Set-Content (Join-Path $run 'progress.json') -Encoding UTF8
$work=Join-Path $source 'product'
$log=Join-Path $run 'tests'
$watch=[Diagnostics.Stopwatch]::StartNew()
$p=Start-Process -FilePath (Join-Path $env:CARGO_HOME 'bin/cargo.exe') -ArgumentList @('+1.88.0-x86_64-pc-windows-gnu','test','--locked','--lib','--test','cache_contract','--test','retry_contract','--test','fairness_contract','--test','stream_lifetime','--test','wire_contract','--target','x86_64-pc-windows-gnu','--','--test-threads=2') -WorkingDirectory $work -NoNewWindow -Wait -PassThru -RedirectStandardOutput ($log+'.stdout.log') -RedirectStandardError ($log+'.stderr.log')
$result.tests_exit=$p.ExitCode
$result.tests_elapsed_seconds=$watch.Elapsed.TotalSeconds
$result | ConvertTo-Json | Set-Content (Join-Path $run 'progress.json') -Encoding UTF8
if ($p.ExitCode -ne 0) {$result | ConvertTo-Json | Set-Content (Join-Path $run 'result.json') -Encoding UTF8; exit $p.ExitCode}
foreach ($test in @('queued_json_reuses_completion_during_rpm_or_concurrency_wait','queued_sse_reuses_completion_during_rpm_or_concurrency_wait','rejection_head_streams_when_body_cannot_change_retry_or_cooldown')) {
 if (-not (Select-String -Path ($log+'.stdout.log') -Pattern ($test+' ... ok') -SimpleMatch -Quiet)) {throw 'new regression absent or not passed: stale or invalid build'}
}
$result.new_regressions_confirmed=$true
$watch.Restart()
$p=Start-Process -FilePath (Join-Path $env:CARGO_HOME 'bin/cargo.exe') -ArgumentList @('+1.88.0-x86_64-pc-windows-gnu','build','--locked','--release','--bin','llmgw','--target','x86_64-pc-windows-gnu') -WorkingDirectory $work -NoNewWindow -Wait -PassThru -RedirectStandardOutput (Join-Path $run 'release.stdout.log') -RedirectStandardError (Join-Path $run 'release.stderr.log')
$result.release_exit=$p.ExitCode
$result.release_elapsed_seconds=$watch.Elapsed.TotalSeconds
if ($p.ExitCode -ne 0) {$result | ConvertTo-Json | Set-Content (Join-Path $run 'result.json') -Encoding UTF8; exit $p.ExitCode}
$binary=Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-gnu/release/llmgw.exe'
$result.binary_sha256=(Get-FileHash $binary -Algorithm SHA256).Hash.ToLower()
$result.binary_bytes=(Get-Item $binary).Length
if ($result.binary_sha256 -eq '2f9cb06e446d3deafae972c8629d88821be498e8c77a19a5986925e1bd96e5c8') {throw 'old release binary reused'}
Copy-Item $binary (Join-Path $run 'llmgw.exe')
$result | ConvertTo-Json | Set-Content (Join-Path $run 'progress.json') -Encoding UTF8
$watch.Restart()
$p=Start-Process -FilePath (Get-Command py.exe).Source -ArgumentList @('-3.11',(Join-Path $run 'windows-probe-wrapper.py')) -WorkingDirectory $run -NoNewWindow -Wait -PassThru -RedirectStandardOutput (Join-Path $run 'probe.stdout.log') -RedirectStandardError (Join-Path $run 'probe.stderr.log')
$result.probe_exit=$p.ExitCode
$result.probe_elapsed_seconds=$watch.Elapsed.TotalSeconds
$result | ConvertTo-Json | Set-Content (Join-Path $run 'result.json') -Encoding UTF8
$result | ConvertTo-Json
exit $p.ExitCode
