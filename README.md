# Simple Screenshot

A lightweight Windows screenshot utility with floating capture previews, file drag/drop, a pending queue, and persistent reference cards. The initial macOS-inspired flow and approved CleanShot-inspired extension are described in [PRD.md](PRD.md). Exact reference parity is not claimed.

## Current milestone: multi-thumbnail queue, pin, and configurable timer

Implemented:

- Global `Alt + Shift + S` hotkey.
- Native notification-area icon with **Take screenshot**, **Auto-close**, **Close all screenshots**, and **Quit**, accessible by mouse or keyboard.
- Release builds run as a Windows GUI/background application, without a console window. Debug builds keep console diagnostics.
- Tray registration is restored when Explorer sends `TaskbarCreated`; shutdown explicitly removes the icon.
- Capture/quit hotkeys work inside the native tray menu. Worker completions and thumbnail lifecycle messages are deferred safely across nested menu/OLE loops.
- Region selection on the monitor under the pointer.
- Frozen desktop preview, dimmed outside the selected region.
- Native Direct2D rendering and physical-pixel selection coordinates.
- Drag in any direction; clamp selection to the chosen monitor. Corner-to-corner selection includes the final row/column of pixels for a complete monitor capture.
- `Esc`, right-click, or switching applications cancels capture.
- Empty selections do not create files.
- Restore the previous foreground window after capture or cancellation, without overriding a deliberate application switch.
- PNG encoding on a background worker using WIC.
- Unique filenames and atomic publication of complete PNG files.
- GPU capture session reused on a sleeping worker, without idle frame polling.
- Capture and save errors are reported rather than silently ignored.
- Floating preview in the capture monitor's bottom-right usable area, above the actual shell taskbar even when Windows reports an incorrect work area. Reserve taskbar thickness for auto-hide reveal too.
- Fixed 220 x 160 logical-pixel rounded card with a subtle shadow; screenshot content fills the card using a centered cover crop, preserving aspect ratio without stretching or letterboxing. Preview cropping never changes the saved PNG. Only DPI or a very small available work area changes the card size.
- Rasterize the card once before composing its shadow, avoiding the Direct2D layer-reuse error reproduced with full-screen captures.
- DirectComposition slide/fade animations, without per-frame CPU repainting.
- No keyboard focus activation when showing, hovering, clicking, or dismissing the preview.
- Monotonic timeout that pauses on hover and resumes the remaining time after mouse exit.
- Event-driven timers: no recurring timer for an idle pin, hidden queue, hovered card, Never setting, or dismissed gallery.
- Hover exposes pin and close controls. Right-click opens pin/unpin, close, older/newer page, and close-all actions. Windows client-area animation preference is respected.
- Newest-first pending queue with up to three visible cards per monitor (fewer if space is limited). Older captures remain accessible through the context menu or mouse wheel; wheel shortcuts require Windows inactive-window scrolling.
- Pins are independent always-on-top reference cards, outside the three-card pending limit. Pinning moves a card beside the dock; Alt+drag repositions it without stealing keyboard focus. Pins keep the fixed cover-card design for now, not a full-size resizable editor/reference window.
- Hide all pending cards and pins before desktop capture; pause their clocks until selection cancellation or PNG publication restores them. Captures do not contain older previews.
- Hidden queued cards release their GPU surfaces and full decoded images; retain small cached card pixels, window state, and file protection. Returning to a page reconstructs the surface from cached pixels without decoding the PNG again.
- Timer options: 5 seconds (default), 15 seconds, 30 seconds, 5 minutes, 10 minutes, and Never. A changed interval starts a fresh budget; hidden/hovered/dragged cards pause their remaining budget. Unpin starts a fresh selected interval.
- Files survive timeout, manual dismissal, close-all, and application shutdown.
- Native OLE file drag using Shell `IDataObject` / `CF_HDROP` and a non-agile `IDropSource` on the UI STA.
- DPI-aware system drag threshold; a click alone does not initiate a drag.
- Copy-only operation: Explorer copies the PNG, and compatible terminals receive its file path.
- Cached, rounded native drag image reuses the exact GPU-rendered card pixels, including the centered cover crop, border, and premultiplied alpha. No second PNG decode or crop occurs when dragging. It follows the pointer even over unsupported targets; no focus activation or target hit-test interference. The entire filled card supports hover, drag, and dismissal.
- Lifetime pauses for the entire OLE modal loop. Escape or rejected drops restore the preview and its remaining lifetime.
- Successful drops dismiss an unpinned preview, not the PNG. A pinned reference remains available after a successful drop. Quit safely cancels an active drag.
- Automatic cleanup at startup and once per hour on a separate sleeping thread, never on the capture/render thread.
- Only generated PNGs and abandoned `.png.part` writes with a last-modified age of at least 24 hours are removed. Recent/future-dated files, unrelated names, directories, and reparse points are left alone.
- Pending entries, pins, and drags hold a native file handle that prevents deletion while still allowing reading/copying. Locked or inaccessible expired files are deferred until a later sweep; cleanup errors are logged without interrupting capture.

Output directory:

