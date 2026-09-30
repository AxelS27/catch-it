param(
    [switch]$Scene,
    [ValidateSet('debug', 'release')][string]$Configuration = 'debug'
)

$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class CaptureInput {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int index);
    public static void MouseAt(int x, int y, uint buttons) {
        uint dx = (uint)(((long)(x - GetSystemMetrics(76)) * 65536 + 32768) / GetSystemMetrics(78));
        uint dy = (uint)(((long)(y - GetSystemMetrics(77)) * 65536 + 32768) / GetSystemMetrics(79));
        mouse_event(0xC001 | buttons, dx, dy, 0, UIntPtr.Zero);
    }
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern IntPtr FindWindow(string cls, string title);
    public static IntPtr SceneWindow() { return FindWindow(null, "SimpleScreenshot E2E Scene"); }
    public static IntPtr OverlayWindow() { return FindWindow("SimpleScreenshot.Selection", null); }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam);
}
'@
[void][CaptureInput]::SetProcessDpiAwarenessContext([IntPtr](-4))
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

if ($Scene) {
    $form = New-Object System.Windows.Forms.Form
    $form.Text = 'SimpleScreenshot E2E Scene'
    $form.FormBorderStyle = 'None'
    $form.StartPosition = 'Manual'
    $form.AutoScaleMode = 'None'
    $form.Location = New-Object System.Drawing.Point(200, 200)
    $form.Size = New-Object System.Drawing.Size(600, 400)
    $form.BackColor = [System.Drawing.Color]::Blue
    $form.TopMost = $true
    foreach ($spec in @(@(50,50,150,100,'Red'), @(200,50,100,100,'Lime'))) {
        $panel = New-Object System.Windows.Forms.Panel
        $panel.Location = New-Object System.Drawing.Point($spec[0], $spec[1])
        $panel.Size = New-Object System.Drawing.Size($spec[2], $spec[3])
        $panel.BackColor = [System.Drawing.Color]::FromName($spec[4])
        $form.Controls.Add($panel)
    }
    [void]$form.ShowDialog()
    exit
}

$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root "target/$Configuration/simple-screenshot.exe"
if (-not (Test-Path $exe)) { throw 'Run cargo build first.' }
$artifacts = Join-Path $root '.pi/capture-smoke'
[void](New-Item -ItemType Directory -Force $artifacts)
$output = Join-Path $env:LOCALAPPDATA 'SimpleScreenshot/Temp'
$created = @()
$app = $null
$sceneProcess = $null
$originalFocus = [CaptureInput]::GetForegroundWindow()

