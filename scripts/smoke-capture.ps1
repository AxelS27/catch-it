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
    [switch]$QuickAccessOnly,
    [switch]$ThemeOnly,
    [switch]$EditorOnly,
    [switch]$BackgroundOnly,
    [switch]$DrawOnly,
    [switch]$HoverOnly,
    [switch]$PickerOnly,
    [switch]$MosaicOnly,
    [switch]$TextOnly,
    [switch]$CropOnly,
    [switch]$ImageOnly,
    [switch]$EditableOnly,
    [switch]$PolishOnly,
    [switch]$SliderOnly,
    [switch]$WebDragOnly,
    [switch]$PlacementOnly,
    [switch]$BrandOnly
)

$GalleryOnly = $GalleryOnly -or $FifoOnly -or $ActionsOnly -or $QuickAccessOnly -or $ThemeOnly -or $EditorOnly -or $BackgroundOnly -or $DrawOnly -or $HoverOnly -or $PickerOnly -or $MosaicOnly -or $TextOnly -or $CropOnly -or $ImageOnly -or $EditableOnly -or $PolishOnly -or $SliderOnly -or $WebDragOnly -or $PlacementOnly -or $BrandOnly

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
    public static void HoldMiddleAt(int x, int y) { ClickAt(x, y, 0x20, 0); }
    public static void DropMiddleAt(int x, int y) { ClickAt(x, y, 0, 0x40); }
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
        while ((window=FindWindowEx(IntPtr.Zero,window,"CatchIt.Thumbnail",null)) != IntPtr.Zero) {
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
    public static IntPtr DragWindow() { return FindWindow("CatchIt.DragImage", null); }
    public static int ThumbnailCount() { return ThumbnailWindows().Length; }
    public static int PendingCount() { return ThumbnailWindows(false).Length; }
    public static IntPtr ControllerWindow() { return FindWindow("CatchIt.Controller", null); }
    public static IntPtr TaskbarWindow() { return FindWindow("Shell_TrayWnd", null); }
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    public static IntPtr DialogWindow(uint process) {
        IntPtr result=IntPtr.Zero;
        EnumWindows((window,data)=>{
            uint pid;GetWindowThreadProcessId(window,out pid);
            if(pid==process && IsWindowVisible(window)){
                var cls=new System.Text.StringBuilder(128);GetClassName(window,cls,128);
                if(cls.ToString()=="#32770"){result=window;return false;}
            }
            return true;
        },IntPtr.Zero);
        return result;
    }
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
    public static IntPtr SceneWindow() { return FindWindow(null, "CatchIt E2E Scene"); }
    public static IntPtr OverlayWindow() { return FindWindow("CatchIt.Selection", null); }
    public static IntPtr EditorWindow() { return FindWindow("CatchIt.Editor", null); }
    public static bool IsEditor(IntPtr window) { var name=new System.Text.StringBuilder(128);GetClassName(window,name,128);return name.ToString()=="CatchIt.Editor"; }
    public static IntPtr[] EditorWindows() { var result=new System.Collections.Generic.List<IntPtr>(); IntPtr w=IntPtr.Zero; while((w=FindWindowEx(IntPtr.Zero,w,"CatchIt.Editor",null))!=IntPtr.Zero){result.Add(w);} return result.ToArray(); }
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
    [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr hwnd);
    [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr hwnd, IntPtr dc);
    [DllImport("gdi32.dll")] static extern uint GetPixel(IntPtr dc, int x, int y);
    public static uint ScreenPixel(int x,int y){var dc=GetDC(IntPtr.Zero);try{return GetPixel(dc,x,y);}finally{ReleaseDC(IntPtr.Zero,dc);}}
}
'@
[void][CaptureInput]::SetProcessDpiAwarenessContext([IntPtr](-4))
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

