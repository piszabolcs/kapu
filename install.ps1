# Install KAPU (kapu, kapu-mcp, kapu-gui and the embedding model) from the
# latest GitHub release, for the current user.
#
#   irm https://raw.githubusercontent.com/piszabolcs/kapu/main/install.ps1 | iex
#
# Environment: KAPU_VERSION (a tag such as v0.3.0; default: the latest
# release), KAPU_HOME (default %LOCALAPPDATA%\Programs\KAPU), KAPU_BASE_URL (a
# mirror of the release downloads), KAPU_NO_PATH=1 (leave PATH alone). The archive's
# SHA-256 is checked against the .sha256 file of the release, and the folder
# is added to the user's PATH.
$ErrorActionPreference = "Stop"
$repo = "piszabolcs/kapu"
$dest = if ($env:KAPU_HOME) { $env:KAPU_HOME } else { Join-Path $env:LOCALAPPDATA "Programs\KAPU" }

if (-not [Environment]::Is64BitOperatingSystem) { throw "kapu install: only 64-bit Windows has a prebuilt archive" }

$tag = $env:KAPU_VERSION
if (-not $tag) {
    $tag = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
    if (-not $tag) { throw "kapu install: could not find the latest release of $repo" }
}
$version = $tag.TrimStart("v")
$name = "kapu-$version-windows-x64"
$base = if ($env:KAPU_BASE_URL) { $env:KAPU_BASE_URL } else { "https://github.com/$repo/releases/download" }
$url = "$base/$tag/$name.zip"

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("kapu-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host "Downloading $url"
    Invoke-WebRequest $url -OutFile "$tmp\$name.zip" -UseBasicParsing
    Invoke-WebRequest "$url.sha256" -OutFile "$tmp\$name.zip.sha256" -UseBasicParsing
    $want = ((Get-Content "$tmp\$name.zip.sha256" -Raw).Trim() -split "\s+")[0].ToLower()
    $have = (Get-FileHash "$tmp\$name.zip" -Algorithm SHA256).Hash.ToLower()
    if ($want -ne $have) { throw "kapu install: checksum mismatch for $name.zip" }

    Expand-Archive "$tmp\$name.zip" -DestinationPath $tmp -Force
    New-Item -ItemType Directory -Path $dest -Force | Out-Null
    foreach ($item in Get-ChildItem "$tmp\$name") {
        $target = Join-Path $dest $item.Name
        try {
            Copy-Item $item.FullName $target -Recurse -Force
        } catch {
            # a running kapu-mcp keeps its exe locked: move it aside, then copy
            $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
            Rename-Item $target ("{0}.old-{1}{2}" -f $item.BaseName, $stamp, $item.Extension)
            Copy-Item $item.FullName $target -Recurse -Force
        }
    }
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (-not $env:KAPU_NO_PATH -and -not (($userPath -split ";") -contains $dest)) {
    [Environment]::SetEnvironmentVariable("Path", (($userPath.TrimEnd(";"), $dest) -join ";").TrimStart(";"), "User")
    $env:Path = "$env:Path;$dest"
    Write-Host "Added $dest to your user PATH (new terminals see it)."
}

Write-Host ""
Write-Host "Installed KAPU $version in $dest"
& (Join-Path $dest "kapu.exe") --version | Select-Object -First 1
Write-Host ""
Write-Host "Next:"
Write-Host "  kapu map `"$dest\examples\kapu-docs.kapu`""
Write-Host "  kapu import --skills `"$env:USERPROFILE\.claude\skills`" skills.kapu"
Write-Host "  claude mcp add kapu -- `"$dest\kapu-mcp.exe`""
