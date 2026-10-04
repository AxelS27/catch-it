# Simple Screenshot

A lightweight Windows screenshot utility with floating capture previews, file drag/drop, a pending queue, and persistent reference cards. The initial macOS-inspired flow and approved CleanShot-inspired extension are described in [PRD.md](PRD.md). Exact reference parity is not claimed.

## Current milestone: native Annotate drawing tools and Background Tool

The Annotate pencil now opens an activated native editor with the original full-resolution image, video-referenced light toolbar/footer, percentage/Fit zoom, pointer-anchored Ctrl+wheel zoom, middle-button hold-to-pan (left drag does not pan), Copy, native PNG Save As, and copy-only `Drag Me` file output. Source thumbnails pause and reserve their queue slots while editing; other unpinned cards still expire. Reopening the same capture raises its existing editor. Each editor independently protects its source file even after its preview closes. Editors hide during capture without losing zoom, pan, or minimized state.

This is **not a complete annotation implementation**. Pencil strokes are smoothed and do not show edit handles immediately after drawing (they can still be selected deliberately with Move); Rectangle, Filled Rectangle, Ellipse, Line, bendable Arrow, image-derived Mosaic, Spotlight, sequential Counter badges, and editable Unicode Text create non-destructive image-space marks; native Crop starts with a full-image selection, offers edge/corner handles with a visible minimum size, move, dimmed outside area and Apply/Cancel instead of cropping immediately on drag; committed crops trim preview/export and support undo/redo. Add Image opens a native PNG/JPEG/BMP picker and places a movable/resizable, alpha-composited image overlay centered in the cropped view, included in preview and full-resolution outputs; imported pixels and undo history have a 128 MiB per-editor budget. New non-Pencil marks stay selected and can be moved, resized via corner/end/control-point handles, and rotated with the circular handle directly without switching to Move. The default drawing color is red, and Counter numerals scale together with their badges during zoom. Rotation also renders in full-resolution Save/Copy/Drag Me and supports undo/redo; the image-derived Mosaic and Spotlight effects and modal Crop remain unrotated. Move selects existing marks after deselection; preset colors or a dark native Direct2D hue/saturation/Hex picker recolor selection, Stroke has a live 1-24 px slider with a color- and width-responsive curved brush sample (inspired by the supplied Snipping Tool screenshot), and Text has an 8-72 pt size slider (arrow keys adjust them), Delete removes selection, and Ctrl+Z/Ctrl+Y undo/redo discrete edits. Preview and full-resolution Save/Copy/Drag Me include these marks without modifying the original PNG. The text-aware Highlighter is intentionally disabled instead of mimicking detection. Mosaic samples underlying image colors and is **visual only, NOT secure redaction**; sensitive details may be recovered, and the original source remains on disk. Smooth/secure blur, solid redaction, exact text styles and smart highlighting remain unimplemented; OCR and AI are excluded. Upload remains unavailable. The Background button opens a left sidebar with 20 original, approximate gradients, three original abstract wallpapers, screenshot-based blur, plain colors, Padding/Inset/Shadow/Corners, Auto-balance (on by default), Reset (restores Padding/Inset/Shadow/Corners and Auto-balance while retaining the chosen backdrop), and None (removes the backdrop). Slider values are continuous; integer PNG dimensions mean Padding/Inset can still advance a source pixel at a time. Live drags compose a bounded temporary preview, then commit the full-resolution result on release. Short windows scroll the panel. Preview and PNG Save As/Copy/Drag Me export the same full-resolution composed image. Auto-balance is an original visual-shadow compensation, not a verified CleanShot algorithm. Exact CleanShot preset artwork and custom background import/presets/aspect-ratio/alignment are unavailable; details and evidence are in [docs/backgroundtool-video-reference.md](docs/backgroundtool-video-reference.md). Four editors can be open simultaneously; decoded images are bounded to 256 MiB each and the graphics device's bitmap-size limit. The original capture PNG is never edited or re-encoded. Save/Copy/drop preserve the editor session.

