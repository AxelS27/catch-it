# CleanShot X Annotate: reference research and implementation specification

## Status and goal

Research recorded on 2026-10-02. User-approved follow-up: reproduce CleanShot X's **image Annotate editor**, including UI and interaction behavior, in Simple Screenshot. This supersedes the initial exclusion of screenshot editing, not the exclusions of cloud, accounts, recording, or a screenshot library.

**This document records the researched specification and delivery status, not a claim of exact parity.** Slice 1 and a core-drawing slice are implemented: Annotate opens a native editor shell with full-resolution viewing, zoom/pan, non-destructive Pencil/Rectangle/Filled Rectangle/Ellipse/Line/bendable Arrow, preset/custom Windows colors and stroke widths, object selection/move/resize handles, undo/redo, Copy/Save As/Drag Me, source ownership, and capture recovery. Native Unicode/IME text input, sequential counter badges, Spotlight, non-destructive Crop with undo/redo, and image-derived visual Mosaic are now present. Text-aware Highlight is deliberately disabled because it cannot identify words without recognition; smooth/secure blur, advanced text styles and tool variants remain pending. The mosaic samples image content and **must not be used to hide secrets**: it is not CleanShot's documented randomized secure pixelation, and the untouched source file still exists. OCR and AI are excluded at user request.

Preserve the existing native Rust/Win32/Direct2D/DirectWrite/WIC implementation. No web UI, Electron, or replacement capture stack. Match the reference's structure and gestures; isolate unavoidable Windows differences such as window management, file dialogs, fonts, accessibility, and Command-to-Control mappings.

## 1. Evidence and reference baseline

### Primary sources

| ID | Source | Evidence used |
| --- | --- | --- |
| F | https://cleanshot.com/features#annotate | Tool inventory, editable projects, palettes, image combining, light/dark, rotate/flip, crop/resize |
| C | https://cleanshot.com/changelog | Version-specific changes and interaction details |
| M | https://cleanshot.com/video/screenshots/markup.mp4 | Direct frame-by-frame visual review of editor chrome, arrow manipulation, text, counters, highlighter |
| B | https://cleanshot.com/video/home/backgroundtool.mp4 | Direct visual review of background sidebar and live changes |
| I | https://cleanshot.com/video/features/combine_images.mp4 | Direct visual review of combining captures and crop/extend mode |
| P | https://cleanshot.com/img/features/color-picker.png | Direct visual review of full custom-color picker |
| J | https://cleanshot.com/img/features/fileformat.png | Official editable-project feature illustration |
| Q | https://cleanshot.com/video/home/quickaccess.mp4 | Existing thumbnail-to-editor entry-point reference |

Downloaded videos and inspection frames are local research artifacts in `.pi/reference/annotation/`, deliberately excluded from Git. Product UI must not embed reference artwork, wallpapers, branding, or extracted proprietary icons. Draw our own equivalent controls and provide original background assets.

Video fingerprints pin the actual visual evidence even if CleanShot later changes these URLs:

| File | SHA-256 |
| --- | --- |
| background.mp4 | 39926C6A00E1A4AE508C1843881C5FA1CF366865EE1429CB1A49F17D00980EB8 |
| combine.mp4 | 6B442249D99D7B715F7A81AC0CAF7CCDF490E57A40E6595D332E82B5E4B9AACD |
| markup.mp4 | DACC244F4BA3682777A27CCA4D801CC4B295EEEFEE363E0C22F7DBEE1CC0F377 |

M is a 1700 x 1200, 27.6-second marketing video. B and I contain different generations of editor chrome. None identifies the exact app build, OS version, display scale, or user preferences. Do not combine their dimensions into a fictional single release.

The fetched changelog currently lists 5.0.1 and describes 5.0 primarily as a video Studio update. It also records a new macOS Tahoe interface in 4.8.5. Consequently, neither an older video nor a screenshot can prove the exact appearance of the current installed editor.

**Working visual baseline:** the user-provided `D:\Downloads\markup.mp4` matches M byte-for-byte (SHA-256 `DACC244F4BA3682777A27CCA4D801CC4B295EEEFEE363E0C22F7DBEE1CC0F377`). Use M for the main editor and P for the expanded color picker. Use B and I as behavioral references for their specialized modes, retaining M's chrome. Final exact-parity sign-off requires a versioned installed Mac reference. There was no interactive Mac/CleanShot instance available during this research.