```text
%LOCALAPPDATA%\SimpleScreenshot\Temp\
```

The timer preference is atomically saved in `%LOCALAPPDATA%\SimpleScreenshot\settings.txt` and loaded on startup. The queue and pins are session-only, not a screenshot history/library, and are not restored after restart.

No screenshot editor, annotation tools, resize/opacity/lock controls, or settings window. Files survive preview dismissal and app shutdown until their 24-hour retention expires. Cleanup runs only while the application is running, so expired files may remain until the next startup or hourly sweep. Files copied elsewhere are not cleaned.

### Provisional UX profile

The floating implementation is functional, but exact macOS or CleanShot parity is **not yet validated**. The three-card limit, paging policy, pin-card size, and timer semantics are our approved Windows design choices, not undocumented CleanShot specifications. Current development defaults are:

| Property | Provisional value |
| --- | --- |
| Preview card bounds | Fixed 220 x 160 logical pixels, centered cover/fill crop |
| Screen-edge margin | 18 logical pixels |
| Corner radius | Up to 6 logical pixels |
| Entrance / exit duration | 180 ms each, native cubic-out interpolation |
| Idle lifetime | 5 seconds by default after entrance; configurable through tray |
| Hover behavior | Pause and resume remaining time, not reset |
| Manual dismissal | Hover close control, context-menu Close, or Close all |

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
5. Drag a card into Explorer or a compatible terminal. Hover for pin/close controls; right-click for other actions. Wheel down for older captures and up for newer captures, or use the context menu.
6. Pin a card to keep it beyond the timer; Alt+drag moves a pin. Unpin returns it to the newest pending page with a fresh timeout. Closing cards never deletes their PNGs.

Debug or redirected console diagnostics include saved file paths. Use **Auto-close** in the tray to choose a timeout or Never; **Close all screenshots** closes pending cards and pins.

Use the notification-area icon (possibly under **Show Hidden Icons**) to take a screenshot or quit. Left-click, right-click, or keyboard activation opens the native menu. `Ctrl + Alt + Q` remains available as a development exit shortcut. If either shortcut is already registered by another application, startup reports an error.

For everyday use, build with `cargo build --release`, then launch `target/release/simple-screenshot.exe` directly. It runs without a console window; errors still appear in a native dialog. No auto-start or installer is configured. Debug builds keep a console, and redirected stdout/stderr remain available for automated tests.

## Engineering checks

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Unit tests cover selection direction, clamping, empty regions, crop boundaries, display-rotation transforms, invalid PNG buffers, and a real WIC PNG round trip through a Unicode filename containing spaces. Thumbnail tests cover layout at 100%, 125%, 150%, and 200% scaling, negative monitor origins, extreme aspect ratios, rounded hit testing, hover/drag/hidden-queue timing, pin/unpin lifecycles, Never, interval changes, interrupted dismissal, disabled motion, distinct DPI-scaled controls, queue paging, free pin slots, and persisted settings. Drag tests check actual COM source behavior, STA affinity, unchanged premultiplied card-pixel upload, invalid drag buffers, system thresholds, and native `CF_HDROP` paths with spaces and Unicode.

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

To additionally test the real notification-area icon/menu and background executable:

```powershell
./scripts/smoke-capture.ps1 -Configuration release -Tray
./scripts/smoke-capture.ps1 -Configuration release -Tray -DragDrop
```

Tray tests use Windows UI Automation and actual mouse/keyboard input. They verify menu capture/quit, keyboard activation, Escape dismissal, hotkeys during a popup, thumbnail lifetime across a long popup, and the release GUI subsystem. Explorer recovery is tested by removing our icon registration and simulating its `TaskbarCreated` notification, not by restarting the user's shell. Combined drag mode verifies that the tray quit request cancels an active OLE drag safely. Tray discovery and visual checks currently expect an English Windows shell with the native light context menu.

To validate the full-screen/layout regressions along with the complete flow:

```powershell
./scripts/smoke-capture.ps1 -Configuration release -Layout -Gallery -Tray -DragDrop
```

Layout checks capture the entire monitor corner-to-corner in both directions and verify exact PNG dimensions/pixels and successful preview rendering. Landscape, portrait, square, and tiny regions must render inside an identical fixed card, with preserved visible content colors, a centered cover crop without letterboxing, and clearance above the actual taskbar. Portrait dragging from the card edge additionally validates identical filled drag pixels, aspect ratio, and rejected-drop recovery. The validation desktop reproduced `rcWork` incorrectly covering all 1080 pixels despite a visible 52-pixel bottom taskbar; the corrected preview reserves those 52 pixels plus its normal card margin. Top/left/right taskbars, auto-hide reveal thickness, negative origins, and DPI layouts are unit-tested; physical multi-monitor and mixed-DPI validation remains pending.

The **Gallery** suite captures five pending screenshots plus a pin, validates the three-card dock and older/newer paging, checks cached card colors and stable GDI resources across repeated pages, moves a pin without focus activation, verifies pin exclusion from capture, and checks staged timeout, unpin, Never, native close-all, and the saved timer preference across an actual process restart. The drag suite also verifies that a pinned reference survives an actual Explorer copy. Tests respect the system inactive-wheel setting and use context-menu paging when wheel routing is disabled.

