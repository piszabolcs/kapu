# Runs every cargo-fuzz target for a fixed time (NFR-03/5, TERV.md M2).
# Usage: powershell -ExecutionPolicy Bypass -File scripts/fuzz.ps1 [-Seconds 3600] [-Targets header,docs]
# Windows needs the MSVC AddressSanitizer component (Visual Studio: "C++ AddressSanitizer");
# its runtime DLL is put on PATH here. The seed corpus is the M1 test vectors.
param(
    [int]$Seconds = 120,
    [string[]]$Targets = @("header", "sections", "docs", "catalog", "validate", "reader")
)
$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

$vs = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -property installationPath
if ($vs) {
    $msvc = Get-ChildItem "$vs\VC\Tools\MSVC" | Sort-Object Name -Descending | Select-Object -First 1
    $env:Path = "$($msvc.FullName)\bin\Hostx64\x64;$env:Path"
}

$failed = @()
foreach ($t in $Targets) {
    $corpus = "fuzz\corpus\$t"
    New-Item -ItemType Directory -Force $corpus | Out-Null
    Copy-Item tests\vectors\*.kapu $corpus -Force
    Write-Host "==> $t ($Seconds s)"
    & cargo +nightly fuzz run $t $corpus --debug-assertions -- "-max_total_time=$Seconds" "-print_final_stats=1" 2>&1 |
        Select-String -Pattern "Done|stat::number_of_executed_units|ERROR|panicked|SUMMARY|crash-|deadly" |
        ForEach-Object { Write-Host "    $_" }
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $t (exit $LASTEXITCODE), see fuzz\artifacts\$t"
        $failed += $t
    }
}
if ($failed.Count -gt 0) { Write-Host "Findings in: $($failed -join ', ')"; exit 1 }
Write-Host "No findings."
exit 0