The official 4.8 update video is linked at https://www.youtube.com/watch?v=o5ypDElAMto. Fetching its video content was unavailable in this environment; it was not visually reviewed.

### Evidence labels

- **Observed:** directly visible in reviewed official video frames or screenshots.
- **Documented:** explicitly stated in F or C, but not necessarily demonstrated in reviewed frames.
- **Proposed:** implementation decision for our Windows app, not a CleanShot fact.
- **Unresolved:** requires installed-reference testing or better evidence before parity sign-off.

Search summaries returned contradictory tool shortcuts and some unsupported details. Those summaries are discovery aids, not specifications. In particular, do not copy the conflicting letter-key maps, purported six stroke sizes, pan shortcuts, or exact arrow-style names without direct evidence.

## 2. Editor anatomy and UI

### Main layout - Observed in M

```text
+-----------------------------------------------------------------------+
| Window controls | Crop / image / background | tools | properties | Save |
+-----------------------------------------------------------------------+
|                                                                       |
|                  Screenshot / annotation canvas                       |
|                                                                       |
+-----------------------------------------------------------------------+
| Zoom menu                  Drag Me                 output icon actions |
+-----------------------------------------------------------------------+
```

- Compact horizontal top toolbar. Not a permanent left drawing toolbox or a large ribbon.
- Utilities precede the grouped annotation tools. The visible utility icons depict crop, image-add, and background.
- The main group visually runs through pointer, outline rectangle, filled rectangle, ellipse, line, arrow, text, mosaic/redaction, framed-region tool, counter, pen-like tool, and text-highlighter-like tool. Tooltips are needed to confirm every ambiguous icon's exact label and grouping.
- Active tool has a bright blue **horizontally elongated pill** (roughly 49 x 34 encoded pixels at frame 0), not a circle; inactive tools are monochrome on a shared muted/translucent strip with subtle separators. The Windows editor redraws that silhouette with a 36 x 25-DIP active/hover surface inside the unchanged 28 x 32-DIP hit target. This is a reference-based approximation, not exact Mac points.
- Context-sensitive property pills sit immediately after the tool group. Arrow mode shows color, stroke, and arrow-style controls. Text mode shows color, a size value such as `20 pt`, and a typography-style control. Counter mode exposes a number-related property.
- Top-right `Save as...` is the prominent blue action in M. B and I instead show `Save as...` and `Done`; this is a version/mode difference, not evidence that both always appear.
- Canvas is central and spacious, with the screenshot's full aspect ratio retained. It must never use the thumbnail's cover crop.
- Bottom bar: percentage zoom pill at left, `Drag Me` pill centered, compact circular output actions at right. Copy, pin, and cloud symbols are visible; not every auxiliary symbol's exact command was verified.
- Window has rounded corners, restrained shadow, soft chrome, and compact glyphs rather than large text-labeled tool buttons.
- Light and dark appearance are documented. M shows light chrome; B and I show dark chrome.
- Bottom-bar window dragging is explicitly documented in C 4.5.1. Interactive controls must not accidentally initiate window movement.

### Source-frame geometry - not Windows layout constants

Approximate manual bounds in M at 0 seconds, measured in **encoded-video pixels**:

| Region | Approximate bounds / size |
| --- | --- |
| Whole editor | x 135..1565, y 83..1116; about 1430 x 1033 |
| Top chrome | y 83..152; about 69 high |
| Bottom chrome | y 1044..1116; about 72 high |
| Screenshot bounds | x 168..1531, y 184..1009 |
| Horizontal screenshot inset | about 33 each side |
| Active tool pill | about 50 x 36 |

These are frame-relative observations, not macOS points or Windows DIPs. Marketing zooms and display scaling prevent exact conversion. Final geometry, icon strokes, palette values, corner radii, typography, blur, and animation timing require a controlled reference capture. Do not label guessed 32-DIP controls or substituted fonts as exact parity.

### Color UI - Observed in M and P

