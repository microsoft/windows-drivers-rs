#Requires -Version 7.0

[CmdletBinding(DefaultParameterSetName = 'Check')]
param(
    [string]$RepositoryPath = '.',
    [Parameter(Mandatory, ParameterSetName = 'List')]
    [switch]$List,
    [Parameter(ParameterSetName = 'Check')]
    [string]$Lockfile = '',
    [Parameter(ParameterSetName = 'Check')]
    [ValidateNotNullOrEmpty()]
    [string]$Toolchain = 'stable'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference = $false

Push-Location -LiteralPath $RepositoryPath
try {
    $root = git rev-parse --show-toplevel
    if ($LASTEXITCODE -ne 0) {
        throw 'Failed to locate the repository checkout.'
    }
    Set-Location -LiteralPath $root

    $lockfiles = @(git -c core.quotePath=false ls-files -- 'Cargo.lock' '**/Cargo.lock')
    if ($LASTEXITCODE -ne 0) {
        throw 'Failed to list tracked Cargo.lock files.'
    }
    if ($lockfiles.Count -eq 0) {
        throw 'No tracked Cargo.lock files found.'
    }
    if ('Cargo.lock' -notin $lockfiles) {
        throw 'The repository root Cargo.lock must be tracked.'
    }

    if ($List) {
        ConvertTo-Json -InputObject $lockfiles -Compress
        return
    }
    if ($Lockfile) {
        $selectedLockfile = $Lockfile.Replace('\', '/')
        if ($selectedLockfile -cnotin $lockfiles) {
            throw "The selected lockfile is not a tracked Cargo.lock: $Lockfile."
        }
        $lockfiles = @($selectedLockfile)
    }

    $failed = [System.Collections.Generic.List[string]]::new()
    foreach ($lockfilePath in $lockfiles) {
        $manifest = [System.IO.Path]::ChangeExtension($lockfilePath, 'toml')
        if (-not (Test-Path -LiteralPath $lockfilePath -PathType Leaf) -or -not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
            Write-Error "Missing tracked lockfile or its Cargo.toml: $lockfilePath." -ErrorAction Continue
            $failed.Add($lockfilePath)
            continue
        }

        Write-Host "Checking $lockfilePath [$Toolchain]"
        $directory = Split-Path -Parent (Join-Path $root $lockfilePath)
        Push-Location -LiteralPath $directory
        try {
            # Compare with the input file, not the index, so valid uncommitted updates can be checked locally.
            $before = (Get-FileHash -LiteralPath Cargo.lock -Algorithm SHA256).Hash
            cargo "+$Toolchain" metadata --locked --all-features --format-version 1 > $null
            $metadataExitCode = $LASTEXITCODE
            $after = (Get-FileHash -LiteralPath Cargo.lock -Algorithm SHA256).Hash

            if ($metadataExitCode -ne 0 -or $before -ne $after) {
                $failed.Add($lockfilePath)
                if ($metadataExitCode -ne 0) {
                    Write-Error "Lockfile validation failed for $lockfilePath [$Toolchain]. Refresh it locally and commit the result." -ErrorAction Continue
                }
                if ($before -ne $after) {
                    Write-Error "Lockfile validation modified $lockfilePath." -ErrorAction Continue
                }
            } else {
                Write-Host "PASS: $lockfilePath [$Toolchain]"
            }
        } finally {
            Pop-Location
        }
    }

    Write-Host "Lockfile checks: $($lockfiles.Count - $failed.Count) passed; $($failed.Count) failed; $($lockfiles.Count) total."
    if ($failed.Count -ne 0) {
        throw "Lockfile consistency checks failed: $($failed -join ', ')."
    }
} finally {
    Pop-Location
}
