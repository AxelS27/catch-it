# Product Requirements Document

## 1. Product Overview

A very lightweight Windows screenshot utility that reproduces the native macOS floating screenshot experience as closely as the Windows platform allows.

Native macOS is the behavioral and visual reference, not merely aesthetic inspiration. Floating thumbnail geometry, animation, timeout, hover, dismissal, and drag interactions must be validated against an observed macOS reference. This does not expand the scope to include the macOS screenshot editor.

The application has one primary purpose:

> Take a screenshot, show it temporarily as a floating thumbnail in the bottom-right corner, and allow the user to drag and drop that screenshot anywhere.

No screenshot editor, library, annotation tools, account system, or complex UI.

**Approved follow-up milestone:** add a CleanShot-inspired pending thumbnail queue, persistent pinned reference cards, and a configurable auto-close timer through the native tray. This is a limited Windows extension, not a complete CleanShot clone. Our screen-derived stack capacity and timer defaults are design choices; CleanShot's exact overflow layout, default lifetime, and hover semantics are not yet verified.

---

## 2. Core User Flow

```text
Screenshot Hotkey
      ↓
Select Screen Region
      ↓
Capture Screenshot
      ↓
Save Temporary PNG
      ↓
Floating Thumbnail Appears (Bottom-Right of Screen)
      ↓
User Can Drag It
      ↓
┌──────────────┬───────────────┬──────────────┐
│   Terminal   │ File Explorer │ Other Apps   │
│              │               │              │
│ Insert Path  │ Drop PNG File │ Drop Image   │
└──────────────┴───────────────┴──────────────┘
```

### Example Usage

1. User takes a screenshot.
2. A small preview appears in the bottom-right corner:

```text
┌──────────────────┐
│                  │
│    Screenshot    │
│                  │
└──────────────────┘
```

3. User drags it into a terminal:

```powershell
> codex "check this error" "C:\Users\User\AppData\Local\App\Temp\shot_001.png"
```

4. Or drags it into File Explorer:

```text
Desktop/
└── shot_001.png
```

---

## 3. Core Features

### 3.1 Screenshot Capture

Provide a global keyboard shortcut.

Suggested default:

```text
Alt + Shift + S
```

When triggered:

1. Screen becomes slightly dimmed.
2. User clicks and drags to select an area.
3. Selected region is captured.
4. Screenshot is immediately saved as a temporary PNG.

Target behavior should feel instant.

### 3.2 Floating Screenshot

After capture, display a small floating thumbnail in the bottom-right corner of the active monitor.

Requirements:

- Borderless window
- Always on top
- Fixed-size preview card, fully filled by a centered cover crop of the screenshot without stretching or letterboxing; capture aspect ratio must not change the card size, and preview cropping must never alter the saved PNG
- Rounded corners
- Small margin from screen edges
- Does not steal keyboard focus
- Configurable automatic dismissal: 5 seconds by default, 15 seconds, 30 seconds, 5 minutes, 10 minutes, or Never. Record reference timings separately; do not present these development choices as measured macOS or CleanShot defaults
- Appearance and dismissal motion matched to the reference, including duration and easing
- DPI-aware preview with preserved image aspect ratio, crisp edges, and reference-matched corner radius, shadow, size, and spacing
- Position above the capture monitor's taskbar with clear spacing, using both the Windows work area and actual shell taskbar bounds where work-area reporting is inconsistent; reserve clearance for auto-hide reveal too

```text
Desktop

┌─────────────────────────────────────────────────────┐
│                                                     │
│                                                     │
│                                                     │
│                                                     │
│                                  ┌───────────────┐  │
│                                  │  Screenshot   │  │
│                                  │    Preview    │  │
│                                  └───────────────┘  │
└─────────────────────────────────────────────────────┘
```

### 3.3 Drag and Drop

The floating thumbnail must behave like a real Windows file when dragged.

The screenshot should be exposed through Windows drag-and-drop APIs as a `.png` file.

Example temporary file:

```text
C:\Users\User\AppData\Local\AppName\Temp\shot_20260930_182245.png
```