- Compact color menu opens vertically below the color control in M.
- It contains ten color swatches and a multicolor custom-picker entry. Selected swatch has an outer selection ring.
- Expanded P shows preset swatches on the left, adjacent empty favorite slots, a saturation/value field, hue slider, transparency slider over checkerboard, current-color preview, eyedropper, Hex and R/G/B/Alpha fields, and `+ Add to My Colors`. The compact menu and expanded picker are now original native Direct2D surfaces. The expanded picker provides preset colors, hue, saturation/value, editable Hex, RGB readouts and selection recoloring; it does not yet offer transparency, favorites, screen sampling, or the reference's full field layout, and does **not** claim CleanShot parity.
- F documents sampling colors from the screen and saving favorites. C 4.8 introduces the new picker.
- Hex example `9425E7` in P is an example selected color, not an app-wide default.
- Preserve alpha during editing and export; do not silently flatten transparency to white.

## 3. Tool inventory and behavior

| Tool / capability | Verified evidence | Implementation requirements / open questions |
| --- | --- | --- |
| Move/select | M shows selectable objects and handles | Image-space hit testing; select, move, resize; confirm multi-select and selection rules on Mac |
| Rectangle | F, visible icon in M | Outline geometry, color, width; exact corner options unresolved |
| Filled rectangle | F, visible icon in M | Solid fill with alpha; do not treat as an outline style toggle unless observed |
| Ellipse | F, visible icon in M | Smooth outline/fill rules and modifier constraints require direct testing |
| Line | F, visible icon in M | Endpoints, width, color; exact line styles unresolved |
| Arrow | F documents four styles including curved; M shows curved arrow and editable handles | Endpoint and curve manipulation must remain editable; exact four style names/head geometry unresolved |
| Text | F documents seven styles; M shows editable text with selection frame | In-place multiline editing, size/style/color; Unicode, emoji, IME; paragraph behavior documented in C 4.7 |
| Counter | F; M shows successive pink numbered badges | Consecutive placements, editable number/size/color; C 4.7.5 makes counters stay on top; deletion/renumber behavior unresolved |
| Pencil | F documents auto-smoothing | Smooth editable strokes, not disconnected raw mouse segments; algorithm unknown |
| Smart highlighter | F and C 4.7; M shows word/line-aligned highlighting | Text-aware sizing and placement; manual highlighting is not full parity; C 4.7.3 disables snapping while Command is held |
| Pixelate | F explicitly says randomized for better security | Current visual mosaic samples the screenshot on a coarse grid and is editable, but is **not safe for secrets**. No guarantee of irreversibility or exact CleanShot algorithm; original source file remains untouched |
| Blur | F documents secure and smooth options | Distinguish visual smoothing from secure hiding; neither blur nor mosaic is an unconditional secrecy guarantee |
| Spotlight | F | Emphasize chosen region, dim surrounding image; exact masking/group behavior unresolved |
| Crop / extend | F; I shows grid, edge handles, aspect menu, fill menu, apply/cancel/revert | Transactional preview; aspect constraints, edge snapping, transparent/custom fill for extension |
| Resize | F and C 4.7 | Explicit output resolution; separate canvas zoom from image resampling; exact resize dialog unresolved |
| Rotate / flip | F and C 4.8 | Transform image and annotation coordinates together; rotation controls not visually reviewed |
| Background | F and B | Dedicated sidebar, live preview, padding/inset/shadow/corners/alignment/aspect, presets |
| Combine images | F and I; C 4.8 improves drop positioning | Drop becomes an editable image object; full-resolution decode, accurate positioning, crop/extend combined canvas |
| Editable project | F | Retain objects for reopening; own versioned format, not unverified `.cleanshot` compatibility |

### Background sidebar - Observed in B

- Opens on the left without replacing the central canvas or persistent toolbar.
- Presets dropdown and add control, `None`, grid of gradient thumbnails, Wallpapers with add control, Blurred choices, and Plain color swatches.
- Live `Padding` and `Inset` controls, `Auto-balance` checkbox, `Shadow`, and `Corners` controls.
- Selecting a gradient immediately frames the screenshot. Padding changes visible surrounding space, corner rounding clips the screenshot, and shadow remains separate from screenshot content.
- Auto Balance visibly changes content placement/spacing. F explains it balances the space around content; the exact detection algorithm is not published.
- F documents custom backgrounds, reusable presets, alignment, aspect ratios, and 20 supplied backgrounds. C 4.8.1 adds 5:4 and 9:16 ratios.
- Use original backgrounds with equivalent categories. Exact duplication of bundled proprietary artwork is not part of behavior parity.