UI/UX evidence, delivery slices, and remaining exact-parity measurements are in [docs/annotation-cleanshot-research.md](docs/annotation-cleanshot-research.md). The supplied `D:\Downloads\markup.mp4` (same SHA-256 as the already researched official video) is the pinned **light** visual baseline. The editor uses the Windows app light/dark appearance; the supplied video remains the light visual baseline. Its compact 1040 x 700-DIP default window (limited to the monitor), lavender-to-neutral chrome, white canvas, tool strip, swatch, and five footer icon positions follow that video. While focused, the editor temporarily rises above topmost preview cards so they cannot cover its footer; it drops back when focus leaves. The tools and contextual property pills are centered together in one custom title-bar row, with utility icons on the left, Save and Windows-style minimize/maximize/close controls on the right, and no visible editor title. Native resizing, title-bar dragging, maximize/restore, and Windows window management remain available. Segoe UI remains a platform substitution; blur and some footer actions remain disabled. The video-derived 10-color dropdown selects drawing colors or recolors marks by mouse/keyboard; its rainbow entry opens an original dark color picker with saturation/value, hue, preset swatches, editable Hex, and RGB readouts; this is not a pixel-perfect CleanShot picker. The active tool surface is a wider-than-tall pill; 20-DIP circular hover highlights ease in/out centered on each icon without covering the blue edge. The blue selection pill follows a damped spring with a restrained elastic overshoot (clamped to the tool strip) and preserves velocity across rapid changes; its timer stops when idle. Icons are original vector interpretations, not copied CleanShot assets. This is not pixel-perfect Mac chrome: frame scale, material blur, fonts, active arrow state, picker transparency/eyedropper/favorites, and the style property menu still need work. Exact installed-version parity and real mixed-DPI visual validation remain pending.

## Current implemented milestone: thumbnail actions, Save As, and automatic image clipboard

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
- Fixed 260 x 184 logical-pixel rounded card with a subtle shadow; screenshot content fills the card using a centered cover crop, preserving aspect ratio without stretching or letterboxing. Preview cropping never changes the saved PNG. Only DPI or a very small available work area changes the card size.
- Rasterize the card once before composing its shadow, avoiding the Direct2D layer-reuse error reproduced with full-screen captures.
- DirectComposition slide/fade animations, without per-frame CPU repainting. The card outline is black in Windows dark app mode and white in light app mode; custom colors/gradients are not implemented yet.
- No keyboard focus activation when showing, hovering, copying, pinning, or dismissing the preview. Save intentionally opens a normal native dialog with keyboard focus.
- Monotonic timeout that pauses on hover and resumes the remaining time after mouse exit. Auto-dismiss is FIFO per visible monitor stack: oldest unpinned card exits first; newer cards wait until its exit finishes, even if their own budgets have already elapsed. Hovering the oldest holds automatic dismissal behind it. Manual close and successful file drops are not FIFO-gated.
- Event-driven timers: no recurring timer for an idle pin, hovered card, Never setting, or dismissed gallery.
- Hover actions follow the relative arrangement in CleanShot's [official quick-access demo](https://cleanshot.com/video/home/quickaccess.mp4): Close top-left, Pin top-right, central Copy above Save, Annotate bottom-left, Upload bottom-right. Copy and Save are active; Annotate opens the native editor. Upload is a visibly disabled placeholder that never initiates a drag. Small cards without enough room hide controls rather than overlapping targets. Relative positions are reference-based; exact macOS pixel parity and blur are not claimed.
- Copy the original screenshot automatically after each successful capture/save, including when the preview queue is full of pins. Publish both the original registered `PNG` bytes and a full-resolution native `CF_DIBV5` image, never a preview crop or file path. Copy can recopy any surviving older screenshot without dismissing it or taking focus. A busy clipboard retries on a 50 ms event timer for up to two seconds, then reports failure with the source PNG and preview intact; Copy allows retry. No recurring clipboard timer after success or failure.
- Save opens Windows' native PNG Save As dialog. Cancel preserves the card; success dismisses an unpinned card and keeps a pin in place. All card clocks pause during the dialog. Unicode/spaced paths and native overwrite confirmation are supported. Export copies the exact original PNG bytes to a private file in the chosen folder, then atomically publishes it; failure preserves the existing target and source PNG. Quit safely cancels an active dialog.
- Right-click opens pin/unpin, close, and close-all actions. Windows client-area animation preference is respected.
- Bounded session-only FIFO preview queue. Each monitor's bottom-right stack fits as many fixed cards as its usable height allows, oldest at the bottom and newest at the top. Overflow permanently removes the oldest unpinned thumbnail and releases its window/resources, without deleting its PNG. At capacity, older unpinned cards are evicted before new previews are inserted. No hidden backlog, wheel browsing, or older/newer paging; removed thumbnails never reappear after another dismissal or capture.
- Pins remain always on top in the same chronological stack, not a separate column. Pin/unpin does not reorder a card; removing a lower card compacts those above it without losing pin state. Pins reserve stack slots and never auto-expire or get evicted by new captures. If every slot is pinned, new PNGs are still saved but their previews cannot enter the full queue. Pins use the same fixed cover card, not a movable/resizable reference window.
- Hide all surviving cards and pins before desktop capture; pause their clocks until selection cancellation or PNG publication restores them. Captures do not contain older previews. Capture hiding is temporary, unlike permanent overflow eviction.
- Timer options: 5 seconds (default), 15 seconds, 30 seconds, 5 minutes, 10 minutes, and Never. A changed interval starts a fresh budget; hidden/hovered/dragged cards pause their remaining budget. Unpin starts a fresh selected interval.
- Files survive timeout, manual dismissal, close-all, and application shutdown.
- Native OLE file drag using Shell `IDataObject` / `CF_HDROP` and a non-agile `IDropSource` on the UI STA.
- DPI-aware system drag threshold; a click alone does not initiate a drag.
- Copy-only operation: Explorer copies the PNG, and compatible terminals receive its file path.
- The miniature native drag image downsamples the cached premultiplied GPU card pixels while retaining its centered cover crop. Theme-dependent borders are drawn only on the on-screen card; no second PNG decode occurs when dragging. It follows the pointer even over unsupported targets; no focus activation or target hit-test interference. The entire filled card supports hover, drag, and dismissal.
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

