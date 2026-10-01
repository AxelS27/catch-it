# Simple Screenshot

A lightweight Windows screenshot utility. The full product targets the native macOS floating-thumbnail experience described in [PRD.md](PRD.md).

## Current milestone: capture, floating thumbnail, file drag-and-drop, and temp cleanup

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
- Floating preview in the capture monitor's bottom-right work area, above the taskbar.
- Transparent, rounded preview with a subtle shadow and preserved image aspect ratio.
- DirectComposition slide/fade animations, without per-frame CPU repainting.
- No keyboard focus activation when showing, hovering, clicking, or dismissing the preview.
- Monotonic timeout that pauses on hover and resumes the remaining time after mouse exit.
- Event-driven timers: no recurring timer while hovered or after dismissal.
- Right-click dismissal and Windows client-area animation preference support.
- One preview at a time; a new capture removes the previous preview before capturing.
- Files survive timeout, manual dismissal, replacement, and application shutdown.
- Native OLE file drag using Shell `IDataObject` / `CF_HDROP` and a non-agile `IDropSource` on the UI STA.
- DPI-aware system drag threshold; a click alone does not initiate a drag.
- Copy-only operation: Explorer copies the PNG, and compatible terminals receive its file path.
- Cached, rounded native drag image follows the pointer even over unsupported targets; no focus activation or target hit-test interference.
- Lifetime pauses for the entire OLE modal loop. Escape or rejected drops restore the preview and its remaining lifetime.
- Successful drops dismiss the preview, not the PNG. Quit safely cancels an active drag.
- Automatic cleanup at startup and once per hour on a separate sleeping thread, never on the capture/render thread.
- Only generated PNGs and abandoned `.png.part` writes with a last-modified age of at least 24 hours are removed. Recent/future-dated files, unrelated names, directories, and reparse points are left alone.
- Active previews and drags hold a native file handle that prevents deletion while still allowing reading/copying. Locked or inaccessible expired files are deferred until a later sweep; cleanup errors are logged without interrupting capture.

Output directory:

```text
%LOCALAPPDATA%\SimpleScreenshot\Temp\
```

No screenshot editor or tray yet. Files survive preview dismissal and app shutdown until their 24-hour retention expires. Cleanup runs only while the application is running, so expired files may remain until the next startup or hourly sweep. Files copied elsewhere are not cleaned.

### Provisional UX profile

The floating implementation is functional, but exact macOS parity is **not yet validated**. Current development defaults are:

| Property | Provisional value |
| --- | --- |
| Preview image bounds | Up to 220 x 160 logical pixels, aspect-fit |
| Screen-edge margin | 18 logical pixels |
| Corner radius | Up to 6 logical pixels |
| Entrance / exit duration | 180 ms each, native cubic-out interpolation |
| Idle lifetime | 5 seconds after entrance completes |
| Hover behavior | Pause and resume remaining time, not reset |
| Manual dismissal | Right-click, as a provisional Windows interaction |

These values are not measured native macOS specifications. Geometry, gestures, animation curves, and timing must still be compared against a versioned macOS reference. Clicking does not open an editor. Holding the left button and moving beyond the system threshold initiates a file drag. When Windows disables client-area animation, transitions become effectively immediate without changing the idle lifetime.

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
4. The preview appears in the bottom-right corner without taking keyboard focus.
5. Drag the preview into Explorer or a compatible terminal. Hover to keep it visible, or right-click to dismiss. The file path is also printed in the console.

`Ctrl + Alt + Q` exits this prototype. It is a temporary development shortcut, not a final product requirement. If either shortcut is already registered by another application, startup reports an error.

The prototype keeps a console for status messages. The final background/tray application will not require it.

## Engineering checks

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Unit tests cover selection direction, clamping, empty regions, crop boundaries, display-rotation transforms, invalid PNG buffers, and a real WIC PNG round trip through a Unicode filename containing spaces. Thumbnail tests cover layout at 100%, 125%, 150%, and 200% scaling, negative monitor origins, extreme aspect ratios, rounded hit testing, hover/drag timing, interrupted lifecycles, and disabled motion. Drag tests check actual COM source behavior, STA affinity, straight-alpha rounding, premultiplication, system thresholds, and native `CF_HDROP` paths with spaces and Unicode.

### Interactive end-to-end test

Close any running instance first. This test temporarily moves the real mouse, sends keyboard input, and opens a colored test window. Do not interact with the desktop while it runs.

```powershell
cargo build --release
./scripts/smoke-capture.ps1 -Configuration release
```

The test exercises the actual global hotkey and native selection window, validates exact PNG dimensions and original pixel colors, checks rendered preview colors and aspect ratio, checks focus preservation including preview clicks, holds hover beyond the idle lifetime, verifies remaining-time resumption, tests manual dismissal and cancellation, checks that a previous preview is not captured in a subsequent PNG, and exits with an active preview. Only the screenshot files created by the test are deleted afterward.

