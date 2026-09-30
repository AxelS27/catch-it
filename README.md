# Simple Screenshot

A lightweight Windows screenshot utility. The full product targets the native macOS floating-thumbnail experience described in [PRD.md](PRD.md).

## Current milestone: capture foundation

Implemented:

- Global `Alt + Shift + S` hotkey.
- Region selection on the monitor under the pointer.
- Frozen desktop preview, dimmed outside the selected region.
- Native Direct2D rendering and physical-pixel selection coordinates.
- Drag in any direction; clamp selection to the chosen monitor.
- `Esc`, right-click, or switching applications cancels capture.
- Empty selections do not create files.
- Restore the previous foreground window after capture or cancellation, without overriding a deliberate application switch.
- PNG encoding on a background worker using WIC.
- Unique filenames and atomic publication of complete PNG files.
- GPU capture session reused on a sleeping worker, without idle frame polling.
- Capture and save errors are reported rather than silently ignored.

Output directory:

```text
%LOCALAPPDATA%\SimpleScreenshot\Temp\
```

No floating thumbnail, drag-and-drop, tray, or automatic file cleanup yet. These belong to subsequent milestones. Files currently remain until manually deleted.

## Run

Requirements:

- Windows with a DXGI Desktop Duplication-capable graphics driver and an interactive desktop.
- Rust 1.88+ with the MSVC toolchain.
- Visual Studio Build Tools with the C++ toolchain and Windows SDK.

```powershell
cargo run --release
```

1. Place the pointer on the monitor to capture.
2. Press `Alt + Shift + S`.
3. Drag to select a region, then release to save.
4. Read the resulting file path in the console.

`Ctrl + Alt + Q` exits this prototype. It is a temporary development shortcut, not a final product requirement. If either shortcut is already registered by another application, startup reports an error.

The prototype keeps a console for status messages. The final background/tray application will not require it.

## Engineering checks

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Unit tests cover selection direction, clamping, empty regions, crop boundaries, display-rotation transforms, and a real WIC PNG round trip through a Unicode filename containing spaces.

### Interactive end-to-end test

Close any running instance first. This test temporarily moves the real mouse, sends keyboard input, and opens a colored test window. Do not interact with the desktop while it runs.

```powershell
cargo build --release
./scripts/smoke-capture.ps1 -Configuration release
```

The test exercises the actual global hotkey and native selection window, validates exact PNG dimensions and original pixel colors, checks focus restoration, tests cancellation and zero-area selection, and verifies clean shutdown. Only the screenshot files created by the test are deleted afterward.

Logs and the selection-overlay screenshot are saved to `.pi/capture-smoke/`, which is ignored by Git.

Initial successful release validation:

- Windows build 26200.9457 (25H2), one 1920 x 1080 display at 100% scaling.
- Hotkey processing to displayed selection overlay: 34-56 ms across six captures after startup warm-up.
- Background encoding and publication of a 350 x 200 PNG: 2-5 ms across two saves.

These are observations on the available desktop, not cross-hardware performance guarantees or measured macOS parity. They do not include keyboard dispatch latency before the application receives the hotkey.

## Remaining validation and limitations

- Mixed-DPI and physical multi-monitor E2E testing remains pending. DPI awareness is enabled; rotation transforms are unit-tested, not yet validated on rotated hardware.
- Selection is limited to one monitor. Cross-monitor region selection is not implemented.
- A lost capture session is discarded and recreated on the next request. Display changes during selection cancel the overlay.
- Secure desktops, protected content, and graphics drivers or remote sessions without Desktop Duplication support are not supported by this prototype.
- Some virtual/remote display drivers bake the pointer into the captured desktop surface. Cursor exclusion is not guaranteed on those drivers yet.
- The exact native macOS floating appearance and interaction timings still need direct reference measurements before the floating-thumbnail milestone.

## Source layout

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Hotkeys, controller message loop, capture lifecycle |
| `src/capture.rs` | DXGI GPU capture, staging readback, rotation, cropping |
| `src/geometry.rs` | Physical-pixel selection bounds |
| `src/overlay.rs` | Win32 selection window, input, Direct2D rendering |
| `src/storage.rs` | WIC PNG encoding and atomic file publication |
| `src/worker.rs` | Reusable GPU session and background work queue |
| `scripts/smoke-capture.ps1` | Interactive Windows end-to-end smoke test |
