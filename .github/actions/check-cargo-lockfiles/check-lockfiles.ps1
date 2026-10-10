#Requires -Version 7.0

param(
    [string]$RepositoryPath = '.'
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

    $failed = [System.Collections.Generic.List[string]]::new()
    foreach ($lockfile in $lockfiles) {
        $manifest = [System.IO.Path]::ChangeExtension($lockfile, 'toml')
        if (-not (Test-Path -LiteralPath $lockfile -PathType Leaf) -or -not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
            Write-Error "Missing tracked lockfile or its Cargo.toml: $lockfile." -ErrorAction Continue
            $failed.Add($lockfile)
            continue
        }

        Write-Host "Checking $lockfile"
        $directory = Split-Path -Parent (Join-Path $root $lockfile)
        Push-Location -LiteralPath $directory
        try {
            # Compare with the input file, not the index, so valid uncommitted updates can be checked locally.
            $before = (Get-FileHash -LiteralPath Cargo.lock -Algorithm SHA256).Hash
            cargo +stable metadata --locked --all-features --format-version 1 > $null
            $metadataExitCode = $LASTEXITCODE
            $after = (Get-FileHash -LiteralPath Cargo.lock -Algorithm SHA256).Hash

            if ($metadataExitCode -ne 0 -or $before -ne $after) {
                $failed.Add($lockfile)
                if ($metadataExitCode -ne 0) {
                    Write-Error "Lockfile validation failed for $lockfile. Refresh it locally and commit the result." -ErrorAction Continue
                }
                if ($before -ne $after) {
                    Write-Error "Lockfile validation modified $lockfile." -ErrorAction Continue
                }
            } else {
                Write-Host "PASS: $lockfile"
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
