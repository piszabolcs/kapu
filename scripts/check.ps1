# Local equivalent of the CI quality gates (TERV.md section 6, NFR-03).
# Usage: powershell -ExecutionPolicy Bypass -File scripts/check.ps1
$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$steps = @(
    @{ Name = "fmt";    Args = @("fmt", "--all", "--", "--check") },
    @{ Name = "clippy"; Args = @("clippy", "--workspace", "--all-targets", "--", "-D", "warnings") },
    @{ Name = "test";   Args = @("test", "--workspace") },
    @{ Name = "deny";   Args = @("deny", "check") },
    @{ Name = "audit";  Args = @("audit") }
)

foreach ($step in $steps) {
    Write-Host "==> $($step.Name)"
    & cargo @($step.Args)
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $($step.Name) (exit $LASTEXITCODE)"
        exit $LASTEXITCODE
    }
}
Write-Host "All checks passed."
exit 0
