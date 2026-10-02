param(
    [switch]$Scene,
    [ValidateSet('debug', 'release')][string]$Configuration = 'debug',
    [switch]$DragDrop,
    [switch]$Tray,
    [switch]$Layout,
    [switch]$Gallery,
    [switch]$GalleryOnly,
    [switch]$FifoOnly,
    [switch]$ActionsOnly,
    [switch]$EditorOnly
)

$GalleryOnly = $GalleryOnly -or $FifoOnly -or $ActionsOnly -or $EditorOnly

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
    [StructLayout(LayoutKind.Sequential)] struct MouseInput {
        public int X, Y; public uint Data, Flags, Time; public UIntPtr Extra;
    }
    [StructLayout(LayoutKind.Sequential)] struct KeyboardInput { public ushort Key, Scan; public uint Flags, Time; public UIntPtr Extra; }
    [StructLayout(LayoutKind.Explicit)] struct InputData {
        [FieldOffset(0)] public MouseInput Mouse;
        [FieldOffset(0)] public KeyboardInput Keyboard;
    }
    [StructLayout(LayoutKind.Sequential)] struct Input {
        public uint Type; public InputData Data;
        public MouseInput Mouse { get { return Data.Mouse; } set { Data.Mouse=value; } }
        public KeyboardInput Keyboard { get { return Data.Keyboard; } set { Data.Keyboard=value; } }
    }
    public static void Chord(ushort key, params ushort[] modifiers) {
        var inputs=new System.Collections.Generic.List<Input>();
        foreach(ushort modifier in modifiers){inputs.Add(new Input { Type=1,Keyboard=new KeyboardInput { Key=modifier } });}
        inputs.Add(new Input { Type=1,Keyboard=new KeyboardInput { Key=key } });
        inputs.Add(new Input { Type=1,Keyboard=new KeyboardInput { Key=key,Flags=2 } });
        for(int i=modifiers.Length-1;i>=0;i--){inputs.Add(new Input { Type=1,Keyboard=new KeyboardInput { Key=modifiers[i],Flags=2 } });}
        if(SendInput((uint)inputs.Count,inputs.ToArray(),Marshal.SizeOf(typeof(Input))) != inputs.Count){throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());}
    }
    public static void TypeText(string text) {
        var inputs=new System.Collections.Generic.List<Input>();
        foreach(char ch in text) {
            inputs.Add(new Input { Type=1, Keyboard=new KeyboardInput { Scan=ch, Flags=4 } });
            inputs.Add(new Input { Type=1, Keyboard=new KeyboardInput { Scan=ch, Flags=6 } });
        }
        if(SendInput((uint)inputs.Count,inputs.ToArray(),Marshal.SizeOf(typeof(Input))) != inputs.Count) { throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error()); }
    }
    [DllImport("user32.dll")] public static extern bool OpenClipboard(IntPtr owner);
    [DllImport("user32.dll")] public static extern bool CloseClipboard();
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern uint RegisterClipboardFormat(string name);
    [DllImport("user32.dll")] static extern IntPtr GetClipboardData(uint format);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalLock(IntPtr memory);
    [DllImport("kernel32.dll")] static extern bool GlobalUnlock(IntPtr memory);
    [DllImport("kernel32.dll")] static extern UIntPtr GlobalSize(IntPtr memory);
    public static byte[] ClipboardPng(int length) {
        if(!OpenClipboard(IntPtr.Zero)) return null;
        try {
            IntPtr memory=GetClipboardData(RegisterClipboardFormat("PNG"));
            if(memory==IntPtr.Zero || GlobalSize(memory).ToUInt64() < (ulong)length) return null;
            IntPtr pointer=GlobalLock(memory); if(pointer==IntPtr.Zero) return null;
            try { byte[] bytes=new byte[length]; Marshal.Copy(pointer,bytes,0,length); return bytes; }
            finally { GlobalUnlock(memory); }
        } finally { CloseClipboard(); }
    }
    [DllImport("user32.dll", SetLastError=true)] static extern uint SendInput(uint count, Input[] inputs, int size);
    public static void ClickAt(int x, int y, uint down = 2, uint up = 4) {
        int dx = (int)(((long)(x - GetSystemMetrics(76)) * 65536 + 32768) / GetSystemMetrics(78));
        int dy = (int)(((long)(y - GetSystemMetrics(77)) * 65536 + 32768) / GetSystemMetrics(79));
        var inputs = new Input[] {
            new Input { Mouse = new MouseInput { X=dx, Y=dy, Flags=0xC001 | down } },
            new Input { Mouse = new MouseInput { X=dx, Y=dy, Flags=0xC001 | up } }
        };
        if (SendInput(2, inputs, Marshal.SizeOf(typeof(Input))) != 2) { throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error()); }
    }
    public static void HoldAt(int x, int y) { ClickAt(x, y, 2, 0); }
    public static void DropAt(int x, int y) {
        int dx = (int)(((long)(x - GetSystemMetrics(76)) * 65536 + 32768) / GetSystemMetrics(78));
        int dy = (int)(((long)(y - GetSystemMetrics(77)) * 65536 + 32768) / GetSystemMetrics(79));
        var inputs = new Input[] {
            new Input { Mouse = new MouseInput { X=dx, Y=dy, Flags=0xC001 } },
            new Input { Mouse = new MouseInput { Flags=4 } }
        };
        if (SendInput(2, inputs, Marshal.SizeOf(typeof(Input))) != 2) { throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error()); }
    }
    public static void MouseAt(int x, int y, uint buttons) {
        uint dx = (uint)(((long)(x - GetSystemMetrics(76)) * 65536 + 32768) / GetSystemMetrics(78));
        uint dy = (uint)(((long)(y - GetSystemMetrics(77)) * 65536 + 32768) / GetSystemMetrics(79));
        mouse_event(0xC001 | buttons, dx, dy, 0, UIntPtr.Zero);
    }
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr process, uint flags);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    delegate bool WindowCallback(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(WindowCallback callback, IntPtr data);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, System.Text.StringBuilder text, int max);
    public static IntPtr TaggedWindow(string tag, uint process = 0) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, data) => {
            var title = new System.Text.StringBuilder(512); GetWindowText(window, title, 512);
            uint pid; GetWindowThreadProcessId(window,out pid);
            if ((process==0 || process==pid) && IsWindowVisible(window) && title.ToString().Contains(tag)) { result = window; return false; }
            return true;
        }, IntPtr.Zero);
        return result;
    }
    public static IntPtr[] ThumbnailWindows(bool visibleOnly = true) {
        var list = new System.Collections.Generic.List<IntPtr>(); IntPtr window = IntPtr.Zero;
        while ((window=FindWindowEx(IntPtr.Zero,window,"SimpleScreenshot.Thumbnail",null)) != IntPtr.Zero) {
            if (!visibleOnly || IsWindowVisible(window)) { list.Add(window); }
        }
        list.Sort((a,b) => {
            if (Pinned(a) != Pinned(b)) { return Pinned(a) ? 1 : -1; }
            Rect ar,br; GetWindowRect(a,out ar); GetWindowRect(b,out br);
            int right=br.Right.CompareTo(ar.Right); return right!=0 ? right : br.Bottom.CompareTo(ar.Bottom);
        });
        return list.ToArray();
    }
    public static bool Pinned(IntPtr window) {
        var title=new System.Text.StringBuilder(256); GetWindowText(window,title,256);
        return title.ToString().StartsWith("Pinned screenshot");
    }
    public static IntPtr ThumbnailWindow() {
        var windows=ThumbnailWindows(); if(windows.Length>0){return windows[0];}
        windows=ThumbnailWindows(false); return windows.Length>0 ? windows[0] : IntPtr.Zero;
    }
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr window,int x,int y,int width,int height,bool repaint);
    [DllImport("user32.dll")] static extern bool SystemParametersInfo(uint action,uint p,out uint result,uint flags);
    public static bool InactiveWheelEnabled(){uint mode;return SystemParametersInfo(8220,0,out mode,0)&&mode==2;}
    public static void WheelAt(int x,int y,int delta) { MouseAt(x,y,0); System.Threading.Thread.Sleep(100); mouse_event(2048,0,0,unchecked((uint)delta),UIntPtr.Zero); }
    public static IntPtr DragWindow() { return FindWindow("SimpleScreenshot.DragImage", null); }
    public static int ThumbnailCount() { return ThumbnailWindows().Length; }
    public static int PendingCount() { return ThumbnailWindows(false).Length; }
    public static IntPtr ControllerWindow() { return FindWindow("SimpleScreenshot.Controller", null); }
    public static IntPtr TaskbarWindow() { return FindWindow("Shell_TrayWnd", null); }
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    public static IntPtr MenuWindow() {
        uint controllerPid; GetWindowThreadProcessId(ControllerWindow(), out controllerPid);
        if (controllerPid == 0) { return IntPtr.Zero; }
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint pid; GetWindowThreadProcessId(window, out pid);
            if (pid == controllerPid && IsWindowVisible(window)) {
                var cls = new System.Text.StringBuilder(256); GetClassName(window, cls, 256);
                if (cls.ToString() == "#32768") { result=window; return false; }
            }
            return true;
        }, IntPtr.Zero);
        return result;
    }
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, System.Text.StringBuilder name, int max);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern uint RegisterWindowMessage(string name);
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct NotifyIconData {
        public uint Size; public IntPtr Window; public uint Id, Flags, Message; public IntPtr Icon;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string Tip;
        public uint State, StateMask;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=256)] public string Info;
        public uint Version;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=64)] public string Title;
        public uint InfoFlags; public Guid Guid; public IntPtr Balloon;
    }
    [DllImport("shell32.dll", CharSet=CharSet.Unicode)] static extern bool Shell_NotifyIcon(uint message, ref NotifyIconData data);
    public static bool RemoveTray(IntPtr window) {
        var data = new NotifyIconData { Size=(uint)Marshal.SizeOf(typeof(NotifyIconData)), Window=window, Id=1 };
        return Shell_NotifyIcon(2, ref data);
    }
    public static IntPtr SceneWindow() { return FindWindow(null, "SimpleScreenshot E2E Scene"); }
    public static IntPtr OverlayWindow() { return FindWindow("SimpleScreenshot.Selection", null); }
    public static IntPtr EditorWindow() { return FindWindow("SimpleScreenshot.Editor", null); }
    public static bool IsEditor(IntPtr window) { var name=new System.Text.StringBuilder(128);GetClassName(window,name,128);return name.ToString()=="SimpleScreenshot.Editor"; }
    public static IntPtr[] EditorWindows() { var result=new System.Collections.Generic.List<IntPtr>(); IntPtr w=IntPtr.Zero; while((w=FindWindowEx(IntPtr.Zero,w,"SimpleScreenshot.Editor",null))!=IntPtr.Zero){result.Add(w);} return result.ToArray(); }
    public static int EditorCount() { return EditorWindows().Length; }
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr hwnd,out Rect rect);
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr hwnd,ref Point point);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd,int command);
    public static Rect ClientBounds(IntPtr hwnd) { Rect rect; GetClientRect(hwnd,out rect); Point p=new Point(); ClientToScreen(hwnd,ref p); return new Rect { Left=p.X,Top=p.Y,Right=p.X+rect.Right,Bottom=p.Y+rect.Bottom }; }
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hwnd);
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
$session = (Get-Process -Id $PID).SessionId
if (Get-Process simple-screenshot -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $session }) { throw 'Close the running prototype before starting the interactive test.' }
$artifacts = Join-Path $root '.pi/capture-smoke'
[void](New-Item -ItemType Directory -Force $artifacts)
$dataRoot = Join-Path $artifacts ('User data 日本 ' + [Guid]::NewGuid().ToString('N'))
$output = Join-Path $dataRoot 'SimpleScreenshot/Temp'
$explorerWindow = $null
$closeExplorer = $false
$terminalWindow = [IntPtr]::Zero
$created = @()
$app = $null
$cleanupLock = $null
$sceneProcess = $null
$originalFocus = [CaptureInput]::GetForegroundWindow()