### Crop/extend mode - Observed in I

- Enters a separate mode with a simplified toolbar rather than drawing a rectangle over the normal editor.
- Aspect dropdown visibly includes `Freeform`; numeric dimension fields are present.
- Displays a rule-of-thirds grid, edge/corner handles, and checkerboard outside image content.
- Top-right actions: `Revert to Original`, `Cancel`, `Crop`.
- Fill menu shows Transparent, White, Gray, Black, and Custom Color.
- Footer shows `Snap to edges` and a hint to hold Command to disable snapping.
- Canvas extends beyond the existing image, then applies the chosen fill on confirmation.
- C 4.5 documents automatic background-color recognition on extension. Do not call a fixed white fill equivalent.

## 4. Interaction contract

### Reference-backed behavior

- Selecting a tool changes the property controls without opening an unrelated modal settings window (M).
- Arrow/text objects remain manipulable after placement (M); export should not destroy the editable document.
- Counters place consecutive badges (M), and counters stay above other objects (C 4.7.5).
- Shift locks the movement axis of an object (C 4.7).
- Command+D duplicates an object (C 4.5).
- Command temporarily disables smart-highlighter snapping (C 4.7.3); crop has the same visible override hint in I.
- Holding Option when clicking Copy can keep the editor/overlay open (C 4.8). Default close behavior and preferences still require installed-reference verification.
- Bottom bar supports moving the window (C 4.5.1).
- Additional screenshots can be dragged into Annotate (F, I).
- Undo/redo exists; C records fixes including a bug where undo changed zoom (4.5.1). Keep view state independent of document-history operations.

### Proposed Windows integration guarantees

These are our safety/architecture decisions, not undocumented CleanShot specifications:

1. Click Annotate intentionally opens and activates a normal editor window. Hovering or copying a thumbnail still does not activate it. Thumbnail background dragging remains file drag, not drawing.
2. Open from the original full-resolution PNG and protect its file from cleanup for the entire editing session. Never edit the preview crop.
3. Preserve original pixels and file. All editing occurs in a separate non-destructive document; exports are new rendered artifacts.
4. Initially allow one editor per capture, raising the existing window on repeated requests rather than creating duplicate sessions. Confirm actual CleanShot behavior before calling this parity.
5. Pause/reserve the source thumbnail while its editor is open; other thumbnails continue their normal lifecycle except during capture or modal operations. Whether the reference hides or replaces the source overlay remains unresolved.
6. Use Control for Command-backed document commands and Alt for Option-backed modifiers. Do not invent single-letter tool shortcuts until verified; suppress tool switching while a text field/IME composition owns input.
7. Escape first cancels a pending gesture or crop transaction. Window close with unexported work must not silently lose edits. Exact reference dirty-close UX remains unresolved.
8. Copy, Save As, and Drag Me render the same full-resolution document revision. No viewport borders, selection handles, or chrome appear in output.
9. Canceling a save/drop preserves work. Clipboard contention retries on the existing bounded event-driven path. Export errors retain original files and document state.
10. Hide editor windows, previews, and pins before a new capture if they are not meant to be captured. Restore them after cancellation or publication without losing edit state.
11. Native shutdown safely cancels active dialogs, gestures, workers, and OLE loops. Idle editors do not require continuous CPU animation or polling.

## 5. Native implementation design

File boundaries, created or planned for the corresponding working slice:

- `src/editor/mod.rs`: activated Win32 window, lifecycle, controller routing, focus/DPI, source-file ownership.
- `src/editor/document.rs`: original image assets, stable object IDs, z-order, shapes/text/paths/redaction, canvas transformations, revision and dirty state.
- `src/editor/history.rs`: transactional undo/redo. One gesture = one command; preview changes are not hundreds of history entries.
- `src/editor/interaction.rs`: native callback/input handling, pan/cancellation, keyboard navigation, in-place text edit, Crop, selection and drawing.
- `src/editor/layout.rs`: shared draw/hit-test geometry in DIPs, responsive toolbar, context properties, popup and sidebar layout.
- `src/editor/render.rs`: Direct2D rendering of document content, shared between viewport and offscreen export; DirectWrite text; UI/selection overlays separate.
- `src/editor/export.rs`: render snapshot revision to full-size pixels, encode off-thread, atomically publish, then reuse clipboard/Save As/OLE integration.

