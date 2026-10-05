param(
    [string]$Iscc = "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
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
    & $Iscc "/DBinaryPath=$binary" (Join-Path $root 'installer\catch-it.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
} finally {
    Pop-Location
}
