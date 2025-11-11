#!/usr/bin/env pwsh
# Copyright (c) Microsoft Corporation
# License: MIT OR Apache-2.0

# This script verifies that the cargo-wdk template Cargo.toml files use the correct
# versions of WDK crates by reading from each crate's [package] section.

$ErrorActionPreference = "Stop"

# Function to extract version from a Cargo.toml [package] section
function Get-PackageVersion {
    param (
        [string]$CargoTomlPath
    )
    
    if (-not (Test-Path $CargoTomlPath)) {
        Write-Error "Cargo.toml not found at: $CargoTomlPath"
        exit 1
    }
    
    $content = Get-Content $CargoTomlPath -Raw
    
    # Match version in [package] section
    if ($content -match '(?ms)\[package\].*?version\s*=\s*"([^"]+)"') {
        return $Matches[1]
    }
    
    Write-Error "Could not find version in [package] section of $CargoTomlPath"
    exit 1
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

Write-Host "Checking template versions against crate package versions..." -ForegroundColor Cyan

# Get actual crate versions from [package] sections
$wdkVersion = Get-PackageVersion "crates/wdk/Cargo.toml"
$wdkAllocVersion = Get-PackageVersion "crates/wdk-alloc/Cargo.toml"
$wdkBuildVersion = Get-PackageVersion "crates/wdk-build/Cargo.toml"
$wdkPanicVersion = Get-PackageVersion "crates/wdk-panic/Cargo.toml"
$wdkSysVersion = Get-PackageVersion "crates/wdk-sys/Cargo.toml"

Write-Host ""
Write-Host "Current crate versions:" -ForegroundColor Green
Write-Host "  wdk: $wdkVersion"
Write-Host "  wdk-alloc: $wdkAllocVersion"
Write-Host "  wdk-build: $wdkBuildVersion"
Write-Host "  wdk-panic: $wdkPanicVersion"
Write-Host "  wdk-sys: $wdkSysVersion"
Write-Host ""

$expectedVersions = @{
    "wdk" = $wdkVersion
    "wdk-alloc" = $wdkAllocVersion
    "wdk-build" = $wdkBuildVersion
    "wdk-panic" = $wdkPanicVersion
    "wdk-sys" = $wdkSysVersion
}

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