Do not build a generic plugin system or speculative editor framework. Use explicit typed objects and a small command model. Keep image pixels in image coordinates, chrome in DIPs, and physical screen coordinates only at native boundaries.

### Existing integration points inspected

- `src/thumbnail/layout.rs`, `src/thumbnail/mod.rs`, `src/thumbnail/render.rs`: Annotate is active, routes an explicit controller request, and remains separate from card-background drag.
- `src/main.rs`: owns capture lifecycle, worker completion, clipboard retries, nested-loop handling, and gallery actions. Add editor routing without burying document logic in this file.
- `src/gallery.rs`: source ownership and pause/restore behavior must coexist with permanent FIFO eviction and pins. Do not reuse global `pause_all()` for the whole editing session.
- `src/storage.rs`: WIC PNG encoding and `protect_png()` already exist. Extend export support without rewriting the original source file or weakening cleanup protections.
- `src/clipboard.rs`: original PNG and native image publication. Editor must supply the rendered document rather than recopying the source PNG.
- `src/export.rs`: existing native Save As and atomic file publication can be reused after rendering a new edited artifact.
- `src/drag_drop.rs`: reuse native copy-only file drag and cancellation, but make Drag Me reference the current rendered revision.

### Important correctness decisions

- Shared layout geometry for render and hit tests; no separate magic-number mouse targets.
- Shared document rendering for display and export; zoom and DPI must not change output pixels or stored object dimensions.
- A worker receives an immutable document snapshot/revision. Ignore or explicitly handle stale completions; never publish old pixels as the newest edits.
- Do not pass apartment-bound UI COM objects to workers. Initialize WIC/COM on the worker thread that uses them.
- Pointer capture ends cleanly on Escape, lost capture, deactivation, display/DPI changes, and shutdown.
- Native text input must support caret/selection, clipboard editing, Unicode, multiline layout, and IME. A hand-rolled ASCII-only key collector is unacceptable.
- Hit-test thin paths using screen-consistent tolerance while storing geometry in image space.
- Treat redaction as a distinct pipeline stage. Export flattened pixels with no embedded original/project layers; document editable projects may contain sensitive original content and must warn accordingly.
- Bound decoded-image dimensions and total memory, and report invalid/corrupt files without losing the current document.

## 6. Delivery sequence

This sequence avoids presenting disconnected demo tools as a complete editor. Slice 1 is delivered as a foundation; slices 2-6 remain pending. Exact reference parity and real mixed-DPI visual checks remain pending even for the shell.

| Slice | Working end-to-end outcome |
| --- | --- |
| 1. Editor shell (implemented) | Real Annotate button -> activated native editor -> original image decoded off-thread; video-referenced light toolbar/footer, Fit/percentage and pointer-anchored zoom, pan/Escape rollback, source reservation/protection, close/capture recovery, original-image Copy/Save/Drag Me |
| 2. Core annotation (partly implemented) | Select/move/resize handles, rectangle/fill/ellipse/line, bendable arrow, smoothed pencil, preset/custom Windows colors and stroke widths, undo/redo, full-size Copy/Save/Drag Me delivered; the exact four arrow styles remain pending |
| 3. Text and steps | In-place Unicode/IME text, seven equivalent text styles, sequential counters, color picker/favorites/eyedropper, transactional property changes |
| 4. Privacy and emphasis | Smooth/secure redaction choices, randomized pixelation, spotlight, real text-aware highlighter and modifier override |
| 5. Canvas and presentation | Crop/extend with snapping and fill, resize, rotate/flip, background sidebar/presets/auto-balance, multi-image positioning |
| 6. Reopen and parity | Own editable project round-trip, exact installed-reference shortcuts/lifecycle measurements, mixed-DPI visual/interaction refinement |