The native editor shell, basic drawing, and a limited Background Tool are implemented; text, redaction, crop, additional arrow styles, opacity/lock controls, and a settings window remain pending. Files survive preview dismissal and app shutdown until their 24-hour retention expires. Cleanup runs only while the application is running, so expired files may remain until the next startup or hourly sweep. Files copied elsewhere are not cleaned.

### Provisional UX profile

The floating implementation is functional, but exact macOS or CleanShot parity is **not yet validated**. The screen-derived stack capacity, permanent FIFO eviction, pin-card size, and timer semantics are our Windows design choices, not undocumented CleanShot specifications. Current development defaults are:

| Property | Provisional value |
| --- | --- |
| Preview card bounds | Fixed 220 x 160 logical pixels, centered cover/fill crop |
| Screen-edge margin | 18 logical pixels |
| Corner radius | Up to 6 logical pixels |
| Entrance / exit duration | 180 ms each, native cubic-out interpolation |
| Idle lifetime | 5 seconds by default after entrance; configurable through tray |
| Hover behavior | Pause and resume remaining time, not reset |
| Manual dismissal | Hover close control, context-menu Close, or Close all |

These values are not measured native macOS specifications. Geometry, gestures, animation curves, and timing must still be compared against a versioned macOS reference. Clicking the card background does not open an editor; its Annotate pencil does. Holding the left button and moving beyond the system threshold initiates a file drag. When Windows disables client-area animation, transitions become effectively immediate without changing the idle lifetime.

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
5. Every new screenshot automatically copies its original image to the clipboard. Hover for Copy, Save, pin, and close; Copy restores an older image, and Save chooses a PNG destination. Annotate opens the native editor; Upload remains disabled. Drag a card's background into Explorer or a compatible terminal to copy its file. Overflow permanently evicts the oldest unpinned thumbnail; there is no preview history.
6. Pin a card to keep it beyond the timer, in its existing stack position. Unpin preserves capture order and starts a fresh timeout. Closing or successfully dragging an unpinned card compacts the stack without changing surviving pins. Closing cards never deletes their PNGs.

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

Unit tests cover selection direction, clamping, empty regions, crop boundaries, display-rotation transforms, invalid PNG buffers, and a real WIC PNG round trip through a Unicode filename containing spaces. Thumbnail tests cover layout at 100%, 125%, 150%, and 200% scaling, negative monitor origins, extreme aspect ratios, rounded hit testing, hover/drag/capture-hidden timing, pin/unpin lifecycles, Never, interval changes, interrupted dismissal, disabled motion, distinct DPI-scaled controls, permanent queue eviction/no-resurrection, chronological pin placement/compaction, screen-derived capacity, and persisted settings. Drag tests check actual COM source behavior, STA affinity, unchanged premultiplied card-pixel upload, invalid drag buffers, system thresholds, and native `CF_HDROP` paths with spaces and Unicode.

### Focused drawing test

```powershell
cargo build --release
pwsh -NoProfile -Sta -File ./scripts/smoke-capture.ps1 -Configuration release -DrawOnly
```

