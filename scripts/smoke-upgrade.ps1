param(
    [string]$OldTag = 'v0.1.0',
    [string]$OldVersion = '0.1.0',
    [string]$Iscc = "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not (Test-Path $Iscc)) { $Iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe' }
if (-not (Test-Path $Iscc)) { throw 'Inno Setup 6 is required.' }
$manifest = Get-Content (Join-Path $root 'Cargo.toml') -Raw
$newVersion = [regex]::Match($manifest, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"').Groups[1].Value
if (-not $newVersion -or $newVersion -eq $OldVersion) { throw 'Build a newer package before testing upgrades.' }
$newBinary = Join-Path $root 'target\package-build\release\catch-it.exe'
if (-not (Test-Path $newBinary)) { throw 'Run scripts/build-installer.ps1 first.' }

# Each run owns its AppId, uninstall registry entry, and scratch folder. Neither
# installer contains startup shortcuts or a post-install app launch.
$guid = [guid]::NewGuid().ToString('D').ToUpperInvariant()
$base = Join-Path $root ".pi\upgrade-smoke-runs\$guid"
$source = Join-Path $base 'old-src'
$target = Join-Path $base 'old-target'
$packages = Join-Path $base 'packages'
$collisionPackages = Join-Path $base 'collision-packages'
$install = Join-Path $base 'install'
$reg = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{' + $guid + '}_is1'
$finished = $false
New-Item -ItemType Directory -Force -Path $source, $target, $packages, $collisionPackages | Out-Null

function Assert($condition, [string]$message) {
    if (-not $condition) { throw $message }
}
function Hash([string]$file) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $stream = [IO.File]::OpenRead($file)
        try { return ([BitConverter]::ToString($sha.ComputeHash($stream)) -replace '-','') }
        finally { $stream.Dispose() }
    } finally { $sha.Dispose() }
}
function Compile-Installer([string]$version, [string]$binary, [string]$output, [bool]$collision) {
    $args = @('/Q', "/O$output", '/DUpgradeSmoke', "/DPackageAppId={{$guid}",
        "/DAppVersion=$version", "/DBinaryPath=$binary")
    if ($collision) { $args += '/DInjectFileCollision' }
    $args += (Join-Path $root 'installer\catch-it.iss')
    & $Iscc $args
    if ($LASTEXITCODE -ne 0) { throw "Test installer compilation failed: $version" }
    return (Join-Path $output "CatchIt-Setup-$version.exe")
}
function Run-Setup([string]$exe, [string]$label) {
    $log = Join-Path $base "$label.log"
    $args = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOICONS', '/TASKS=',
        "/DIR=`"$install`"", "/LOG=`"$log`"")
    $process = Start-Process -FilePath $exe -ArgumentList $args -PassThru -Wait
    return $process.ExitCode
}