No cloud Upload, account, history UI, recording, or Raycast integration added as part of image-editor parity. Show unsupported external integrations honestly rather than creating buttons that pretend to work.

## 7. Acceptance and research gaps

### Functional desktop tests

- Capture a known pixel fixture; open Annotate through the real thumbnail action; verify deliberate activation and original dimensions, including portrait and extreme aspect ratios.
- Draw, select, move, resize, recolor, duplicate, delete, undo and redo; export all three ways and compare dimensions/pixels at multiple canvas zooms.
- Edit multiline Indonesian text, emoji, and IME text; verify caret and selections and that typed tool letters do not switch tools.
- Crop cancel/apply/revert and canvas extension; resize/rotate/flip followed by object edits and export.
- Place counters, overlap with shapes, verify top ordering; verify numbering and deletions against a recorded Mac session.
- Redact known sensitive regions; inspect flattened PNG and clipboard pixels, not just a zoomed viewport. Verify editable-project warnings separately.
- Background None/gradient/custom/alpha, padding/inset/shadow/corners/auto-balance, preset save/reload.
- Drop a second screenshot and reposition; reject corrupt/oversized input while retaining edits.
- Save cancellation/overwrite/failure, clipboard lock, long OLE drag, Escape/rejected drop, quit during export and modal loops.
- Capture while editing; ensure app surfaces do not leak into capture and document state survives.
- Cleanup boundary while editing beyond retention; original and current drag/export artifacts remain protected.

### Visual acceptance

Record exact CleanShot build, macOS version, light/dark theme, accent, display resolution/scale, window size, zoom, tool properties, and original fixture image. Compare equivalent Windows captures at 100%, 125%, 150%, 200%, and mixed monitor DPI.

Review toolbar order/spacing, glyph weight, active pills, palette menu alignment, screenshot inset, property transitions, selection handles, caret, curved-arrow rendering, backgrounds, shadows, crop grid, zoom/Drag Me/footer alignment, and narrow-window behavior. No overlapping targets, clipped icons, jitter, stretched images, unexpected focus changes, or blurred exports.

Treat timings and curves as unmeasured until recorded on an installed reference. Screenshots cannot establish input latency or animation duration. Measure opening, drawing, color preview, and export responsiveness on a named Windows machine without idle polling.

### Exact-parity blockers still open

1. Installed CleanShot build/OS baseline and precise chrome geometry/fonts/materials.
2. Complete tool shortcuts, default properties, exact four arrow and seven text-style definitions.
3. Move/multi-select rules, snapping thresholds, handles, minimum sizes, zoom/pan, wheel, and axis constraints for each tool.
4. Overlay/editor/pin ownership, repeated Annotate requests, default Copy/Save/Drag Me closing behavior, preferences, and dirty-close flow.
5. Undo grouping for property changes, crop, background, image insertion, and text.
6. Redaction mode details, actual randomized-pixelation security properties, smart-highlighter and Auto Balance outputs.
7. Every output icon/menu command and compact-toolbar overflow behavior.

These are explicit reference-validation tasks, not reasons to invent behavior and call it identical.

## 8. Branch baseline validation

