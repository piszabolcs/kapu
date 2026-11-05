# Portable package (NFR-06, M9): binaries + model folder, no installation.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/package.ps1
# Output: dist/kapu-<version>-windows-x64/ and a .zip of it: kapu, kapu-mcp, kapu-gui, kapu-create,
# the model folder and the example files examples/kapu-docs.kapu and kapu-spec.kapu.
# KAPU_PDFIUM_DLL: the pdfium.dll to put next to kapu-create (bblanchon/pdfium-binaries, KD4); without
# it PDFs are read by the plain text fallback. The release workflow downloads and verifies it.
# Needs models/multilingual-e5-small-q8 (tools/onnx_to_kapu.py makes it from the
# downloaded int8 ONNX file, D172).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$name = "kapu-$version-windows-x64"
$model = "models\multilingual-e5-small-q8"
if (-not (Test-Path "$model\model.safetensors")) { throw "missing $model (run tools/onnx_to_kapu.py)" }

& cargo build --release -p kapu-cli -p kapu-mcp -p kapu-gui -p kapu-create --bins
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$bin = if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR "release" } else { "target\release" }
$out = "dist\$name"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force "$out\models\multilingual-e5-small-q8" | Out-Null
Copy-Item "$bin\kapu.exe", "$bin\kapu-mcp.exe", "$bin\kapu-gui.exe", "$bin\kapu-create.exe" $out
if ($env:KAPU_PDFIUM_DLL) { Copy-Item $env:KAPU_PDFIUM_DLL $out }
New-Item -ItemType Directory -Force "$out\examples" | Out-Null
Copy-Item examples\kapu-spec.kapu, examples\kapu-docs.kapu "$out\examples"
Copy-Item "$model\model.safetensors", "$model\tokenizer.json", "$model\config.json", "$model\README.md" "$out\models\multilingual-e5-small-q8"
Copy-Item LICENSE, README.md, SPEC-KAPU.md, SPEC-KAPU.hu.md, SPEC-KAPU-CREATE.md, THIRD-PARTY.md $out
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
    & .\kapu-create.exe new "$tmp\create.kapu" README.md --no-model --no-cache | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "kapu-create smoke test failed" }
    & .\kapu.exe validate --full "$tmp\create.kapu" | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "kapu-create file check failed" }
    & .\kapu.exe validate --full examples\kapu-spec.kapu | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "demo file check failed" }
} finally { Pop-Location }

$zip = "dist\$name.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $out -DestinationPath $zip
$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
Write-Host "package: $zip ($size MB)"
