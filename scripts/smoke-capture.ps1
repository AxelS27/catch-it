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
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] private static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(Point point);
    public static IntPtr CursorWindow() { Point point; GetCursorPos(out point); return WindowFromPoint(point); }
    public static string CursorInfo() { Point point; GetCursorPos(out point); return point.X + "," + point.Y + " window=" + WindowFromPoint(point); }
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
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
    public static IntPtr ThumbnailWindow() { return FindWindow("SimpleScreenshot.Thumbnail", null); }
    public static int ThumbnailCount() {
        int count = 0; IntPtr window = IntPtr.Zero;
        while ((window = FindWindowEx(IntPtr.Zero, window, "SimpleScreenshot.Thumbnail", null)) != IntPtr.Zero) { count++; }
        return count;
    }
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
function Wait-Thumbnail([bool]$Visible, [int]$TimeoutMs = 6000) {
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    while ($clock.ElapsedMilliseconds -lt $TimeoutMs) {
        $hwnd = [CaptureInput]::ThumbnailWindow()
        if (($hwnd -ne [IntPtr]::Zero) -eq $Visible) { return $hwnd }
        Start-Sleep -Milliseconds 25
    }
    throw "Thumbnail visibility did not become $Visible. Check $artifacts logs."
}
function Assert-Preview([IntPtr]$Window) {
    $rect = New-Object CaptureInput+Rect
    [void][CaptureInput]::GetWindowRect($Window, [ref]$rect)
    $scale = [CaptureInput]::GetDpiForWindow($Window) / 96.0
    $work = [System.Windows.Forms.Screen]::FromHandle($Window).WorkingArea
    if ($rect.Right -gt $work.Right -or $rect.Bottom -gt $work.Bottom -or $rect.Left -lt $work.Left -or $rect.Top -lt $work.Top) { throw 'Preview is outside the monitor work area.' }
    $padding = [int][Math]::Round(14 * $scale, [MidpointRounding]::AwayFromZero)
    $margin = [int][Math]::Round(18 * $scale, [MidpointRounding]::AwayFromZero)
    $cardWidth = $rect.Right - $rect.Left - $padding - $margin
    $cardHeight = $rect.Bottom - $rect.Top - $padding - $margin
    if ([Math]::Abs($cardWidth / $cardHeight - 1.75) -gt 0.015) { throw 'Thumbnail image aspect ratio changed.' }
    $bitmap = New-Object System.Drawing.Bitmap(($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top))
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        # Composition is asynchronous. Wait for actual rendered pixels instead
        # of assuming a fixed sleep means the entrance animation has finished.
        $ready = $false
        $clock = [System.Diagnostics.Stopwatch]::StartNew()
        while ($clock.ElapsedMilliseconds -lt 1500) {
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
            $ready = $true
            foreach ($sample in @(@(20,170,'Blue'), @(30,30,'Red'), @(200,30,'Lime'))) {
                $x = $padding + [int][Math]::Floor($sample[0] * $cardWidth / 350)
                $y = $padding + [int][Math]::Floor($sample[1] * $cardHeight / 200)
                if ($bitmap.GetPixel($x,$y).ToArgb() -ne [System.Drawing.Color]::FromName($sample[2]).ToArgb()) { $ready = $false; break }
            }
            if ($ready) { break }
            Start-Sleep -Milliseconds 25
        }
        $bitmap.Save((Join-Path $artifacts 'thumbnail.png'))
        if (-not $ready) { throw "Thumbnail preview never reached the expected rendered colors at $x,${y}: $($bitmap.GetPixel($x,$y)). See thumbnail.png." }
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    if ([CaptureInput]::ThumbnailCount() -ne 1) { throw 'Expected exactly one floating preview.' }
    return $rect
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
        $preview = Wait-Thumbnail $true
        $previewRect = Assert-Preview $preview
        if ([CaptureInput]::GetForegroundWindow() -ne $script:expectedFocus) { throw "Focus was not restored after capture (expected=$script:expectedFocus actual=$([CaptureInput]::GetForegroundWindow()))." }
        Write-Host "PASS: capture direction reverse=$reverse; exact PNG, correct preview colors/aspect/work area, preserved focus"
        $centerX = [int](($previewRect.Left + $previewRect.Right) / 2)
        $centerY = [int](($previewRect.Top + $previewRect.Bottom) / 2)
        if (-not $reverse) {
            Start-Sleep -Milliseconds 1800
            [CaptureInput]::MouseAt($centerX, $centerY, 2)
            [CaptureInput]::MouseAt($centerX, $centerY, 4)
            Start-Sleep -Milliseconds 100
            if ([CaptureInput]::GetForegroundWindow() -ne $script:expectedFocus) { throw 'Clicking the preview stole keyboard focus.' }
            # Keep injecting the hover position during the hold, just as the
            # drag test controls its position. Remote pointer updates can reset
            # a one-off synthetic move even when no physical mouse is moving.
            $hold = [System.Diagnostics.Stopwatch]::StartNew()
            $hoverSamples = 0
            $outsideSamples = 0
            while ($hold.ElapsedMilliseconds -lt 5500) {
                [CaptureInput]::MouseAt($centerX, $centerY, 0)
                Start-Sleep -Milliseconds 20
                if ([CaptureInput]::ThumbnailWindow() -eq [IntPtr]::Zero) { throw 'Preview expired while hovered.' }
                if ([CaptureInput]::CursorWindow() -eq $preview) { $hoverSamples++ } else { $outsideSamples++ }
            }
            if ($hoverSamples -lt 20 -or $outsideSamples -gt $hoverSamples) {
                throw "External pointer input prevented reliable hover testing ($hoverSamples on-preview, $outsideSamples outside)."
            }
            Write-Host 'PASS: hover survives longer than the entire idle lifetime; clicking does not activate'
            $remaining = [System.Diagnostics.Stopwatch]::StartNew()
            [CaptureInput]::MouseAt($sceneRect.Left + 25, $sceneRect.Top + 25, 0)
            [void](Wait-Thumbnail $false 4000)
            if (-not (Test-Path -LiteralPath $shot)) { throw 'Dismissal deleted the PNG.' }
            Write-Host "PASS: mouse exit resumes remaining time ($($remaining.ElapsedMilliseconds) ms), file survives automatic dismissal"
        } else {
            [CaptureInput]::MouseAt($centerX, $centerY, 8)
            [CaptureInput]::MouseAt($centerX, $centerY, 16)
            [void](Wait-Thumbnail $false 1500)
            if (-not (Test-Path -LiteralPath $shot)) { throw 'Manual dismissal deleted the PNG.' }
            if ([CaptureInput]::GetForegroundWindow() -ne $script:expectedFocus) { throw 'Dismissal stole keyboard focus.' }
            Write-Host 'PASS: right-click dismisses without stealing focus or deleting the file'
        }
    }

    # Place a solid-color part of the test scene beneath the thumbnail. A new
    # capture must show blue there, not the red pixels of the previous preview.
    $work = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
    [void][CaptureInput]::SetWindowPos($sceneWindow, [IntPtr]::Zero, $work.Right - 600, $work.Bottom - 400, 0, 0, 0x15)
    [void][CaptureInput]::GetWindowRect($sceneWindow, [ref]$sceneRect)
    Start-Sleep -Milliseconds 150
    $before = @(Get-Shots)
    [void](Start-Selection)
    Drag-Selection $false
    $shot = Wait-NewShot $before
    $created += $shot
    $preview = Wait-Thumbnail $true
    $previewRect = Assert-Preview $preview
    $oldShot = $shot
    $before = @(Get-Shots)
    [void](Start-Selection)
    if ([CaptureInput]::ThumbnailWindow() -ne [IntPtr]::Zero) { throw 'Old preview remained visible during capture.' }
    # The old HWND is now destroyed; use the known DPI of this desktop instead.
    $scale = [CaptureInput]::GetDpiForWindow($sceneWindow) / 96.0
    $padding = [int][Math]::Round(14 * $scale, [MidpointRounding]::AwayFromZero)
    $offset = [int][Math]::Round(20 * $scale, [MidpointRounding]::AwayFromZero)
    $x = $previewRect.Left + $padding + $offset
    $y = $previewRect.Top + $padding + $offset
    [CaptureInput]::MouseAt($x, $y, 2)
    [CaptureInput]::MouseAt($x + 60, $y + 40, 4)
    [void](Wait-Overlay $false)
    $replacement = Wait-NewShot $before
    $created += $replacement
    $bitmap = New-Object System.Drawing.Bitmap($replacement)
    try {
        if ($bitmap.Width -ne 60 -or $bitmap.Height -ne 40) { throw 'Replacement capture has incorrect bounds.' }
        if ($bitmap.GetPixel(10,10).ToArgb() -ne [System.Drawing.Color]::Blue.ToArgb()) { throw 'The old floating thumbnail leaked into the new screenshot.' }
    } finally { $bitmap.Dispose() }
    [void](Wait-Thumbnail $true)
    if ([CaptureInput]::ThumbnailCount() -ne 1 -or -not (Test-Path -LiteralPath $oldShot)) { throw 'Preview replacement lost the old file or created multiple thumbnails.' }
    Write-Host 'PASS: consecutive capture removes old preview before capture, creates one new preview, preserves old PNG'

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
        if ([CaptureInput]::ThumbnailWindow() -ne [IntPtr]::Zero) { throw "$cancel left a stale thumbnail visible." }
        Write-Host "PASS: $cancel cancels without creating a file or stale thumbnail"
    }

    $before = @(Get-Shots)
    [void](Start-Selection)
    Drag-Selection $false
    $finalShot = Wait-NewShot $before
    $created += $finalShot
    [void](Wait-Thumbnail $true)
    [CaptureInput]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    [CaptureInput]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    Press-Key 0x51
    [CaptureInput]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
    [CaptureInput]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    if (-not $app.WaitForExit(5000)) { throw 'Exit shortcut did not stop the application.' }
    if ($app.ExitCode -ne 0) { throw "Application exited with $($app.ExitCode)." }
    if ([CaptureInput]::ThumbnailWindow() -ne [IntPtr]::Zero -or -not (Test-Path -LiteralPath $finalShot)) { throw 'Shutdown left a window or removed the PNG.' }
    Write-Host 'PASS: clean shutdown with an active preview; file survives'
    Write-Host "Visual artifacts: $artifacts/overlay.png and thumbnail.png"
} finally {
    # Release synthetic input even when a test fails.
    [CaptureInput]::mouse_event(4 -bor 16, 0, 0, 0, [UIntPtr]::Zero)
    foreach ($key in @(0x10, 0x11, 0x12)) { [CaptureInput]::keybd_event($key, 0, 2, [UIntPtr]::Zero) }
    if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
    if ($sceneProcess -and -not $sceneProcess.HasExited) { Stop-Process -Id $sceneProcess.Id -Force }
    foreach ($path in $created) { Remove-Item -LiteralPath $path -ErrorAction SilentlyContinue }
    [void][CaptureInput]::SetForegroundWindow($originalFocus)
}