Logs and visual artifacts (`gallery-four-visible.png`, `gallery-restored-card.png`, `overlay.png`, `thumbnail.png`, `tray-menu.png`, `fullscreen-False.png`, `fullscreen-True.png`, `thumbnail-portrait.png`, `thumbnail-taskbar.png`, `drag-portrait.png`, `drag-unsupported.png`, `drag-explorer.png`) are saved to `.pi/capture-smoke/`, which is ignored by Git.

Initial successful release validation:

- Windows build 26200.9457 (25H2), one 1920 x 1080 display at 100% scaling.
- Hotkey processing to displayed selection overlay: 34-56 ms across six captures after startup warm-up.
- Background encoding and publication of a 350 x 200 PNG: 2-5 ms across two saves.

Floating-thumbnail release validation on the same desktop observed 7-17 ms from submitting PNG work to completing preview-window setup, including PNG encoding. Actual display presentation and the entrance animation are asynchronous.

Drag-and-drop E2E passed on the same desktop with actual Explorer and Windows Terminal + Windows PowerShell, including spaces and Japanese characters in the source directory. Repeated drag operations showed stable GDI handle usage (16 before and after).

The queue/pin/timer milestone passed 46 unit tests, strict Clippy, release build, and the combined `-Layout -Gallery -Tray -DragDrop` E2E suite. Validation included five pending cards plus a pin, repeated native wheel/context paging, unchanged cached colors, successful pinned Explorer copy, Never across a process restart, and stable GDI handles (23 before and after).

These are observations on the available desktop, not cross-hardware performance guarantees or measured macOS parity. They do not include keyboard dispatch latency before the application receives the hotkey. Compositor animations are native; frame pacing still needs measured visual validation on target hardware.

## Remaining validation and limitations

- Explorer recovery has been validated through registration loss plus a simulated `TaskbarCreated` message, not a real Explorer restart. Native popup menus defer controller lifecycle updates until they close; a preview's remaining timeout can resume afterward.
- Mixed-DPI and physical multi-monitor E2E testing remains pending. DPI awareness is enabled; rotation transforms are unit-tested, not yet validated on rotated hardware.
- Selection is limited to one monitor. Cross-monitor region selection is not implemented.
- A lost capture session is discarded and recreated on the next request. Display changes during selection cancel the overlay.
- Secure desktops, protected content, and graphics drivers or remote sessions without Desktop Duplication support are not supported by this prototype.
- Some virtual/remote display drivers bake the pointer into the captured desktop surface. Cursor exclusion is not guaranteed on those drivers yet.
- The exact native macOS/CleanShot floating appearance, drag/cancel motion, overflow layout, and interaction timings still need direct reference measurements before fidelity acceptance. Pin resizing, opacity, click-through locking, and editing remain out of scope.
- Other terminal hosts, CMD-specific input behavior, and individual CLI attachment integrations still require compatibility testing. The utility supplies a real file object, not simulated typing or application-specific uploads.
- Drops into elevated applications can be blocked by Windows integrity/UIPI restrictions. Prefer matching, non-elevated permissions.
- Display/DPI changes dismiss existing previews and pins; creating the next preview recalculates its monitor DPI and work area.
- A rendering-device loss may require restarting this prototype. Automatic compositor-device recovery is not yet implemented.

## Source layout

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Hotkeys, hidden controller, capture lifecycle, nested-loop message deferral |
| `src/tray.rs` | Native notification icon, timer/close-all menus, Explorer recovery, and icon ownership |
| `src/gallery.rs` | Session queue, per-monitor paging, pin placement, GPU surface visibility, context menu |
| `src/settings.rs` | Persisted auto-close interval and atomic preference publication |
| `src/capture.rs` | DXGI GPU capture, staging readback, rotation, cropping |
| `src/geometry.rs` | Physical-pixel selection bounds |
| `src/drag_drop.rs` | OLE STA, Shell file object, copy-only source, cached layered drag visual |
| `src/overlay.rs` | Win32 selection window, input, Direct2D rendering |
| `src/storage.rs` | WIC PNG encoding, atomic file publication, and active-file protection |
| `src/cleanup.rs` | Conservative 24-hour temp retention and sleeping cleanup worker |
| `src/worker.rs` | Reusable GPU session and background work queue |
| `src/thumbnail/mod.rs` | Non-activating card/pin windows, controls, visibility, cached surfaces, drag interactions |
| `src/thumbnail/layout.rs` | Fixed DPI-aware card, centered cover crop, rounded hit testing |
| `src/thumbnail/work_area.rs` | Monitor work area corrected for real shell taskbar/reveal bounds |
| `src/thumbnail/lifecycle.rs` | Monotonic timeout, hover/drag/hidden pauses, pin, and dismissal states |
| `src/thumbnail/render.rs` | Shared Direct2D/DirectComposition rendering and animation |
| `scripts/smoke-capture.ps1` | Interactive Windows end-to-end smoke test |
