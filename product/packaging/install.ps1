[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $Archive,
    [Parameter(Mandatory = $true)] [string] $ChecksumManifest,
    [string] $InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\llmgw\bin"),
    [ValidateSet("Preview", "None")] [string] $PathAction = "Preview"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Copy-Input([string] $Source, [string] $Destination) {
    if ($Source -match '^https?://') {
        Invoke-WebRequest -Uri $Source -OutFile $Destination -UseBasicParsing
    } else {
        if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
            throw "artifact_missing: $Source"
        }
        Copy-Item -LiteralPath $Source -Destination $Destination
    }
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("llmgw-install-" + [Guid]::NewGuid().ToString("N"))
$candidate = $null
$backup = $null
$preserveRecovery = $false
try {
    [System.IO.Directory]::CreateDirectory($work) | Out-Null
    if ([Uri]::IsWellFormedUriString($Archive, [UriKind]::Absolute) -and $Archive -match '^https?://') {
        $archiveName = [System.IO.Path]::GetFileName(([Uri]$Archive).AbsolutePath)
    } else {
        $archiveName = [System.IO.Path]::GetFileName($Archive)
    }
    if ([string]::IsNullOrWhiteSpace($archiveName)) { throw "archive_name_empty" }
    $archiveFile = Join-Path $work $archiveName
    $manifestFile = Join-Path $work "checksums.sha256"
    Copy-Input $Archive $archiveFile
    Copy-Input $ChecksumManifest $manifestFile

    $lines = @(Get-Content -LiteralPath $manifestFile | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($lines.Count -ne 1 -or $lines[0] -notmatch '^([0-9A-Fa-f]{64})\s+\*?([^\s]+)$') {
        throw "invalid_checksum_manifest"
    }
    if ($Matches[2] -cne $archiveName) { throw "checksum_archive_name_mismatch" }
    $actualHash = (Get-FileHash -LiteralPath $archiveFile -Algorithm SHA256).Hash
    if ($actualHash -cne $Matches[1].ToUpperInvariant()) { throw "checksum_mismatch" }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($archiveFile)
    try {
        $allowed = @{
            "llmgw.exe" = $true
            "README.md" = $true
            "docs/installation.md" = $true
            "docs/runtime-contract.md" = $true
            "docs/client-compatibility.md" = $true
        }
        $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
        $binaryEntry = $null
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.Replace('\', '/')
            if (-not $seen.Add($name)) { throw "duplicate_archive_entry: $name" }
            if (-not $allowed.ContainsKey($name)) { throw "unexpected_archive_entry: $name" }
            if ($name.EndsWith('/') -or [string]::IsNullOrEmpty($entry.Name)) { throw "non_file_archive_entry: $name" }
            $unixType = (($entry.ExternalAttributes -shr 16) -band 0xF000)
            if ($unixType -ne 0 -and $unixType -ne 0x8000) { throw "archive_link_not_allowed: $name" }
            if ($name -ceq "llmgw.exe") { $binaryEntry = $entry }
        }
        if ($null -eq $binaryEntry) { throw "archive_missing_llmgw_exe" }
        $staged = Join-Path $work "llmgw.exe"
        $input = $binaryEntry.Open()
        try {
            $output = [System.IO.File]::Open($staged, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
            try { $input.CopyTo($output) } finally { $output.Dispose() }
        } finally { $input.Dispose() }
        if ((Get-Item -LiteralPath $staged).Length -eq 0) { throw "empty_llmgw_exe" }
    } finally { $zip.Dispose() }

    [System.IO.Directory]::CreateDirectory($InstallDir) | Out-Null
    $target = Join-Path $InstallDir "llmgw.exe"
    if (Test-Path -LiteralPath $target) {
        $existing = Get-Item -LiteralPath $target -Force
        if (-not $existing.PSIsContainer -and ($existing.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
            throw "refusing_reparse_point_destination"
        }
        if ($existing.PSIsContainer) { throw "refusing_non_file_destination" }
    }
    $candidate = Join-Path $InstallDir (".llmgw-install-" + [Guid]::NewGuid().ToString("N"))
    [System.IO.File]::Copy($staged, $candidate, $false)
    $stagedHash = (Get-FileHash -LiteralPath $staged -Algorithm SHA256).Hash
    if ([System.IO.File]::Exists($target)) {
        $backup = Join-Path $InstallDir (".llmgw-backup-" + [Guid]::NewGuid().ToString("N"))
        $preserveRecovery = $true
        try {
            [System.IO.File]::Replace($candidate, $target, $backup, $true)
            if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -cne $stagedHash) {
                throw "installed_hash_mismatch"
            }
            $candidate = $null
            Remove-Item -LiteralPath $backup -Force
            $backup = $null
            $preserveRecovery = $false
        } catch {
            $replaceError = $_.Exception.Message
            throw "replace_failed_recovery_unconfirmed: $replaceError; target=$target; backup=$backup; candidate=$candidate"
        }
    } else {
        $preserveRecovery = $true
        try {
            [System.IO.File]::Move($candidate, $target)
            if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -cne $stagedHash) {
                throw "installed_hash_mismatch"
            }
        } catch {
            throw "new_install_failed_recovery_candidate_preserved: $($_.Exception.Message); target=$target; candidate=$candidate"
        }
        $candidate = $null
        $preserveRecovery = $false
    }
    Write-Output "Installed verified llmgw at $target"

    if ($PathAction -eq "Preview") {
        $effective = (($env:PATH -split ';') -contains $InstallDir)
        Write-Output "PATH effective in this process: $effective"
        Write-Output "PATH preview only; the installer does not write the user PATH registry."
        Write-Output "Add this directory to the user PATH: $InstallDir"
        Write-Output "Open a new PowerShell window, then run: llmgw setup"
    }
} finally {
    if (-not $preserveRecovery -and $null -ne $candidate) {
        Remove-Item -LiteralPath $candidate -Force -ErrorAction SilentlyContinue
    }
    if (-not $preserveRecovery -and $null -ne $backup) {
        Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
