# Quick Access video reference

Source: `D:\Downloads\quickaccess.mp4`, 1360 x 960, 60 fps, 14.356 s. The encoded frames are a composed macOS product demonstration, not a recording of this Windows desktop. Local frames/contact sheet are ignored under `.pi/reference/annotation/`.

## Observable details

- At 2.8-3.0 s, the new chart card occupies approximately x=50..309, y=509..696: ~260 x 184 px, 16 px corners, bright blue ~3 px outline. The earlier cat card occupies x=50..309, y=708..891: same size, 12 px gap, neutral outline. On a 960 px canvas the bottom inset is ~68 px. Windows positions the corresponding card relative to the *usable monitor work area*, excluding the taskbar, and scales with DPI.
- On hover at ~4.5 s the chart darkens, showing light circular Close/Pin top controls, Annotate/Upload bottom controls and centered Copy/Save pills. The video includes cloud upload, but upload is intentionally disabled in this native app until an actual service exists.
- During the drag at ~5.4 s, the original stays visible at reduced opacity; a smaller image (~160 x 113 px) follows the cursor with its right edge near the pointer. The green plus and dashed blue destination highlight belong to the receiving application/OS, not to Quick Access. Windows uses copy-only OLE file drag and lets the receiving app provide its own feedback.
- After the accepted drop (~6.4-6.7 s), the original slides left and disappears. The preview enters from the left. The video does not demonstrate unhovered auto-dismiss, canceled drops, every DPI/monitor configuration or reduced-motion settings; those retain the app's existing semantics. 180 ms entrance/exit and 5 s default idle lifetime are implementation choices, not measurements of CleanShot's timer settings.

## Focused verification

`pwsh -NoProfile -Sta -File scripts/smoke-capture.ps1 -QuickAccessOnly -Configuration release` tests card geometry, pointer-relative mini-preview, retained dim source, Escape, rejected target and lossless copy-only Explorer drop. `-ActionsOnly` checks hover/action hit targets, stacking, clipboard and Save As. Both tests move the actual system pointer. Artifacts under `.pi/capture-smoke/` are local and ignored.

Visual comparison is approximate because the fixture contains different image content, background and Windows scaling. This is not a claim of pixel-perfect macOS parity.