if ($Scene) {
    $form = New-Object System.Windows.Forms.Form
    $form.Text = 'CatchIt E2E Scene'
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
$exe = Join-Path $root "target/$Configuration/catch-it.exe"
if (-not (Test-Path $exe)) { throw 'Run cargo build first.' }
$session = (Get-Process -Id $PID).SessionId
if (Get-Process catch-it -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $session }) { throw 'Close the running prototype before starting the interactive test.' }
$artifacts = Join-Path $root '.pi/capture-smoke'
[void](New-Item -ItemType Directory -Force $artifacts)
$dataRoot = Join-Path $artifacts ('User data 日本 ' + [Guid]::NewGuid().ToString('N'))
$output = Join-Path $dataRoot 'CatchIt/Temp'
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
    $Samples = @(@(80,170,'Blue'), @(80,55,'Red'), @(200,55,'Lime')), [string]$Artifact = 'thumbnail.png', [int]$ExpectedCount = 1, [int]$ExpectedSlot = 0, [switch]$SkipAnchor) {
    $rect = New-Object CaptureInput+Rect
    [void][CaptureInput]::GetWindowRect($Window, [ref]$rect)
    $scale = [CaptureInput]::GetDpiForWindow($Window) / 96.0
    $work = [System.Windows.Forms.Screen]::FromHandle($Window).WorkingArea
    if ($rect.Right -gt $work.Right -or $rect.Bottom -gt $work.Bottom -or $rect.Left -lt $work.Left -or $rect.Top -lt $work.Top) { throw 'Preview is outside the monitor work area.' }
    $padding = [int][Math]::Round(14 * $scale, [MidpointRounding]::AwayFromZero)
    $cardWidth = $rect.Right - $rect.Left - 2*$padding
    $cardHeight = $rect.Bottom - $rect.Top - 2*$padding
    if ($cardWidth -ne [int][Math]::Round(260*$scale) -or $cardHeight -ne [int][Math]::Round(184*$scale)) { throw 'Quick Access card is not 260x184 logical pixels.' }
    $effectiveBottom = $work.Bottom
    $taskbar = [CaptureInput]::TaskbarWindow()
    if ($taskbar -ne [IntPtr]::Zero) {
        $bar = New-Object CaptureInput+Rect
        [void][CaptureInput]::GetWindowRect($taskbar, [ref]$bar)
        $bounds = [System.Windows.Forms.Screen]::FromHandle($Window).Bounds
        if ($bar.Right -gt $bounds.Left -and $bar.Left -lt $bounds.Right -and $bar.Top -gt $bounds.Top + $bounds.Height/2 -and $bar.Bottom-$bar.Top -lt $bar.Right-$bar.Left) {
            $reservedTop = $bounds.Bottom - ($bar.Bottom-$bar.Top)
            $effectiveBottom = [Math]::Min($effectiveBottom, $reservedTop)
            if ($rect.Bottom -gt $reservedTop) { throw 'Thumbnail/shadow collides with the actual taskbar (including auto-hide reveal area).' }
        }
    }
    $expectedBottom=$effectiveBottom-[int][Math]::Round(20*$scale)-$ExpectedSlot*[int][Math]::Round(196*$scale)
    if (-not $SkipAnchor -and ($rect.Right-$padding -ne $work.Right-[int][Math]::Round(20*$scale) -or $rect.Bottom-$padding -ne $expectedBottom)) { throw "Quick Access inset differs: cardRight=$($rect.Right-$padding),cardBottom=$($rect.Bottom-$padding) expectedBottom=$expectedBottom dpi=$scale slot=$ExpectedSlot." }
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
        # All fixtures here inspect the newly created (highlighted) card.
        $light = (Get-ItemPropertyValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'AppsUseLightTheme' -ErrorAction SilentlyContinue) -ne 0
        $border = $bitmap.GetPixel(($padding+[int]($cardWidth/2)), ($padding+1))
        if (($light -and ($border.R -lt 220 -or $border.G -lt 220 -or $border.B -lt 220)) -or (-not $light -and ($border.R -gt 35 -or $border.G -gt 35 -or $border.B -gt 35))) { throw "Quick Access border must be $(if($light){'white'}else{'black'}) in this Windows app theme, got $border." }
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
function New-TestPreview([switch]$KeepPreviews, [switch]$SkipAnchor) {
    $beforeWindows=[CaptureInput]::ThumbnailWindows($false)
    $before = @(Get-Shots)
    [void](Start-Selection -KeepPreviews:$KeepPreviews)
    Drag-Selection $false
    $shot = Wait-NewShot $before
    $script:created += $shot
    Assert-Png $shot
    $preview = Wait-NewPreview $beforeWindows
    [void](Assert-Preview $preview -ExpectedCount $(if($KeepPreviews){0}else{1}) -ExpectedSlot $(if($KeepPreviews){@($beforeWindows).Count}else{0}) -SkipAnchor:$SkipAnchor)
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
    $window = [CaptureInput]::DragWindow()
    if ($window -eq [IntPtr]::Zero) { throw 'Native drag preview did not appear.' }
    $rect = New-Object CaptureInput+Rect
    [void][CaptureInput]::GetWindowRect($window, [ref]$rect)
    $scale = [CaptureInput]::GetDpiForWindow($window)/96.0
    $width = $rect.Right-$rect.Left; $height = $rect.Bottom-$rect.Top
    if ($width -ne [int][Math]::Round(260*$scale*.615) -or $height -ne [int][Math]::Round(184*$scale*.615)) { throw "Drag preview dimensions differ: ${width}x${height}." }
    if ($rect.Right -ne $X+[int][Math]::Round(8*$scale) -or $rect.Bottom -ne $Y+[int][Math]::Round(20*$scale)) { throw 'Drag image does not follow the pointer at its reference corner.' }
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $artifacts $Name), [System.Drawing.Imaging.ImageFormat]::Png)
        foreach ($sample in @(@(.20,.27,[Drawing.Color]::Red), @(.76,.27,[Drawing.Color]::Lime), @(.25,.80,[Drawing.Color]::Blue))) {
            $actual = $bitmap.GetPixel([int]($sample[0]*$width), [int]($sample[1]*$height))
            if ($actual.ToArgb() -ne $sample[2].ToArgb()) { throw "Drag image is missing or incorrectly cropped: $actual instead of $($sample[2])." }
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
    $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Catch It - Alt + Shift + S')
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
    try{return ($reader.ReadToEnd() -split '\r?\n' | Where-Object {$_ -like 'auto_close=*'} | Select-Object -First 1)}finally{$reader.Dispose()}
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
        if($previous -and ($rect.Left -ne $previous.Left -or $rect.Bottom-[int][Math]::Round(14*$scale) -gt $previous.Top+[int][Math]::Round(14*$scale))){
            Save-GalleryScreenshot 'gallery-order-failure.png'
            throw 'Stack must be one right-aligned column, oldest below newest, without overlap.'
        }
        $previous=$rect
    }
}
function Click-PreviewAction([IntPtr]$Window,[string]$Action) {
    $rect=Preview-Rect $Window
    $scale=[CaptureInput]::GetDpiForWindow($Window)/96.0
    $x=[int]($rect.Left+144*$scale)
    $y=[int]($rect.Top+$(if($Action -eq 'Copy'){84}else{132})*$scale)
    if($Action -eq 'Annotate'){$x=[int]($rect.Left+35*$scale);$y=[int]($rect.Top+177*$scale)}
    if($Action -eq 'Upload'){$x=[int]($rect.Right-35*$scale);$y=[int]($rect.Top+177*$scale)}
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
function Editor-ToolOrigin([IntPtr]$Window) {
    $r=[CaptureInput]::ClientBounds($Window);$s=[CaptureInput]::GetDpiForWindow($Window)/96.0
    $width=($r.Right-$r.Left)/$s
    $span=if($width -ge 870){484.0}else{348.0}
    return [Math]::Min([Math]::Max(($width-$span)/2,132.0),$width-242.0-$span)
}
function Click-EditorAction([IntPtr]$Window,[string]$Action) {
    $r=[CaptureInput]::ClientBounds($Window);$s=[CaptureInput]::GetDpiForWindow($Window)/96.0
    $origin=Editor-ToolOrigin $Window
    switch($Action){
        'Save' {$x=$r.Right-186*$s;$y=$r.Top+24*$s}
        'Minimize' {$x=$r.Right-105*$s;$y=$r.Top+24*$s}
        'Maximize' {$x=$r.Right-63*$s;$y=$r.Top+24*$s}
        'Close' {$x=$r.Right-21*$s;$y=$r.Top+24*$s}
        'Copy' {$x=$r.Right-68*$s;$y=$r.Bottom-24*$s}
        'Zoom' {$x=$r.Left+56*$s;$y=$r.Bottom-24*$s}
        'Background' {$x=$r.Left+88*$s;$y=$r.Top+24*$s}
        'Crop' {$x=$r.Left+28*$s;$y=$r.Top+24*$s}
        'AddImage' {$x=$r.Left+66*$s;$y=$r.Top+24*$s}
        'Move' {$x=$r.Left+($origin+14)*$s;$y=$r.Top+24*$s}
        'Rectangle' {$x=$r.Left+($origin+43)*$s;$y=$r.Top+24*$s}
        'Fill' {$x=$r.Left+($origin+72)*$s;$y=$r.Top+24*$s}
        'Ellipse' {$x=$r.Left+($origin+101)*$s;$y=$r.Top+24*$s}
        'Line' {$x=$r.Left+($origin+130)*$s;$y=$r.Top+24*$s}
        'Arrow' {$x=$r.Left+($origin+159)*$s;$y=$r.Top+24*$s}
        'Text' {$x=$r.Left+($origin+188)*$s;$y=$r.Top+24*$s}
        'Pixelate' {$x=$r.Left+($origin+217)*$s;$y=$r.Top+24*$s}
        'Spotlight' {$x=$r.Left+($origin+246)*$s;$y=$r.Top+24*$s}
        'Counter' {$x=$r.Left+($origin+275)*$s;$y=$r.Top+24*$s}
        'Pencil' {$x=$r.Left+($origin+304)*$s;$y=$r.Top+24*$s}
        'Highlighter' {$x=$r.Left+($origin+333)*$s;$y=$r.Top+24*$s}
        'Stroke' {$x=$r.Left+($origin+423)*$s;$y=$r.Top+24*$s}
        'Color' {$x=$r.Left+($origin+381)*$s;$y=$r.Top+24*$s}
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
function Test-Brand {
    $current=Join-Path $dataRoot 'CatchIt/settings.txt'
    if(-not(Test-Path $current) -or [IO.File]::ReadAllText($current) -notmatch 'placement=top_left'){
        throw 'Old preferences were not imported into Catch It.'
    }
    if(-not(Test-Path $legacyShot) -or [IO.File]::ReadAllText($legacyShot) -ne 'legacy file'){
        throw 'Old screenshot was moved or deleted during rebrand.'
    }
    $record=New-TestPreview -SkipAnchor
    if(-not $record.Shot.StartsWith($output,[StringComparison]::OrdinalIgnoreCase)){
        throw "Catch It saved screenshot to the wrong folder: $($record.Shot)"
    }
    $rect=Preview-Rect $record.Window
    if($rect.Left -gt 100 -or $rect.Top -gt 100){throw 'Catch It did not load the old top-left preference.'}
    if(-not (Test-Path $legacyShot)){throw 'Capture touched the previous app data.'}
    Write-Host 'PASS: Catch It executable, window classes, new data directory and legacy settings import; old PNG intact'
}
function Test-Placement {
    if([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA'){throw 'Run PlacementOnly with pwsh -Sta.'}
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $first=New-TestPreview
    $hash=(Get-FileHash -LiteralPath $first.Shot -Algorithm SHA256).Hash
    $settings=Join-Path (Split-Path $output -Parent) 'settings.txt'
    foreach($entry in @(@('top_left',0,0),@('top_right',1,0),@('bottom_left',0,1),@('bottom_right',1,1))){
        $value=$entry[0]
        $index=@('top_left','top_right','bottom_left','bottom_right').IndexOf($value)
        [void][CaptureInput]::PostMessage([CaptureInput]::ControllerWindow(),0x8012,[IntPtr]$index,[IntPtr]::Zero)
        $placed=$false
        for($i=0;$i -lt 100;$i++){
            $content=if(Test-Path $settings){[IO.File]::ReadAllText($settings)}else{''}
            $rect=Preview-Rect $first.Window
            $left=($rect.Left+$rect.Right)/2 -lt [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width/2
            $top=($rect.Top+$rect.Bottom)/2 -lt [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height/2
            if($content -match "(?m)^placement=$value$" -and $left -eq ($entry[1] -eq 0) -and $top -eq ($entry[2] -eq 0)){$placed=$true;break}
            Start-Sleep -Milliseconds 25
        }
        if(-not $placed){Save-GalleryScreenshot 'placement-failure.png';throw "Quick Access did not move to $value or persist it."}
    }
    [void][CaptureInput]::PostMessage([CaptureInput]::ControllerWindow(),0x8012,[IntPtr]0,[IntPtr]::Zero)
    for($i=0;$i -lt 100;$i++){
        if((Preview-Rect $first.Window).Left -lt 200){break}
        Start-Sleep -Milliseconds 25
    }
    $second=New-TestPreview -KeepPreviews -SkipAnchor
    if((Preview-Rect $second.Window).Top -le (Preview-Rect $first.Window).Top){throw 'Top corner did not stack new screenshot downward.'}
    [void][CaptureInput]::PostMessage([CaptureInput]::ControllerWindow(),0x8012,[IntPtr]2,[IntPtr]::Zero)
    for($i=0;$i -lt 100;$i++){
        if((Preview-Rect $second.Window).Top -lt (Preview-Rect $first.Window).Top){break}
        Start-Sleep -Milliseconds 25
    }
    if((Preview-Rect $second.Window).Top -ge (Preview-Rect $first.Window).Top){throw 'Bottom corner did not stack new screenshot upward.'}
    if((Get-FileHash -LiteralPath $first.Shot -Algorithm SHA256).Hash -ne $hash){throw 'Changing position modified original PNG.'}
    Write-Host 'PASS: four corners, live relocation, top/bottom stack directions, persisted choice, original PNG unchanged'
}
function Test-WebDrag {
    $chrome='C:\Program Files\Google\Chrome\Application\chrome.exe'
    if(-not (Test-Path -LiteralPath $chrome)){throw 'Chrome is required for the focused web drop-zone test.'}
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-1),0,0,0,0,0x13)
    $capture=New-TestPreview
    $hash=(Get-FileHash -LiteralPath $capture.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    $html=Join-Path $artifacts 'web-drop-zone.html';$profile=Join-Path $artifacts 'web-drop-profile'
    @'
<!doctype html><meta charset="utf-8"><title>CatchIt Drop Test</title>
<style>body{margin:0;background:#25252b;color:white;font:26px Segoe UI;display:grid;place-items:center;height:100vh}#drop{width:75%;height:70%;display:grid;place-items:center;border:4px dashed #999;border-radius:20px;background:#555}#drop.hover{background:#645bc5}#drop.done{background:#1e7846}</style>
<div id="drop">Drop PNG here</div><script>
const zone=document.getElementById('drop');let slowed=false;
document.addEventListener('dragover',e=>{e.preventDefault();e.dataTransfer.dropEffect='copy';if(!slowed){slowed=true;const until=performance.now()+180;while(performance.now()<until){}}zone.className='hover'});
document.addEventListener('dragleave',e=>{if(!e.relatedTarget||!document.contains(e.relatedTarget))zone.className=''});
document.addEventListener('drop',e=>{e.preventDefault();let files=e.dataTransfer.files;zone.className=files.length&&files[0].type==='image/png'?'done':'';zone.textContent=files.length?'Received '+files[0].name:'No file received';document.title=files.length?'CatchIt Drop OK':'CatchIt Drop Failed'});
</script>
'@ | Set-Content -LiteralPath $html -Encoding utf8
    $uri=[Uri]::new((Resolve-Path -LiteralPath $html).Path).AbsoluteUri
    try {
        $browser=Start-Process -FilePath $chrome -ArgumentList @("--user-data-dir=$profile",'--no-first-run','--disable-extensions','--window-size=1050,680','--window-position=280,100',"--app=$uri") -PassThru
        $window=[IntPtr]::Zero
        for($i=0;$i -lt 100;$i++){
            $browser.Refresh();$window=[IntPtr]$browser.MainWindowHandle
            if($window -ne [IntPtr]::Zero){break}
            Start-Sleep -Milliseconds 50
        }
        if($window -eq [IntPtr]::Zero){throw 'Isolated browser drop-zone did not open.'}
        $bounds=[CaptureInput]::ClientBounds($window)
        $tx=[int](($bounds.Left+$bounds.Right)/2);$ty=[int](($bounds.Top+$bounds.Bottom)/2)
        $rect=Preview-Rect $capture.Window
        $x=[int](($rect.Left+$rect.Right)/2);$y=[int](($rect.Top+$rect.Bottom)/2)
        [CaptureInput]::HoldAt($x,$y);Start-Sleep -Milliseconds 60
        [CaptureInput]::MouseAt($x-42,$y-24,0)
        for($i=0;$i -lt 100 -and [CaptureInput]::DragWindow() -eq [IntPtr]::Zero;$i++){Start-Sleep -Milliseconds 20}
        $visual=[CaptureInput]::DragWindow()
        if($visual -eq [IntPtr]::Zero){throw 'OLE drag preview did not start.'}
        $dragRect=New-Object CaptureInput+Rect;$latencies=@()
        $fromX=$x-42;$fromY=$y-24
        for($i=1;$i -le 18;$i++){
            $px=[int]($fromX+($tx-$fromX)*$i/18);$py=[int]($fromY+($ty-$fromY)*$i/18)
            $step=[Diagnostics.Stopwatch]::StartNew();[CaptureInput]::MouseAt($px,$py,0)
            $seen=$false
            while($step.ElapsedMilliseconds -lt 1200){
                [void][CaptureInput]::GetWindowRect($visual,[ref]$dragRect)
                if([Math]::Abs($dragRect.Right-($px+8)) -le 3 -and [Math]::Abs($dragRect.Bottom-($py+20)) -le 3){$seen=$true;break}
                Start-Sleep -Milliseconds 3
            }
            if(-not $seen){throw "OLE drag preview lagged behind pointer step $i"}
            $latencies+=$step.ElapsedMilliseconds
        }
        $hovered=$false
        for($i=0;$i -lt 60;$i++){
            $hover=[CaptureInput]::ScreenPixel(($tx+60),($ty+60))
            if(($hover -band 255) -gt 85 -and (($hover -shr 16) -band 255) -gt 150){$hovered=$true;break}
            Start-Sleep -Milliseconds 20
        }
        if(-not $hovered){Save-GalleryScreenshot 'web-drop-hover-failure.png';throw 'Chrome did not receive the native file drag-over event.'}
        [CaptureInput]::DropAt($tx,$ty)
        $received=$false
        for($i=0;$i -lt 100;$i++){
            $rgb=[CaptureInput]::ScreenPixel($tx,$ty)
            if(($rgb -band 255) -lt 90 -and (($rgb -shr 8) -band 255) -gt 100 -and (($rgb -shr 16) -band 255) -lt 140){$received=$true;break}
            Start-Sleep -Milliseconds 30
        }
        Save-GalleryScreenshot 'web-drop-zone.png'
        if(-not $received){throw 'Browser drop zone did not receive the screenshot PNG.'}
        if((Get-FileHash -LiteralPath $capture.Shot -Algorithm SHA256).Hash -ne $hash){throw 'Web drop modified the original PNG.'}
        $latencies=@($latencies|Sort-Object)
        Write-Host "PASS: Chrome HTML5 drop received original PNG; 18-step drag preview latency median=$($latencies[8]) ms, max=$($latencies[-1]) ms"
        [void][CaptureInput]::SetForegroundWindow($window);Press-Key 0x74 # reload isolated drop zone
        [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-1),0,0,0,0,0x13)
        $again=New-TestPreview
        $againHash=(Get-FileHash -LiteralPath $again.Shot -Algorithm SHA256).Hash
        [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
        Click-PreviewAction $again.Window 'Annotate';$editor=Wait-Editor
        [void][CaptureInput]::MoveWindow($editor,1050,210,800,640,$true)
        $editorBounds=[CaptureInput]::ClientBounds($editor)
        $ex=[int](($editorBounds.Left+$editorBounds.Right)/2);$ey=[int]($editorBounds.Bottom-24)
        [CaptureInput]::HoldAt($ex,$ey);Start-Sleep -Milliseconds 70
        [CaptureInput]::MouseAt(($ex-65),($ey-55),0)
        for($i=0;$i -lt 100 -and [CaptureInput]::DragWindow() -eq [IntPtr]::Zero;$i++){Start-Sleep -Milliseconds 20}
        if([CaptureInput]::DragWindow() -eq [IntPtr]::Zero){throw 'Editor Drag Me did not start.'}
        for($i=1;$i -le 18;$i++){
            [CaptureInput]::MouseAt([int]($ex-65+($tx-$ex+65)*$i/18),[int]($ey-55+($ty-$ey+55)*$i/18),0)
            Start-Sleep -Milliseconds 15
        }
        $hovered=$false
        for($i=0;$i -lt 60;$i++){
            $hover=[CaptureInput]::ScreenPixel(($tx+130),($ty+90))
            if(($hover -band 255) -gt 85 -and (($hover -shr 16) -band 255) -gt 150){$hovered=$true;break}
            Start-Sleep -Milliseconds 20
        }
        if(-not $hovered){Save-GalleryScreenshot 'web-drop-editor-hover-failure.png';throw "Browser did not receive editor Drag Me over drop zone at $($tx+130),$($ty+90): $hover"}
        [CaptureInput]::DropAt($tx,$ty)
        $received=$false
        for($i=0;$i -lt 100;$i++){
            $rgb=[CaptureInput]::ScreenPixel($tx,$ty)
            if(($rgb -band 255) -lt 90 -and (($rgb -shr 8) -band 255) -gt 100 -and (($rgb -shr 16) -band 255) -lt 140){$received=$true;break}
            Start-Sleep -Milliseconds 30
        }
        Save-GalleryScreenshot 'web-drop-editor.png'
        if(-not $received){throw 'Editor drag did not deliver PNG into browser.'}
        if((Get-FileHash -LiteralPath $again.Shot -Algorithm SHA256).Hash -ne $againHash){throw 'Editor web drop changed source PNG.'}
        Write-Host 'PASS: editor Drag Me also drops original PNG into Chrome without changing source'
    }finally {
        Get-CimInstance Win32_Process -Filter "name = 'chrome.exe'" |
            Where-Object { $_.ExecutablePath -eq $chrome -and $_.CommandLine -like "*$profile*" } |
            ForEach-Object { Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }
        Start-Sleep -Milliseconds 100
        Remove-Item -LiteralPath $profile -Recurse -Force -ErrorAction SilentlyContinue
    }
}
function Test-Sliders {
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-1),0,0,0,0,0x13)
    $capture=New-TestPreview
    $sourceHash=(Get-FileHash -LiteralPath $capture.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $capture.Window 'Annotate';$editor=Wait-Editor
    $r=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    [CaptureInput]::ClickAt([int]($r.Left+88*$s),[int]($r.Top+24*$s))
    Start-Sleep -Milliseconds 200
    $r=[CaptureInput]::ClientBounds($editor)
    $at={param([double]$X,[double]$Y)[CaptureInput]::ClickAt([int]($r.Left+$X*$s),[int]($r.Top+$Y*$s))}
    & $at 131 285 # gradient
    Save-GalleryScreenshot 'background-slider-default.png'
    $image=[Drawing.Bitmap]::new('.pi/capture-smoke/background-slider-default.png')
    try{$toggle=$image.GetPixel([int]($r.Left+234*$s),[int]($r.Top+704*$s));if($toggle.R -lt 210){throw "Auto-balance is not enabled by default: $toggle"}}finally{$image.Dispose()}
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$initial=Wait-NewShot $before
    & $at 222 660; & $at 109 710; & $at 241 759; & $at 228 704 # move sliders and turn auto-balance off
    & $at 211 626 # Reset adjustments; keep the chosen background style
    Save-GalleryScreenshot 'background-slider-reset.png'
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$reset=Wait-NewShot $before
    $a=[IO.File]::ReadAllBytes($initial);$b=[IO.File]::ReadAllBytes($reset)
    if(-not [Linq.Enumerable]::SequenceEqual([byte[]]$a,[byte[]]$b)){throw 'Reset did not restore the original background output and selected style.'}
    if((Get-FileHash -LiteralPath $capture.Shot -Algorithm SHA256).Hash -ne $sourceHash){throw 'Slider controls changed the source PNG.'}
    Write-Host 'PASS: default auto-balance on, Reset restores adjustments without changing style or source'
    & $at 55 553 # white backdrop makes any shadow loss obvious
    & $at 16 759 # Shadow off
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$noShadow=Wait-NewShot $before
    $shadowX=[int]($r.Left+16*$s);$shadowY=[int]($r.Top+759*$s)
    [CaptureInput]::HoldAt($shadowX,$shadowY)
    [CaptureInput]::MouseAt([int]($r.Left+112*$s),$shadowY,0)
    Start-Sleep -Milliseconds 90;Save-GalleryScreenshot 'background-shadow-while-dragging.png'
    [CaptureInput]::DropAt([int]($r.Left+112*$s),$shadowY)
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$withShadow=Wait-NewShot $before
    $off=[Drawing.Bitmap]::new($noShadow);$on=[Drawing.Bitmap]::new($withShadow)
    try{
        $frame=[int](($on.Width-350)/2);$sampleX=$frame+175;$sampleY=$frame+200+1
        $white=$off.GetPixel($sampleX,$sampleY);$shade=$on.GetPixel($sampleX,$sampleY)
        if($shade.R -ge $white.R-10){throw "Shadow vanished after dragging: $white -> $shade"}
        $during=[Drawing.Bitmap]::new('.pi/capture-smoke/background-shadow-while-dragging.png')
        try{
            $scale=1.6*$s;$canvasW=$r.Right-$r.Left-260*$s
            $imageTop=$r.Top+48*$s+($r.Bottom-$r.Top-96*$s-$on.Height*$scale)/2
            $contentBottom=$imageTop+($frame-[Math]::Round(200*0.03*(96/102))+200)*$scale
            $screenX=[int]($r.Left+260*$s+$canvasW/2);$darkest=255
            for($dy=4;$dy -le 18;$dy+=2){$darkest=[Math]::Min($darkest,$during.GetPixel($screenX,[int]($contentBottom+$dy)).R)}
            if($darkest -gt 235){throw "Shadow missing from live drag preview: lightest dark ring=$darkest"}
        }finally{$during.Dispose()}
    }finally{$off.Dispose();$on.Dispose()}
    Click-EditorAction $editor 'Close';Close-AllPreviews
    $screen=[Windows.Forms.Screen]::PrimaryScreen.Bounds
    $x1=$screen.Left+80;$y1=$screen.Top+80
    $x2=[Math]::Min($screen.Right-60,$x1+1600);$y2=[Math]::Min($screen.Bottom-60,$y1+800)
    $beforeWindows=[CaptureInput]::ThumbnailWindows($false);$before=@(Get-Shots)
    [void](Start-Selection)
    [CaptureInput]::HoldAt($x1,$y1)
    [CaptureInput]::MouseAt($x2,$y2,0);[CaptureInput]::DropAt($x2,$y2)
    $largeShot=Wait-NewShot $before;$largePreview=Wait-NewPreview $beforeWindows
    $largeHash=(Get-FileHash -LiteralPath $largeShot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $largePreview 'Annotate';$largeEditor=Wait-Editor
    $r=[CaptureInput]::ClientBounds($largeEditor);$s=[CaptureInput]::GetDpiForWindow($largeEditor)/96.0
    [CaptureInput]::ClickAt([int]($r.Left+88*$s),[int]($r.Top+24*$s));Start-Sleep -Milliseconds 140
    $r=[CaptureInput]::ClientBounds($largeEditor)
    [CaptureInput]::ClickAt([int]($r.Left+131*$s),[int]($r.Top+285*$s))
    Start-Sleep -Milliseconds 220
    $sliderY=[int]($r.Top+660*$s);$start=[int]($r.Left+45*$s)
    [CaptureInput]::HoldAt($start,$sliderY);Start-Sleep -Milliseconds 70
    $clock=[Diagnostics.Stopwatch]::StartNew();$latencies=@()
    for($i=1;$i -le 15;$i++){
        $target=[int]($start+12*$s*$i)
        $step=[Diagnostics.Stopwatch]::StartNew()
        [CaptureInput]::MouseAt($target,$sliderY,0)
        $seen=$false
        while($step.ElapsedMilliseconds -lt 1200){
            $rgb=[CaptureInput]::ScreenPixel(($target-[int](4*$s)),($sliderY+[int](4*$s)))
            if(($rgb -band 255) -gt 215 -and (($rgb -shr 8) -band 255) -gt 215 -and (($rgb -shr 16) -band 255) -gt 215){$seen=$true;break}
            Start-Sleep -Milliseconds 3
        }
        if(-not $seen){throw "Padding thumb did not reach pointer step $i within 1200 ms"}
        $latencies+=$step.ElapsedMilliseconds
        if($i -eq 8){Save-GalleryScreenshot 'background-slider-drag-large.png'}
    }
    [CaptureInput]::DropAt([int]($start+180*$s),$sliderY)
    $clock.Stop()
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$output=Wait-NewShot $before
    if((Get-FileHash -LiteralPath $largeShot -Algorithm SHA256).Hash -ne $largeHash){throw 'Large capture source changed while sliding.'}
    $src=[Drawing.Bitmap]::new($largeShot);$dst=[Drawing.Bitmap]::new($output)
    try{
        if($dst.Width -le $src.Width -or $dst.Height -le $src.Height){throw 'Large capture background export lost its padding.'}
        if($dst.GetPixel(0,0).ToArgb() -eq $src.GetPixel(0,0).ToArgb()){throw 'Large capture gradient was not applied.'}
    }finally{$src.Dispose();$dst.Dispose()}
    $during=[Drawing.Bitmap]::new('.pi/capture-smoke/background-slider-drag-large.png')
    try{
        $thumbX=[int]($r.Left+(45+12*8)*$s);$thumbY=[int]($r.Top+660*$s)
        $thumb=$during.GetPixel($thumbX,$thumbY)
        if($thumb.R -lt 210 -or $thumb.G -lt 210){throw "Slider preview did not keep up with drag midpoint: $thumb"}
    }finally{$during.Dispose()}
    $ordered=@($latencies|Sort-Object)
    Write-Host "PASS: Padding follows large-capture drag ($($clock.ElapsedMilliseconds) ms / 15 steps, median=$($ordered[7]) ms, max=$($ordered[-1]) ms)"
    foreach($case in @(@('Inset',20,710),@('Shadow',20,759),@('Corners',146,759))){
        $label=$case[0];$x=[int]($r.Left+[int]$case[1]*$s);$y=[int]($r.Top+[int]$case[2]*$s)
        [CaptureInput]::HoldAt($x,$y);Start-Sleep -Milliseconds 90
        $times=@()
        for($i=1;$i -le 15;$i++){
            $target=[int]($x+6*$s*$i);$step=[Diagnostics.Stopwatch]::StartNew()
            [CaptureInput]::MouseAt($target,$y,0);$seen=$false
            while($step.ElapsedMilliseconds -lt 1200){
                $rgb=[CaptureInput]::ScreenPixel(($target-[int](4*$s)),($y+[int](4*$s)))
                if(($rgb -band 255) -gt 215 -and (($rgb -shr 8) -band 255) -gt 215 -and (($rgb -shr 16) -band 255) -gt 215){$seen=$true;break}
                Start-Sleep -Milliseconds 3
            }
            if(-not $seen){throw "$label thumb did not follow pointer step $i"}
            $times+=$step.ElapsedMilliseconds
        }
        [CaptureInput]::DropAt([int]($x+90*$s),$y)
        $sorted=@($times|Sort-Object)
        Write-Host "PASS: $label follows large-capture drag (median=$($sorted[7]) ms, max=$($sorted[-1]) ms)"
        Start-Sleep -Milliseconds 120
    }
}
function Test-Background {
    if([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA'){throw 'Run BackgroundOnly with pwsh -Sta.'}
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-1),0,0,0,0,0x13)
    $first=New-TestPreview
    $hash=(Get-FileHash -LiteralPath $first.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $first.Window 'Annotate'
    $editor=Wait-Editor
    $client=[CaptureInput]::ClientBounds($editor)
    $s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    [CaptureInput]::ClickAt([int]($client.Left+88*$s),[int]($client.Top+24*$s))
    Start-Sleep -Milliseconds 300
    $client=[CaptureInput]::ClientBounds($editor)
    if($client.Bottom-$client.Top -lt 770*$s){throw 'Background sidebar failed to expand the native editor viewport.'}
    $at = { param([double]$X,[double]$Y) [CaptureInput]::ClickAt([int]($client.Left+$X*$s),[int]($client.Top+$Y*$s)) }
    & $at 131 285 # gradient row 3, column 3 in the video
    Start-Sleep -Milliseconds 250
    Save-GalleryScreenshot 'background-gradient.png'
    $before=@(Get-Shots)
    [void][CaptureInput]::SetForegroundWindow($editor)
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10)) # Ctrl+Shift+C
    $firstExport=Wait-NewShot $before
    Assert-ClipboardImage $firstExport
    $source=[Drawing.Bitmap]::new($first.Shot)
    $composite=[Drawing.Bitmap]::new($firstExport)
    try {
        if($composite.Width -le $source.Width -or $composite.Height -le $source.Height){throw 'Background Copy did not include padding.'}
        if($composite.GetPixel(0,0).ToArgb() -eq $source.GetPixel(0,0).ToArgb()){throw 'Gradient background did not appear in the output.'}
        $pad=[int](($composite.Width-$source.Width)/2)
        if($composite.GetPixel($pad+35,$pad+35).ToArgb() -ne $source.GetPixel(35,35).ToArgb()){throw 'Background output changed the source screenshot interior pixels.'}
    } finally {$source.Dispose();$composite.Dispose()}
    $before=@(Get-Shots)
    & $at 225 660 # near maximum padding on the native sidebar slider
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    $secondExport=Wait-NewShot $before
    $large=[Drawing.Bitmap]::new($secondExport)
    $small=[Drawing.Bitmap]::new($firstExport)
    try{if($large.Width -le $small.Width -or $large.Height -le $small.Height){throw 'Padding slider failed to update the exported pixel dimensions.'}}
    finally{$large.Dispose();$small.Dispose()}
    $saved=Join-Path $artifacts ('Background export 日本 '+[Guid]::NewGuid().ToString('N')+'.png');$script:created+=$saved
    Click-EditorAction $editor 'Save';Enter-SavePath $saved
    Assert-SavedCopy $saved $secondExport
    if($DragDrop){
        $folder=Join-Path $artifacts ('Background drop '+[Guid]::NewGuid().ToString('N'))
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
        if(-not $script:explorerWindow){throw 'Background drag Explorer folder did not open.'}
        $script:closeExplorer=$script:explorerWindow.HWND -notin $existing
        $explorer=[IntPtr]$script:explorerWindow.HWND
        $screen=[Windows.Forms.Screen]::PrimaryScreen.WorkingArea
        [void][CaptureInput]::MoveWindow($explorer,($screen.Right-720),($screen.Top+60),700,650,$true)
        [void][CaptureInput]::SetForegroundWindow($explorer);Start-Sleep -Milliseconds 150
        $bounds=Preview-Rect $explorer
        $tx=[int]($bounds.Left+($bounds.Right-$bounds.Left)*0.8);$ty=[int]($bounds.Top+($bounds.Bottom-$bounds.Top)*0.6)
        [void][CaptureInput]::SetForegroundWindow($editor)
        $bounds=[CaptureInput]::ClientBounds($editor)
        $x=[int](($bounds.Left+$bounds.Right)/2);$y=[int]($bounds.Bottom-24*$s)
        [CaptureInput]::HoldAt($x,$y);Start-Sleep -Milliseconds 75
        for($i=1;$i -le 25;$i++){
            [CaptureInput]::MouseAt([int]($x+($tx-$x)*$i/25),[int]($y+($ty-$y)*$i/25),0)
            Start-Sleep -Milliseconds 10
        }
        for($i=0;$i -lt 100 -and [CaptureInput]::DragWindow() -eq [IntPtr]::Zero;$i++){Start-Sleep -Milliseconds 25}
        if([CaptureInput]::DragWindow() -eq [IntPtr]::Zero){
            [CaptureInput]::DropAt($tx,$ty)
            Save-GalleryScreenshot 'background-drag-failure.png'
            throw 'Background drag never started after captured multi-step pointer motion.'
        }
        Start-Sleep -Milliseconds 200;[CaptureInput]::DropAt($tx,$ty)
        $copied=Join-Path $folder (Split-Path $secondExport -Leaf);$script:created+=$copied
        Assert-SavedCopy $copied $secondExport
        Write-Host 'PASS: Explorer receives the composed full-resolution PNG by native copy-only OLE drag'
        [void][CaptureInput]::SetForegroundWindow($editor)
    }
    $before=@(Get-Shots)
    & $at 229 704 # Auto-balance compensates for the cast shadow
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    $balanced=Wait-NewShot $before
    if([Convert]::ToBase64String([IO.File]::ReadAllBytes($balanced)) -eq [Convert]::ToBase64String([IO.File]::ReadAllBytes($secondExport))){throw 'Auto-balance did not change the actual export.'}
    $before=@(Get-Shots)
    & $at 35 419 # original abstract wallpaper preset
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    $wallpaper=Wait-NewShot $before
    Assert-ClipboardImage $wallpaper
    $before=@(Get-Shots)
    & $at 35 488 # screenshot-based blur preset
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    $blurred=Wait-NewShot $before
    Assert-ClipboardImage $blurred
    [void][CaptureInput]::MoveWindow($editor,450,180,800,460,$true)
    $client=[CaptureInput]::ClientBounds($editor)
    [CaptureInput]::WheelAt([int]($client.Left+110*$s),[int]($client.Top+270*$s),-720)
    Start-Sleep -Milliseconds 100
    Save-GalleryScreenshot 'background-scrolled.png'
    $before=@(Get-Shots)
    [CaptureInput]::ClickAt([int]($client.Left+16*$s),[int]($client.Top+294*$s)) # Padding at minimum after scrolling
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    $compact=Wait-NewShot $before
    $large=[Drawing.Bitmap]::new($blurred);$small=[Drawing.Bitmap]::new($compact)
    try{if($small.Width -ge $large.Width){throw 'Scroll could not reach the background controls in a short editor window.'}}
    finally{$small.Dispose();$large.Dispose()}
    [void][CaptureInput]::MoveWindow($editor,440,190,1040,820,$true)
    $client=[CaptureInput]::ClientBounds($editor)
    [CaptureInput]::WheelAt([int]($client.Left+110*$s),[int]($client.Top+270*$s),720)
    Start-Sleep -Milliseconds 75
    & $at 55 553 # Solid white makes the outside-of-image shadow measurable
    & $at 110 759 # Strong shadow
    & $at 143 759 # Square screenshot corners
    $before=@(Get-Shots)
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    $squareShadow=Wait-NewShot $before
    & $at 241 759 # Maximum rounding: the shadow should bend too
    Save-GalleryScreenshot 'background-rounded-shadow.png'
    $before=@(Get-Shots)
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    $roundedShadow=Wait-NewShot $before
    $square=[Drawing.Bitmap]::new($squareShadow);$rounded=[Drawing.Bitmap]::new($roundedShadow)
    $original=[Drawing.Bitmap]::new($first.Shot)
    try {
        if($square.Width -ne $rounded.Width -or $square.Height -ne $rounded.Height){throw 'Changing Corners changed the background size.'}
        $frame=[int](($square.Width-$original.Width)/2)
        $cornerBefore=$square.GetPixel($frame-1,$frame-1)
        $cornerAfter=$rounded.GetPixel($frame-1,$frame-1)
        if($cornerBefore.ToArgb() -eq $cornerAfter.ToArgb()){
            Save-GalleryScreenshot 'background-corner-shadow-failure.png'
            throw 'Corners rounds the screenshot but leaves its outside drop shadow square.'
        }
    } finally {$square.Dispose();$rounded.Dispose();$original.Dispose()}
    & $at 16 759 # Shadow off to isolate fractional corner motion
    & $at 142 759 # Corners at zero
    $before=@(Get-Shots)
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    $zeroCorner=Wait-NewShot $before
    $cornerX=[int]($client.Left+142*$s);$cornerY=[int]($client.Top+759*$s)
    [CaptureInput]::HoldAt($cornerX,$cornerY)
    [CaptureInput]::MouseAt(($cornerX+[int]$s),$cornerY,0)
    [CaptureInput]::DropAt(($cornerX+[int]$s),$cornerY) # One-pixel drag, not just a click
    $before=@(Get-Shots)
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    $fractionalCorner=Wait-NewShot $before
    $zero=[Drawing.Bitmap]::new($zeroCorner);$fractional=[Drawing.Bitmap]::new($fractionalCorner)
    try {
        $frame=[int](($zero.Width-350)/2)
        if($zero.GetPixel($frame,$frame).ToArgb() -eq $fractional.GetPixel($frame,$frame).ToArgb()){
            throw 'Moving Corners by one slider pixel was silently rounded away in the actual preview/export.'
        }
    } finally {$zero.Dispose();$fractional.Dispose()}
    & $at 142 759 # Square corner isolates the shadow intensity
    & $at 16 759 # No shadow
    & $at 17 759 # First slider pixel must fade in, not jump to full opacity
    $before=@(Get-Shots)
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    $subpixelShadow=Wait-NewShot $before
    $shadowImage=[Drawing.Bitmap]::new($subpixelShadow)
    try {
        $frame=[int](($shadowImage.Width-350)/2)
        $edge=$shadowImage.GetPixel($frame+175,$frame+200)
        if($edge.R -lt 235 -or $edge.G -lt 235 -or $edge.B -lt 235){
            throw "Shadow jumps from zero to full strength at the first slider pixel: $edge"
        }
    } finally {$shadowImage.Dispose()}
    & $at 70 101 # None restores the original, non-destructive output
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10))
    Assert-ClipboardImage $first.Shot
    if((Get-FileHash -LiteralPath $first.Shot -Algorithm SHA256).Hash -ne $hash){throw 'Background Tool changed the original PNG.'}
    Write-Host 'PASS: native Background panel, gradients/wallpapers/blur, live controls on small windows, auto-balance, Save/Copy output, None removes background and original PNG intact'
}
function Test-Image {
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $preview=New-TestPreview
    $sourceHash=(Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $preview.Window 'Annotate'
    $editor=Wait-Editor
    $fixture=Join-Path $artifacts 'annotation-import.png'
    $overlay=[Drawing.Bitmap]::new(24,24,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try{
        for($y=0;$y -lt 24;$y++){for($x=0;$x -lt 24;$x++){$overlay.SetPixel($x,$y,[Drawing.Color]::FromArgb(128,255,0,255))}}
        $overlay.Save($fixture,[Drawing.Imaging.ImageFormat]::Png)
    }finally{$overlay.Dispose()}
    Click-EditorAction $editor 'AddImage'
    $dialog=[IntPtr]::Zero
    for($i=0;$i -lt 100;$i++){
        $candidate=[CaptureInput]::GetForegroundWindow()
        if($candidate -ne $editor -and $candidate -ne [IntPtr]::Zero){$dialog=$candidate;break}
        Start-Sleep -Milliseconds 50
    }
    if($dialog -eq [IntPtr]::Zero){throw 'Native Add Image dialog did not activate.'}
    [void][CaptureInput]::SetForegroundWindow($dialog)
    [CaptureInput]::keybd_event(18,0,0,[UIntPtr]::Zero);Press-Key 0x4E;[CaptureInput]::keybd_event(18,0,2,[UIntPtr]::Zero)
    [CaptureInput]::keybd_event(17,0,0,[UIntPtr]::Zero);Press-Key 0x41;[CaptureInput]::keybd_event(17,0,2,[UIntPtr]::Zero)
    [CaptureInput]::TypeText($fixture);Press-Key 13
    for($i=0;$i -lt 100 -and [CaptureInput]::IsWindowVisible($dialog);$i++){Start-Sleep -Milliseconds 50}
    if([CaptureInput]::IsWindowVisible($dialog)){throw 'Add Image dialog did not close.'}
    Start-Sleep -Milliseconds 700
    Save-GalleryScreenshot 'editor-added-image.png'
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$out=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($out)
    try{
        $p=$image.GetPixel(171,100)
        if($p.R -lt 240 -or $p.B -lt 115 -or $p.B -gt 140){throw "Imported transparent image was not composited in export: $p"}
    }finally{$image.Dispose()}
    if((Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash -ne $sourceHash){throw 'Import modified source PNG.'}
    Write-Host 'PASS: native Add Image dialog, transparent imported PNG in full-resolution output, source unchanged'
}
function Count-WhiteGlyph([string]$Path,[int]$X,[int]$Y,[int]$Radius) {
    $image=[Drawing.Bitmap]::new($Path);$count=0
    try{
        for($dy=-$Radius;$dy -le $Radius;$dy++) {for($dx=-$Radius;$dx -le $Radius;$dx++) {
            $p=$image.GetPixel($X+$dx,$Y+$dy)
            if($p.R -gt 220 -and $p.G -gt 220 -and $p.B -gt 220){$count++}
        }}
    }finally{$image.Dispose()}
    return $count
}
function Test-Polish {
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $preview=New-TestPreview
    $hash=(Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $preview.Window 'Annotate';$editor=Wait-Editor
    $bounds=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $sx=[int]($bounds.Left+($bounds.Right-$bounds.Left-350*$s)/2)
    $sy=[int]($bounds.Top+48*$s+($bounds.Bottom-$bounds.Top-96*$s-200*$s)/2)
    Click-EditorAction $editor 'Counter';[CaptureInput]::ClickAt([int]($sx+175*$s),[int]($sy+100*$s))
    Start-Sleep -Milliseconds 100;Save-GalleryScreenshot 'editor-polish-counter-100.png'
    $normal=Count-WhiteGlyph '.pi/capture-smoke/editor-polish-counter-100.png' ([int]($sx+175*$s)) ([int]($sy+100*$s)) ([int](9*$s))
    Click-EditorAction $editor 'Zoom';Select-TrayItem '200%' -Accessible
    Start-Sleep -Milliseconds 150;Save-GalleryScreenshot 'editor-polish-counter-200.png'
    $enlarged=Count-WhiteGlyph '.pi/capture-smoke/editor-polish-counter-200.png' ([int]($sx+175*$s)) ([int]($sy+100*$s)) ([int](18*$s))
    Click-EditorAction $editor 'Zoom';Select-TrayItem '100%' -Accessible
    Click-EditorAction $editor 'Pencil'
    [CaptureInput]::HoldAt([int]($sx+250*$s),[int]($sy+175*$s))
    [CaptureInput]::MouseAt([int]($sx+290*$s),[int]($sy+175*$s),0)
    [CaptureInput]::DropAt([int]($sx+290*$s),[int]($sy+175*$s))
    Start-Sleep -Milliseconds 140;Save-GalleryScreenshot 'editor-polish-pencil.png'
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$output=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($output)
    try{$badge=$image.GetPixel(175,90);if($badge.R -lt 180 -or $badge.G -gt 130){throw "Default color is not red: $badge"}}finally{$image.Dispose()}
    if($normal -lt 10 -or $enlarged -lt $normal*2.5){throw "Counter numeral does not scale with zoom: $normal -> $enlarged white pixels"}
    $image=[Drawing.Bitmap]::new('.pi/capture-smoke/editor-polish-pencil.png')
    try{$p=$image.GetPixel([int]($sx+250*$s),[int]($sy+175*$s));if($p.R -gt 220 -and $p.G -gt 220 -and $p.B -gt 220){throw "Pencil is still selected with a white resize handle: $p"}}finally{$image.Dispose()}
    if((Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash -ne $hash){throw 'Editing changed the source.'}
    Write-Host 'PASS: default red, Pencil ends without edit handles, Counter numeral scales with zoom, source unchanged'
}
function Test-Editable {
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $preview=New-TestPreview
    $hash=(Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $preview.Window 'Annotate'
    $editor=Wait-Editor
    $bounds=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $sx=[int]($bounds.Left+($bounds.Right-$bounds.Left-350*$s)/2)
    $sy=[int]($bounds.Top+48*$s+($bounds.Bottom-$bounds.Top-96*$s-200*$s)/2)
    Click-EditorAction $editor 'Rectangle'
    Click-EditorAction $editor 'Color'
    $origin=Editor-ToolOrigin $editor
    [CaptureInput]::ClickAt([int]($bounds.Left+($origin+381)*$s),[int]($bounds.Top+102*$s))
    [CaptureInput]::HoldAt([int]($sx+40*$s),[int]($sy+145*$s))
    [CaptureInput]::MouseAt([int]($sx+120*$s),[int]($sy+185*$s),0)
    [CaptureInput]::DropAt([int]($sx+120*$s),[int]($sy+185*$s))
    Start-Sleep -Milliseconds 160
    Save-GalleryScreenshot 'editor-editable-after-placement.png'
    $screen=[Drawing.Bitmap]::new('.pi/capture-smoke/editor-editable-after-placement.png')
    try{$p=$screen.GetPixel([int]($sx+40*$s),[int]($sy+145*$s));if($p.R -lt 225 -or $p.G -lt 225 -or $p.B -lt 225){throw "New shape has no edit handle: $p"}}finally{$screen.Dispose()}
    [CaptureInput]::HoldAt([int]($sx+80*$s),[int]($sy+165*$s))
    [CaptureInput]::MouseAt([int]($sx+100*$s),[int]($sy+145*$s),0)
    [CaptureInput]::DropAt([int]($sx+100*$s),[int]($sy+145*$s))
    [CaptureInput]::HoldAt([int]($sx+140*$s),[int]($sy+165*$s))
    [CaptureInput]::MouseAt([int]($sx+160*$s),[int]($sy+175*$s),0)
    [CaptureInput]::DropAt([int]($sx+160*$s),[int]($sy+175*$s))
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$resized=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($resized)
    try{$p=$image.GetPixel(160,155);if($p.R -lt 210 -or $p.B -gt 110){throw "Shape did not resize directly in drawing mode: $p"}}finally{$image.Dispose()}
    [CaptureInput]::HoldAt([int]($sx+110*$s),[int]($sy+89*$s))
    [CaptureInput]::MouseAt([int]($sx+171*$s),[int]($sy+150*$s),0)
    [CaptureInput]::DropAt([int]($sx+171*$s),[int]($sy+150*$s))
    Start-Sleep -Milliseconds 120
    Save-GalleryScreenshot 'editor-editable-rotated.png'
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$rotated=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($rotated)
    try{
        $edge=$image.GetPixel(135,150);$ghost=$image.GetPixel(60,145)
        if($edge.R -lt 190 -or $edge.B -gt 150){throw "Rotated rectangle did not export its new edge: $edge"}
        if($ghost.B -lt 180 -or $ghost.R -gt 100){throw "Old edge remained after rotation: $ghost"}
    }finally{$image.Dispose()}
    [CaptureInput]::Chord(0x5a,[ushort[]]@(0x11))
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$undone=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($undone)
    try{$p=$image.GetPixel(160,155);if($p.R -lt 210 -or $p.B -gt 110){throw 'Undo did not restore the unrotated rectangle.'}}finally{$image.Dispose()}
    if((Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash -ne $hash){throw 'Editing changed source PNG.'}
    Write-Host 'PASS: new shape stays selected, direct move/resize/rotation, undo and unchanged source'
}
function Test-Crop {
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $preview=New-TestPreview
    $sourceHash=(Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $preview.Window 'Annotate'
    $editor=Wait-Editor
    $bounds=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $sx=[int]($bounds.Left+($bounds.Right-$bounds.Left-350*$s)/2)
    $sy=[int]($bounds.Top+48*$s+($bounds.Bottom-$bounds.Top-96*$s-200*$s)/2)
    Click-EditorAction $editor 'Crop'
    Save-GalleryScreenshot 'editor-crop-initial.png'
    $x1=[int]($sx+30*$s);$y1=[int]($sy+20*$s);$x2=[int]($sx+280*$s);$y2=[int]($sy+170*$s)
    [CaptureInput]::HoldAt($sx,$sy);[CaptureInput]::MouseAt($x1,$y1,0);[CaptureInput]::DropAt($x1,$y1)
    [CaptureInput]::HoldAt([int]($sx+350*$s),[int]($sy+200*$s));[CaptureInput]::MouseAt($x2,$y2,0);[CaptureInput]::DropAt($x2,$y2)
    Save-GalleryScreenshot 'editor-crop-pending.png'
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));Assert-ClipboardImage $preview.Shot
    [CaptureInput]::ClickAt([int]($bounds.Right-40*$s),[int]($bounds.Top+76*$s))
    Save-GalleryScreenshot 'editor-cropped.png'
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$cropped=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($cropped)
    try{
        if($image.Width -ne 250 -or $image.Height -ne 150){throw "Crop export size mismatch: $($image.Width)x$($image.Height)"}
        if($image.GetPixel(20,50).ToArgb() -ne [Drawing.Color]::Red.ToArgb()){throw 'Crop content offset is wrong.'}
    }finally{$image.Dispose()}
    [CaptureInput]::Chord(0x5a,[ushort[]]@(0x11))
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));Assert-ClipboardImage $preview.Shot
    [CaptureInput]::Chord(0x59,[ushort[]]@(0x11))
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$redone=Wait-NewShot $before
    $again=[Drawing.Bitmap]::new($redone);try{if($again.Width -ne 250 -or $again.Height -ne 150){throw 'Redo did not reapply crop.'}}finally{$again.Dispose()}
    Click-EditorAction $editor 'Crop'
    [CaptureInput]::ClickAt([int]($bounds.Right-115*$s),[int]($bounds.Top+76*$s))
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));Assert-ClipboardImage $redone
    Click-EditorAction $editor 'Crop'
    [CaptureInput]::HoldAt($x2,$y2)
    [CaptureInput]::MouseAt([int]($x1+5*$s),[int]($y1+5*$s),0)
    [CaptureInput]::DropAt([int]($x1+5*$s),[int]($y1+5*$s))
    Start-Sleep -Milliseconds 100
    Save-GalleryScreenshot 'editor-crop-minimum-pending.png'
    [CaptureInput]::ClickAt([int]($bounds.Right-40*$s),[int]($bounds.Top+76*$s))
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$minimum=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($minimum)
    try{if($image.Width -lt 32 -or $image.Height -lt 32){throw "Crop collapsed to $($image.Width)x$($image.Height) pixels."}}finally{$image.Dispose()}
    if((Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash -ne $sourceHash){throw 'Crop modified source PNG.'}
    Write-Host 'PASS: crop handle minimum, explicit apply/cancel, undo/redo, unchanged original PNG'
}
function Test-Text {
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $preview=New-TestPreview
    $sourceHash=(Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $preview.Window 'Annotate'
    $editor=Wait-Editor
    $bounds=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $sx=[int]($bounds.Left+($bounds.Right-$bounds.Left-350*$s)/2)
    $sy=[int]($bounds.Top+48*$s+($bounds.Bottom-$bounds.Top-96*$s-200*$s)/2)
    Click-EditorAction $editor 'Text'
    [CaptureInput]::ClickAt([int]($sx+45*$s),[int]($sy+125*$s))
    [CaptureInput]::TypeText('Halo ')
    [CaptureInput]::TypeText('世界')
    Start-Sleep -Milliseconds 250
    Save-GalleryScreenshot 'editor-native-text-input.png'
    [CaptureInput]::Chord(0x0d,[ushort[]]@(0x11)) # Ctrl+Enter commits native EDIT
    Start-Sleep -Milliseconds 150
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$out=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($out)
    try{
        $changed=$false
        for($y=125;$y -lt 155;$y++){for($x=45;$x -lt 180;$x++){
            $p=$image.GetPixel($x,$y);if($p.R -gt 160 -and $p.B -lt 170){$changed=$true;break}
        };if($changed){break}}
        if(-not $changed){throw 'Text did not appear in full-resolution PNG.'}
    }finally{$image.Dispose()}
    if((Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash -ne $sourceHash){throw 'Text mutated original capture PNG.'}
    Write-Host 'PASS: native Unicode text input, DirectWrite raster export, source PNG unchanged'
}
function Test-Mosaic {
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $preview=New-TestPreview
    $sourceHash=(Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $preview.Window 'Annotate'
    $editor=Wait-Editor
    $bounds=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $sx=[int]($bounds.Left+($bounds.Right-$bounds.Left-350*$s)/2)
    $sy=[int]($bounds.Top+48*$s+($bounds.Bottom-$bounds.Top-96*$s-200*$s)/2)
    Click-EditorAction $editor 'Pixelate'
    $x1=[int]($sx+45*$s);$y1=[int]($sy+65*$s)
    $x2=[int]($sx+290*$s);$y2=[int]($sy+140*$s)
    [CaptureInput]::HoldAt($x1,$y1)
    for($i=1;$i -le 8;$i++){[CaptureInput]::MouseAt([int]($x1+($x2-$x1)*$i/8),[int]($y1+($y2-$y1)*$i/8),0)}
    [CaptureInput]::DropAt($x2,$y2)
    Save-GalleryScreenshot 'editor-mosaic-source-based.png'
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$rendered=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($rendered)
    try {
        $red=$image.GetPixel(80,90);$green=$image.GetPixel(230,90)
        if($red.R -le $red.G*2 -or $green.G -le $green.R*2){throw "Mosaic is not derived from red/green screenshot regions: $red / $green"}
        if($image.GetPixel(20,20).ToArgb() -ne [Drawing.Color]::Blue.ToArgb()){throw 'Mosaic modified pixels outside selection.'}
    }finally{$image.Dispose()}
    Click-EditorAction $editor 'Spotlight'
    $x1=[int]($sx+45*$s);$y1=[int]($sy+65*$s)
    $x2=[int]($sx+290*$s);$y2=[int]($sy+140*$s)
    [CaptureInput]::HoldAt($x1,$y1);[CaptureInput]::MouseAt($x2,$y2,0);[CaptureInput]::DropAt($x2,$y2)
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$spotlight=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($spotlight)
    try {
        $inside=$image.GetPixel(80,90);$outside=$image.GetPixel(20,20)
        if($inside.R -le $inside.G*2 -or $outside.B -ge 130){throw "Spotlight export didn't dim outside while retaining focus: $inside / $outside"}
    }finally{$image.Dispose()}
    Click-EditorAction $editor 'Counter'
    [CaptureInput]::ClickAt([int]($sx+305*$s),[int]($sy+90*$s))
    [CaptureInput]::ClickAt([int]($sx+305*$s),[int]($sy+135*$s))
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$counted=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($counted)
    try {
        $first=$image.GetPixel(315,90);$second=$image.GetPixel(315,135)
        if($first.R -lt 190 -or $second.R -lt 190 -or $first.B -lt 60){throw "Counters did not export as colored badges: $first / $second"}
    }finally{$image.Dispose()}
    if((Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash -ne $sourceHash){throw 'Annotation modified source PNG.'}
    Write-Host 'PASS: image-derived mosaic, Spotlight and sequential Counter export; original PNG unchanged'
}
function Test-Picker {
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $preview=New-TestPreview
    $sourceHash=(Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $preview.Window 'Annotate'
    $editor=Wait-Editor
    $bounds=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $origin=Editor-ToolOrigin $editor
    $sx=[int]($bounds.Left+($bounds.Right-$bounds.Left-350*$s)/2)
    $sy=[int]($bounds.Top+48*$s+($bounds.Bottom-$bounds.Top-96*$s-200*$s)/2)
    Click-EditorAction $editor 'Rectangle'
    $aX=[int]($sx+20*$s);$aY=[int]($sy+160*$s)
    $bX=[int]($sx+120*$s);$bY=[int]($sy+190*$s)
    [CaptureInput]::HoldAt($aX,$aY);[CaptureInput]::MouseAt($bX,$bY,0);[CaptureInput]::DropAt($bX,$bY)
    Click-EditorAction $editor 'Move'
    Click-EditorAction $editor 'Color'
    $colorX=[int]($bounds.Left+($origin+381)*$s)
    [CaptureInput]::ClickAt($colorX,[int]($bounds.Top+390*$s))
    Save-GalleryScreenshot 'editor-custom-picker.png'
    $pickerX=[int]($bounds.Left+($origin+362-220)*$s)
    $pickerY=[int]($bounds.Top+52*$s)
    [CaptureInput]::ClickAt([int]($pickerX+100*$s),[int]($pickerY+258*$s))
    [CaptureInput]::TypeText('20A0F0')
    Press-Key 13
    [CaptureInput]::ClickAt([int]($pickerX+270*$s),[int]($pickerY+339*$s))
    Click-EditorAction $editor 'Stroke'
    Start-Sleep -Milliseconds 180
    Save-GalleryScreenshot 'editor-stroke-slider.png'
    $sliderX=[int]($bounds.Left+($origin+423-152)*$s)
    $sliderY=[int]($bounds.Top+52*$s)
    $startX=[int]($sliderX+18*$s);$dragX=[int]($sliderX+156*$s);$trackY=[int]($sliderY+121*$s)
    [CaptureInput]::HoldAt($startX,$trackY)
    for($i=1;$i -le 10;$i++){[CaptureInput]::MouseAt([int]($startX+($dragX-$startX)*$i/10),$trackY,0)}
    [CaptureInput]::DropAt($dragX,$trackY)
    Save-GalleryScreenshot 'editor-stroke-slider-thick.png'
    [CaptureInput]::ClickAt([int]($bounds.Left+50*$s),[int]($bounds.Top+340*$s))
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$rendered=Wait-NewShot $before
    $image=[Drawing.Bitmap]::new($rendered)
    try{
        $c=$image.GetPixel(60,164)
        if($c.R -ne 32 -or $c.G -ne 160 -or $c.B -ne 240){throw "Picker or slider did not update selected annotation: $c"}
    }finally{$image.Dispose()}
    if((Get-FileHash -LiteralPath $preview.Shot -Algorithm SHA256).Hash -ne $sourceHash){throw 'Picker modified original screenshot.'}
    Write-Host 'PASS: dark custom picker Hex recolors selected mark, live stroke slider updates export, source remains intact'
}
function Test-Hover {
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $preview=New-TestPreview
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $preview.Window 'Annotate'
    $editor=Wait-Editor
    Click-EditorAction $editor 'Rectangle'
    $bounds=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $origin=Editor-ToolOrigin $editor
    $rectX=[int]($bounds.Left+($origin+43)*$s)
    $fillX=[int]($bounds.Left+($origin+72)*$s)
    $y=[int]($bounds.Top+24*$s)
    [CaptureInput]::MouseAt($fillX,$y,0)
    Start-Sleep -Milliseconds 240
    Save-GalleryScreenshot 'editor-hover-adjacent.png'
    $bitmap=[Drawing.Bitmap]::new((Join-Path $artifacts 'editor-hover-adjacent.png'))
    try{
        $edge=$bitmap.GetPixel([int]($rectX+16*$s),$y)
        $left=$bitmap.GetPixel([int]($fillX-8*$s),$y)
        $right=$bitmap.GetPixel([int]($fillX+8*$s),$y)
    }finally{$bitmap.Dispose()}
    if($edge.R -gt 30 -or $edge.G -lt 90 -or $edge.B -lt 180){throw "Hover obscures blue active tool: $edge"}
    if($left.ToArgb() -ne $right.ToArgb()){throw "Hover pill is off-center: left=$left right=$right"}
    Write-Host 'PASS: adjacent hover stays centered on its icon and does not obscure the blue active pill'
    [CaptureInput]::ClickAt($fillX,$y)
    $samples=@();$clock=[Diagnostics.Stopwatch]::StartNew()
    for($frame=0;$frame -lt 20;$frame++){
        $strip=[Drawing.Bitmap]::new(92,1);$g=[Drawing.Graphics]::FromImage($strip)
        try{
            $g.CopyFromScreen(($rectX-24),$y,0,0,$strip.Size)
            $blue=@(for($i=0;$i -lt $strip.Width;$i++){$c=$strip.GetPixel($i,0);if($c.R -lt 25 -and $c.G -gt 100 -and $c.B -gt 210){$i}})
            if($blue.Count -gt 0){$samples+=('{0}:{1:n1}' -f $clock.ElapsedMilliseconds,(($blue[0]+$blue[-1])/2))}
        } finally {$g.Dispose();$strip.Dispose()}
        Start-Sleep -Milliseconds 15
    }
    Write-Host "Blue pill centers (ms:screen-px): $($samples -join ', ')"
}
function Test-Drawing {
    if([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA'){throw 'Run DrawOnly with pwsh -Sta.'}
    Close-AllPreviews;Set-AutoClose 'Never' 'never'
    $first=New-TestPreview
    $sourceHash=(Get-FileHash -LiteralPath $first.Shot -Algorithm SHA256).Hash
    [void][CaptureInput]::SetWindowPos($sceneWindow,[IntPtr](-2),0,0,0,0,0x13)
    Click-PreviewAction $first.Window 'Annotate'
    $editor=Wait-Editor
    $bounds=[CaptureInput]::ClientBounds($editor);$s=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $sx=[int]($bounds.Left+($bounds.Right-$bounds.Left-350*$s)/2)
    $sy=[int]($bounds.Top+48*$s+($bounds.Bottom-$bounds.Top-96*$s-200*$s)/2)
    $point={param([int]$X,[int]$Y) @{X=[int]($sx+$X*$s);Y=[int]($sy+$Y*$s)}}
    $sample={param([string]$Path,[int]$X,[int]$Y) $image=[Drawing.Bitmap]::new($Path);try{$image.GetPixel($X,$Y)}finally{$image.Dispose()}}
    Click-EditorAction $editor 'Rectangle'
    Click-EditorAction $editor 'Color'
    $cx=[int]($bounds.Left+((Editor-ToolOrigin $editor)+381)*$s)
    [CaptureInput]::ClickAt($cx,[int]($bounds.Top+102*$s)) # red preset
    $a=& $point 20 160;$b=& $point 120 190
    [CaptureInput]::HoldAt($a.X,$a.Y)
    for($i=1;$i -le 12;$i++){[CaptureInput]::MouseAt([int]($a.X+($b.X-$a.X)*$i/12),[int]($a.Y+($b.Y-$a.Y)*$i/12),0)}
    [CaptureInput]::DropAt($b.X,$b.Y)
    Save-GalleryScreenshot 'editor-drawing-rectangle.png'
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$outlined=Wait-NewShot $before
    Assert-ClipboardImage $outlined
    if((& $sample $outlined 60 160).ToArgb() -ne [Drawing.Color]::FromArgb(249,45,58).ToArgb()){throw 'Rectangle outline was not exported at original image coordinates.'}
    if((& $sample $outlined 60 175).ToArgb() -ne [Drawing.Color]::Blue.ToArgb()){throw 'Outline rectangle filled its interior.'}
    Click-EditorAction $editor 'Move'
    $a=& $point 60 160;$b=& $point 90 175
    [CaptureInput]::HoldAt($a.X,$a.Y);[CaptureInput]::MouseAt($b.X,$b.Y,0);[CaptureInput]::DropAt($b.X,$b.Y)
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$moved=Wait-NewShot $before
    if((& $sample $moved 90 175).ToArgb() -ne [Drawing.Color]::FromArgb(249,45,58).ToArgb()){throw 'Move did not update the editable object/export.'}
    if((& $sample $moved 60 160).ToArgb() -ne [Drawing.Color]::Blue.ToArgb()){throw 'Move retained a baked-in ghost in the screenshot.'}
    [CaptureInput]::Chord(0x5a,[ushort[]]@(0x11)) # Ctrl+Z
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$undone=Wait-NewShot $before
    if((& $sample $undone 60 160).ToArgb() -ne [Drawing.Color]::FromArgb(249,45,58).ToArgb()){throw 'Undo did not restore the original object position.'}
    [CaptureInput]::Chord(0x59,[ushort[]]@(0x11)) # Ctrl+Y
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$redone=Wait-NewShot $before
    if((& $sample $redone 90 175).ToArgb() -ne [Drawing.Color]::FromArgb(249,45,58).ToArgb()){throw 'Redo did not reapply object movement.'}
    Click-EditorAction $editor 'Pencil'
    Click-EditorAction $editor 'Color'
    [CaptureInput]::ClickAt($cx,[int]($bounds.Top+166*$s)) # yellow preset
    $a=& $point 200 165;$b=& $point 300 165
    [CaptureInput]::HoldAt($a.X,$a.Y)
    foreach($xy in @(@(230,180),@(270,180),@(300,165))){$mid=& $point $xy[0] $xy[1];[CaptureInput]::MouseAt($mid.X,$mid.Y,0)}
    [CaptureInput]::DropAt($b.X,$b.Y)
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$drawn=Wait-NewShot $before
    $yellow=& $sample $drawn 300 165
    if($yellow.R -lt 240 -or $yellow.G -lt 200 -or $yellow.B -gt 20){throw "Pencil/export color is wrong: $yellow"}
    $destination=Join-Path $artifacts ('Annotated export 日本 '+[Guid]::NewGuid().ToString('N')+'.png');$script:created+=$destination
    Click-EditorAction $editor 'Save';Enter-SavePath $destination
    Assert-SavedCopy $destination $drawn
    Click-EditorAction $editor 'Arrow'
    $a=& $point 160 105;$b=& $point 280 105
    [CaptureInput]::HoldAt($a.X,$a.Y);[CaptureInput]::MouseAt($b.X,$b.Y,0);[CaptureInput]::DropAt($b.X,$b.Y)
    Click-EditorAction $editor 'Move'
    $a=& $point 220 105;$b=& $point 220 145
    [CaptureInput]::HoldAt($a.X,$a.Y);[CaptureInput]::MouseAt($b.X,$b.Y,0);[CaptureInput]::DropAt($b.X,$b.Y)
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$bent=Wait-NewShot $before
    $bendPixel=& $sample $bent 220 125
    if($bendPixel.R -lt 240 -or $bendPixel.G -lt 200 -or $bendPixel.B -gt 20){throw "Dragging the arrow control handle did not bend the exported arrow: $bendPixel"}
    $straightPixel=& $sample $bent 220 105
    if($straightPixel.R -gt 240 -and $straightPixel.G -gt 200 -and $straightPixel.B -lt 20){throw 'Bent arrow left a baked straight-line ghost.'}
    Click-EditorAction $editor 'Highlighter' # deliberately disabled until text-aware highlighting exists
    $a=& $point 5 15;$b=& $point 90 40
    [CaptureInput]::HoldAt($a.X,$a.Y);[CaptureInput]::MouseAt($b.X,$b.Y,0);[CaptureInput]::DropAt($b.X,$b.Y)
    [CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));Assert-ClipboardImage $bent
    Click-EditorAction $editor 'Pixelate'
    $a=& $point 300 20;$b=& $point 340 65
    [CaptureInput]::HoldAt($a.X,$a.Y);[CaptureInput]::MouseAt($b.X,$b.Y,0);[CaptureInput]::DropAt($b.X,$b.Y)
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$redacted=Wait-NewShot $before
    Save-GalleryScreenshot 'editor-mosaic-source-based.png'
    $mosaicPixel=& $sample $redacted 320 30
    if($mosaicPixel.B -lt 200 -or $mosaicPixel.R -gt 80 -or $mosaicPixel.A -ne 255){
        throw "Image-derived mosaic failed to preserve screenshot colors: $mosaicPixel"
    }
    Click-EditorAction $editor 'Move'
    $selectArrow=& $point 220 125
    [CaptureInput]::ClickAt($selectArrow.X,$selectArrow.Y)
    Click-EditorAction $editor 'Background'
    $panel=[CaptureInput]::ClientBounds($editor)
    [CaptureInput]::ClickAt([int]($panel.Left+131*$s),[int]($panel.Top+285*$s))
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$framed=Wait-NewShot $before
    $base=[Drawing.Bitmap]::new($first.Shot);$outputImage=[Drawing.Bitmap]::new($framed)
    try {
        if($outputImage.Width -le $base.Width -or $outputImage.Height -le $base.Height){throw 'Background disappeared after drawing.'}
        $pad=[int](($outputImage.Width-$base.Width)/2)
        if($outputImage.GetPixel($pad+90,$pad+175).ToArgb() -ne [Drawing.Color]::FromArgb(249,45,58).ToArgb()){
            throw 'Annotations shifted relative to the original after enabling Background Tool.'
        }
    } finally {$base.Dispose();$outputImage.Dispose()}
    Click-EditorAction $editor 'Color'
    [CaptureInput]::ClickAt($cx,[int]($bounds.Top+390*$s)) # custom color wheel
    if([CaptureInput]::DialogWindow([uint32]$app.Id) -ne [IntPtr]::Zero){throw 'Color picker unexpectedly opened the old modal dialog.'}
    Save-GalleryScreenshot 'editor-custom-picker.png'
    $pickerX=[int]($bounds.Left+((Editor-ToolOrigin $editor)+362-220)*$s)
    $pickerY=[int]($bounds.Top+52*$s)
    [CaptureInput]::ClickAt([int]($pickerX+100*$s),[int]($pickerY+258*$s))
    [CaptureInput]::TypeText('FFFF00');Press-Key 13
    [CaptureInput]::ClickAt([int]($pickerX+270*$s),[int]($pickerY+339*$s))
    if([CaptureInput]::EditorCount() -ne 1){throw 'Color picker lost its editor session.'}
    $before=@(Get-Shots);[CaptureInput]::Chord(0x43,[ushort[]]@(0x11,0x10));$recolored=Wait-NewShot $before
    $newColor=& $sample $recolored ($pad+220) ($pad+125)
    if($newColor.R -ne 255 -or $newColor.G -ne 255 -or $newColor.B -ne 0){throw "Custom picker did not recolor selected arrow: $newColor"}
    if((Get-FileHash -LiteralPath $first.Shot -Algorithm SHA256).Hash -ne $sourceHash){throw 'Annotate modified the original capture PNG.'}
    Write-Host 'PASS: native Rectangle/Pencil/curved Arrow/image-derived Mosaic, disabled smart Highlighter, Move and control handle, undo/redo, clipboard/Save/Background, native dark custom color picker, source intact'
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
    if([CaptureInput]::TaggedWindow('Catch It editor',[uint32]$app.Id) -ne $editor){throw 'Editor accessible window name is missing.'}
    $frame=Preview-Rect $editor;$client=[CaptureInput]::ClientBounds($editor)
    if($client.Top -ne $frame.Top -or $client.Left -ne $frame.Left){Save-GalleryScreenshot 'editor-caption-failure.png';throw "Caption offsets: window $($frame.Left),$($frame.Top); client $($client.Left),$($client.Top)."}
    if($frame.Right-$frame.Left -gt 1200*[CaptureInput]::GetDpiForWindow($editor)/96.0){throw 'Editor opens too large by default.'}
    Save-GalleryScreenshot 'editor-shell.png'
    Click-PreviewAction $first.Window 'Annotate'
    if([CaptureInput]::EditorCount() -ne 1){throw 'Repeated Annotate created duplicate sessions.'}
    Click-EditorAction $editor 'Rectangle'
    if([CaptureInput]::EditorCount() -ne 1 -or [CaptureInput]::DragWindow() -ne [IntPtr]::Zero){throw 'Selecting Rectangle launched an unrelated action.'}
    Click-EditorAction $editor 'Move'
    Write-Host 'PASS: real Annotate opens one activated native editor, original full-resolution pixels, drawing tool selection stays in editor'

    $bounds=[CaptureInput]::ClientBounds($editor);$scale=[CaptureInput]::GetDpiForWindow($editor)/96.0
    $colorX=[int]($bounds.Left+((Editor-ToolOrigin $editor)+381)*$scale)
    $swatchX=[int]($colorX-4*$scale)
    Click-EditorAction $editor 'Color'
    Assert-EditorPalettePixel $colorX ([int]($bounds.Top+102*$scale)) ([Drawing.Color]::FromArgb(249,45,58).ToArgb())
    Save-GalleryScreenshot 'editor-palette.png'
    [CaptureInput]::ClickAt($colorX,[int]($bounds.Top+102*$scale))
    Assert-EditorPalettePixel $swatchX ([int]($bounds.Top+24*$scale)) ([Drawing.Color]::FromArgb(249,45,58).ToArgb())
    Click-EditorAction $editor 'Color';Press-Key 0x28;Press-Key 0x0d
    Assert-EditorPalettePixel $swatchX ([int]($bounds.Top+24*$scale)) ([Drawing.Color]::FromArgb(254,129,1).ToArgb())
    Click-EditorAction $editor 'Color'
    [CaptureInput]::ClickAt($colorX,[int]($bounds.Top+198*$scale))
    Click-EditorAction $editor 'Color';Press-Key 27
    $canvasColor=if((Get-ItemPropertyValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'AppsUseLightTheme') -eq 0){[Drawing.Color]::FromArgb(25,25,29)}else{[Drawing.Color]::White}
    Assert-EditorPalettePixel $colorX ([int]($bounds.Top+102*$scale)) ($canvasColor.ToArgb())
    Click-EditorAction $editor 'Color'
    [CaptureInput]::ClickAt([int]($bounds.Left+400*$scale),[int]($bounds.Top+400*$scale))
    Assert-EditorPalettePixel $colorX ([int]($bounds.Top+102*$scale)) ($canvasColor.ToArgb())
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
    $startX=[int]($client.Left+180*$dpi);$startY=[int]($client.Top+24*$dpi)
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
    [CaptureInput]::MouseAt(100,100,0);Assert-EditorScreenColor ([int]($cx+20*$s)) $cy 'Lime'
    [CaptureInput]::HoldMiddleAt($cx,$cy);[CaptureInput]::MouseAt(([int]($cx+60*$s)),$cy,0);[CaptureInput]::DropMiddleAt(([int]($cx+60*$s)),$cy)
    [CaptureInput]::MouseAt(100,100,0);Assert-EditorScreenColor ([int]($cx+20*$s)) $cy 'Red'
    [CaptureInput]::HoldMiddleAt($cx,$cy);[CaptureInput]::MouseAt(([int]($cx-60*$s)),$cy,0);Start-Sleep -Milliseconds 100
    Press-Key 27;[CaptureInput]::DropMiddleAt(([int]($cx-60*$s)),$cy);[CaptureInput]::MouseAt(100,100,0)
    Assert-EditorScreenColor ([int]($cx+20*$s)) $cy 'Red'
    Press-EditorChord 0x30
    Assert-EditorPixels $editor $first.Shot
    [void][CaptureInput]::MoveWindow($editor,450,180,800,520,$true)
    Assert-EditorPixels $editor $first.Shot
    Write-Host 'PASS: left drag leaves viewport unchanged; middle-button pan/Escape rollback, Fit and resize share correct geometry; Copy exports original pixels, not viewport'

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
function Test-Theme {
    $record = New-TestPreview
    Click-PreviewAction $record.Window 'Annotate'
    $editor = Wait-Editor
    Start-Sleep -Milliseconds 200
    Save-GalleryScreenshot 'theme-annotate.png'
    $bounds = [CaptureInput]::ClientBounds($editor)
    $bitmap = [Drawing.Bitmap]::new(1,1)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen(($bounds.Left+60),($bounds.Top+120),0,0,$bitmap.Size)
        $pixel = $bitmap.GetPixel(0,0)
        $light = (Get-ItemPropertyValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'AppsUseLightTheme' -ErrorAction SilentlyContinue) -ne 0
        if (($light -and $pixel.R -lt 245) -or (-not $light -and $pixel.R -gt 45)) { throw "Annotate canvas does not match the current Windows app theme: $pixel." }
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    Click-EditorAction $editor 'Color'
    Start-Sleep -Milliseconds 100
    Save-GalleryScreenshot 'theme-annotate-palette.png'
    Write-Host 'PASS: Quick Access neutral border and Annotate chrome/palette follow Windows app theme'
}
function Test-DragDrop([switch]$PreviewOnly) {
    $targetX = $sceneRect.Left + 400
    $targetY = $sceneRect.Top + 250
    $record = New-TestPreview
    Begin-PreviewDrag $record.Window $targetX $targetY
    Wait-DragLog 'Drag started:'
    [CaptureInput]::MouseAt($targetX, $targetY, 0)
    Start-Sleep -Milliseconds 150
    if (-not [CaptureInput]::IsWindowVisible($record.Window)) { throw 'Source card disappeared instead of dimming behind drag.' }
    Save-GalleryScreenshot 'quickaccess-drag-desktop.png'
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
    if ($PreviewOnly) {
        $source = New-TestPreview
        Click-PreviewAction $source.Window 'Annotate'
        $editor = Wait-Editor
        $record = New-TestPreview
        # Capture fixtures raise the scene for DXGI; leave it below the active editor.
        [void][CaptureInput]::SetWindowPos($sceneWindow, [IntPtr](-2), 0, 0, 0, 0, 0x13)
        [void][CaptureInput]::SetForegroundWindow($editor)
        Start-Sleep -Milliseconds 120
        $hoverX = $sceneRect.Left + 35
        $hoverY = $sceneRect.Top + 35
        Begin-PreviewDrag $record.Window $hoverX $hoverY
        Wait-DragLog 'Drag started:'
        [CaptureInput]::MouseAt($hoverX, $hoverY, 0)
        Start-Sleep -Milliseconds 900
        if ([CaptureInput]::GetForegroundWindow() -ne $sceneWindow) {
            Save-GalleryScreenshot 'drag-hover-activation-failure.png'
            [CaptureInput]::DropAt($hoverX, $hoverY)
            throw 'Hovering another application during thumbnail drag did not bring it to the foreground.'
        }
        Press-Key 0x1B
        [CaptureInput]::DropAt($hoverX, $hoverY)
        Wait-DragLog 'Drag result: canceled'
        Write-Host 'PASS: hovering another window during thumbnail drag activates it above the editor'
        return
    }

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

    $tag = 'CatchIt Terminal E2E ' + $PID
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
    if($BrandOnly){
        $legacyRoot=Join-Path $dataRoot 'SimpleScreenshot'
        $legacyTemp=Join-Path $legacyRoot 'Temp'
        [void](New-Item -ItemType Directory -Force $legacyTemp)
        [IO.File]::WriteAllText((Join-Path $legacyRoot 'settings.txt'),"auto_close=never`nplacement=top_left`n")
        $legacyShot=Join-Path $legacyTemp 'shot_1_2_3.png'
        [IO.File]::WriteAllText($legacyShot,'legacy file')
        [IO.File]::SetLastWriteTimeUtc($legacyShot,[DateTime]::UtcNow.AddHours(-25))
        $created+=$legacyShot
    }
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
    if ($BrandOnly) { Test-Brand }
    elseif ($PlacementOnly) { Test-Placement }
    elseif ($QuickAccessOnly) { Test-DragDrop -PreviewOnly }
    elseif ($ThemeOnly) { Test-Theme }
    elseif ($EditorOnly) { Test-Editor }
    elseif ($BackgroundOnly) { Test-Background }
    elseif ($DrawOnly) { Test-Drawing }
    elseif ($HoverOnly) { Test-Hover }
    elseif ($PickerOnly) { Test-Picker }
    elseif ($MosaicOnly) { Test-Mosaic }
    elseif ($TextOnly) { Test-Text }
    elseif ($CropOnly) { Test-Crop }
    elseif ($ImageOnly) { Test-Image }
    elseif ($EditableOnly) { Test-Editable }
    elseif ($PolishOnly) { Test-Polish }
    elseif ($SliderOnly) { Test-Sliders }
    elseif ($WebDragOnly) { Test-WebDrag }
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
    [CaptureInput]::mouse_event(4 -bor 16 -bor 64, 0, 0, 0, [UIntPtr]::Zero)
    foreach ($key in @(0x10, 0x11, 0x12)) { [CaptureInput]::keybd_event($key, 0, 2, [UIntPtr]::Zero) }
    if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
    if ($sceneProcess -and -not $sceneProcess.HasExited) { Stop-Process -Id $sceneProcess.Id -Force }
    if ($closeExplorer -and $explorerWindow) { try { $explorerWindow.Quit() } catch {} }
    if ($terminalWindow -ne [IntPtr]::Zero) { [void][CaptureInput]::PostMessage($terminalWindow, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) }
    foreach ($path in $created) { Remove-Item -LiteralPath $path -ErrorAction SilentlyContinue }
    [void][CaptureInput]::SetForegroundWindow($originalFocus)
}
