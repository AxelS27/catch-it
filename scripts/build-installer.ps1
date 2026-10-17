param(
    [string]$Iscc = "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    [string]$SignThumbprint = ''
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not (Test-Path $Iscc)) {
    $Iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
}
if (-not (Test-Path $Iscc)) { throw 'Install Inno Setup 6 or pass -Iscc <path to ISCC.exe>.' }
Push-Location $root
try {
    # Separate output avoids locking a developer's currently running release exe.
    $env:CARGO_TARGET_DIR = Join-Path $root 'target\package-build'
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed.' }
    $binary = Join-Path $env:CARGO_TARGET_DIR 'release\catch-it.exe'
    if ($SignThumbprint) {
        if ($SignThumbprint -notmatch '^[A-Fa-f0-9]{40}$') { throw 'Expected a SHA-1 certificate thumbprint (40 hex characters).' }
        if (-not (Get-Command signtool.exe -ErrorAction SilentlyContinue)) { throw 'SignTool is required for signed releases.' }
        & signtool.exe sign /sha1 $SignThumbprint /fd SHA256 /tr https://timestamp.digicert.com /td SHA256 $binary
        if ($LASTEXITCODE -ne 0) { throw 'Signing app executable failed.' }
    }
    $manifest = Get-Content (Join-Path $root 'Cargo.toml') -Raw
    $version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"').Groups[1].Value
    if (-not $version) { throw 'Cannot read package version from Cargo.toml.' }
    & $Iscc "/DBinaryPath=$binary" "/DAppVersion=$version" (Join-Path $root 'installer\catch-it.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
    $name = "CatchIt-Setup-$version.exe"
    $installer = Join-Path $root "dist\$name"
    if ($SignThumbprint) {
        & signtool.exe sign /sha1 $SignThumbprint /fd SHA256 /tr https://timestamp.digicert.com /td SHA256 $installer
        if ($LASTEXITCODE -ne 0) { throw 'Signing installer failed.' }
    }
    $hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$installer.sha256", "$hash  $name`n", [Text.Encoding]::ASCII)
    Write-Host "Upload dist/$name and dist/$name.sha256 to GitHub Release v$version"
} finally {
    Pop-Location
}
