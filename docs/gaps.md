# Gaps

What stands between BoardCraft and a beta that professionals can rely on, ranked by impact for a
working storyboard artist (from [user-research.md](user-research.md) and the
[parity measure](target-app-parity.md)). Hours are rough estimates.

## Ranked: the top 15

| # | Gap | Category | Beta | Hours | Doc |
|---|---|---|---|---|---|
| 1 | ~~Audio track under the animatic~~ done: import, waveform, mix, anchors keep sync on retime; left: scrub audio, recording, per-clip fades | Playback | yes | 6 | [user-research](user-research.md) A5, R2 |
| 2 | ~~Movie export with burn-ins~~ done: MP4 (H.264/AAC), panel number, timecode, dialogue; left: MOV/ProRes, burn-in layout options | Export | yes | 4 | A8, R4 |
| 3 | Pen tablet tuning: ~~pressure curve, stabilizer~~ (done); per-OS tablet tests remain | Drawing | yes | 6 | A1 |
| 4 | ~~Lasso select / move / transform, canvas rotation, fill~~ (done); anti-aliased selection edges, free-distort handles remain | Drawing | no | 6 | R6 |
| 5 | ~~Remappable shortcuts (craft-keymap)~~ done; a printable shortcut sheet remains | UX | yes | 2 | A15, R11 |
| 6 | ~~Board-sheet options in the app: fields, header text, notes lines~~ done; sheet templates (logo, custom layouts) remain | Printing | no | 6 | A10, R5 |
| 7 | ~~Camera rotation and a multi-key camera editor~~ done; left: per-key easing curves, keys on the main timeline | Camera | no | 4 | R1 |
| 8 | ~~Camera moves exported as FilmCraft Motion keyframes~~ done (OTIO, FCP7 XML, FCPXML) | Interchange | no | 0 | [interop](interop.md) |
| 9 | Vector strokes: ~~editable, resolution-free~~ done as annotations; left: pressure-shaped vector brush, vector layers in the layer stack | Drawing | no | 24 | — |
| 10 | ~~Arrow and motion symbols as an annotation layer~~ done (arrow, motion arrow, camera arrow, SVG in/out) | Drawing | no | 0 | R21 |
| 11 | ~~MCP server over the commands~~ done (`boardcraft-cli mcp`); a live channel into the running app remains | Agents | no | 8 | [mcp](mcp.md), R24 |
| 12 | Translations: catalogs `locales/<locale>/boardcraft.tsv` in the craft-i18n repository (translators; 356 source texts listed in docs/i18n); translated core error texts | Suite | yes | 8 | [localization-parity](localization-parity.md) |
| 13 | ~~Icon renders; macOS, Windows, Flatpak packaging~~ done (`cargo xtask icons`, packaging/); left: final ArtCraft icon artwork, icon embedded in boardcraft.exe, Windows install-policy check, a CI packaging run | Packaging | yes | 4 | [assets/app-icon/README.md](../assets/app-icon/README.md) |
| 14 | Large boards: ~~textures for visible panels only, incremental autosave~~ done; left: tiled layers, lazy layer loading | Performance | yes | 10 | [architecture](architecture.md) |
| 15 | ~~Conform applies order and deletions from the edit~~ done (order, and flags for left-out panels; opt-in) | Interchange | no | 0 | [interop](interop.md) |

### Audio track (done)
Clips decode through craft-audio, play through cpal, show a waveform strip under the timeline and
follow their anchor panel when the board is retimed. Left: audio scrubbing while dragging the
playhead, recording from a microphone, fades, and decoding on a background thread (long files
decode on the UI thread the first time they are drawn).

### Movie export (done)
H.264 (craft-video) and AAC (craft-audio) in MP4 (craft-media), with burn-ins. Left: MOV/ProRes
for editorial, a choice of burn-in fields and positions, and streaming large movies to disk (the
file is built in memory so its index can go first).

### Done in this round (items 5, 6, 11, 15)
- **Shortcuts.** Every shortcut goes through `craft-keymap`: the shared Keyboard Shortcuts dialog
  (Edit ▸ Keyboard Shortcuts…) changes, clears or resets them, menus show the current chord, and the
  changes are kept in `config.toml` in the data folder. Missing: a printable shortcut sheet in the
  app (the dialog's Summarize gives the text), named shortcut sets and importing other apps' sets
  (the dialog supports both once BoardCraft passes sets and readers).
- **Board-sheet options.** File ▸ Export Board Sheet (PDF)… chooses paper, grid, the fields that
  print, a header line and ruled note lines (also `export-sheet` flags). Missing: a logo, custom
  templates, and keeping the choices between sessions.
- **MCP.** `boardcraft-cli mcp` serves one board file. Missing: driving the running app (its open
  board and undo) from an agent.
- **Conform order.** Opt-in reorder and marking of panels the edit leaves out, with a summary before
  applying in the app.

### Large boards
Every layer is a full-canvas RGBA buffer (3.7 MB at 1280×720); undo and autosave share unchanged
buffers. Saves and autosaves now encode only layers painted since the last save, and textures
exist only for panels on screen (see [architecture](architecture.md#large-boards)). Still open:
every layer stays in memory (a 300-panel board with two layers is about 2.2 GB at 1280×720), so
tiles or loading layers on demand remain the next step.

### Drawing tools (items 3 and 4)
Done: pressure curve with a preview, minimum line size and a 0–10 stabilizer (Pen Settings, saved
in the suite's `config.toml` through craft-settings); Lasso and Rectangle select with move, scale
and rotate by drag, arrow keys or typed values, Enter/Esc, cut/copy/paste/delete; Fill with
tolerance, gap closing and layer or all-layer sampling; view rotation (Shift+[ / Shift+] / Shift+0,
R+drag, typed). Remaining: testing real tablets on Windows (Windows Ink), macOS and Linux
(Wayland/X11) to confirm pressure arrives through egui; selections are hard-edged (no feathering or
anti-aliased lasso edge); scale handles are uniform (typed width/height for non-uniform); the pixel
clipboard is BoardCraft's own (copies also go to the system clipboard as an image, but Ctrl+V on
Wayland only pastes BoardCraft's own copy); the lasso has no keyboard drawing (Select All plus typed
values is the keyboard path).

### Camera and annotations (items 7 to 10)
Camera keys rotate and a panel takes any number of them (key strip in the Panel view); timeline
exports carry the move as Motion keyframes. Annotations are vector shapes over the drawing. Open:
per-key ease curves (every segment eases in and out), camera keys shown on the main timeline, a
pressure-shaped vector brush and vector layers interleaved with raster layers (annotations always
draw on top).