Before merging `feat/thumbnail-save-actions` into `main`:

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test`: 55 passed.
- `cargo build --release`: passed.
- `pwsh -NoProfile -Sta -File ./scripts/smoke-capture.ps1 -Configuration release -ActionsOnly`: passed on the interactive 96-DPI desktop, including original PNG/image clipboard, contention retry, focus, disabled placeholders, Save As cancel, Unicode/spaced path, overwrite, pin retention, and quit during Save As.

Merged baseline: `570dc0e`. Annotation research branch: `feat/annotation-cleanshot-parity`. No remote push performed.

### Slice 1 validation

- Fmt, clippy with warnings denied, 62 unit tests, and release build passed.
- `-EditorOnly` desktop E2E passed repeatedly on the available 1080p/96-DPI desktop: activated editor/caption, one session per capture, exact original pixels, 200% zoom, 400% pan with Escape rollback, Fit/resize, Copy, Save As cancel and Unicode export, canceled native file drag, reserved source timeout without pausing other cards, exclusion from capture, minimized-state preservation, independent editors/close, protected source after closing all previews, and quit during editor Save As.
- `-EditorOnly -DragDrop` additionally passed actual accepted file drops into an isolated Explorer folder, checking exact copied PNG bytes and preservation of source/editor.
- Full capture smoke, ActionsOnly, FifoOnly, and repeated GalleryOnly passed after repairing two pre-existing test assumptions: waiting for HWND counts was not waiting for survivor reflow; moving the capture scene into the preview corner left the pointer hovering/dimming the card during unhovered pixel checks.
- Native tooltips use the compatible TOOLINFO V2 size; first editor presentation intentionally ignores launcher `SW_HIDE`. The original slice used a standard native caption, superseded by the custom-title-bar pass below.
- A capture/minimization regression was reproduced end to end and traced with LLDB to `MacGenieAnimThreadClassic` in the installed Windhawk minimize-animation hook. Capture hiding/restoring now uses visibility-only `SetWindowPos`, retaining minimized state without invoking minimize again. User Windhawk settings were not changed.
- Source sessions/actions use monotonic IDs so an old decode or deferred action cannot target a recycled HWND. Original PNGs are unchanged; output is not based on the viewport or preview crop.
- Explicit Windows choices: retain a visible, paused source card and reserve its stack slot; up to four editors; 256 MiB decoded-image bound; Segoe UI and Windows window-management conventions; output keeps the editor open. These are not claims about undocumented CleanShot preferences.
- Local visual evidence: `.pi/capture-smoke/editor-shell.png`. Exact Mac comparison, real mixed-DPI validation, screen-reader automation for the custom chrome, and the remaining drawing/editing tools are not signed off.

### User-supplied video visual pass

- The exact supplied M video is light appearance, so the editor now deliberately uses light chrome regardless of the Windows theme; a dark rendering would not match this reference. Its white canvas, lavender-to-neutral header/footer, white utility/property pills, muted grouped tool strip, blue active/select state, green demonstration swatch, and five circular footer targets follow frame 0. Frame 0 is an **arrow-active** example; the editor keeps **Move** active because arrows are not implemented and must not pretend otherwise.
- The first visual pass still had a native Windows caption above the toolbar and a large 84% x 86% work-area window; both are superseded by the compact custom-title-bar pass below. Background blur/material transparency, authentic text and vector glyph metrics, expanded custom picker, active arrow mode, and image/example content have not reached pixel parity. The extra footer buttons are visual disabled placeholders, not working Share/Pin/Presentation features. While active the editor floats above topmost source previews, and drops back when deactivated so it does not cover other apps.
- The supplied M frame at 1.2 seconds confirms a vertical preset popup below the color pill: black, red, orange, yellow, green, turquoise, blue, purple, pink, white, and a custom-picker entry. Their center RGB values were sampled from the encoded video; the 10 presets are now selectable by pointer and Up/Down/Enter. The rainbow entry is a nonfunctional visual placeholder until the expanded picker is implemented. Swatch state does **not** change image pixels or exports. Keyboard/menu/selection behavior is our interim UI, not verified CleanShot behavior.
- Desktop `-EditorOnly -DragDrop` and `-ActionsOnly` passed after the initial styling change. The enlarged-window and palette pass re-ran `-EditorOnly -DragDrop` with sampled real desktop menu pixels and mouse/keyboard selection. 63 unit tests, clippy, and release build passed. Side-by-side source frame: `.pi/reference/annotation/current-frame-0.png`; color menu reference: `.pi/reference/annotation/user-palette-12.png`; captured editor after the latest pass: `.pi/capture-smoke/editor-shell.png` and `.pi/capture-smoke/editor-palette.png`.

### Middle-button pan refinement

Canvas panning now starts only while holding the middle mouse button; left drag no longer moves the viewport and remains free for future annotation gestures. Releasing middle button commits the view position; Escape or lost capture rolls an in-progress pan back. This is a user-requested Windows behavior, not a verified CleanShot shortcut.

### Centered toolbar refinement

The `markup.mp4` frame places drawing tools plus their contextual property pills around the editor's horizontal center, while utility buttons remain at left and Save/window controls at right. The original custom title bar clustered these tools too far left. Toolbar and property pills now share a centered layout origin derived from the window width, clamped to leave nonoverlapping utilities and Save. Below 870 DIPs the property pills are omitted to keep useful hit targets; the group background and separators move with the same layout geometry. This is a layout correction, not a claim of exact icon/typography parity.

### Compact custom-title-bar pass

- User requested a smaller default editor and the tool strip inside a **single custom title bar**, with Windows-style minimize/maximize/close controls on the right and no visible "Annotate" text. Default is now 1040 x 700 DIPs (bounded by the monitor work area), rather than 84% x 86% of the desktop.
- Win32 retains overlapped resizable/system-menu styles while both `WM_NCCALCSIZE` forms remove the second native caption. The blank title-bar area reports `HTCAPTION` for OS-driven dragging; physical resize edges report the correct native `HT*` values; maximized client insets account for the resize frame. Right-side controls issue real system commands. Accessible window text remains `Simple Screenshot editor`, but nothing is painted as a title.
- Focused native desktop `-EditorOnly` smoke passed custom-caption absence, default footprint, minimize/maximize/restore, dragging, toolbar/color, zoom/pan, Save/Copy, capture recovery, and close. Tests were scoped to this UI change at the user's request, not rerun against the entire unrelated gallery/FIFO suite. Exact Win11 snap-layout flyout, system-menu behavior under all themes, mixed-monitor DPI, and Mac pixel parity remain open.

### Dark color picker and stroke slider refinement

The old stock Windows ChooseColor dialog is gone. The compact color menu's rainbow entry now opens a native dark Direct2D popover with original artwork: 10 presets, a draggable saturation/value field, hue slider, editable 3/6-digit Hex and live RGB readouts. The Stroke control opens a draggable 1-24 px slider with live preview and arrow-key increments. Dragging either property on a selected mark records one undoable change on release, and full-resolution Copy/Save/Drag continue to use non-destructive compositions. This is a Windows-specific approximation, **not** the video picker: transparency, eyedropper, favorites, and exact layout/animation are not implemented. Focused `-PickerOnly` desktop smoke checked a typed custom Hex color, slider export, unchanged source PNG, and the appearance of both popovers; Clippy, one color-conversion unit test, and release build passed.

### Image-based mosaic and additional native tools

The user asked that Mosaic visually follow screenshot content. The old source-independent gray tile generator was replaced with deterministic, image-derived block colors: a stratified average from the original, unannotated image (or the Background Tool composition), shared between preview and full-resolution export. This is **visual pixelation**, not secure redaction; it can reveal content and cannot claim CleanShot's documented randomized protection. The Highlighter glyph remains visible but its hit target is disabled instead of posing as smart text detection.

Text uses a native multiline EDIT control for Unicode/IME editing and DirectWrite glyph masks for export, with an 8-72 pt size slider. Counter badges place consecutive numbers and Spotlight dims the area outside a draggable rectangle. Crop now enters a reversible selection mode with a full-image starting box, eight handles with a minimum of 32 image pixels or roughly 32 screen DIPs (whichever requires a larger source rectangle, bounded by tiny source images), dragging the selection to move it, dimming outside the selection, and explicit Apply/Cancel (Enter/Escape); only Apply changes the rendered viewport and full-resolution output dimensions without changing the captured PNG, and it shares document undo/redo. This is a Windows Snipping Tool-inspired interaction, not a claim of pixel-identical chrome. These original Windows implementations follow the visible workflow, not the reference's exact typography, seven text styles, crop snapping/extend/resize/rotate controls, or undisclosed privacy behavior. Focused desktop `-MosaicOnly`, `-TextOnly` and `-CropOnly` checks covered source colors, spotlight/counter pixels, native text input, crop dimensions/history, and unchanged source files. Add Image now opens a native PNG/JPEG/BMP file picker and inserts a movable, resizable alpha-composited overlay. Preview and full-resolution output use separate Direct2D and CPU paths; the captured PNG is unchanged. Undo history retains imported pixels (capped at 128 MiB per editor, including all retained images). Focused `-ImageOnly` desktop smoke confirmed the modal picker, transparent-pixel export, editor preview, and source integrity. Multiple independently positioned imports work within that budget, but exact reference import behavior, animated formats, rotation, and pixel-perfect screenshot/image resampling remain unverified. Solid/secure blur and other disabled utilities still need real implementations.
