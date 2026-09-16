$dqRoot=Join-Path $env:LOCALAPPDATA 'deskquota-windows-lab-20260915'
$run=Join-Path $dqRoot 'workflow-20260916'
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
$archive=Join-Path $run 'rust-source.tar.gz'
$sha='0aff89243340bbf4b9f30c9f2477ac1691657f47a3c7730a873461ff975f63c0'
if ((Get-FileHash $archive -Algorithm SHA256).Hash.ToLower() -ne $sha) {throw 'archive hash mismatch'}
$manifestPath=Join-Path $run 'rust-source.json'
if ((Get-FileHash $manifestPath -Algorithm SHA256).Hash.ToLower() -ne '7861a7fc39e830a444d87bf8db9ff89a8149a7b24fb4e3c91904c5e44850e86a') {throw 'manifest hash mismatch'}
$source=Join-Path $run 'source'
New-Item -ItemType Directory -Path $source | Out-Null
& tar.exe -xzf $archive -C $source
if ($LASTEXITCODE -ne 0) {throw 'source extraction failed'}
$manifest=Get-Content $manifestPath -Raw | ConvertFrom-Json
$files=@($manifest.PSObject.Properties)
if ($files.Count -ne 94) {throw 'unexpected source count'}
foreach ($file in $files) {if ((Get-FileHash (Join-Path $source $file.Name) -Algorithm SHA256).Hash.ToLower() -ne $file.Value) {throw 'source hash mismatch'}}
foreach ($file in $files) {(Get-Item (Join-Path $source $file.Name)).LastWriteTimeUtc=[datetime]::UtcNow}
$probeManifest=Get-Content (Join-Path $run 'probe-source.json') -Raw | ConvertFrom-Json
New-Item -ItemType Directory -Path (Join-Path $source 'scripts') | Out-Null
foreach ($file in $probeManifest.PSObject.Properties) {if ((Get-FileHash (Join-Path $run $file.Name) -Algorithm SHA256).Hash.ToLower() -ne $file.Value) {throw 'probe hash mismatch'};Copy-Item (Join-Path $run $file.Name) (Join-Path $source ('scripts/'+$file.Name))}
$result=[ordered]@{stage='windows-workflow-validation';platform=[Environment]::OSVersion.VersionString;source_archive_sha256=$sha;source_manifest_sha256='7861a7fc39e830a444d87bf8db9ff89a8149a7b24fb4e3c91904c5e44850e86a';verified_source_files=$files.Count;source_mtimes_refreshed_after_verification=$true;toolchain='1.88.0-x86_64-pc-windows-gnu';build_jobs=2;commands=@();modes=@()}
$result | ConvertTo-Json -Depth 7 | Set-Content (Join-Path $run 'build-progress.json') -Encoding UTF8
$work=Join-Path $source 'product'
foreach ($mode in @('default','bpe')) {
 $testArgs=@('+1.88.0-x86_64-pc-windows-gnu','test','--locked')
 if ($mode -eq 'bpe') {$testArgs+=@('--features','bpe','--lib','--test','input_estimation_contract','--test','config_contract','--test','setup_contract','--test','retry_contract','--test','cache_contract','--test','stream_lifetime','--test','wire_contract')}
 $testArgs+=@('--target','x86_64-pc-windows-gnu','--','--test-threads=2')
 $watch=[Diagnostics.Stopwatch]::StartNew()
 $p=Start-Process -FilePath (Join-Path $env:CARGO_HOME 'bin/cargo.exe') -ArgumentList $testArgs -WorkingDirectory $work -NoNewWindow -Wait -PassThru -RedirectStandardOutput (Join-Path $run ($mode+'-tests.stdout.log')) -RedirectStandardError (Join-Path $run ($mode+'-tests.stderr.log'))
 $row=[ordered]@{mode=$mode;tests_exit=$p.ExitCode;tests_elapsed_seconds=$watch.Elapsed.TotalSeconds}
 $result.commands+=,@{mode=$mode;phase='test';program='cargo.exe';arguments=$testArgs}
 if ($p.ExitCode -ne 0) {$result.modes+=,$row;$result | ConvertTo-Json -Depth 7 | Set-Content (Join-Path $run 'build-result.json') -Encoding UTF8;exit $p.ExitCode}
 $testlog=Join-Path $run ($mode+'-tests.stdout.log')
 $required=@('retry_veto_false_fields_block_replay_without_expanding_true','retry_veto_preserves_cooldown_and_only_skips_unneeded_body_probe')
 if ($mode -eq 'default') {$required+='default_build_rejects_bpe_config_for_every_quota_kind'} else {$required+='explicit_bpe_configuration_is_accepted'}
 foreach ($test in $required) {if (-not (Select-String -Path $testlog -Pattern ($test+' ... ok') -SimpleMatch -Quiet)) {throw 'new mode regression absent or not passed'}}
 $row.new_mode_regressions_executed=$required
 $buildArgs=@('+1.88.0-x86_64-pc-windows-gnu','build','--locked','--release','--bin','llmgw')
 if ($mode -eq 'bpe') {$buildArgs+=@('--features','bpe')}
 $buildArgs+=@('--target','x86_64-pc-windows-gnu')
 $watch.Restart()
 $p=Start-Process -FilePath (Join-Path $env:CARGO_HOME 'bin/cargo.exe') -ArgumentList $buildArgs -WorkingDirectory $work -NoNewWindow -Wait -PassThru -RedirectStandardOutput (Join-Path $run ($mode+'-release.stdout.log')) -RedirectStandardError (Join-Path $run ($mode+'-release.stderr.log'))
 $result.commands+=,@{mode=$mode;phase='release';program='cargo.exe';arguments=$buildArgs}
 $row.release_exit=$p.ExitCode;$row.release_elapsed_seconds=$watch.Elapsed.TotalSeconds
 if ($p.ExitCode -ne 0) {$result.modes+=,$row;$result | ConvertTo-Json -Depth 7 | Set-Content (Join-Path $run 'build-result.json') -Encoding UTF8;exit $p.ExitCode}
 $binary=Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-gnu/release/llmgw.exe'
 $saved=Join-Path $run ('llmgw-'+$mode+'.exe')
 Copy-Item $binary $saved
 $row.binary_sha256=(Get-FileHash $saved -Algorithm SHA256).Hash.ToLower();$row.binary_bytes=(Get-Item $saved).Length
 $result.modes+=,$row
 $result | ConvertTo-Json -Depth 7 | Set-Content (Join-Path $run 'build-progress.json') -Encoding UTF8
}
$result | ConvertTo-Json -Depth 7 | Set-Content (Join-Path $run 'build-result.json') -Encoding UTF8
$result | ConvertTo-Json -Depth 7