#### Terminal

Dragging the screenshot into a compatible terminal should insert its file path.

Example:

```text
"C:\Users\User\AppData\Local\AppName\Temp\shot_20260930_182245.png"
```

Primary targets:

- Windows Terminal
- PowerShell
- CMD
- AI CLI applications running inside a terminal

#### File Explorer

Dragging into File Explorer should copy the PNG file. Do not offer a move operation that removes the temporary source file, since another application may already hold its path.

Example:

```text
Downloads/
└── shot_20260930_182245.png
```

#### Other Applications

Applications supporting Windows file drag-and-drop should receive the screenshot as a normal PNG file.

---

## 4. Temporary File Management

Screenshots are initially stored in:

```text
%LOCALAPPDATA%\AppName\Temp\
```

Example:

```text
shot_20260930_182245.png
```

- The file must **NOT** be deleted when the floating thumbnail disappears.
- Temporary screenshots can be automatically cleaned after 24 hours.
- This ensures applications receiving the file path still have enough time to access the screenshot.

---

## 5. Floating Thumbnail Behavior

The thumbnail lifecycle must be based on direct observation of native macOS, with the reference OS version recorded before finalizing implementation.

### 5.1 Reference Measurements

Record and reproduce:

- Idle lifetime and the event that starts the dismissal timer
- Hover behavior, including whether mouse exit resumes the remaining time or resets it
- Appearance and dismissal animation duration, motion, and easing
- Drag threshold, drag preview, and behavior after successful, unsupported, or canceled drops
- Manual dismissal behavior and the corresponding Windows input mapping
- Behavior when a new screenshot is taken while a thumbnail is visible

Apple's public documentation describes the lifetime as "a few seconds" rather than an exact duration. Exact timing and undocumented interactions remain pending direct measurement; do not treat guessed values as macOS specifications.

### 5.2 Interaction Guarantees

- Never dismiss or invalidate a screenshot during an active drag operation.
- Make the completed PNG available before allowing a file drag.
- A canceled or rejected drop must leave the screenshot available for retry.
- Thumbnail interactions must not unexpectedly activate another window or steal keyboard focus.
- Capture overlays and existing thumbnails must not appear in the captured image.
- Use explicit lifecycle states so hover, drag, new capture, and animation completion cannot trigger conflicting transitions.
- Preserve temporary files independently of thumbnail dismissal, even where macOS save behavior differs.

### 5.3 Approved Queue, Pin, and Timer Extension

- Use a bounded, session-only FIFO preview queue. Capacity follows the monitor's usable height and DPI. Show one bottom-right stack, oldest at the bottom and newest at the top. New captures permanently evict the oldest unpinned thumbnail on overflow; survivors compact downward. For capacity five, 1-6 leaves only 2-6. Eviction destroys the thumbnail window and releases its resources, never its PNG.
- No hidden backlog or older/newer paging. Evicted thumbnails must never return after timeout, manual close, successful drag, capture cancellation, or a new capture.
- Expose pin and close controls on hover, plus native context-menu actions and tray close-all.
- Pins remain always on top and do not auto-expire. Pin/unpin preserves capture order and the existing stack position, without moving cards to another column. Pins reserve slots and remain visible during overflow. Removing a card compacts cards above it, preserving pin state. If every visible slot is pinned, new PNGs are saved but cannot add a preview to the full queue.
- A successful file drop closes an unpinned card but preserves a pinned reference. Closing a card never deletes its PNG.
- Pins currently use the same fixed centered-cover card. Resizing, opacity, click-through locking, full-size reference windows, and editing are not part of this milestone.
- Hide all previews and pins before capture and pause their clocks through selection and PNG publication. Restore them after completion or cancellation, including error recovery.
- Changing the selected timer starts a fresh interval; unpinning also starts a fresh interval. Hover and active drag pause rather than reset the remaining interval. Automatic dismissal is FIFO within each visible monitor stack: skip pins, finish the oldest unpinned card's exit before a newer card starts exiting. Newer elapsed budgets wait without resetting; hovering the oldest blocks automatic dismissal behind it. Permanently evicted overflow never blocks or returns to the surviving queue. Manual close and successful file drops remain independent of FIFO.
- Persist the timer preference across launches, not the queue/pin session. Closing the app or changing display/DPI dismisses reference cards; temporary files remain governed by retention independently.