Uses a real capture and native editor to draw a rectangle, smoothed pencil, bendable arrow and image-derived mosaic; change preset/custom colors, move/undo/redo an object, and verify output pixels for Copy and Save As while the original PNG remains unchanged. The minimize glyph is centered in its window control. Screenshot: `.pi/capture-smoke/editor-drawing-rectangle.png`.

### Focused editor test

```powershell
cargo build --release
pwsh -NoProfile -Sta -File ./scripts/smoke-capture.ps1 -Configuration release -EditorOnly
# Also test an accepted editor file drop into a real isolated Explorer folder:
pwsh -NoProfile -Sta -File ./scripts/smoke-capture.ps1 -Configuration release -EditorOnly -DragDrop
```

Uses actual desktop capture and the thumbnail Annotate button, checking activation, custom single-row caption/Windows window controls, one session per capture, full-image pixels, zoom menu, middle-button pan/Escape rollback, Fit/resize, original image clipboard, Save As cancel/Unicode export, OLE drag/Escape, optional accepted Explorer copy, independent source/other-card timeouts, exclusion from new captures, minimized-state preservation, concurrent editors/native close, protection after preview close, and quit during editor Save As. The original PNG and clipboard survive shutdown. Local artifacts include `.pi/capture-smoke/editor-shell.png`.

Passed repeatedly on the available 1080p/96-DPI desktop. Fmt, clippy, focused unit tests, full capture smoke, ActionsOnly, FifoOnly, and repeated GalleryOnly also passed. Editor unit coverage includes geometry, Fit/aspect, pointer-anchored zoom, pan bounds, alpha premultiplication, bounded decoding, preview output, and source reservation. Real mixed-DPI monitors, installed Mac comparison, and remaining annotation tools are not yet validated. Tests move the actual pointer and open dialogs; do not interact while they run. All test files/settings are isolated under `.pi/`.

### Focused Background Tool test

```powershell
cargo build --release
pwsh -NoProfile -Sta -File ./scripts/smoke-capture.ps1 -Configuration release -BackgroundOnly -DragDrop
```

Tests the real native sidebar, gradient, wallpaper and blur selections, live controls, short-window scrolling, Auto-balance, subpixel corner dragging, shadow fade-in and rounded-corner shadow shape, output pixel dimensions, Save As, clipboard PNG+DIB, Explorer file drag, None selection, and untouched source PNG. Test screenshots are in `.pi/capture-smoke/background-gradient.png`, `.pi/capture-smoke/background-scrolled.png`, and `.pi/capture-smoke/background-rounded-shadow.png`. Like other desktop tests, this moves the real pointer and opens an isolated Explorer folder; do not interact until it exits.

For a focused slider, default Auto-balance, and Reset check including a larger real capture:

```powershell
pwsh -NoProfile -Sta -File ./scripts/smoke-capture.ps1 -Configuration release -SliderOnly
```

This captures part of the desktop into ignored `.pi/capture-smoke/` evidence, so close sensitive windows first.

### Focused Save As / clipboard test

```powershell
cargo test clipboard::tests
cargo test export::tests
cargo test thumbnail::layout::tests::controls
cargo build --release
pwsh -NoProfile -Sta -File ./scripts/smoke-capture.ps1 -Configuration release -ActionsOnly
```

This mode runs only capture/image clipboard and thumbnail actions, not the complete suite. It checks original PNG bytes and native image pixels/dimensions, automatic replacement on the next capture, clipboard-lock retry, manual recopy of an older capture, disabled Upload/focus, Save As cancellation beyond the idle budget, Unicode/spaced export, unpinned dismissal, unchanged pin placement, real overwrite confirmation, quit during Save As, and clipboard persistence after shutdown. Five focused unit tests cover clipboard buffers, atomic success/failure and unchanged targets, and all six control targets at four DPI scales. The real desktop, mouse, keyboard and clipboard are used; do not interact while it runs. The native save dialog must expose English Save/Cancel labels. File/settings fixtures are isolated under `.pi/capture-smoke/`.

### Focused gallery regression test

```powershell
cargo test gallery::tests
cargo test thumbnail::layout::tests::stack_capacity
cargo build --release
./scripts/smoke-capture.ps1 -Configuration release -GalleryOnly
```

This mode runs only native capture/preview, capacity/overflow, in-place pin/unpin, removal compaction, permanent eviction/no-resurrection after close and capture cancellation, timeout, and clean shutdown checks. It uses the tray's controller action to set the timer without depending on notification-area discovery. It moves the real mouse and requires an idle interactive desktop with room for at least four cards; source files/settings are isolated under a test-only `LOCALAPPDATA`. Four focused queue regressions and this E2E mode passed on the available 1080p/100% desktop. Real Explorer/terminal drops and the complete smoke suite were not rerun for this change.

