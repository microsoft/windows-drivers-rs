#!/usr/bin/env pwsh
# Copyright (c) Microsoft Corporation
# License: MIT OR Apache-2.0

# This script verifies that the cargo-wdk template Cargo.toml files use the correct
# versions of WDK crates by reading from the workspace Cargo.toml [workspace.dependencies] section.

$ErrorActionPreference = "Stop"

# Function to extract versions from workspace Cargo.toml [workspace.dependencies] section
function Get-WorkspaceDependencyVersions {
    param (
        [string]$WorkspaceCargoToml = "Cargo.toml"
    )
    
    if (-not (Test-Path $WorkspaceCargoToml)) {
        Write-Error "Workspace Cargo.toml not found at: $WorkspaceCargoToml"
        exit 1
    }
    
    $content = Get-Content $WorkspaceCargoToml -Raw
    $versions = @{}
    
    # Match versions in [workspace.dependencies] section
    # Pattern matches: wdk = { path = "...", version = "0.4.0" }
    # or: wdk-alloc = { path = "...", version = "0.4.0" }
    $pattern = '(wdk[a-z-]*)\s*=\s*\{[^}]*version\s*=\s*"([^"]+)"'
    $matches = [regex]::Matches($content, $pattern)
    
    foreach ($match in $matches) {
        $depName = $match.Groups[1].Value
        $depVersion = $match.Groups[2].Value
        $versions[$depName] = $depVersion
    }
    
    if ($versions.Count -eq 0) {
        Write-Error "Could not find any wdk dependency versions in [workspace.dependencies] section"
        exit 1
    }
    
    return $versions
}

# Function to extract dependency versions from a template file
function Get-TemplateDependencies {
    param (
        [string]$TemplatePath
    )
    
    if (-not (Test-Path $TemplatePath)) {
        Write-Error "Template not found at: $TemplatePath"
        exit 1
    }
    
    $content = Get-Content $TemplatePath -Raw
    $dependencies = @{}
    
    # Extract wdk-* dependencies and their versions
    $pattern = '(wdk[a-z-]*)\s*=\s*"([^"]+)"'
    $matches = [regex]::Matches($content, $pattern)
    
    foreach ($match in $matches) {
        $depName = $match.Groups[1].Value
        $depVersion = $match.Groups[2].Value
        $dependencies[$depName] = $depVersion
    }
    
    return $dependencies
}

Write-Host "Checking template versions against workspace dependency versions..." -ForegroundColor Cyan

# Get versions from workspace [workspace.dependencies] section
$expectedVersions = Get-WorkspaceDependencyVersions

Write-Host ""
Write-Host "Workspace dependency versions:" -ForegroundColor Green
foreach ($dep in $expectedVersions.Keys | Sort-Object) {
    Write-Host "  $dep : $($expectedVersions[$dep])"
}
Write-Host ""

$templates = @(
    @{
        Name = "KMDF"
        Path = "crates/cargo-wdk/templates/kmdf/Cargo.toml.tmp"
        ExpectedDeps = @("wdk", "wdk-alloc", "wdk-build", "wdk-panic", "wdk-sys")
    },
    @{
        Name = "UMDF"
        Path = "crates/cargo-wdk/templates/umdf/Cargo.toml.tmp"
        ExpectedDeps = @("wdk", "wdk-build", "wdk-sys")
    },
    @{
        Name = "WDM"
        Path = "crates/cargo-wdk/templates/wdm/Cargo.toml.tmp"
        ExpectedDeps = @("wdk", "wdk-alloc", "wdk-build", "wdk-panic", "wdk-sys")
    }
)

$hasErrors = $false

foreach ($template in $templates) {
    Write-Host "Checking $($template.Name) template..." -ForegroundColor Cyan
    
    $templateDeps = Get-TemplateDependencies $template.Path
    
    foreach ($depName in $template.ExpectedDeps) {
        if (-not $templateDeps.ContainsKey($depName)) {
            Write-Host "  ERROR: $depName is missing from template" -ForegroundColor Red
            $hasErrors = $true
            continue
        }
        
        $templateVersion = $templateDeps[$depName]
        $expectedVersion = $expectedVersions[$depName]
        
        if ($templateVersion -ne $expectedVersion) {
            Write-Host "  ERROR: $depName version mismatch" -ForegroundColor Red
            Write-Host "    Template has: $templateVersion" -ForegroundColor Red
            Write-Host "    Expected:     $expectedVersion" -ForegroundColor Red
            $hasErrors = $true
        } else {
            Write-Host "  OK: $depName = $templateVersion" -ForegroundColor Green
        }
    }
    
    Write-Host ""
}

if ($hasErrors) {
    Write-Host "Template version check FAILED!" -ForegroundColor Red
    exit 1
} else {
    Write-Host "All template versions are correct!" -ForegroundColor Green
    exit 0
}
