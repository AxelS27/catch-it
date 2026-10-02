# Annotate Background Tool - supplied video baseline

Source: `D:\Downloads\backgroundtool.mp4`, 1360 x 960, 60 fps, 12.3 s, SHA-256 `39926c6a00e1a4ae508c1843881c5fa1cf366865ee1429cb1a49f17d00980eb8`. This is byte-identical to the official background video previously reviewed in `docs/annotation-cleanshot-research.md` (source B). The video shows an older, dark CleanShot editor; `markup.mp4` shows different chrome. Treat this video as the authority for Background Tool behavior, not as proof that both versions share identical window controls. Inspection frames/contact sheets are ignored under `.pi/reference/annotation/backgroundtool-*`.

## Direct observations

- 0-1.2 s: a centered, uncropped screenshot with no decorative background. Background toolbar button opens a *persistent* left sidebar, leaving top toolbar and footer in place; canvas shifts right rather than being covered.
- 1.8 s: sidebar at approximately x=94-354, y=121-839 in the 1360 x 960 encoded frame. It contains `Presets...` dropdown and `+`; `None`; 20 gradient tiles in a 5 x 4 grid; collapsible `Gradients` section; `Wallpapers` and `+`; three `Blurred` tiles; two rows of `Plain color` circular swatches; `Padding`, `Inset`, `Shadow`, `Corners` sliders and `Auto-balance` checkbox. The exact pixel values depend on the marketing-video scale.
- ~2.7-3.2 s: clicking gradient tile row 3, column 3 highlights it with a blue outline and immediately replaces the canvas with a purple/lilac/cyan gradient **behind** the original screenshot. The screenshot remains un-stretched and has padding and a distinct soft shadow. Changing padding later changes output framing rather than zooming the source pixels.
- ~4-7 s: continuous slider drags update the canvas live. Shadow and corner sliders are separate; corner rounding clips the screenshot, not the entire app window. The background can extend well beyond the source image bounds.
- ~8.3 s: `Auto-balance` becomes checked (blue checkbox) and the framing changes. The precise algorithm and interaction with asymmetric image content are not inferable from the video.
- ~10.5 s: toolbar background icon is clicked again; no export is shown. No visible demonstration of the other 19 full-size gradients, preset persistence, custom wallpaper import, blurred variants, custom color picker, aspect-ratio controls, or resulting PNG/clipboard/drag bytes.

## Other sourced evidence

[Official CleanShot feature page](https://cleanshot.com/features) explicitly advertises 20 hand-picked backgrounds, adding your own background, reusable presets, Auto Balance, alignment, aspect ratio, and padding. It does **not** publish the full-resolution preset artwork, output byte rules, numeric ranges, or Auto Balance algorithm. Search summaries suggesting exact values/aspect ratios or that Auto Balance detects content are **not** accepted as specifications without direct evidence.

## Search for identical assets (public sources checked)

Google and the official [feature page](https://cleanshot.com/features) identify the **count** and features, but did not reveal direct URLs for all 20 full-resolution preset files or an explicit license to redistribute them. The [screenshots page](https://cleanshot.com/screenshots) and public video illustrate only selected output, not downloadable original presets. Claims in search-generated summaries that a particular `Assets.car` contains every preset, or that unrelated GitHub projects mirror the actual assets, remain **unverified**. A Mac installation may contain the source artwork, but the resource paths and rights would need checking against a specific owned version. Do not treat public visual access as permission to commit proprietary image files to this repository.

## Parity boundary / engineering requirements

The existing Windows editor currently has a disabled Background toolbar icon, no left sidebar, and exports the original screenshot unchanged. A cosmetic-only panel would be misleading. Implementing the feature means the preview and Copy/Save As/Drag Me must all use the composed full-resolution image, while preserving the original capture and keeping zoom/pan independent of export dimensions. The reference artwork/icons cannot be extracted and shipped as product assets; original gradients/wallpapers or user-supplied licensed assets are needed. A video alone cannot prove 1:1 behavior for unseen modes, exact gradients, or exports. Keep observed behavior separate from proposed Windows choices and validate end-to-end with actual saved PNGs before claiming parity.
