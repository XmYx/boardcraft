# Gaps

What stands between BoardCraft and a beta that professionals can rely on, ranked by impact for a
working storyboard artist (from [user-research.md](user-research.md) and the
[parity measure](target-app-parity.md)). Hours are rough estimates.

## Ranked: the top 15

| # | Gap | Category | Beta | Hours | Doc |
|---|---|---|---|---|---|
| 1 | Audio track under the animatic (import WAV/MP3, waveform, scrub, keep sync on retime) | Playback | yes | 30 | [user-research](user-research.md) A5, R2 |
| 2 | Movie export (MP4/MOV) with burn-ins: panel number, dialogue, timecode | Export | yes | 24 | A8, R4 |
| 3 | Pen tablet tuning: pressure curve, per-OS tests, stabilizer | Drawing | yes | 16 | A1 |
| 4 | Lasso select / move / transform, canvas rotation, fill | Drawing | yes | 30 | R6 |
| 5 | Remappable shortcuts (craft-keymap) and a shortcut sheet | UX | yes | 10 | A15, R11 |
| 6 | Board-sheet options in the app: fields, header text, notes lines | Printing | yes | 8 | A10, R5 |
| 7 | Camera rotation and a multi-key camera editor on the timeline | Camera | no | 16 | R1 |
| 8 | Camera moves exported as FilmCraft Motion keyframes | Interchange | no | 10 | [interop](interop.md) |
| 9 | Vector strokes (editable, resolution-free) | Drawing | no | 40 | — |
| 10 | Arrow and motion symbols as an annotation layer | Drawing | no | 12 | R21 |
| 11 | MCP server / control channel over the commands (CLI done) | Agents | no | 10 | [cli](cli.md), R24 |
| 12 | Translations in craft-i18n; translated core error texts | Suite | yes | 8 | [localization-parity](localization-parity.md) |
| 13 | Final app icon and renders; macOS, Windows, Flatpak packaging | Packaging | yes | 8 | [assets/app-icon/README.md](../assets/app-icon/README.md) |
| 14 | Large boards: tiled layers, textures for visible panels only, incremental autosave | Performance | yes | 16 | [architecture](architecture.md) |
| 15 | Conform applies order and deletions from the edit (timing is applied today) | Interchange | no | 6 | [interop](interop.md) |

### Audio track
The most requested missing feature. Needs decoding (craft-audio in craft-libs), an output stream,
a waveform strip under the timeline and sync rules when panels are retimed.

### Movie export
Needs a video encoder; craft-libs has no encoder yet (FilmCraft has one). Until then the OTIO/EDL/XML
export plus FilmCraft covers it.

### Large boards
Every layer is a full-canvas RGBA buffer (3.7 MB at 1280×720); undo and autosave share unchanged
buffers, but a 300-panel board with several layers needs tiles and on-demand textures. Autosave
re-encodes every layer each time.