try {
    $archive = Join-Path $base 'old.tar'
    & git -C $root archive --format=tar "--output=$archive" $OldTag
    if ($LASTEXITCODE -ne 0) { throw "Cannot archive old tag $OldTag" }
    & tar -xf $archive -C $source
    if ($LASTEXITCODE -ne 0) { throw 'Cannot unpack old source.' }
    $previousTarget = $env:CARGO_TARGET_DIR
    try {
        $env:CARGO_TARGET_DIR = $target
        & cargo build --release --locked --manifest-path (Join-Path $source 'Cargo.toml')
        if ($LASTEXITCODE -ne 0) { throw 'Old release build failed.' }
    } finally { $env:CARGO_TARGET_DIR = $previousTarget }
    $oldBinary = Join-Path $target 'release\catch-it.exe'
    $oldSetup = Compile-Installer $OldVersion $oldBinary $packages $false
    $newSetup = Compile-Installer $newVersion $newBinary $packages $false
    $failedSetup = Compile-Installer $newVersion $newBinary $collisionPackages $true

    Assert ((Run-Setup $oldSetup 'install-old') -eq 0) 'Old version did not install.'
    $files = @('catch-it.exe', 'assets\catch-it-logo.png', 'assets\catch-it.ico',
        'assets\google-signin-button.bmp', 'install-layout.txt')
    $oldHashes = @{}
    foreach ($file in $files) { $oldHashes[$file] = Hash (Join-Path $install $file) }
    Assert ((Hash (Join-Path $install 'catch-it.exe')) -eq (Hash $oldBinary)) 'Old binary mismatch.'
    Assert ((Get-ItemProperty $reg).DisplayVersion -eq $OldVersion) 'Wrong old uninstall version.'
    $screenshot = Join-Path $install 'Temp\shot_1700000000000_999999_3.png'
    $unrelated = Join-Path $install 'Temp\custom-note.txt'
    [IO.File]::WriteAllBytes($screenshot, [byte[]](20, 21, 22))
    [IO.File]::WriteAllText($unrelated, 'keep')

    # Copying the exe succeeds; a later test-only file collides with a directory.
    $collision = Join-Path $install 'test-failure-hook.txt'
    New-Item -ItemType Directory -Path $collision | Out-Null
    Assert ((Run-Setup $failedSetup 'failed-mid-upgrade') -ne 0) 'Injected failure reported success.'
    foreach ($file in $files) {
        Assert ((Hash (Join-Path $install $file)) -eq $oldHashes[$file]) "Rollback changed $file"
    }
    Assert ((Get-ItemProperty $reg).DisplayVersion -eq $OldVersion) 'Failed upgrade changed registration.'
    Assert ((Test-Path $screenshot) -and (Test-Path $unrelated)) 'Failed upgrade lost user data.'
    Assert (-not (Test-Path (Join-Path $install '.catch-it-upgrade-backup'))) 'Rollback left backup files.'
    Remove-Item -LiteralPath $collision -Recurse -Force

    # Simulate a process/power interruption after the backup was completed and
    # the new executable was copied. The next installer must restore first.
    $backup = Join-Path $install '.catch-it-upgrade-backup'
    New-Item -ItemType Directory -Force (Join-Path $backup 'assets') | Out-Null
    foreach ($file in $files) { Copy-Item -LiteralPath (Join-Path $install $file) -Destination (Join-Path $backup $file) }
    [IO.File]::WriteAllText((Join-Path $backup 'in-progress.txt'), 'backup')
    [IO.File]::WriteAllText((Join-Path $backup 'ready.txt'), 'ready')
    Copy-Item -LiteralPath $newBinary -Destination (Join-Path $install 'catch-it.exe') -Force
    Assert ((Run-Setup $newSetup 'resume-after-interruption') -eq 0) 'Recovery upgrade failed.'
    Assert ((Hash (Join-Path $install 'catch-it.exe')) -eq (Hash $newBinary)) 'New binary mismatch.'
    Assert ((Get-ItemProperty $reg).DisplayVersion -eq $newVersion) 'Wrong upgraded version.'
    Assert (-not (Test-Path $backup)) 'Successful upgrade retained backup.'
    Assert ((Test-Path $screenshot) -and (Test-Path $unrelated)) 'Upgrade lost user data.'
    Assert (Select-String -LiteralPath (Join-Path $base 'resume-after-interruption.log') -Pattern 'Recovered interrupted Catch It upgrade' -Quiet) 'Interrupted upgrade was not recovered.'

    $uninstaller = Join-Path $install 'unins000.exe'
    $uninstall = Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') -PassThru -Wait
    Assert ($uninstall.ExitCode -eq 0) 'Test uninstall failed.'
    Assert (-not (Test-Path $screenshot)) 'Uninstall retained app-generated screenshot.'
    Assert (Test-Path $unrelated) 'Uninstall removed unrelated file.'
    Assert (-not (Test-Path (Join-Path $install 'catch-it.exe'))) 'Uninstall retained app executable.'
    Assert (-not (Test-Path $reg)) 'Test uninstall registration survived.'
    Write-Host "PASS: isolated $OldVersion -> $newVersion upgrade, mid-copy rollback, interrupted recovery, uninstall"
    $finished = $true
} finally {
    # Never touch the real AppId or install path, even when a test assertion fails.
    $uninstaller = Join-Path $install 'unins000.exe'
    if (Test-Path $uninstaller) {
        $process = Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') -PassThru -Wait
        if ($process.ExitCode -ne 0) { Write-Warning "Test uninstall failed ($($process.ExitCode)); inspect $base" }
    }
    if (Test-Path $reg) {
        Write-Warning "Test AppId remains registered; inspect $base and uninstall $reg manually."
    } else {
        if ($finished) { Remove-Item -LiteralPath $base -Recurse -Force }
        else { Write-Warning "Upgrade test failed; isolated logs retained under $base" }
    }
}