---

## 6. Performance Requirements

The application should remain extremely lightweight.

Goals:

- Native Windows application
- Near-instant screenshot response
- Minimal background CPU usage
- Minimal memory usage
- No Electron
- No browser runtime
- No background services unless required
- Application can stay in the system tray

The application should feel invisible until the screenshot shortcut is pressed.

### 6.1 UX Acceptance Criteria

- Compare the complete capture-to-drop flow against a recorded native macOS reference, not just static screenshots.
- Establish a validation machine and record display refresh rate, resolution, DPI, OS version, and capture size with performance results.
- Target at least 60 fps during visible animation on the validation machine, with refresh-rate-aware presentation and no visible stutter, flashing, or blank frames.
- Region selection must track the pointer without perceptible lag or coordinate jumps.
- PNG encoding and cleanup must not block input handling or animation rendering.
- Validate thumbnail geometry and pointer alignment at 100%, 125%, 150%, and 200% Windows scaling, including mixed-DPI monitors.
- Test idle dismissal, hover entry/exit, successful drag, rejected drop, canceled drag, and rapid consecutive captures end to end.
- Verify focus preservation by taking and dragging screenshots while typing in a terminal.
- Test file drops in File Explorer and Windows Terminal with PowerShell and CMD, including paths containing spaces. Record elevated-target limitations rather than claiming universal compatibility.
- Measure hotkey-to-overlay and selection-release-to-thumbnail latency; finalize numerical budgets after reference measurement and capture prototyping.
- MVP acceptance requires both functional correctness and visual review. A working flow with noticeably rough motion or incorrect timing is not complete.

---

## 7. Technology Stack

Selected stack:

```text
Language
└── Rust

Windows Integration
├── windows crate
├── Win32 windows, global hotkey, and system tray
├── DXGI Desktop Duplication for screen capture
├── Windows Imaging Component (WIC) for PNG encoding
└── Windows OLE drag-and-drop (IDataObject, IDropSource, CF_HDROP)

Rendering
├── Native Win32 windows
├── Direct2D for region selection and thumbnail drawing
└── DirectComposition for composited thumbnail animation
```

No Slint, Electron, or browser runtime. Native Windows APIs provide direct control over focus, DPI, rendering, and file drag-and-drop.

Validate capture behavior across multiple monitors and display changes during prototyping. Stack selection does not itself guarantee macOS fidelity; reference measurements and end-to-end visual testing are required.

For drag-and-drop:

```text
Floating Thumbnail
        ↓
   IDataObject
        ↓
    CF_HDROP
        ↓
  Temporary PNG
```

This allows Windows applications to treat the screenshot as a normal file.

---

## 8. Initial MVP Scope

The initial baseline contains:

1. Global screenshot shortcut
2. Region selection
3. PNG screenshot creation
4. Floating bottom-right thumbnail
5. Drag-and-drop thumbnail
6. File-path drop into compatible terminals
7. Normal file drop into File Explorer
8. Automatic thumbnail dismissal
9. Temporary file cleanup

The approved follow-up adds a session-only multi-thumbnail queue, pinned reference cards, native timer choices, and close-all as specified in section 5.3. An editor or screenshot library is still excluded.

---

## 9. Explicitly Out of Scope

Do **NOT** implement:

- Screenshot editor
- Drawing
- Arrows
- Text annotations
- Cropping after capture
- Screenshot history UI
- Cloud sync
- Login/account
- OCR
- AI features
- Recording
- GIF capture
- Sharing service
- Screenshot library
- Complex settings UI

These features should not be added unless explicitly requested later.

---

## 10. Product Principle

The application should do one thing extremely well:

> **Screenshot -> Float -> Drag.**

Taking a screenshot and using it elsewhere should require almost no interruption to the user's current workflow.