function Press-Key([byte]$Key) {
    [CaptureInput]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [CaptureInput]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
}
function Press-Capture {
    [CaptureInput]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [CaptureInput]::keybd_event(0x10, 0, 0, [UIntPtr]::Zero)
    Press-Key 0x53
    [CaptureInput]::keybd_event(0x10, 0, 2, [UIntPtr]::Zero)
    [CaptureInput]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
}
function Wait-Overlay([bool]$Visible) {
    for ($i = 0; $i -lt 100; $i++) {
        $hwnd = [CaptureInput]::OverlayWindow()
        if (($hwnd -ne [IntPtr]::Zero) -eq $Visible) { return $hwnd }
        Start-Sleep -Milliseconds 50
    }
    throw "Overlay visibility did not become $Visible. Check $artifacts logs."
}
function Start-Selection {
    [void][CaptureInput]::SetForegroundWindow($script:sceneWindow)
    [void][CaptureInput]::SetCursorPos($script:sceneRect.Left + 25, $script:sceneRect.Top + 25)
    Start-Sleep -Milliseconds 100
    [void][CaptureInput]::SetForegroundWindow($script:sceneWindow)
    $script:expectedFocus = [CaptureInput]::GetForegroundWindow()
    Press-Capture
    $window = Wait-Overlay $true
    Start-Sleep -Milliseconds 150
    return $window
}
function Drag-Selection([bool]$Reverse) {
    $x1 = $script:sceneRect.Left + 25
    $y1 = $script:sceneRect.Top + 25
    $x2 = $x1 + 350
    $y2 = $y1 + 200
    if ($Reverse) { $x1, $x2 = $x2, $x1; $y1, $y2 = $y2, $y1 }
    [CaptureInput]::MouseAt($x1, $y1, 2)
    Start-Sleep -Milliseconds 50
    for ($i = 1; $i -le 20; $i++) {
        [CaptureInput]::MouseAt([int]($x1 + ($x2-$x1)*$i/20), [int]($y1 + ($y2-$y1)*$i/20), 0)
        Start-Sleep -Milliseconds 10
    }
    Start-Sleep -Milliseconds 100
    if (-not $Reverse) {
        $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
        $bitmap = New-Object System.Drawing.Bitmap($screen.Width, $screen.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
            $bitmap.Save((Join-Path $artifacts 'overlay.png'))
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
    }
    [CaptureInput]::MouseAt($x2, $y2, 4)
    [void](Wait-Overlay $false)
}
function Get-Shots {
    if (Test-Path $output) { return @(Get-ChildItem $output -Filter '*.png' | Select-Object -ExpandProperty FullName) }
    return @()
}
function Wait-NewShot($Before) {
    for ($i = 0; $i -lt 100; $i++) {
        $new = @(Get-Shots | Where-Object { $_ -notin $Before })
        if ($new.Count -eq 1) { return $new[0] }
        if ($new.Count -gt 1) { throw 'Unexpected extra screenshot files.' }
        Start-Sleep -Milliseconds 50
    }
    throw 'PNG was not saved.'
}
function Assert-Png([string]$Path) {
    $bitmap = New-Object System.Drawing.Bitmap($Path)
    try {
        if ($bitmap.Width -ne 350 -or $bitmap.Height -ne 200) { throw "Wrong PNG size: $($bitmap.Size)" }
        foreach ($sample in @(@(5,170,'Blue'), @(30,30,'Red'), @(200,30,'Lime'))) {
            $actual = $bitmap.GetPixel($sample[0], $sample[1]).ToArgb()
            $expected = [System.Drawing.Color]::FromName($sample[2]).ToArgb()
            if ($actual -ne $expected) { throw "Incorrect PNG pixel at $($sample[0]),$($sample[1]); overlay or coordinates leaked into capture." }
        }
    } finally { $bitmap.Dispose() }
}

try {
    $shell = (Get-Process -Id $PID).Path
    $sceneProcess = Start-Process $shell -ArgumentList @('-NoProfile', '-File', ('"' + $PSCommandPath + '"'), '-Scene') -PassThru -RedirectStandardOutput (Join-Path $artifacts 'scene-stdout.log') -RedirectStandardError (Join-Path $artifacts 'scene-stderr.log')
    for ($i = 0; $i -lt 600; $i++) {
        $sceneProcess.Refresh()
        if ($sceneProcess.HasExited) { throw 'Test scene exited. Check scene-stderr.log.' }
        $sceneWindow = [CaptureInput]::SceneWindow()
        if ($sceneWindow -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 50
    }
    if ($sceneWindow -eq [IntPtr]::Zero) { throw 'Test scene did not open.' }
    $sceneRect = New-Object CaptureInput+Rect
    [void][CaptureInput]::GetWindowRect($sceneWindow, [ref]$sceneRect)
    Write-Host "Display DPI: $([CaptureInput]::GetDpiForWindow($sceneWindow)); scene at $($sceneRect.Left),$($sceneRect.Top)"
    $app = Start-Process $exe -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $artifacts 'stdout.log') -RedirectStandardError (Join-Path $artifacts 'stderr.log')
    Start-Sleep -Milliseconds 800
    if ($app.HasExited) { throw 'Capture application exited during startup.' }

    foreach ($reverse in @($false, $true)) {
        $before = @(Get-Shots)
        [void](Start-Selection)
        Drag-Selection $reverse
        $shot = Wait-NewShot $before
        $created += $shot
        Assert-Png $shot
        if ([CaptureInput]::GetForegroundWindow() -ne $script:expectedFocus) { throw "Focus was not restored after capture (expected=$script:expectedFocus actual=$([CaptureInput]::GetForegroundWindow()))." }
        Write-Host "PASS: capture direction reverse=$reverse; exact 350x200 PNG, original colors, restored focus"
    }

    foreach ($cancel in @('Escape', 'RightClick', 'ZeroArea', 'Deactivate')) {
        $before = @(Get-Shots)
        [void](Start-Selection)
        switch ($cancel) {
            'Escape' {
                [CaptureInput]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
                Press-Key 0x1B
                [CaptureInput]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
            }
            'RightClick' {
                [CaptureInput]::mouse_event(8, 0, 0, 0, [UIntPtr]::Zero)
                [CaptureInput]::mouse_event(16, 0, 0, 0, [UIntPtr]::Zero)
            }
            'ZeroArea' {
                [CaptureInput]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
                [CaptureInput]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
            }
            'Deactivate' { [void][CaptureInput]::SetForegroundWindow($sceneWindow) }
        }
        [void](Wait-Overlay $false)
        Start-Sleep -Milliseconds 200
        if (@(Get-Shots | Where-Object { $_ -notin $before }).Count -ne 0) { throw "$cancel unexpectedly saved a PNG." }
        Write-Host "PASS: $cancel cancels without creating a file"
    }

    [CaptureInput]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    [CaptureInput]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    Press-Key 0x51
    [CaptureInput]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
    [CaptureInput]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    if (-not $app.WaitForExit(5000)) { throw 'Exit shortcut did not stop the application.' }
    if ($app.ExitCode -ne 0) { throw "Application exited with $($app.ExitCode)." }
    Write-Host 'PASS: clean shutdown'
    Write-Host "Visual artifact: $artifacts/overlay.png"
} finally {
    # Release synthetic input even when a test fails.
    [CaptureInput]::mouse_event(4 -bor 16, 0, 0, 0, [UIntPtr]::Zero)
    foreach ($key in @(0x10, 0x11, 0x12)) { [CaptureInput]::keybd_event($key, 0, 2, [UIntPtr]::Zero) }
    if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
    if ($sceneProcess -and -not $sceneProcess.HasExited) { Stop-Process -Id $sceneProcess.Id -Force }
    foreach ($path in $created) { Remove-Item -LiteralPath $path -ErrorAction SilentlyContinue }
    [void][CaptureInput]::SetForegroundWindow($originalFocus)
}
