#Requires -Version 7.0

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference = $false

$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) "wdk-lockfile-tests-$([guid]::NewGuid())"
$repository = Join-Path $testRoot 'consumer checkout'
$actionCopy = Join-Path $testRoot 'copied action'
$script:passed = 0
$originalOffline = $env:CARGO_NET_OFFLINE

function Write-Package {
    param([string]$Directory, [string]$Name, [string]$Extra = '[workspace]')

    $null = New-Item -ItemType Directory -Path $Directory -Force
    $manifest = @'
[package]
name = "{0}"
version = "0.1.0"
edition = "2024"

[lib]
path = "lib.rs"

{1}
'@ -f $Name, $Extra
    [System.IO.File]::WriteAllText((Join-Path $Directory 'Cargo.toml'), $manifest)
    [System.IO.File]::WriteAllText((Join-Path $Directory 'lib.rs'), '')
}

function Invoke-Checker {
    param([string]$Path = $repository)

    $output = & pwsh -NoProfile -NonInteractive -File (Join-Path $actionCopy 'check-lockfiles.ps1') -RepositoryPath $Path 2>&1
    [pscustomobject]@{ ExitCode = $LASTEXITCODE; Output = $output | Out-String }
}

function Assert-Check {
    param([string]$Name, $Result, [bool]$Success, [string]$ExpectedOutput)

    if (($Result.ExitCode -eq 0) -ne $Success -or $Result.Output -notmatch $ExpectedOutput) {
        throw "$Name failed (exit $($Result.ExitCode)):`n$($Result.Output)"
    }
    $script:passed++
    Write-Host "PASS: $Name"
}

function New-Lockfile {
    param([string]$Directory)

    Push-Location -LiteralPath $Directory
    try {
        cargo +stable generate-lockfile --offline
        if ($LASTEXITCODE -ne 0) {
            throw "Could not generate the fixture lockfile in $Directory."
        }
    } finally {
        Pop-Location
    }
}

try {
    $env:CARGO_NET_OFFLINE = 'true'
    Write-Package $repository 'root-fixture' "[workspace]`nmembers = [`"member`"]"
    Write-Package (Join-Path $repository 'member') 'member-fixture' ''
    $nested = Join-Path $repository 'nested driver'
    Write-Package $nested 'nested-fixture' "[workspace]`n`n[dependencies]`nwdk-lockfile-fixture-dependency = `"0.1.0`""
    Write-Package (Join-Path $nested 'patch-dep') 'wdk-lockfile-fixture-dependency' ''
    $null = New-Item -ItemType Directory -Path (Join-Path $nested '.cargo')
    [System.IO.File]::WriteAllText((Join-Path $nested '.cargo/config.toml'), @'
[patch.crates-io]
wdk-lockfile-fixture-dependency = { path = "patch-dep" }
'@)

    Copy-Item -LiteralPath $PSScriptRoot -Destination $actionCopy -Recurse
    git -C $repository init --quiet
    if ($LASTEXITCODE -ne 0) { throw 'Could not initialize the fixture repository.' }
    New-Lockfile $repository
    New-Lockfile $nested
    git -C $repository -c core.autocrlf=false add --all
    if ($LASTEXITCODE -ne 0) { throw 'Could not track the fixture files.' }

    Assert-Check 'copied action checks the consumer, including local Cargo config and spaces' (Invoke-Checker) $true 'Lockfile checks: 2 passed; 0 failed; 2 total'

    $rootManifest = Join-Path $repository 'Cargo.toml'
    $nestedManifest = Join-Path $nested 'Cargo.toml'
    $rootLockfile = Join-Path $repository 'Cargo.lock'
    $nestedLockfile = Join-Path $nested 'Cargo.lock'
    $rootContent = [System.IO.File]::ReadAllText($rootManifest).Replace('version = "0.1.0"', 'version = "0.2.0"')
    [System.IO.File]::WriteAllText($rootManifest, $rootContent)
    New-Lockfile $repository
    Assert-Check 'valid uncommitted lockfile update' (Invoke-Checker) $true 'Lockfile checks: 2 passed; 0 failed'

    $nestedContent = [System.IO.File]::ReadAllText($nestedManifest)
    $before = @(Get-FileHash -LiteralPath $rootLockfile, $nestedLockfile | Select-Object -ExpandProperty Hash)
    [System.IO.File]::WriteAllText($rootManifest, $rootContent.Replace('version = "0.2.0"', 'version = "0.3.0"'))
    [System.IO.File]::WriteAllText($nestedManifest, $nestedContent.Replace('version = "0.1.0"', 'version = "0.2.0"'))
    Assert-Check 'reports both stale graphs instead of stopping at the first' (Invoke-Checker) $false 'Lockfile checks: 0 passed; 2 failed; 2 total'
    $after = @(Get-FileHash -LiteralPath $rootLockfile, $nestedLockfile | Select-Object -ExpandProperty Hash)
    if (Compare-Object $before $after) { throw 'Checking stale lockfiles modified them.' }
    [System.IO.File]::WriteAllText($rootManifest, $rootContent)
    [System.IO.File]::WriteAllText($nestedManifest, $nestedContent)

    foreach ($path in @($rootLockfile, $nestedLockfile, $nestedManifest)) {
        $content = [System.IO.File]::ReadAllBytes($path)
        Remove-Item -LiteralPath $path
        Assert-Check "missing tracked file: $([System.IO.Path]::GetRelativePath($repository, $path))" (Invoke-Checker) $false 'Lockfile checks: 1 passed; 1 failed; 2 total'
        [System.IO.File]::WriteAllBytes($path, $content)
    }

    git -C $repository rm --cached --force --quiet -- Cargo.lock
    if ($LASTEXITCODE -ne 0) { throw 'Could not untrack the root fixture lockfile.' }
    Assert-Check 'root lockfile must be tracked' (Invoke-Checker) $false 'repository root Cargo.lock must be tracked'

    $empty = Join-Path $testRoot 'empty repository'
    $null = New-Item -ItemType Directory -Path $empty
    git -C $empty init --quiet
    if ($LASTEXITCODE -ne 0) { throw 'Could not initialize the empty fixture repository.' }
    Assert-Check 'empty inventory fails' (Invoke-Checker $empty) $false 'No tracked Cargo.lock files'
    Assert-Check 'Git failure is not treated as an empty successful check' (Invoke-Checker $testRoot) $false 'Failed to locate the repository checkout'

    Write-Host "Lockfile checker tests: $script:passed passed; 0 failed."
} finally {
    $env:CARGO_NET_OFFLINE = $originalOffline
    if (Test-Path -LiteralPath $testRoot) {
        Remove-Item -LiteralPath $testRoot -Recurse -Force
    }
}

# Expected failing child processes must not set the suite's exit status in GitHub Actions.
exit 0
