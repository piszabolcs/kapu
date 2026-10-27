# Portable package (NFR-06, M9): binaries + model folder, no installation.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/package.ps1
# Output: dist/kapu-<version>-windows-x64/ and a .zip of it: kapu, kapu-mcp, kapu-gui,
# the model folder and the example files examples/kapu-docs.kapu and kapu-spec.kapu.
# Needs models/potion-multilingual-128M-i8 (tools/quantize.py makes it from the
# downloaded f32 model).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$name = "kapu-$version-windows-x64"
$model = "models\potion-multilingual-128M-i8"
if (-not (Test-Path "$model\model.safetensors")) { throw "missing $model (run tools/quantize.py)" }

& cargo build --release -p kapu-cli -p kapu-mcp -p kapu-gui --bins
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$out = "dist\$name"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force "$out\models\potion-multilingual-128M-i8" | Out-Null
Copy-Item target\release\kapu.exe, target\release\kapu-mcp.exe, target\release\kapu-gui.exe $out
New-Item -ItemType Directory -Force "$out\examples" | Out-Null
Copy-Item examples\kapu-spec.kapu, examples\kapu-docs.kapu "$out\examples"
Copy-Item "$model\model.safetensors", "$model\tokenizer.json", "$model\config.json", "$model\README.md" "$out\models\potion-multilingual-128M-i8"
Copy-Item LICENSE, README.md, SPEC-KAPU.md, SPEC-KAPU.hu.md, THIRD-PARTY.md $out
Set-Content -Encoding utf8 "$out\LICENSE-SPEC.txt" "SPEC-KAPU.md and SPEC-KAPU.hu.md are licensed under Creative Commons Attribution 4.0 International (CC BY 4.0): https://creativecommons.org/licenses/by/4.0/`nAuthor and originator: Pinter Szabolcs."

# smoke test of the package, from inside the package folder
Push-Location $out
try {
    $tmp = Join-Path $env:TEMP "kapu-package-smoke"
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    New-Item -ItemType Directory $tmp | Out-Null
    & .\kapu.exe new "$tmp\smoke.kapu" | Out-Host
    & .\kapu.exe validate --full "$tmp\smoke.kapu" | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "smoke test failed" }
    & .\kapu-mcp.exe --version | Out-Host
    & .\kapu.exe validate --full examples\kapu-spec.kapu | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "demo file check failed" }
} finally { Pop-Location }

$zip = "dist\$name.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $out -DestinationPath $zip
$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
Write-Host "package: $zip ($size MB)"