The hover hold repeatedly injects its target position to counter remote-desktop pointer resets; do not interact with the desktop during the test. Rendered pixels are checked asynchronously rather than assuming a fixed delay completes composition.

To additionally exercise real file drops, install Windows Terminal and run:

```powershell
./scripts/smoke-capture.ps1 -Configuration release -DragDrop
```

This opens an isolated Explorer folder and a new Windows Terminal window with a PowerShell `Read-Host` receiver. It validates copy bytes, source-file survival, Unicode/spaced path delivery without executing the dropped text, long-drag timeout protection, Escape/rejection retry, GDI resource stability, and quit during a drag. Screenshots use a test-only `LOCALAPPDATA` directory. The terminal window and newly opened Explorer window are closed afterward; existing Explorer windows are not closed.

Input clicks and final drag releases use atomic `SendInput` batches to prevent remote pointer packets from splitting gestures. Startup readiness and conflicting prototype instances are checked before sending the capture hotkey.

Both smoke modes use a unique test-only `LOCALAPPDATA` directory, never the user's real screenshots. Startup cleanup is tested with expired, recent, unrelated, abandoned-write, and locked-file fixtures. Unit tests additionally cover the exact 24-hour boundary, future timestamps, release of active-file protection, and cleanup-worker shutdown.

Logs and visual artifacts (`overlay.png`, `thumbnail.png`, `drag-unsupported.png`, `drag-explorer.png`) are saved to `.pi/capture-smoke/`, which is ignored by Git.

Initial successful release validation:

- Windows build 26200.9457 (25H2), one 1920 x 1080 display at 100% scaling.
- Hotkey processing to displayed selection overlay: 34-56 ms across six captures after startup warm-up.
- Background encoding and publication of a 350 x 200 PNG: 2-5 ms across two saves.

Floating-thumbnail release validation on the same desktop observed 7-17 ms from submitting PNG work to completing preview-window setup, including PNG encoding. Actual display presentation and the entrance animation are asynchronous.

Drag-and-drop E2E passed on the same desktop with actual Explorer and Windows Terminal + Windows PowerShell, including spaces and Japanese characters in the source directory. Repeated drag operations showed stable GDI handle usage (16 before and after).

These are observations on the available desktop, not cross-hardware performance guarantees or measured macOS parity. They do not include keyboard dispatch latency before the application receives the hotkey. Compositor animations are native; frame pacing still needs measured visual validation on target hardware.

## Remaining validation and limitations

- Mixed-DPI and physical multi-monitor E2E testing remains pending. DPI awareness is enabled; rotation transforms are unit-tested, not yet validated on rotated hardware.
- Selection is limited to one monitor. Cross-monitor region selection is not implemented.
- A lost capture session is discarded and recreated on the next request. Display changes during selection cancel the overlay.
- Secure desktops, protected content, and graphics drivers or remote sessions without Desktop Duplication support are not supported by this prototype.
- Some virtual/remote display drivers bake the pointer into the captured desktop surface. Cursor exclusion is not guaranteed on those drivers yet.
- The exact native macOS floating appearance, drag/cancel motion, and interaction timings still need direct reference measurements before fidelity acceptance.
- Other terminal hosts, CMD-specific input behavior, and individual CLI attachment integrations still require compatibility testing. The utility supplies a real file object, not simulated typing or application-specific uploads.
- Drops into elevated applications can be blocked by Windows integrity/UIPI restrictions. Prefer matching, non-elevated permissions.
- Display/DPI changes dismiss an existing preview; creating the next preview recalculates its monitor DPI and work area.
- A rendering-device loss may require restarting this prototype. Automatic compositor-device recovery is not yet implemented.

## Source layout

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Hotkeys, controller message loop, capture lifecycle |
| `src/capture.rs` | DXGI GPU capture, staging readback, rotation, cropping |
| `src/geometry.rs` | Physical-pixel selection bounds |
| `src/drag_drop.rs` | OLE STA, Shell file object, copy-only source, cached layered drag visual |
| `src/overlay.rs` | Win32 selection window, input, Direct2D rendering |
| `src/storage.rs` | WIC PNG encoding, atomic file publication, and active-file protection |
| `src/cleanup.rs` | Conservative 24-hour temp retention and sleeping cleanup worker |
| `src/worker.rs` | Reusable GPU session and background work queue |
| `src/thumbnail/mod.rs` | Non-activating Win32 floating window and interactions |
| `src/thumbnail/layout.rs` | DPI-aware preview bounds and rounded hit testing |
| `src/thumbnail/lifecycle.rs` | Monotonic timeout, hover/drag pauses, and dismissal states |
| `src/thumbnail/render.rs` | Shared Direct2D/DirectComposition rendering and animation |
| `scripts/smoke-capture.ps1` | Interactive Windows end-to-end smoke test |