### Focused FIFO timeout regression

```powershell
cargo test fifo
cargo build --release
./scripts/smoke-capture.ps1 -Configuration release -FifoOnly
```

Runs only three-card FIFO timeout checks, with the oldest hovered beyond five seconds, then verifies bottom-to-top disappearance and skipping a pinned bottom card. Like `-GalleryOnly`, it uses an isolated data directory and moves the actual pointer. Newer elapsed budgets are not reset while waiting; only automatic dismissal is gated, without idle polling. Overflow is permanently evicted and never blocks or returns to the surviving queue.

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

The **Gallery** suite captures beyond the monitor's capacity, validates bottom-to-top ordering, pins in slots 1 and 3, removal of slot 2, in-place unpin, overflow with pins retained, permanent overflow eviction with no hidden windows or resurrection after close/capture, and staged timeout. Full `-Gallery` mode additionally checks the saved Never preference across an actual process restart. The drag suite also verifies that a pinned reference survives an actual Explorer copy. Overflow checks assert that evicted native windows are destroyed, survivors compact downward, and source PNGs remain.

Logs and visual artifacts (`thumbnail-actions-hover.png`, `save-as-dialog.png`, `gallery-capacity.png`, `gallery-newest.png`, `overlay.png`, `thumbnail.png`, `tray-menu.png`, `fullscreen-False.png`, `fullscreen-True.png`, `thumbnail-portrait.png`, `thumbnail-taskbar.png`, `drag-portrait.png`, `drag-unsupported.png`, `drag-explorer.png`) are saved to `.pi/capture-smoke/`, which is ignored by Git.

Initial successful release validation:

- Windows build 26200.9457 (25H2), one 1920 x 1080 display at 100% scaling.
- Hotkey processing to displayed selection overlay: 34-56 ms across six captures after startup warm-up.
- Background encoding and publication of a 350 x 200 PNG: 2-5 ms across two saves.

Floating-thumbnail release validation on the same desktop observed 7-17 ms from submitting PNG work to completing preview-window setup, including PNG encoding. Actual display presentation and the entrance animation are asynchronous.

Drag-and-drop E2E passed on the same desktop with actual Explorer and Windows Terminal + Windows PowerShell, including spaces and Japanese characters in the source directory. Repeated drag operations showed stable GDI handle usage (16 before and after).

The previous three-card/independent-pin milestone passed 46 unit tests, strict Clippy, release build, and the combined `-Layout -Gallery -Tray -DragDrop` E2E suite. Validation included five pending cards plus a pin, repeated native wheel/context paging, unchanged cached colors, successful pinned Explorer copy, Never across a process restart, and stable GDI handles (23 before and after).

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
| `src/gallery.rs` | Bounded FIFO queue, permanent per-monitor eviction, chronological stack/pin placement, GPU surface visibility, context menu |
| `src/settings.rs` | Persisted auto-close interval and atomic preference publication |
| `src/capture.rs` | DXGI GPU capture, staging readback, rotation, cropping |
| `src/geometry.rs` | Physical-pixel selection bounds |
| `src/drag_drop.rs` | OLE STA, Shell file object, copy-only source, cached layered drag visual |
| `src/overlay.rs` | Win32 selection window, input, Direct2D rendering |
| `src/storage.rs` | WIC PNG encoding, atomic file publication, and active-file protection |
| `src/clipboard.rs` | Original PNG + native DIBV5 clipboard payloads, memory ownership, lossless recopy |
| `src/export.rs` | Native PNG Save As, modal cancellation, atomic lossless export |
| `src/cleanup.rs` | Conservative 24-hour temp retention and sleeping cleanup worker |
| `src/worker.rs` | Reusable GPU session and background work queue |
| `src/thumbnail/mod.rs` | Non-activating card/pin windows, controls, visibility, cached surfaces, drag interactions |
| `src/thumbnail/layout.rs` | Fixed DPI-aware card, centered cover crop, rounded hit testing |
| `src/thumbnail/work_area.rs` | Monitor work area corrected for real shell taskbar/reveal bounds |
| `src/thumbnail/lifecycle.rs` | Monotonic timeout, hover/drag/hidden pauses, pin, and dismissal states |
| `src/thumbnail/render.rs` | Shared Direct2D/DirectComposition rendering and animation |
| `scripts/smoke-capture.ps1` | Interactive Windows end-to-end smoke test |