function Press-Key([byte]$Key) {
    [CaptureInput]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [CaptureInput]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
}
function Press-Capture {
    # Keep the actual hotkey gesture indivisible across remote input packets.
    [CaptureInput]::Chord(0x53,[ushort[]]@(0x12,0x10))
}
function Wait-Overlay([bool]$Visible) {
    for ($i = 0; $i -lt 100; $i++) {
        $hwnd = [CaptureInput]::OverlayWindow()
        if (($hwnd -ne [IntPtr]::Zero) -eq $Visible) { return $hwnd }
        Start-Sleep -Milliseconds 50
    }
    throw "Overlay visibility did not become $Visible. Check $artifacts logs."
}
function Close-AllPreviews {
    [void][CaptureInput]::PostMessage([CaptureInput]::ControllerWindow(), 0x800d, [IntPtr]::Zero, [IntPtr]::Zero)
    for ($i=0; $i -lt 100; $i++) {
        if ([CaptureInput]::PendingCount() -eq 0) { return }
        Start-Sleep -Milliseconds 25
    }
    throw 'Fixture cleanup could not close pending previews.'
}
function Click-PreviewControl([IntPtr]$Window, [switch]$Pin) {
    $rect=New-Object CaptureInput+Rect
    [void][CaptureInput]::GetWindowRect($Window,[ref]$rect)
    $scale=[CaptureInput]::GetDpiForWindow($Window)/96.0
    $x=if($Pin){[int]($rect.Right-36*$scale)}else{[int]($rect.Left+32*$scale)}
    $y=[int]($rect.Top+32*$scale)
    [CaptureInput]::MouseAt($x,$y,0)
    Start-Sleep -Milliseconds 100
    [CaptureInput]::ClickAt($x,$y)
    Start-Sleep -Milliseconds 180
}
function Start-Selection([switch]$KeepPreviews) {
    if (-not $KeepPreviews) { Close-AllPreviews }
    if($GalleryOnly){
        [void][CaptureInput]::SetWindowPos($script:sceneWindow,[IntPtr](-1),0,0,0,0,0x13)
        [CaptureInput]::ClickAt(($script:sceneRect.Left+450),($script:sceneRect.Top+350))
    }
    [void][CaptureInput]::SetForegroundWindow($script:sceneWindow)
    # Keep a software-composited RDP cursor away from sampled capture pixels.
    [void][CaptureInput]::SetCursorPos($script:sceneRect.Left + 450, $script:sceneRect.Top + 350)
    Start-Sleep -Milliseconds 100
    [void][CaptureInput]::SetForegroundWindow($script:sceneWindow)
    $script:expectedFocus = [CaptureInput]::GetForegroundWindow()
    Press-Capture
    $window = Wait-Overlay $true
    if($EditorOnly){
        foreach($editorWindow in [CaptureInput]::EditorWindows()){
            if([CaptureInput]::IsWindowVisible($editorWindow)){throw 'An editor remained visible while capturing the desktop.'}
        }
    }
    Start-Sleep -Milliseconds 150
    return $window
}
function Drag-Selection([bool]$Reverse) {
    $x1 = $script:sceneRect.Left + 25
    $y1 = $script:sceneRect.Top + 25
    $x2 = $x1 + 350
    $y2 = $y1 + 200
    if ($Reverse) { $x1, $x2 = $x2, $x1; $y1, $y2 = $y2, $y1 }
    [CaptureInput]::HoldAt($x1, $y1)
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
    [CaptureInput]::DropAt($x2, $y2)
    [void](Wait-Overlay $false)
}
function Wait-Thumbnail([bool]$Visible, [int]$TimeoutMs = 6000) {
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    while ($clock.ElapsedMilliseconds -lt $TimeoutMs) {
        $hwnd = [CaptureInput]::ThumbnailWindow()
        if (($hwnd -ne [IntPtr]::Zero -and [CaptureInput]::IsWindowVisible($hwnd)) -eq $Visible) { return $hwnd }
        Start-Sleep -Milliseconds 25
    }
    throw "Thumbnail visibility did not become $Visible. Check $artifacts logs."
}
function Assert-Preview([IntPtr]$Window, [int]$ImageWidth = 350, [int]$ImageHeight = 200,
    $Samples = @(@(80,170,'Blue'), @(80,55,'Red'), @(200,55,'Lime')), [string]$Artifact = 'thumbnail.png', [int]$ExpectedCount = 1) {
    $rect = New-Object CaptureInput+Rect
    [void][CaptureInput]::GetWindowRect($Window, [ref]$rect)
    $scale = [CaptureInput]::GetDpiForWindow($Window) / 96.0
    $work = [System.Windows.Forms.Screen]::FromHandle($Window).WorkingArea
    if ($rect.Right -gt $work.Right -or $rect.Bottom -gt $work.Bottom -or $rect.Left -lt $work.Left -or $rect.Top -lt $work.Top) { throw 'Preview is outside the monitor work area.' }
    $padding = [int][Math]::Round(14 * $scale, [MidpointRounding]::AwayFromZero)
    $margin = [int][Math]::Round(18 * $scale, [MidpointRounding]::AwayFromZero)
    $cardWidth = $rect.Right - $rect.Left - $padding - $margin
    $cardHeight = $rect.Bottom - $rect.Top - $padding - $margin
    if ($cardWidth -ne [int][Math]::Round(220*$scale) -or $cardHeight -ne [int][Math]::Round(160*$scale)) { throw 'Thumbnail card is not fixed at 220x160 logical pixels.' }
    $taskbar = [CaptureInput]::TaskbarWindow()
    if ($taskbar -ne [IntPtr]::Zero) {
        $bar = New-Object CaptureInput+Rect
        [void][CaptureInput]::GetWindowRect($taskbar, [ref]$bar)
        $bounds = [System.Windows.Forms.Screen]::FromHandle($Window).Bounds
        if ($bar.Right -gt $bounds.Left -and $bar.Left -lt $bounds.Right -and $bar.Top -gt $bounds.Top + $bounds.Height/2 -and $bar.Bottom-$bar.Top -lt $bar.Right-$bar.Left) {
            $reservedTop = $bounds.Bottom - ($bar.Bottom-$bar.Top)
            if ($rect.Bottom -gt $reservedTop -or $rect.Bottom-$margin -gt $reservedTop-$margin) { throw 'Thumbnail/shadow collides with the actual taskbar (including auto-hide reveal area).' }
        }
    }
    $cover = [Math]::Max($cardWidth/$ImageWidth, $cardHeight/$ImageHeight)
    $cropLeft = ($ImageWidth-$cardWidth/$cover)/2
    $cropTop = ($ImageHeight-$cardHeight/$cover)/2
    $bitmap = New-Object System.Drawing.Bitmap(($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top))
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        # Composition is asynchronous. Wait for actual rendered pixels instead
        # of assuming a fixed sleep means the entrance animation has finished.
        $ready = $false
        $clock = [System.Diagnostics.Stopwatch]::StartNew()
        while ($clock.ElapsedMilliseconds -lt 1500) {
            # The exclusion fixture moves the scene into the preview corner.
            # Its capture pointer can hover the new card and correctly dim it.
            # Inspect explicitly unhovered pixels, not that dimmed UI.
            [CaptureInput]::MouseAt(($work.Left+10),($work.Top+10),0)
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
            $ready = $true
            foreach ($sample in $Samples) {
                $x = $padding + [int][Math]::Floor(($sample[0]-$cropLeft)*$cover)
                $y = $padding + [int][Math]::Floor(($sample[1]-$cropTop)*$cover)
                if ($x -lt $padding+2 -or $y -lt $padding+2 -or $x -ge $padding+$cardWidth-2 -or $y -ge $padding+$cardHeight-2) { continue }
                if ($bitmap.GetPixel($x,$y).ToArgb() -ne [System.Drawing.Color]::FromName($sample[2]).ToArgb()) { $ready = $false; break }
            }
            if ($ready -and $ImageWidth -eq 10 -and $ImageHeight -eq 10) {
                foreach ($edge in @(@(($padding+4), ($padding+[int]($cardHeight/2))), @(($padding+[int]($cardWidth/2)), ($padding+4)))) {
                    if ($bitmap.GetPixel($edge[0],$edge[1]).ToArgb() -ne [Drawing.Color]::Blue.ToArgb()) { $ready=$false; break }
                }
            }
            if ($ready) { break }
            Start-Sleep -Milliseconds 25
        }
        $bitmap.Save((Join-Path $artifacts $Artifact))
        if (-not $ready) { throw "Thumbnail preview never reached the expected rendered colors at $x,${y}: $($bitmap.GetPixel($x,$y)). See thumbnail.png." }
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    if ($ExpectedCount -gt 0 -and [CaptureInput]::ThumbnailCount() -ne $ExpectedCount) { throw "Expected $ExpectedCount visible previews." }
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

function Wait-NewPreview([IntPtr[]]$Before) {
    for($i=0;$i -lt 100;$i++){
        foreach($window in [CaptureInput]::ThumbnailWindows()){
            if($window -notin $Before){return $window}
        }
        Start-Sleep -Milliseconds 25
    }
    throw 'New capture did not publish a new visible preview.'
}
function New-TestPreview([switch]$KeepPreviews) {
    $beforeWindows=[CaptureInput]::ThumbnailWindows($false)
    $before = @(Get-Shots)
    [void](Start-Selection -KeepPreviews:$KeepPreviews)
    Drag-Selection $false
    $shot = Wait-NewShot $before
    $script:created += $shot
    Assert-Png $shot
    $preview = Wait-NewPreview $beforeWindows
    [void](Assert-Preview $preview -ExpectedCount $(if($KeepPreviews){0}else{1}))
    return @{ Shot = $shot; Window = $preview; Foreground = [CaptureInput]::GetForegroundWindow() }
}
function Wait-DragLog([string]$Text) {
    for ($i = 0; $i -lt 150; $i++) {
        $lines = @(Get-Content (Join-Path $artifacts 'stdout.log') -ErrorAction SilentlyContinue | Select-Object -Skip $script:dragBaseline)
        if (($lines -join "`n").Contains($Text)) { return }
        $app.Refresh()
        if ($app.HasExited) { throw 'Application exited during drag.' }
        Start-Sleep -Milliseconds 20
    }
    throw "No '$Text' received from native drag loop."
}
function Begin-PreviewDrag([IntPtr]$Window, [int]$TargetX, [int]$TargetY, [switch]$Edge) {
    $script:dragBaseline = @(Get-Content (Join-Path $artifacts 'stdout.log')).Count
    $rect = New-Object CaptureInput+Rect
    [void][CaptureInput]::GetWindowRect($Window, [ref]$rect)
    $x = if ($Edge) { $rect.Left + [int][Math]::Round(20*[CaptureInput]::GetDpiForWindow($Window)/96.0) } else { [int](($rect.Left + $rect.Right) / 2) }
    $y = [int](($rect.Top + $rect.Bottom) / 2)
    [CaptureInput]::HoldAt($x, $y)
    Start-Sleep -Milliseconds 50
    for ($i = 1; $i -le 25; $i++) {
        [CaptureInput]::MouseAt([int]($x + ($TargetX-$x)*$i/25), [int]($y + ($TargetY-$y)*$i/25), 0)
        Start-Sleep -Milliseconds 15
    }
}
function Save-DragVisual([string]$Name, [int]$X, [int]$Y) {
    $bitmap = New-Object System.Drawing.Bitmap 320, 240
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($X-160, $Y-120, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $artifacts $Name), [System.Drawing.Imaging.ImageFormat]::Png)
        foreach ($sample in @(@(80,80,[Drawing.Color]::Red), @(200,80,[Drawing.Color]::Lime), @(80,160,[Drawing.Color]::Blue))) {
            if ($bitmap.GetPixel($sample[0], $sample[1]).ToArgb() -ne $sample[2].ToArgb()) { throw 'Drag image is missing, moved away from the pointer, or has incorrect colors.' }
        }
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}
function Test-Layout {
    $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $fullSamples = @(
        @(($sceneRect.Left+100-$screen.Left), ($sceneRect.Top+100-$screen.Top), 'Red'),
        @(($sceneRect.Left+230-$screen.Left), ($sceneRect.Top+100-$screen.Top), 'Lime'),
        @(($sceneRect.Left+450-$screen.Left), ($sceneRect.Top+250-$screen.Top), 'Blue'))
    $reference = $null
    foreach ($reverse in @($false, $true)) {
        $before = @(Get-Shots)
        [void](Start-Selection)
        $x1, $y1, $x2, $y2 = $screen.Left, $screen.Top, ($screen.Right-1), ($screen.Bottom-1)
        if ($reverse) { $x1, $x2 = $x2, $x1; $y1, $y2 = $y2, $y1 }
        [CaptureInput]::HoldAt($x1, $y1)
        Start-Sleep -Milliseconds 100
        for ($i = 1; $i -le 20; $i++) {
            [CaptureInput]::MouseAt([int]($x1+($x2-$x1)*$i/20), [int]($y1+($y2-$y1)*$i/20), 0)
            Start-Sleep -Milliseconds 15
        }
        [CaptureInput]::DropAt($x2, $y2)
        [void](Wait-Overlay $false)
        $shot = Wait-NewShot $before
        $script:created += $shot
        $bitmap = [Drawing.Bitmap]::new($shot)
        try {
            if ($bitmap.Width -ne $screen.Width -or $bitmap.Height -ne $screen.Height) { throw "Full-screen PNG omits monitor edge pixels: $($bitmap.Size)" }
            foreach ($sample in $fullSamples) {
                if ($bitmap.GetPixel($sample[0],$sample[1]).ToArgb() -ne [Drawing.Color]::FromName($sample[2]).ToArgb()) { throw 'Full-screen PNG contains overlay pixels or incorrect coordinates.' }
            }
        } finally { $bitmap.Dispose() }
        $preview = Wait-Thumbnail $true
        $reference = Assert-Preview $preview $screen.Width $screen.Height $fullSamples "fullscreen-$reverse.png"
        Write-Host "PASS: full-screen corner-to-corner reverse=$reverse saves exact $($screen.Width)x$($screen.Height) PNG and renders preview without error"
    }
    foreach ($spec in @(
        @{ Width=350; Height=200; Name='landscape'; Samples=@(@(80,170,'Blue'),@(80,55,'Red'),@(200,55,'Lime')) },
        @{ Width=200; Height=350; Name='portrait'; Samples=@(@(80,115,'Red'),@(185,115,'Lime'),@(80,200,'Blue')) },
        @{ Width=200; Height=200; Name='square'; Samples=@(@(55,55,'Red'),@(185,55,'Lime'),@(20,170,'Blue')) },
        @{ Width=10; Height=10; Name='tiny'; Samples=@(,@(5,5,'Blue')) })) {
        $before = @(Get-Shots)
        [void](Start-Selection)
        $x = $sceneRect.Left+25; $y = $sceneRect.Top+25
        [CaptureInput]::HoldAt($x, $y)
        Start-Sleep -Milliseconds 75
        [CaptureInput]::DropAt($x+$spec.Width, $y+$spec.Height)
        [void](Wait-Overlay $false)
        $shot = Wait-NewShot $before
        $script:created += $shot
        $bitmap = [Drawing.Bitmap]::new($shot)
        try {
            if ($bitmap.Width -ne $spec.Width -or $bitmap.Height -ne $spec.Height) { throw 'Region dimensions changed.' }
        } finally { $bitmap.Dispose() }
        $preview = Wait-Thumbnail $true
        $rect = Assert-Preview $preview $spec.Width $spec.Height $spec.Samples ("thumbnail-"+$spec.Name+'.png')
        if ($rect.Left -ne $reference.Left -or $rect.Top -ne $reference.Top -or $rect.Right -ne $reference.Right -or $rect.Bottom -ne $reference.Bottom) { throw 'Thumbnail card changes size or position with capture aspect ratio.' }
        Write-Host "PASS: $($spec.Name) content center-crops to fill the same fixed card; actual taskbar clearance verified"
        if ($DragDrop -and $spec.Name -eq 'portrait') {
            $targetX = $sceneRect.Left+400; $targetY = $sceneRect.Top+250
            Begin-PreviewDrag $preview $targetX $targetY -Edge
            Wait-DragLog 'Drag started:'
            $dragRect = New-Object CaptureInput+Rect
            [void][CaptureInput]::GetWindowRect([CaptureInput]::DragWindow(), [ref]$dragRect)
            $dragWidth = $dragRect.Right-$dragRect.Left; $dragHeight = $dragRect.Bottom-$dragRect.Top
            $cover = [Math]::Max($dragWidth/200, $dragHeight/350)
            $cropLeft = (200-$dragWidth/$cover)/2
            $cropTop = (350-$dragHeight/$cover)/2
            $bitmap = [Drawing.Bitmap]::new($dragWidth, $dragHeight)
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            try {
                $ready = $false
                for ($i = 0; $i -lt 100; $i++) {
                    [CaptureInput]::MouseAt($targetX, $targetY, 0)
                    [void][CaptureInput]::GetWindowRect([CaptureInput]::DragWindow(), [ref]$dragRect)
                    $graphics.CopyFromScreen($dragRect.Left, $dragRect.Top, 0, 0, $bitmap.Size)
                    $ready = $true
                    foreach ($sample in $spec.Samples) {
                        $color = $bitmap.GetPixel([int][Math]::Floor(($sample[0]-$cropLeft)*$cover), [int][Math]::Floor(($sample[1]-$cropTop)*$cover))
                        if ($color.ToArgb() -ne [Drawing.Color]::FromName($sample[2]).ToArgb()) { $ready=$false; break }
                    }
                    if ($ready) { break }
                    Start-Sleep -Milliseconds 25
                }
                $bitmap.Save((Join-Path $artifacts 'drag-portrait.png'))
                if (-not $ready) { throw 'Portrait drag image does not preserve the centered cover crop.' }
                if ($bitmap.GetPixel(4, [int]($dragHeight/2)).ToArgb() -ne [Drawing.Color]::Blue.ToArgb()) { throw 'Drag image has a letterbox instead of filling the card.' }
            } finally { $graphics.Dispose(); $bitmap.Dispose() }
            [CaptureInput]::DropAt($targetX, $targetY)
            Wait-DragLog 'Drag result: canceled'
            [void](Wait-Thumbnail $true)
            if (-not (Test-Path -LiteralPath $shot)) { throw 'Portrait drag lost the source PNG.' }
            Write-Host 'PASS: dragging from the card edge preserves the exact filled portrait preview; rejected drop restores preview'
        }
    }
    $bitmap = [Drawing.Bitmap]::new([Math]::Min(400,$screen.Width), [Math]::Min(320,$screen.Height))
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($screen.Right-$bitmap.Width, $screen.Bottom-$bitmap.Height, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $artifacts 'thumbnail-taskbar.png'))
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    # The card edge remains interactive after changing from fit to cover.
    $scale = [CaptureInput]::GetDpiForWindow($preview)/96.0
    Click-PreviewControl $preview
    [void](Wait-Thumbnail $false 1500)
    Write-Host 'PASS: filled card supports a close control without changing its bounds'
}
function Find-TrayButton {
    $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Simple Screenshot - Alt + Shift + S')
    $fallback=$null
    foreach($button in [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition)){
        if(-not $button.Current.IsOffscreen){return $button}
        $fallback=$button
    }
    return $fallback
}
function Open-TrayMenu {
    $button = Find-TrayButton
    if (-not $button -or $button.Current.IsOffscreen) {
        # An overflow popup can survive a previous process/menu. Close it before
        # toggling the chevron, otherwise that click can hide rather than open it.
        $scene=New-Object CaptureInput+Rect
        [void][CaptureInput]::GetWindowRect($sceneWindow,[ref]$scene)
        [CaptureInput]::ClickAt(($scene.Left+10),($scene.Top+10))
        Start-Sleep -Milliseconds 150
        $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Show Hidden Icons')
        $overflow = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($overflow -and -not $overflow.Current.IsOffscreen) {
            $overflow.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        }
        for ($i = 0; $i -lt 100; $i++) {
            $button = Find-TrayButton
            if ($button -and -not $button.Current.IsOffscreen) { break }
            Start-Sleep -Milliseconds 50
        }
    }
    if (-not $button -or $button.Current.IsOffscreen) {
        Save-GalleryScreenshot 'tray-unavailable.png'
        $shell=[System.Windows.Automation.AutomationElement]::FromHandle([CaptureInput]::TaskbarWindow())
        @($shell.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object { $_.Current | Select-Object Name,AutomationId,ClassName,IsOffscreen,BoundingRectangle }) | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $artifacts 'tray-uia.json')
        throw 'Real notification-area icon not accessible (test expects English Windows shell). See tray-unavailable.png.'
    }
    $rect = $button.Current.BoundingRectangle
    [CaptureInput]::MouseAt([int]($rect.X+$rect.Width/2),[int]($rect.Y+$rect.Height/2),0)
    Start-Sleep -Milliseconds 100
    [CaptureInput]::ClickAt([int]($rect.X + $rect.Width/2), [int]($rect.Y + $rect.Height/2), 8, 16)
    for ($i = 0; $i -lt 100; $i++) {
        if ([CaptureInput]::MenuWindow() -ne [IntPtr]::Zero) { return }
        Start-Sleep -Milliseconds 25
    }
    throw 'Real tray right-click did not open the native menu.'
}
function Select-TrayItem([string]$Name, [switch]$Accessible) {
    $condition = [System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $Name),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::MenuItem))
    for ($i = 0; $i -lt 100; $i++) {
        $matches = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
        $item = $null
        foreach ($candidate in $matches) {
            if ($candidate.Current.ProcessId -eq $app.Id -and -not $candidate.Current.IsOffscreen) { $item=$candidate; break }
        }
        if ($item) {
            if ($Accessible) {
                $item.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
            } else {
                $rect = $item.Current.BoundingRectangle
                [CaptureInput]::ClickAt([int]($rect.X + $rect.Width/2), [int]($rect.Y + $rect.Height/2))
            }
            return
        }
        Start-Sleep -Milliseconds 25
    }
    Save-GalleryScreenshot 'menu-item-unavailable.png'
    throw "Native tray menu item not found: $Name. See menu-item-unavailable.png."
}
function Read-TimerSetting([string]$Path) {
    # Do not lock out the app's atomic settings replacement while polling.
    $stream=[IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    $reader=[IO.StreamReader]::new($stream)
    try{return $reader.ReadToEnd().Trim()}finally{$reader.Dispose()}
}
function Set-AutoClose([string]$Label, [string]$Value) {
    if($GalleryOnly){
        # Gallery-only testing uses the tray's controller action, not shell UI discovery.
        $index=@('5','15','30','300','600','never').IndexOf($Value)
        [void][CaptureInput]::PostMessage([CaptureInput]::ControllerWindow(),0x800c,[IntPtr]$index,[IntPtr]::Zero)
        $settings=Join-Path (Split-Path $output -Parent) 'settings.txt'
        for($i=0;$i -lt 100;$i++){
            if((Test-Path $settings) -and (Read-TimerSetting $settings) -eq "auto_close=$Value"){return}
            Start-Sleep -Milliseconds 25
        }
        throw 'Gallery timer action was not applied.'
    }
    Open-TrayMenu
    $condition=[System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Auto-close'),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::MenuItem))
    $item=$null
    foreach($candidate in [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition)){
        if($candidate.Current.ProcessId -eq $app.Id -and -not $candidate.Current.IsOffscreen){$item=$candidate;break}
    }
    if(-not $item){throw 'Auto-close submenu missing.'}
    $r=$item.Current.BoundingRectangle
    [CaptureInput]::MouseAt([int]($r.X+$r.Width/2),[int]($r.Y+$r.Height/2),0)
    Start-Sleep -Milliseconds 100
    $item.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Select-TrayItem $Label -Accessible
    $settings=Join-Path (Split-Path $output -Parent) 'settings.txt'
    for($i=0;$i -lt 100;$i++){
        if((Test-Path $settings) -and (Read-TimerSetting $settings) -eq "auto_close=$Value"){return}
        Start-Sleep -Milliseconds 25
    }
    throw "Timer setting $Label was not persisted."
}
function Wait-GalleryCounts([int]$Visible,[int]$Pending,[int]$TimeoutMs=2500) {
    $clock=[Diagnostics.Stopwatch]::StartNew()
    while($clock.ElapsedMilliseconds -lt $TimeoutMs){
        if([CaptureInput]::ThumbnailCount() -eq $Visible -and [CaptureInput]::PendingCount() -eq $Pending){return}
        Start-Sleep -Milliseconds 25
    }
    Save-GalleryScreenshot 'gallery-count-failure.png'
    throw "Gallery counts: visible=$([CaptureInput]::ThumbnailCount()), pending=$([CaptureInput]::PendingCount()); expected $Visible/$Pending."
}
function Save-GalleryScreenshot([string]$Name) {
    $screen=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bitmap=[Drawing.Bitmap]::new($screen.Width,$screen.Height)
    $graphics=[Drawing.Graphics]::FromImage($bitmap)
    try{$graphics.CopyFromScreen($screen.Left,$screen.Top,0,0,$bitmap.Size);$bitmap.Save((Join-Path $artifacts $Name))}
    finally{$graphics.Dispose();$bitmap.Dispose()}
}
function Preview-Rect([IntPtr]$Window) {
    $rect=New-Object CaptureInput+Rect
    if(-not [CaptureInput]::GetWindowRect($Window,[ref]$rect)){throw "Missing preview $Window"}
    return $rect
}
function Wait-PreviewTop([IntPtr]$Window,[int]$Top) {
    # DestroyWindow changes counts before the controller finishes reflowing survivors.
    # Wait for the visible outcome, not for a guessed animation/scheduler delay.
    $clock=[Diagnostics.Stopwatch]::StartNew()
    while($clock.ElapsedMilliseconds -lt 2500){
        $r=Preview-Rect $Window
        if($r.Top -eq $Top){return $r}
        Start-Sleep -Milliseconds 10
    }
    Save-GalleryScreenshot 'gallery-compaction-failure.png'
    throw "Preview did not compact to $Top (actual=$($r.Top))."
}
function Assert-Stack($Records) {
    # Records are oldest first, matching bottom-to-top visual order.
    $previous=$null
    foreach($record in $Records){
        if(-not [CaptureInput]::IsWindowVisible($record.Window)){throw 'Expected stack card is hidden.'}
        $rect=Preview-Rect $record.Window
        $scale=[CaptureInput]::GetDpiForWindow($record.Window)/96.0
        # Native windows include overlapping transparent shadow/margin padding.
        if($previous -and ($rect.Left -ne $previous.Left -or $rect.Bottom-[int][Math]::Round(18*$scale) -gt $previous.Top+[int][Math]::Round(14*$scale))){
            Save-GalleryScreenshot 'gallery-order-failure.png'
            throw 'Stack must be one right-aligned column, oldest below newest, without overlap.'
        }
        $previous=$rect
    }
}
function Click-PreviewAction([IntPtr]$Window,[string]$Action) {
    $rect=Preview-Rect $Window
    $scale=[CaptureInput]::GetDpiForWindow($Window)/96.0
    $x=[int]($rect.Left+124*$scale)
    $y=[int]($rect.Top+$(if($Action -eq 'Copy'){74}else{114})*$scale)
    if($Action -eq 'Annotate'){$x=[int]($rect.Left+32*$scale);$y=[int]($rect.Top+156*$scale)}
    if($Action -eq 'Upload'){$x=[int]($rect.Right-36*$scale);$y=[int]($rect.Top+156*$scale)}
    [CaptureInput]::MouseAt($x,$y,0)
    Start-Sleep -Milliseconds 100
    [CaptureInput]::ClickAt($x,$y)
}
function Wait-SaveDialog([bool]$Visible) {
    $clock=[Diagnostics.Stopwatch]::StartNew()
    while($clock.ElapsedMilliseconds -lt 5000){
        $dialog=[CaptureInput]::TaggedWindow('Save screenshot',[uint32]$app.Id)
        if(-not $Visible -and $dialog -eq [IntPtr]::Zero){return $dialog}
        if($Visible -and $dialog -ne [IntPtr]::Zero){
            $root=[System.Windows.Automation.AutomationElement]::FromHandle($dialog)
            $cancel=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Cancel'))
            if($cancel -and $cancel.Current.IsEnabled){return $dialog}
        }
        Start-Sleep -Milliseconds 25
    }
    Save-GalleryScreenshot 'save-dialog-failure.png'
    throw "Save As dialog visibility did not become $Visible."
}
function Enter-SavePath([string]$Path,[switch]$Overwrite) {
    $dialog=Wait-SaveDialog $true
    [void][CaptureInput]::SetForegroundWindow($dialog)
    [CaptureInput]::keybd_event(18,0,0,[UIntPtr]::Zero);Press-Key 0x4E;[CaptureInput]::keybd_event(18,0,2,[UIntPtr]::Zero)
    [CaptureInput]::keybd_event(17,0,0,[UIntPtr]::Zero);Press-Key 0x41;[CaptureInput]::keybd_event(17,0,2,[UIntPtr]::Zero)
    [CaptureInput]::TypeText($Path)
    Press-Key 13
    if($Overwrite){
        for($i=0;$i -lt 100 -and [CaptureInput]::TaggedWindow('Confirm Save As',[uint32]$app.Id) -eq [IntPtr]::Zero;$i++){Start-Sleep -Milliseconds 25}
        if([CaptureInput]::TaggedWindow('Confirm Save As',[uint32]$app.Id) -eq [IntPtr]::Zero){throw 'Native overwrite confirmation missing.'}
        [CaptureInput]::keybd_event(18,0,0,[UIntPtr]::Zero);Press-Key 0x59;[CaptureInput]::keybd_event(18,0,2,[UIntPtr]::Zero)
    }
    [void](Wait-SaveDialog $false)
    for($i=0;$i -lt 100 -and -not (Test-Path $Path);$i++){Start-Sleep -Milliseconds 25}
    if(-not (Test-Path $Path)){throw 'Save As did not create the selected file.'}
}
function Assert-SavedCopy([string]$Destination,[string]$Source) {
    $expected=[Convert]::ToBase64String([IO.File]::ReadAllBytes($Source))
    $clock=[Diagnostics.Stopwatch]::StartNew()
    while($clock.ElapsedMilliseconds -lt 5000){
        if(Test-Path $Destination){
            # The shell dialog can close before publication. Poll bytes without
            # denying FILE_SHARE_DELETE to the app's atomic rename.
            $file=[IO.File]::Open($Destination,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
            $memory=[IO.MemoryStream]::new()
            try{$file.CopyTo($memory);$actual=[Convert]::ToBase64String($memory.ToArray())}
            finally{$file.Dispose();$memory.Dispose()}
            if($actual -eq $expected){return}
        }
        Start-Sleep -Milliseconds 25
    }
    Save-GalleryScreenshot 'saved-copy-failure.png'
    throw 'Export did not publish the original PNG bytes.'
}
function Assert-ClipboardImage([string]$Path) {
    $expected=[IO.File]::ReadAllBytes($Path)
    $clock=[Diagnostics.Stopwatch]::StartNew()
    $match=$false
    while($clock.ElapsedMilliseconds -lt 3000){
        $actual=[CaptureInput]::ClipboardPng($expected.Length)
        if($actual -and [Convert]::ToBase64String($actual) -eq [Convert]::ToBase64String($expected)){$match=$true;break}
        Start-Sleep -Milliseconds 25
    }
    if(-not $match){throw 'Clipboard PNG does not match the original full source file.'}
    $source=[Drawing.Bitmap]::new($Path)
    $image=[System.Windows.Forms.Clipboard]::GetImage()
    try{
        if(-not $image -or $image.Width -ne $source.Width -or $image.Height -ne $source.Height){throw 'Native clipboard image paste is missing or cropped to thumbnail dimensions.'}
        $samples=@(@(0,0),@(($source.Width-1),($source.Height-1)))
        if($source.Width -eq 350){$samples=@(@(5,170),@(30,30),@(200,30))}
        foreach($point in $samples){if($image.GetPixel($point[0],$point[1]).ToArgb() -ne $source.GetPixel($point[0],$point[1]).ToArgb()){throw 'Native clipboard image pixels changed.'}}
    }finally{$source.Dispose();if($image){$image.Dispose()}}
}
function Test-Actions {
    if([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA'){throw 'Run ActionsOnly with pwsh -Sta.'}
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    Close-AllPreviews
    Set-AutoClose 'Never' 'never'
    $first=New-TestPreview
    Assert-ClipboardImage $first.Shot
    Write-Host 'PASS: native hotkey capture automatically copies exact PNG bytes and original image pixels'
    if(-not [CaptureInput]::OpenClipboard([IntPtr]::Zero)){throw 'Could not hold clipboard for busy-lock fixture.'}
    try{
        $beforeWindows=[CaptureInput]::ThumbnailWindows($false)
        $before=@(Get-Shots)
        [void](Start-Selection -KeepPreviews)
        [CaptureInput]::HoldAt(($sceneRect.Left+350),($sceneRect.Top+230))
        Start-Sleep -Milliseconds 75
        [CaptureInput]::DropAt(($sceneRect.Left+410),($sceneRect.Top+270))
        [void](Wait-Overlay $false)
        $shot=Wait-NewShot $before
        $script:created+=$shot
        $second=@{Shot=$shot;Window=(Wait-NewPreview $beforeWindows)}
        Start-Sleep -Milliseconds 200
    }finally{[void][CaptureInput]::CloseClipboard()}
    Assert-ClipboardImage $second.Shot
    Wait-GalleryCounts 2 2
    Write-Host 'PASS: next capture replaces clipboard; temporary clipboard contention retries without blocking capture'

    [void][CaptureInput]::SetForegroundWindow($sceneWindow)
    $focus=[CaptureInput]::GetForegroundWindow()
    Click-PreviewAction $first.Window 'Copy'
    Assert-ClipboardImage $first.Shot
    Wait-GalleryCounts 2 2
    if([CaptureInput]::GetForegroundWindow() -ne $focus){throw 'Copy stole keyboard focus.'}
    Click-PreviewAction $first.Window 'Upload'
    Wait-GalleryCounts 2 2
    if([CaptureInput]::GetForegroundWindow() -ne $focus -or [CaptureInput]::DragWindow() -ne [IntPtr]::Zero){throw 'Disabled placeholder triggered an action or drag.'}
    Save-GalleryScreenshot 'thumbnail-actions-hover.png'
    Write-Host 'PASS: Copy restores older original image, Upload is inert, preview actions preserve focus'

    Set-AutoClose '5 seconds' '5'
    Click-PreviewAction $first.Window 'Save'
    [void](Wait-SaveDialog $true)
    Start-Sleep -Seconds 6
    Wait-GalleryCounts 2 2
    Save-GalleryScreenshot 'save-as-dialog.png'
    $dialog=Wait-SaveDialog $true
    $root=[System.Windows.Automation.AutomationElement]::FromHandle($dialog)
    $cancel=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Cancel'))
    $bounds=$cancel.Current.BoundingRectangle
    [CaptureInput]::ClickAt([int]($bounds.X+$bounds.Width/2),[int]($bounds.Y+$bounds.Height/2))
    [void](Wait-SaveDialog $false)
    Set-AutoClose 'Never' 'never'
    Wait-GalleryCounts 2 2
    if(-not (Test-Path $first.Shot)){throw 'Cancelling Save As removed original PNG.'}
    Write-Host 'PASS: native Save As pauses all clocks; cancel preserves both thumbnails and source file'

    $destination=Join-Path $artifacts ('Saved image 日本 '+[Guid]::NewGuid().ToString('N')+'.png')
    $script:created+=$destination
    Click-PreviewAction $first.Window 'Save'
    Enter-SavePath $destination
    Wait-GalleryCounts 1 1
    Assert-SavedCopy $destination $first.Shot
    if([CaptureInput]::IsWindow($first.Window)){throw 'Successful unpinned Save As did not dismiss its preview.'}
    $beforePin=Preview-Rect $second.Window
    Click-PreviewControl $second.Window -Pin
    if(-not [CaptureInput]::Pinned($second.Window)){throw 'Save fixture did not pin.'}
    [IO.File]::WriteAllText($destination,'previous contents')
    Click-PreviewAction $second.Window 'Save'
    Enter-SavePath $destination -Overwrite
    Wait-GalleryCounts 1 1
    $afterPin=Preview-Rect $second.Window
    if($afterPin.Left -ne $beforePin.Left -or $afterPin.Top -ne $beforePin.Top -or -not [CaptureInput]::Pinned($second.Window)){throw 'Pinned Save As moved, dismissed or unpinned the card.'}
    Assert-SavedCopy $destination $second.Shot
    Write-Host 'PASS: real Save As exports losslessly to Unicode/spaced path, dismisses unpinned, preserves pin and confirms overwrite'

    Click-PreviewAction $second.Window 'Save'
    [void](Wait-SaveDialog $true)
    [void][CaptureInput]::PostMessage([CaptureInput]::ControllerWindow(),0x8008,[IntPtr]::Zero,[IntPtr]::Zero)
    if(-not $app.WaitForExit(5000) -or $app.ExitCode -ne 0){Save-GalleryScreenshot 'save-quit-failure.png';throw 'Quit did not cancel the active Save As modal dialog.'}
    [void](Wait-SaveDialog $false)
    if([CaptureInput]::PendingCount() -ne 0 -or -not (Test-Path $second.Shot)){throw 'Save As shutdown left windows or deleted source PNG.'}
    Assert-ClipboardImage $first.Shot
    Write-Host 'PASS: quit safely cancels the native Save As dialog; source files and pasted clipboard image survive shutdown'
}
function Press-EditorChord([byte]$Key,[switch]$Shift) {
    $modifiers=if($Shift){[ushort[]]@(17,16)}else{[ushort[]]@(17)}
    [CaptureInput]::Chord($Key,$modifiers)
}
function Wait-Editor([bool]$Visible=$true) {
    for($i=0;$i -lt 150;$i++){
        $w=[CaptureInput]::EditorWindow()
        if(-not $Visible -and $w -eq [IntPtr]::Zero){return $w}
        if($Visible -and $w -ne [IntPtr]::Zero -and [CaptureInput]::IsWindowVisible($w)){
            $log=Get-Content (Join-Path $artifacts 'stdout.log') -Raw
            if($log.Contains('Editor loaded:')){return $w}
        }
        Start-Sleep -Milliseconds 25
    }
    Save-GalleryScreenshot 'editor-failure.png'
    throw "Editor visibility did not become $Visible."
}
function Click-EditorAction([IntPtr]$Window,[string]$Action) {
    $r=[CaptureInput]::ClientBounds($Window);$s=[CaptureInput]::GetDpiForWindow($Window)/96.0
    switch($Action){
        'Save' {$x=$r.Right-186*$s;$y=$r.Top+24*$s}
        'Minimize' {$x=$r.Right-105*$s;$y=$r.Top+24*$s}
        'Maximize' {$x=$r.Right-63*$s;$y=$r.Top+24*$s}
        'Close' {$x=$r.Right-21*$s;$y=$r.Top+24*$s}
        'Copy' {$x=$r.Right-68*$s;$y=$r.Bottom-24*$s}
        'Zoom' {$x=$r.Left+56*$s;$y=$r.Bottom-24*$s}
        'Rectangle' {$x=$r.Left+164*$s;$y=$r.Top+24*$s}
        'Color' {$x=$r.Left+517*$s;$y=$r.Top+24*$s}
        default {throw 'Unknown editor action'}
    }
    [CaptureInput]::ClickAt([int]$x,[int]$y)
}
function Assert-EditorPixels([IntPtr]$Window,[string]$Source,[double]$Zoom=1.0) {
    $r=[CaptureInput]::ClientBounds($Window);$s=[CaptureInput]::GetDpiForWindow($Window)/96.0
    $sourceImage=[Drawing.Bitmap]::new($Source)
    $clock=[Diagnostics.Stopwatch]::StartNew()
    $match=$false
    try{
        while($clock.ElapsedMilliseconds -lt 3000){
            $bitmap=[Drawing.Bitmap]::new(($r.Right-$r.Left),($r.Bottom-$r.Top))
            $g=[Drawing.Graphics]::FromImage($bitmap)
            try{
                $g.CopyFromScreen($r.Left,$r.Top,0,0,$bitmap.Size)
                $left=($bitmap.Width-$sourceImage.Width*$Zoom*$s)/2
                $top=48*$s+($bitmap.Height-96*$s-$sourceImage.Height*$Zoom*$s)/2
                $match=$true
                foreach($p in @(@(5,170),@(30,30),@(200,30))){
                    $x=[int][Math]::Floor($left+($p[0]+0.5)*$Zoom*$s);$y=[int][Math]::Floor($top+($p[1]+0.5)*$Zoom*$s)
                    if($x -lt 0 -or $x -ge $bitmap.Width -or $y -lt 0 -or $y -ge $bitmap.Height -or $bitmap.GetPixel($x,$y).ToArgb() -ne $sourceImage.GetPixel($p[0],$p[1]).ToArgb()){$match=$false;break}
                }
            }finally{$g.Dispose();$bitmap.Dispose()}
            if($match){return}
            Start-Sleep -Milliseconds 25
        }
    }finally{$sourceImage.Dispose()}
    Save-GalleryScreenshot 'editor-pixels-failure.png'
    throw 'Editor changed original pixels, cropped image, or used incorrect zoom/DPI geometry.'
}
function Assert-EditorScreenColor([int]$X,[int]$Y,[string]$Color) {
    $expected=[Drawing.Color]::FromName($Color).ToArgb();$clock=[Diagnostics.Stopwatch]::StartNew()
    while($clock.ElapsedMilliseconds -lt 3000){
        $bitmap=[Drawing.Bitmap]::new(1,1);$g=[Drawing.Graphics]::FromImage($bitmap)
        try{$g.CopyFromScreen($X,$Y,0,0,$bitmap.Size);$match=$bitmap.GetPixel(0,0).ToArgb() -eq $expected}
        finally{$g.Dispose();$bitmap.Dispose()}
        if($match){return};Start-Sleep -Milliseconds 25
    }
    Save-GalleryScreenshot 'editor-pan-failure.png';throw 'Editor pan did not transform image pixels or cancel back to the previous view.'
}
function Assert-EditorPalettePixel([int]$X,[int]$Y,[int]$Argb) {
    $clock=[Diagnostics.Stopwatch]::StartNew()
    while($clock.ElapsedMilliseconds -lt 2000){
        $bitmap=[Drawing.Bitmap]::new(1,1);$g=[Drawing.Graphics]::FromImage($bitmap)
        try{$g.CopyFromScreen($X,$Y,0,0,$bitmap.Size);if($bitmap.GetPixel(0,0).ToArgb() -eq $Argb){return}}
        finally{$g.Dispose();$bitmap.Dispose()}
        Start-Sleep -Milliseconds 25
    }
    Save-GalleryScreenshot 'editor-palette-failure.png'
    throw "Palette pixel $X,$Y did not match color $Argb"
}
function Test-Editor {
    if([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA'){throw 'Run EditorOnly with pwsh -Sta.'}
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $first=New-TestPreview
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $first.Window 'Annotate'
    $editor=Wait-Editor
    if([CaptureInput]::GetForegroundWindow() -ne $editor){throw 'Annotate did not intentionally activate the editor.'}
    Assert-EditorPixels $editor $first.Shot
    if([CaptureInput]::TaggedWindow('Simple Screenshot editor',[uint32]$app.Id) -ne $editor){throw 'Editor accessible window name is missing.'}
    $frame=Preview-Rect $editor;$client=[CaptureInput]::ClientBounds($editor)
    if($client.Top -ne $frame.Top -or $client.Left -ne $frame.Left){Save-GalleryScreenshot 'editor-caption-failure.png';throw "Caption offsets: window $($frame.Left),$($frame.Top); client $($client.Left),$($client.Top)."}
    if($frame.Right-$frame.Left -gt 1200*[CaptureInput]::GetDpiForWindow($editor)/96.0){throw 'Editor opens too large by default.'}
    Save-GalleryScreenshot 'editor-shell.png'
    Click-PreviewAction $first.Window 'Annotate'
    if([CaptureInput]::EditorCount() -ne 1){throw 'Repeated Annotate created duplicate sessions.'}
    Click-EditorAction $editor 'Rectangle'
    if([CaptureInput]::EditorCount() -ne 1 -or [CaptureInput]::DragWindow() -ne [IntPtr]::Zero){throw 'Disabled drawing tool launched an unrelated action.'}
    Write-Host 'PASS: real Annotate opens one activated native editor, original full-resolution pixels, disabled drawing tools are inert'

    $bounds=[CaptureInput]::ClientBounds($editor);$scale=[CaptureInput]::GetDpiForWindow($editor)/96.0
    Click-EditorAction $editor 'Color'
    Assert-EditorPalettePixel ([int]($bounds.Left+517*$scale)) ([int]($bounds.Top+102*$scale)) ([Drawing.Color]::FromArgb(249,45,58).ToArgb())
    Save-GalleryScreenshot 'editor-palette.png'
    [CaptureInput]::ClickAt([int]($bounds.Left+517*$scale),[int]($bounds.Top+102*$scale))
    Assert-EditorPalettePixel ([int]($bounds.Left+513*$scale)) ([int]($bounds.Top+24*$scale)) ([Drawing.Color]::FromArgb(249,45,58).ToArgb())
    Click-EditorAction $editor 'Color';Press-Key 0x28;Press-Key 0x0d
    Assert-EditorPalettePixel ([int]($bounds.Left+513*$scale)) ([int]($bounds.Top+24*$scale)) ([Drawing.Color]::FromArgb(254,129,1).ToArgb())
    Click-EditorAction $editor 'Color'
    [CaptureInput]::ClickAt([int]($bounds.Left+517*$scale),[int]($bounds.Top+198*$scale))
    Click-EditorAction $editor 'Color';Press-Key 27
    Assert-EditorPalettePixel ([int]($bounds.Left+517*$scale)) ([int]($bounds.Top+102*$scale)) ([Drawing.Color]::White.ToArgb())
    Click-EditorAction $editor 'Color'
    [CaptureInput]::ClickAt([int]($bounds.Left+400*$scale),[int]($bounds.Top+400*$scale))
    Assert-EditorPalettePixel ([int]($bounds.Left+517*$scale)) ([int]($bounds.Top+102*$scale)) ([Drawing.Color]::White.ToArgb())
    Write-Host 'PASS: native color menu matches video swatch order; pointer/keyboard selection and Escape/outside dismissal work'

    Click-EditorAction $editor 'Maximize'
    for($i=0;$i -lt 50 -and -not [CaptureInput]::IsZoomed($editor);$i++){Start-Sleep -Milliseconds 25}
    if(-not [CaptureInput]::IsZoomed($editor)){throw 'Custom maximize button failed.'}
    Click-EditorAction $editor 'Maximize'
    for($i=0;$i -lt 50 -and [CaptureInput]::IsZoomed($editor);$i++){Start-Sleep -Milliseconds 25}
    if([CaptureInput]::IsZoomed($editor)){throw 'Custom restore button failed.'}
    Click-EditorAction $editor 'Minimize'
    for($i=0;$i -lt 50 -and -not [CaptureInput]::IsIconic($editor);$i++){Start-Sleep -Milliseconds 25}
    if(-not [CaptureInput]::IsIconic($editor)){throw 'Custom minimize button failed.'}
    [void][CaptureInput]::ShowWindow($editor,9)
    $before=Preview-Rect $editor;$client=[CaptureInput]::ClientBounds($editor);$dpi=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $startX=[int]($client.Left+700*$dpi);$startY=[int]($client.Top+24*$dpi)
    [CaptureInput]::HoldAt($startX,$startY)
    [CaptureInput]::MouseAt(($startX+60),($startY+20),0)
    [CaptureInput]::DropAt(($startX+60),($startY+20))
    for($i=0;$i -lt 40;$i++){
        $after=Preview-Rect $editor
        if($after.Left -gt $before.Left+10){break}
        Start-Sleep -Milliseconds 25
    }
    if($after.Left -le $before.Left+10){Save-GalleryScreenshot 'editor-title-drag-failure.png';throw "Title drag did not move window: before=$($before.Left),$($before.Top) after=$($after.Left),$($after.Top) pointer=$startX,$startY."}
    Write-Host 'PASS: single-row custom title bar, compact default size, Windows minimize/maximize/restore, and native window drag'

    # Constrain the pan fixture viewport so its 400% image genuinely overflows.
    [void][CaptureInput]::MoveWindow($editor,480,180,960,640,$true)
    Assert-EditorPixels $editor $first.Shot
    Click-EditorAction $editor 'Zoom'
    for($i=0;$i -lt 100 -and [CaptureInput]::MenuWindow() -eq [IntPtr]::Zero;$i++){Start-Sleep -Milliseconds 25}
    if([CaptureInput]::MenuWindow() -eq [IntPtr]::Zero){throw 'Zoom menu did not open.'}
    Select-TrayItem '200%' -Accessible
    [CaptureInput]::MouseAt(100,100,0)
    Assert-EditorPixels $editor $first.Shot 2.0
    Click-EditorAction $editor 'Copy'
    Assert-ClipboardImage $first.Shot
    [void][CaptureInput]::SetForegroundWindow($editor)
    Press-EditorChord 0x30
    Assert-EditorPixels $editor $first.Shot
    Click-EditorAction $editor 'Zoom';Select-TrayItem '400%' -Accessible
    $bounds=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $cx=[int](($bounds.Left+$bounds.Right)/2);$cy=[int](($bounds.Top+$bounds.Bottom)/2)
    [CaptureInput]::MouseAt(100,100,0);Assert-EditorScreenColor ([int]($cx+20*$s)) $cy 'Lime'
    [CaptureInput]::HoldAt($cx,$cy);[CaptureInput]::MouseAt(([int]($cx+60*$s)),$cy,0);[CaptureInput]::DropAt(([int]($cx+60*$s)),$cy)
    [CaptureInput]::MouseAt(100,100,0);Assert-EditorScreenColor ([int]($cx+20*$s)) $cy 'Red'
    [CaptureInput]::HoldAt($cx,$cy);[CaptureInput]::MouseAt(([int]($cx-60*$s)),$cy,0);Start-Sleep -Milliseconds 100
    Press-Key 27;[CaptureInput]::DropAt(([int]($cx-60*$s)),$cy);[CaptureInput]::MouseAt(100,100,0)
    Assert-EditorScreenColor ([int]($cx+20*$s)) $cy 'Red'
    Press-EditorChord 0x30
    Assert-EditorPixels $editor $first.Shot
    [void][CaptureInput]::MoveWindow($editor,450,180,800,520,$true)
    Assert-EditorPixels $editor $first.Shot
    Write-Host 'PASS: native zoom menu, actual pan/Escape rollback, Fit and resize share correct geometry; Copy exports original pixels, not viewport'

    Click-EditorAction $editor 'Save';[void](Wait-SaveDialog $true)
    Press-Key 27;[void](Wait-SaveDialog $false)
    if(-not [CaptureInput]::IsWindow($editor)){throw 'Save cancellation destroyed editor.'}
    $destination=Join-Path $artifacts ('Editor export 日本 '+[Guid]::NewGuid().ToString('N')+'.png');$script:created+=$destination
    Click-EditorAction $editor 'Save';Enter-SavePath $destination
    Assert-SavedCopy $destination $first.Shot
    if(-not [CaptureInput]::IsWindow($editor)){throw 'Save destroyed the editable session.'}
    Write-Host 'PASS: editor native Save As cancel/Unicode export preserves original bytes and session'

    $r=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $x=[int](($r.Left+$r.Right)/2);$y=[int]($r.Bottom-24*$s)
    [CaptureInput]::HoldAt($x,$y);Start-Sleep -Milliseconds 100
    [CaptureInput]::MouseAt(($x+60),($y-60),0);Start-Sleep -Milliseconds 100
    for($i=0;$i -lt 100 -and [CaptureInput]::DragWindow() -eq [IntPtr]::Zero;$i++){Start-Sleep -Milliseconds 25}
    if([CaptureInput]::DragWindow() -eq [IntPtr]::Zero){throw 'Drag Me did not start native OLE file drag.'}
    Press-Key 27;[CaptureInput]::DropAt(($x+60),($y-60))
    for($i=0;$i -lt 100 -and [CaptureInput]::DragWindow() -ne [IntPtr]::Zero;$i++){Start-Sleep -Milliseconds 25}
    if([CaptureInput]::DragWindow() -ne [IntPtr]::Zero -or -not [CaptureInput]::IsWindow($editor)){throw 'Canceled Drag Me did not preserve editor.'}
    Write-Host 'PASS: Drag Me starts actual OLE drag; Escape preserves original image and session'
    if($DragDrop){
        $folder=Join-Path $artifacts ('Editor drop '+[Guid]::NewGuid().ToString('N'))
        [void](New-Item -ItemType Directory -Force $folder)
        $shellApp=New-Object -ComObject Shell.Application
        $existing=@($shellApp.Windows() | ForEach-Object {$_.HWND})
        $shellApp.Explore($folder)
        for($i=0;$i -lt 200;$i++){
            foreach($window in $shellApp.Windows()){
                try{if($window.Document.Folder.Self.Path -eq $folder){$script:explorerWindow=$window;break}}catch{}
            }
            if($script:explorerWindow){break};Start-Sleep -Milliseconds 25
        }
        if(-not $script:explorerWindow){throw 'Editor drop Explorer folder did not open.'}
        $script:closeExplorer=$script:explorerWindow.HWND -notin $existing
        $explorer=[IntPtr]$script:explorerWindow.HWND
        $screen=[Windows.Forms.Screen]::PrimaryScreen.WorkingArea
        [void][CaptureInput]::MoveWindow($explorer,($screen.Right-720),($screen.Top+60),700,650,$true)
        [void][CaptureInput]::SetForegroundWindow($explorer);Start-Sleep -Milliseconds 150
        $bounds=Preview-Rect $explorer;$tx=[int]($bounds.Left+($bounds.Right-$bounds.Left)*0.8);$ty=[int]($bounds.Top+($bounds.Bottom-$bounds.Top)*0.6)
        [void][CaptureInput]::SetForegroundWindow($editor)
        $bounds=[CaptureInput]::ClientBounds($editor);$x=[int](($bounds.Left+$bounds.Right)/2);$y=[int]($bounds.Bottom-24*$s)
        [CaptureInput]::HoldAt($x,$y);Start-Sleep -Milliseconds 75
        [CaptureInput]::MouseAt($tx,$ty,0)
        for($i=0;$i -lt 100 -and [CaptureInput]::DragWindow() -eq [IntPtr]::Zero;$i++){Start-Sleep -Milliseconds 25}
        if([CaptureInput]::DragWindow() -eq [IntPtr]::Zero){throw 'Editor Explorer drag never started.'}
        Start-Sleep -Milliseconds 200;[CaptureInput]::DropAt($tx,$ty)
        $copied=Join-Path $folder (Split-Path $first.Shot -Leaf);$script:created+=$copied
        Assert-SavedCopy $copied $first.Shot
        if(-not [CaptureInput]::IsWindow($editor) -or -not (Test-Path $first.Shot)){throw 'Accepted editor drag lost session or original.'}
        Write-Host 'PASS: actual Drag Me drop into Explorer copies exact original PNG and preserves editor'
    }

    Set-AutoClose '5 seconds' '5';[CaptureInput]::MouseAt(100,100,0)
    Start-Sleep -Seconds 6
    if(-not [CaptureInput]::IsWindow($first.Window)){throw 'Editing source auto-expired.'}
    $second=New-TestPreview -KeepPreviews
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    if(-not [CaptureInput]::IsWindowVisible($editor)){throw 'New capture did not restore editor.'}
    Assert-Png $second.Shot
    [CaptureInput]::MouseAt(100,100,0)
    for($i=0;$i -lt 250 -and [CaptureInput]::IsWindow($second.Window);$i++){Start-Sleep -Milliseconds 25}
    if([CaptureInput]::IsWindow($second.Window) -or -not [CaptureInput]::IsWindow($first.Window)){throw 'Editing paused unrelated previews or failed to reserve source.'}
    Set-AutoClose 'Never' 'never'
    [void][CaptureInput]::ShowWindow($editor,6)
    [void](Start-Selection -KeepPreviews);Press-Key 27;[void](Wait-Overlay $false)
    if(-not [CaptureInput]::IsIconic($editor)){throw 'Capture cancellation restored minimized editor as a normal window.'}
    [void][CaptureInput]::ShowWindow($editor,9)
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    [void][CaptureInput]::SetForegroundWindow($editor)
    Assert-EditorPixels $editor $first.Shot
    Write-Host 'PASS: source timeout is paused independently, editor is excluded from new capture, capture cancellation preserves minimized state'

    $third=New-TestPreview -KeepPreviews
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $third.Window 'Annotate'
    $other=[IntPtr]::Zero
    for($i=0;$i -lt 150;$i++){
        $candidate=[CaptureInput]::GetForegroundWindow()
        if($candidate -ne $editor -and [CaptureInput]::IsEditor($candidate)){$other=$candidate;break}
        Start-Sleep -Milliseconds 25
    }
    if([CaptureInput]::EditorCount() -ne 2){throw 'Opening another capture did not create an independent editor.'}
    if($other -eq [IntPtr]::Zero){throw 'Second editor was not activated.'}
    Click-EditorAction $other 'Close'
    for($i=0;$i -lt 100 -and [CaptureInput]::EditorCount() -ne 1;$i++){Start-Sleep -Milliseconds 25}
    if([CaptureInput]::EditorCount() -ne 1 -or -not [CaptureInput]::IsWindow($editor)){throw 'Closing second editor affected first session.'}
    [void][CaptureInput]::SetForegroundWindow($editor)
    Write-Host 'PASS: concurrent editors use independent lifetimes; native close tears down only the requested session'

    Close-AllPreviews
    if(-not [CaptureInput]::IsWindow($editor)){Save-GalleryScreenshot 'editor-close-source-failure.png';throw "Closing source previews destroyed editor: expected=$editor actual=$([CaptureInput]::EditorWindow()) count=$([CaptureInput]::EditorCount()) appExited=$($app.HasExited)."}
    try{[IO.File]::Delete($first.Shot);throw 'Editor failed to protect active source from cleanup.'}
    catch [IO.IOException] {if(($_.Exception.HResult -band 0xffff) -ne 32){throw}}
    [void][CaptureInput]::SetForegroundWindow($editor);Press-EditorChord 0x43 -Shift
    Assert-ClipboardImage $first.Shot
    Click-EditorAction $editor 'Save';[void](Wait-SaveDialog $true)
    [void][CaptureInput]::PostMessage([CaptureInput]::ControllerWindow(),0x8008,[IntPtr]::Zero,[IntPtr]::Zero)
    if(-not $app.WaitForExit(5000) -or $app.ExitCode -ne 0){throw 'Quit did not cancel editor Save As.'}
    if([CaptureInput]::EditorCount() -ne 0 -or -not (Test-Path $first.Shot)){throw 'Shutdown left editor windows or removed original.'}
    Assert-ClipboardImage $first.Shot
    Write-Host 'PASS: independent source protection and copy survive preview close; quit cancels modal output and releases editor windows'
}
function Test-Fifo {
    Close-AllPreviews
    Set-AutoClose 'Never' 'never'
    $records=@()
    for($i=0;$i -lt 3;$i++){$records+=New-TestPreview -KeepPreviews}
    Assert-Stack $records
    $oldest=Preview-Rect $records[0].Window
    $x=[int](($oldest.Left+$oldest.Right)/2);$y=[int](($oldest.Top+$oldest.Bottom)/2)
    [CaptureInput]::MouseAt($x,$y,0)
    Start-Sleep -Milliseconds 100
    Set-AutoClose '5 seconds' '5'
    $hold=[Diagnostics.Stopwatch]::StartNew()
    while($hold.ElapsedMilliseconds -lt 6000){
        [CaptureInput]::MouseAt($x,$y,0)
        if([CaptureInput]::ThumbnailCount() -ne 3){
            Save-GalleryScreenshot 'fifo-order-failure.png'
            throw 'FIFO violation: newer screenshot expired while the oldest was paused on hover.'
        }
        Start-Sleep -Milliseconds 25
    }
    Write-Host 'PASS: hovered queue head prevents newer screenshots from disappearing first'
    [CaptureInput]::MouseAt(($sceneRect.Left+450),($sceneRect.Top+350),0)
    $clock=[Diagnostics.Stopwatch]::StartNew()
    $gone=@($false,$false,$false)
    $order=@()
    while($clock.ElapsedMilliseconds -lt 12000){
        for($i=0;$i -lt 3;$i++){
            if(-not $gone[$i] -and -not [CaptureInput]::IsWindowVisible($records[$i].Window)){
                for($older=0;$older -lt $i;$older++){
                    if([CaptureInput]::IsWindowVisible($records[$older].Window)){
                        Save-GalleryScreenshot 'fifo-order-failure.png'
                        throw "FIFO violation: screenshot $($i+1) disappeared before older screenshot $($older+1)."
                    }
                }
                $gone[$i]=$true
                $order+=$i+1
            }
        }
        if($order.Count -eq 3){break}
        Start-Sleep -Milliseconds 5
    }
    if(($order -join ',') -ne '1,2,3'){throw "FIFO dismissal order: $order"}
    foreach($record in $records){if(-not (Test-Path $record.Shot)){throw 'FIFO timeout deleted a PNG.'}}
    Write-Host 'PASS: actual auto-dismiss order is FIFO: 1,2,3, oldest bottom first'
    Set-AutoClose 'Never' 'never'
    $records=@()
    for($i=0;$i -lt 3;$i++){$records+=New-TestPreview -KeepPreviews}
    Click-PreviewControl $records[0].Window -Pin
    if(-not [CaptureInput]::Pinned($records[0].Window)){throw 'FIFO pin fixture did not pin.'}
    Set-AutoClose '5 seconds' '5'
    [CaptureInput]::MouseAt(($sceneRect.Left+450),($sceneRect.Top+350),0)
    $clock=[Diagnostics.Stopwatch]::StartNew()
    while($clock.ElapsedMilliseconds -lt 8000){
        if(-not [CaptureInput]::IsWindowVisible($records[2].Window) -and [CaptureInput]::IsWindowVisible($records[1].Window)){throw 'Pinned bottom caused reverse timeout order.'}
        if(-not [CaptureInput]::IsWindowVisible($records[0].Window)){throw 'FIFO timeout removed a pin.'}
        if([CaptureInput]::PendingCount() -eq 1){break}
        Start-Sleep -Milliseconds 5
    }
    Wait-GalleryCounts 1 1
    Write-Host 'PASS: FIFO skips pinned bottom; screenshots 2 then 3 expire and pin 1 survives'
    Close-AllPreviews
    Set-AutoClose 'Never' 'never'
}
function Test-Gallery {
    Close-AllPreviews
    Set-AutoClose 'Never' 'never'
    $records=@()
    for($i=0;$i -lt 4;$i++){
        $records+=New-TestPreview -KeepPreviews
        Wait-GalleryCounts ($i+1) ($i+1)
    }
    Assert-Stack $records
    Write-Host 'PASS: fourth screenshot remains visible; oldest bottom, newest top'
    $bottom=Preview-Rect $records[0].Window
    $second=Preview-Rect $records[1].Window
    $step=$bottom.Top-$second.Top
    $screen=[System.Windows.Forms.Screen]::FromHandle($records[0].Window)
    $capacity=1+[int][Math]::Floor(($bottom.Top-$screen.WorkingArea.Top)/$step)
    if($capacity -lt 4){throw 'Gallery fixture needs a desktop fitting at least four cards.'}
    # Plain FIFO overflow: 1..capacity+1 becomes 2..capacity+1 permanently.
    while($records.Count -le $capacity){$records+=New-TestPreview -KeepPreviews}
    Wait-GalleryCounts $capacity $capacity
    if([CaptureInput]::IsWindow($records[0].Window)){throw 'Evicted screenshot 1 still has a hidden window/backlog entry.'}
    $newBottom=Wait-PreviewTop $records[1].Window $bottom.Top
    if($newBottom.Top -ne $bottom.Top){throw 'Screenshot 2 did not drop into the vacated bottom slot.'}
    Assert-Stack @($records | Select-Object -Skip 1)
    Click-PreviewControl $records[1].Window
    Wait-GalleryCounts ($capacity-1) ($capacity-1)
    $third=Wait-PreviewTop $records[2].Window $bottom.Top
    if($third.Top -ne $bottom.Top -or [CaptureInput]::IsWindow($records[0].Window)){Save-GalleryScreenshot 'gallery-compaction-failure.png';throw "Removal resurrected screenshot 1 or failed to compact screenshot 3: top=$($third.Top), expected=$($bottom.Top), evictedExists=$([CaptureInput]::IsWindow($records[0].Window))."}
    foreach($record in $records){if(-not (Test-Path $record.Shot)){throw 'Overflow removed a source PNG.'}}
    Write-Host 'PASS: 1 is permanently evicted; 2 drops down; closing 2 drops 3 down without resurrecting 1'
    Close-AllPreviews
    $records=@()
    for($i=0;$i -lt 4;$i++){$records+=New-TestPreview -KeepPreviews}
    $bottom=Preview-Rect $records[0].Window
    $second=Preview-Rect $records[1].Window
    $focus=[CaptureInput]::GetForegroundWindow()
    foreach($index in @(0,2)){
        $before=Preview-Rect $records[$index].Window
        Click-PreviewControl $records[$index].Window -Pin
        $after=Preview-Rect $records[$index].Window
        if(-not [CaptureInput]::Pinned($records[$index].Window) -or $before.Left -ne $after.Left -or $before.Top -ne $after.Top){throw 'Pin moved the card from its stack slot.'}
    }
    Click-PreviewControl $records[1].Window
    Wait-GalleryCounts 3 3
    $shifted=Wait-PreviewTop $records[2].Window $second.Top
    $stillBottom=Preview-Rect $records[0].Window
    if($shifted.Top -ne $second.Top -or $stillBottom.Top -ne $bottom.Top -or -not [CaptureInput]::Pinned($records[2].Window)){throw 'Closing card 2 must compact pinned card 3 into slot 2 without unpinning.'}
    Assert-Stack @($records[0],$records[2],$records[3])
    Click-PreviewControl $records[2].Window -Pin
    $unpinned=Preview-Rect $records[2].Window
    if([CaptureInput]::Pinned($records[2].Window) -or $unpinned.Top -ne $shifted.Top){throw 'Unpin reordered the screenshot.'}
    Click-PreviewControl $records[2].Window -Pin
    if([CaptureInput]::GetForegroundWindow() -ne $focus){throw 'Pin/close controls stole keyboard focus.'}
    Write-Host 'PASS: pin/unpin preserve position and focus; removal compacts pins without losing state'

    # Create enough cards to overflow the actual monitor capacity by two.
    while($records.Count-1 -lt $capacity+2){
        $records+=New-TestPreview -KeepPreviews
        Wait-GalleryCounts ([Math]::Min($capacity,$records.Count-1)) ([Math]::Min($capacity,$records.Count-1))
    }
    $latest=@($records | Select-Object -Last ($capacity-2))
    Assert-Stack @(@($records[0],$records[2])+$latest)
    if([CaptureInput]::IsWindow($records[3].Window)){throw 'Overflow retained the oldest unpinned card instead of destroying its window.'}
    foreach($record in $records){if(-not (Test-Path $record.Shot)){throw 'Queue eviction removed the source PNG.'}}
    Save-GalleryScreenshot 'gallery-capacity.png'
    [void](Assert-Preview $latest[-1].Window -Artifact 'gallery-newest.png' -ExpectedCount $capacity)
    Write-Host "PASS: screen-derived capacity=$capacity; overflow keeps pins and newest captures, files survive"

    # Closing a survivor and cancelling a subsequent capture must not resurrect overflow.
    Click-PreviewControl $latest[0].Window
    Wait-GalleryCounts ($capacity-1) ($capacity-1)
    Assert-Stack @(@($records[0],$records[2])+@($latest | Select-Object -Skip 1))
    [void](Start-Selection -KeepPreviews)
    Press-Key 0x1B
    [void](Wait-Overlay $false)
    Wait-GalleryCounts ($capacity-1) ($capacity-1)
    if([CaptureInput]::IsWindow($records[3].Window)){throw 'Capture restoration resurrected an evicted preview.'}
    $new=New-TestPreview -KeepPreviews
    Wait-GalleryCounts $capacity $capacity
    Assert-Stack @(@($records[0],$records[2])+@($latest | Select-Object -Skip 1)+@($new))
    Write-Host 'PASS: permanent eviction survives close, capture cancellation and new capture; no hidden backlog'

    Set-AutoClose '5 seconds' '5'
    [CaptureInput]::MouseAt(($sceneRect.Left+450),($sceneRect.Top+350),0)
    Wait-GalleryCounts 2 2 15000
    Assert-Stack @($records[0],$records[2])
    foreach($index in @(0,2)){if(-not [CaptureInput]::Pinned($records[$index].Window)){throw 'Timeout removed a pin.'}}
    Click-PreviewControl $records[2].Window -Pin
    [CaptureInput]::MouseAt(($sceneRect.Left+450),($sceneRect.Top+350),0)
    Wait-GalleryCounts 1 1 7000
    if(-not (Test-Path $records[2].Shot)){throw 'Unpin timeout removed the PNG.'}
    Close-AllPreviews
    Set-AutoClose 'Never' 'never'
    Write-Host 'PASS: timeout drains surviving queue, pins remain, unpin gets a fresh lifetime'
}
function Test-Tray {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    $controller = [CaptureInput]::ControllerWindow()
    if ($controller -eq [IntPtr]::Zero -or [CaptureInput]::IsWindowVisible($controller)) { throw 'Tray controller missing or unexpectedly visible.' }
    # Release builds must be Windows GUI executables, not console executables.
    if ($Configuration -eq 'release') {
        $bytes = [IO.File]::ReadAllBytes($exe)
        $pe = [BitConverter]::ToInt32($bytes, 0x3c)
        if ([BitConverter]::ToUInt16($bytes, $pe + 24 + 68) -ne 2) { throw 'Release executable still uses the console subsystem.' }
    }
    Open-TrayMenu
    $menu = [CaptureInput]::MenuWindow()
    $rect = New-Object CaptureInput+Rect
    [void][CaptureInput]::GetWindowRect($menu, [ref]$rect)
    $bitmap = [Drawing.Bitmap]::new($rect.Right-$rect.Left, $rect.Bottom-$rect.Top)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $rendered = $false
        for ($i = 0; $i -lt 100; $i++) {
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
            $background = $bitmap.GetPixel($bitmap.Width-8, $bitmap.Height-8)
            if ($background.R -ge 230 -and $background.G -ge 230 -and $background.B -ge 230) { $rendered = $true; break }
            Start-Sleep -Milliseconds 25
        }
        $bitmap.Save((Join-Path $artifacts 'tray-menu.png'))
        if (-not $rendered) { throw 'Native light tray menu never reached its rendered background. See tray-menu.png.' }
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    Press-Key 0x1B
    Start-Sleep -Milliseconds 200
    if ([CaptureInput]::MenuWindow() -ne [IntPtr]::Zero) { throw 'Escape did not dismiss the tray menu.' }
    Write-Host 'PASS: real tray icon opens native menu, Escape dismisses, release has no console subsystem'
    $button = Find-TrayButton
    $button.SetFocus()
    Start-Sleep -Milliseconds 100
    Press-Key 0x0D
    for ($i = 0; $i -lt 100 -and [CaptureInput]::MenuWindow() -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 25 }
    if ([CaptureInput]::MenuWindow() -eq [IntPtr]::Zero) { throw 'Keyboard activation of the focused tray icon did not open its menu.' }
    Press-Key 0x1B
    Start-Sleep -Milliseconds 200
    Write-Host 'PASS: focused tray icon opens via keyboard Enter and dismisses via Escape'

    # Simulate Explorer losing our registration, without restarting the user's shell.
    if (-not [CaptureInput]::RemoveTray($controller)) { throw 'Cannot remove tray registration for restart simulation.' }
    $taskbarCreated = [CaptureInput]::RegisterWindowMessage('TaskbarCreated')
    [void][CaptureInput]::PostMessage($controller, $taskbarCreated, [IntPtr]::Zero, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 400
    Open-TrayMenu
    $before = @(Get-Shots)
    Select-TrayItem 'Take screenshot' -Accessible
    [void](Wait-Overlay $true)
    Start-Sleep -Milliseconds 150
    Drag-Selection $false
    $shot = Wait-NewShot $before
    $script:created += $shot
    Assert-Png $shot
    $preview = Wait-Thumbnail $true
    $previewRect = Assert-Preview $preview
    Click-PreviewControl $preview
    [void](Wait-Thumbnail $false 1500)
    Write-Host 'PASS: TaskbarCreated simulation restores icon; real tray Take screenshot captures exact PNG'
    $record = New-TestPreview
    Open-TrayMenu
    Start-Sleep -Milliseconds 5500
    Press-Key 0x1B
    # Moving to the tray can briefly hover the preview. Deferred hover/timer
    # processing may resume its remaining budget only when the menu closes.
    [CaptureInput]::MouseAt($sceneRect.Left + 25, $sceneRect.Top + 25, 0)
    [void](Wait-Thumbnail $false 6000)
    if (-not (Test-Path -LiteralPath $record.Shot)) { throw 'Tray popup timeout deleted the screenshot.' }
    Write-Host 'PASS: thumbnail timeout resumes after a long native tray-menu loop; source survives'
    Open-TrayMenu
    Press-Capture
    [void](Wait-Overlay $true)
    Press-Key 0x1B
    [void](Wait-Overlay $false)
    Write-Host 'PASS: capture hotkey closes an open tray menu and starts selection without reentering App'
}
function Test-DragDrop {
    $targetX = $sceneRect.Left + 400
    $targetY = $sceneRect.Top + 250
    $record = New-TestPreview
    Begin-PreviewDrag $record.Window $targetX $targetY
    Wait-DragLog 'Drag started:'
    [CaptureInput]::MouseAt($targetX, $targetY, 0)
    Start-Sleep -Milliseconds 150
    Save-DragVisual 'drag-unsupported.png' $targetX $targetY
    if ([CaptureInput]::GetForegroundWindow() -ne $record.Foreground) { throw 'Dragging stole keyboard focus.' }
    $hold = [System.Diagnostics.Stopwatch]::StartNew()
    while ($hold.ElapsedMilliseconds -lt 5600) {
        [CaptureInput]::MouseAt($targetX, $targetY, 0)
        Start-Sleep -Milliseconds 20
        if ([CaptureInput]::ThumbnailWindow() -eq [IntPtr]::Zero -or -not (Test-Path -LiteralPath $record.Shot)) { throw 'Active drag expired or lost its source PNG.' }
    }
    Press-Key 0x1B
    [CaptureInput]::DropAt($targetX, $targetY)
    Wait-DragLog 'Drag result: canceled'
    [void](Wait-Thumbnail $true)
    if (-not [CaptureInput]::IsWindowVisible($record.Window)) { throw 'Canceled drag did not restore its preview.' }
    $gdiBaseline = [CaptureInput]::GetGuiResources($app.Handle, 0)
    Write-Host 'PASS: long native drag pauses lifetime beyond five seconds; Escape restores preview and source file'

    Begin-PreviewDrag $record.Window $targetX $targetY
    Wait-DragLog 'Drag started:'
    [CaptureInput]::DropAt($targetX, $targetY)
    Wait-DragLog 'Drag result: canceled'
    if (-not [CaptureInput]::IsWindowVisible($record.Window)) { throw 'Rejected drop did not restore its preview.' }
    Write-Host 'PASS: unsupported target rejects file without losing the preview'

    $destination = Join-Path $artifacts ('Explorer drop ' + $PID)
    [void](New-Item -ItemType Directory -Force $destination)
    $shellApp = New-Object -ComObject Shell.Application
    $existingWindows = @($shellApp.Windows() | ForEach-Object { $_.HWND })
    $shellApp.Explore($destination)
    for ($i = 0; $i -lt 200; $i++) {
        foreach ($window in $shellApp.Windows()) {
            try {
                if ($window.Document.Folder.Self.Path -eq $destination) { $script:explorerWindow = $window; break }
            } catch {}
        }
        if ($script:explorerWindow) { break }
        Start-Sleep -Milliseconds 50
    }
    if (-not $script:explorerWindow) { throw 'Explorer did not open the isolated test folder.' }
    $script:closeExplorer = $script:explorerWindow.HWND -notin $existingWindows
    $explorerHandle = [IntPtr]$script:explorerWindow.HWND
    $bounds = New-Object CaptureInput+Rect
    [void][CaptureInput]::GetWindowRect($explorerHandle, [ref]$bounds)
    $x = [int]($bounds.Left + ($bounds.Right-$bounds.Left)*0.75)
    $y = [int]($bounds.Top + ($bounds.Bottom-$bounds.Top)*0.60)
    $record = New-TestPreview
    $hash = (Get-FileHash -LiteralPath $record.Shot).Hash
    Begin-PreviewDrag $record.Window $x $y
    Wait-DragLog 'Drag started:'
    Start-Sleep -Milliseconds 150
    Save-DragVisual 'drag-explorer.png' $x $y
    [CaptureInput]::DropAt($x, $y)
    Wait-DragLog 'Drag result: copied'
    [void](Wait-Thumbnail $false)
    $copy = Join-Path $destination (Split-Path -Leaf $record.Shot)
    for ($i = 0; $i -lt 200 -and -not (Test-Path -LiteralPath $copy); $i++) { Start-Sleep -Milliseconds 25 }
    if (-not (Test-Path -LiteralPath $copy) -or -not (Test-Path -LiteralPath $record.Shot)) { throw 'Explorer did not copy the PNG or moved the source.' }
    if ((Get-FileHash -LiteralPath $copy).Hash -ne $hash) { throw 'Explorer copy has incorrect bytes.' }
    Write-Host 'PASS: real Explorer drop copies identical PNG bytes and keeps the source'
    Remove-Item -LiteralPath $copy

    $record=New-TestPreview
    Click-PreviewControl $record.Window -Pin
    if(-not [CaptureInput]::Pinned($record.Window)){throw 'Drag fixture did not pin.'}
    $hash=(Get-FileHash -LiteralPath $record.Shot).Hash
    Begin-PreviewDrag $record.Window $x $y
    Wait-DragLog 'Drag started:'
    [CaptureInput]::DropAt($x,$y)
    Wait-DragLog 'Drag result: copied'
    Wait-GalleryCounts 1 1
    $copy=Join-Path $destination (Split-Path -Leaf $record.Shot)
    for($i=0;$i -lt 200 -and -not(Test-Path -LiteralPath $copy);$i++){Start-Sleep -Milliseconds 25}
    if(-not [CaptureInput]::Pinned($record.Window) -or -not(Test-Path -LiteralPath $record.Shot) -or
        -not(Test-Path -LiteralPath $copy) -or (Get-FileHash -LiteralPath $copy).Hash -ne $hash){
        throw 'Successful pinned drag lost the reference/source or changed copy bytes.'
    }
    Click-PreviewControl $record.Window
    Wait-GalleryCounts 0 0
    Remove-Item -LiteralPath $copy
    Write-Host 'PASS: successful real Explorer drop keeps pinned reference and unchanged original PNG'

    $tag = 'SimpleScreenshot Terminal E2E ' + $PID
    $receiver = Join-Path $artifacts 'terminal-receiver.ps1'
    $resultFile = Join-Path $artifacts 'terminal-path.txt'
    Remove-Item -LiteralPath $resultFile -ErrorAction SilentlyContinue
    @'
param([string]$ResultFile)
$value = Read-Host 'Drop screenshot here, then press Enter'
[IO.File]::WriteAllText($ResultFile, $value, [Text.Encoding]::UTF8)
'@ | Set-Content -LiteralPath $receiver -Encoding utf8
    $terminalArgs = '-w new nt --title "' + $tag + '" --suppressApplicationTitle --startingDirectory "' + $artifacts + '" powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "' + $receiver + '" -ResultFile "' + $resultFile + '"'
    [void](Start-Process wt.exe -ArgumentList $terminalArgs -PassThru)
    for ($i = 0; $i -lt 200; $i++) {
        $script:terminalWindow = [CaptureInput]::TaggedWindow($tag)
        if ($script:terminalWindow -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 50
    }
    if ($script:terminalWindow -eq [IntPtr]::Zero) { throw 'Isolated Windows Terminal window did not open.' }
    [void][CaptureInput]::GetWindowRect($script:terminalWindow, [ref]$bounds)
    $x = [int]($bounds.Left + ($bounds.Right-$bounds.Left)*0.75)
    $y = [int]($bounds.Top + ($bounds.Bottom-$bounds.Top)*0.50)
    [void][CaptureInput]::SetForegroundWindow($script:terminalWindow)
    $record = New-TestPreview
    Begin-PreviewDrag $record.Window $x $y
    Wait-DragLog 'Drag started:'
    [CaptureInput]::DropAt($x, $y)
    Wait-DragLog 'Drag result: copied'
    [void](Wait-Thumbnail $false)
    [CaptureInput]::ClickAt($x, $y)
    Press-Key 0x0D
    for ($i = 0; $i -lt 200 -and -not (Test-Path -LiteralPath $resultFile); $i++) { Start-Sleep -Milliseconds 25 }
    if (-not (Test-Path -LiteralPath $resultFile)) { throw 'Terminal did not receive the dropped path.' }
    $received = [IO.File]::ReadAllText($resultFile).Trim().Trim('"')
    if ($received -ne $record.Shot -or -not (Test-Path -LiteralPath $record.Shot)) { throw "Terminal received incorrect path: $received" }
    Write-Host 'PASS: actual Windows Terminal + PowerShell receives the file path, including spaces and Unicode'
    Start-Sleep -Milliseconds 300
    $gdiAfter = [CaptureInput]::GetGuiResources($app.Handle, 0)
    if ($gdiAfter -gt $gdiBaseline) { throw "Drag leaked GDI resources: $gdiBaseline -> $gdiAfter" }
    Write-Host "PASS: GDI handles do not grow across repeated native drags ($gdiBaseline -> $gdiAfter)"
}

try {
    $shell = (Get-Process -Id $PID).Path
    $sceneProcess = Start-Process $shell -ArgumentList @('-NoProfile', '-Sta', '-File', ('"' + $PSCommandPath + '"'), '-Scene') -PassThru -RedirectStandardOutput (Join-Path $artifacts 'scene-stdout.log') -RedirectStandardError (Join-Path $artifacts 'scene-stderr.log')
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
    $launch = @{
        FilePath = $exe; WindowStyle = 'Hidden'; PassThru = $true
        RedirectStandardOutput = (Join-Path $artifacts 'stdout.log')
        RedirectStandardError = (Join-Path $artifacts 'stderr.log')
    }
    $launch.Environment = @{ LOCALAPPDATA = $dataRoot }
    [void](New-Item -ItemType Directory -Force $output)
    $expired = Join-Path $output 'shot_1_1_1.png'
    $abandoned = Join-Path $output 'shot_1_1_2.png.part'
    $fresh = Join-Path $output 'shot_1_1_3.png'
    $unrelated = Join-Path $output 'personal.png'
    $locked = Join-Path $output 'shot_1_1_4.png'
    foreach ($file in @($expired, $abandoned, $fresh, $unrelated, $locked)) {
        [IO.File]::WriteAllText($file, 'cleanup fixture')
        $created += $file
        if ($file -ne $fresh) { [IO.File]::SetLastWriteTimeUtc($file, [DateTime]::UtcNow.AddHours(-25)) }
    }
    $cleanupLock = [IO.File]::Open($locked, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    $app = Start-Process @launch
    $ready = $false
    for ($i = 0; $i -lt 200; $i++) {
        $app.Refresh()
        $startupError = Get-Content (Join-Path $artifacts 'stderr.log') -Raw -ErrorAction SilentlyContinue
        if ($startupError) { throw "Capture application failed during startup: $startupError" }
        if ($app.HasExited) { throw 'Capture application exited during startup.' }
        $startup = Get-Content (Join-Path $artifacts 'stdout.log') -Raw -ErrorAction SilentlyContinue
        if ($startup -and $startup.Contains('Timing is provisional.')) { $ready = $true; break }
        Start-Sleep -Milliseconds 50
    }
    if (-not $ready) { throw 'Capture application never reported startup readiness.' }
    for ($i = 0; $i -lt 100; $i++) {
        if (-not (Test-Path -LiteralPath $expired) -and -not (Test-Path -LiteralPath $abandoned)) { break }
        Start-Sleep -Milliseconds 50
    }
    if ((Test-Path -LiteralPath $expired) -or (Test-Path -LiteralPath $abandoned)) { throw 'Startup cleanup did not remove expired screenshots/private writes.' }
    foreach ($file in @($fresh, $unrelated, $locked)) {
        if (-not (Test-Path -LiteralPath $file)) { throw "Cleanup incorrectly removed protected file: $file" }
    }
    $cleanupLock.Dispose()
    $cleanupLock = $null
    Write-Host 'PASS: real app startup cleans expired files, preserves recent/unrelated/locked files'
    Start-Sleep -Milliseconds 350
    if ($Layout) { Test-Layout }
    if ($EditorOnly) { Test-Editor }
    elseif ($ActionsOnly) { Test-Actions }
    elseif ($FifoOnly) { Test-Fifo }
    elseif ($Gallery -or $GalleryOnly) { Test-Gallery }
    if ($GalleryOnly) {
        if(-not $app.HasExited){[void][CaptureInput]::PostMessage([CaptureInput]::ControllerWindow(), 0x8008, [IntPtr]::Zero, [IntPtr]::Zero)}
        if (-not $app.WaitForExit(5000) -or $app.ExitCode -ne 0) { throw 'Gallery-only shutdown failed.' }
        return
    }
    if ($Tray) { Test-Tray }

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
            [CaptureInput]::MouseAt($centerX,$centerY,0)
            Start-Sleep -Milliseconds 100
            # Focus at the gesture is authoritative; activation before it must
            # not be misattributed to a non-activating preview click.
            $clickFocus=[CaptureInput]::GetForegroundWindow()
            [CaptureInput]::ClickAt($centerX, $centerY)
            Start-Sleep -Milliseconds 100
            if ([CaptureInput]::GetForegroundWindow() -ne $clickFocus) { throw 'Clicking the preview stole keyboard focus.' }
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
            Click-PreviewControl $preview
            [void](Wait-Thumbnail $false 1500)
            if (-not (Test-Path -LiteralPath $shot)) { throw 'Manual dismissal deleted the PNG.' }
            if ([CaptureInput]::GetForegroundWindow() -ne $script:expectedFocus) { throw 'Dismissal stole keyboard focus.' }
            Write-Host 'PASS: close control dismisses without stealing focus or deleting the file'
        }
    }

    if ($DragDrop) { Test-DragDrop }

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
    if ([CaptureInput]::ThumbnailCount() -ne 0) { throw 'Old preview remained visible during capture.' }
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

    if($Gallery){
        [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr]::Zero,200,200,0,0,0x15)
        [void][CaptureInput]::GetWindowRect($sceneWindow,[ref]$sceneRect)
        Set-AutoClose 'Never' 'never'
    }
    $before = @(Get-Shots)
    [void](Start-Selection)
    Drag-Selection $false
    $finalShot = Wait-NewShot $before
    $created += $finalShot
    $finalPreview = Wait-Thumbnail $true
    if ($DragDrop) {
        Begin-PreviewDrag $finalPreview ($sceneRect.Left + 400) ($sceneRect.Top + 250)
        Wait-DragLog 'Drag started:'
    }
    if ($Tray -and -not $DragDrop) {
        # The topmost color fixture was moved into this corner for exclusion tests;
        # move it away so it cannot obscure Windows' tray overflow panel.
        [void][CaptureInput]::SetWindowPos($sceneWindow, [IntPtr]::Zero, 200, 200, 0, 0, 0x15)
        Open-TrayMenu
        Select-TrayItem 'Quit'
    } elseif ($Tray) {
        # The same quit request must be observed inside OLE's nested message loop.
        [void][CaptureInput]::PostMessage([CaptureInput]::ControllerWindow(), 0x8008, [IntPtr]::Zero, [IntPtr]::Zero)
    } else {
        [CaptureInput]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
        [CaptureInput]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
        Press-Key 0x51
        [CaptureInput]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
        [CaptureInput]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    }
    if ($DragDrop) {
        for ($i = 0; $i -lt 100 -and -not $app.HasExited; $i++) {
            [CaptureInput]::MouseAt($sceneRect.Left + 400, $sceneRect.Top + 250, 0)
            Start-Sleep -Milliseconds 20
            $app.Refresh()
        }
        [CaptureInput]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    }
    if (-not $app.WaitForExit(5000)) { throw 'Exit shortcut did not stop the application.' }
    if ($app.ExitCode -ne 0) { throw "Application exited with $($app.ExitCode)." }
    if ([CaptureInput]::ThumbnailWindow() -ne [IntPtr]::Zero -or -not (Test-Path -LiteralPath $finalShot)) { throw 'Shutdown left a window or removed the PNG.' }
    if ($Tray -and [CaptureInput]::ControllerWindow() -ne [IntPtr]::Zero) { throw 'Tray controller survived quit.' }
    Write-Host "PASS: clean shutdown with active preview/drag (drag=$DragDrop tray=$Tray); file survives"
    if ($Tray -or $Gallery) {
        # Preserve the first process's diagnostics before the restart reopens streams.
        Copy-Item (Join-Path $artifacts 'stdout.log') (Join-Path $artifacts 'stdout-first-session.log') -Force
        Copy-Item (Join-Path $artifacts 'stderr.log') (Join-Path $artifacts 'stderr-first-session.log') -Force
        # Validate quit while TrackPopupMenu, rather than the outer loop, owns input.
        [void][CaptureInput]::SetWindowPos($sceneWindow, [IntPtr]::Zero, 200, 200, 0, 0, 0x15)
        [void][CaptureInput]::GetWindowRect($sceneWindow,[ref]$sceneRect)
        $app = Start-Process @launch
        for ($i = 0; $i -lt 100 -and [CaptureInput]::ControllerWindow() -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 50 }
        Start-Sleep -Milliseconds 500
        if($Gallery){
            $record=New-TestPreview
            [CaptureInput]::MouseAt(($sceneRect.Left+450),($sceneRect.Top+350),0)
            Start-Sleep -Seconds 6
            Wait-GalleryCounts 1 1
            Click-PreviewControl $record.Window
            Wait-GalleryCounts 0 0
            Write-Host 'PASS: selected Never preference survives an actual process restart'
        }
        Open-TrayMenu
        [CaptureInput]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
        [CaptureInput]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
        Press-Key 0x51
        [CaptureInput]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
        [CaptureInput]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
        if (-not $app.WaitForExit(5000) -or $app.ExitCode -ne 0) { throw 'Quit hotkey was lost in the native tray-menu loop.' }
        Write-Host 'PASS: quit hotkey unwinds open tray menu and shuts down cleanly'
    }
    Write-Host "Visual artifacts: $artifacts/overlay.png and thumbnail.png"
} finally {
    if ($cleanupLock) { $cleanupLock.Dispose() }
    # Release synthetic input even when a test fails.
    if($ActionsOnly){[void][CaptureInput]::CloseClipboard()}
    [CaptureInput]::mouse_event(4 -bor 16, 0, 0, 0, [UIntPtr]::Zero)
    foreach ($key in @(0x10, 0x11, 0x12)) { [CaptureInput]::keybd_event($key, 0, 2, [UIntPtr]::Zero) }
    if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
    if ($sceneProcess -and -not $sceneProcess.HasExited) { Stop-Process -Id $sceneProcess.Id -Force }
    if ($closeExplorer -and $explorerWindow) { try { $explorerWindow.Quit() } catch {} }
    if ($terminalWindow -ne [IntPtr]::Zero) { [void][CaptureInput]::PostMessage($terminalWindow, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) }
    foreach ($path in $created) { Remove-Item -LiteralPath $path -ErrorAction SilentlyContinue }
    [void][CaptureInput]::SetForegroundWindow($originalFocus)
}
