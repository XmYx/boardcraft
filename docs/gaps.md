# Gaps

What stands between BoardCraft and a usable beta, most important first. Hours are rough estimates.

## Ranked: the top 15

| # | Gap | Category | Beta | Hours | Doc |
|---|---|---|---|---|---|
| 1 | Board sheet export (PDF/PNG pages with panel text) | Interchange | yes | 16 | — |
| 2 | Full undo/redo (structure, text, camera) | Editing | yes | 12 | [architecture](architecture.md) |
| 3 | Camera moves exported as FilmCraft Motion keyframes | Interchange | yes | 10 | [interop](interop.md) |
| 4 | Audio track (dialogue/scratch) in the animatic | Playback | yes | 24 | — |
| 5 | Vector strokes (resolution-free, editable) | Drawing | no | 40 | — |
| 6 | Onion skin / light table across panels | Drawing | yes | 8 | — |
| 7 | Multi-key camera editor on the timeline | Camera | yes | 12 | — |
| 8 | Re-import from ScriptCraft by scene id (update, not duplicate) | Interchange | yes | 10 | [interop](interop.md) |
| 9 | AI panel sketch through craft-ai-client | AI | no | 16 | [architecture](architecture.md) |
| 10 | Panel transitions (cut, dissolve) in playback and export | Playback | no | 8 | — |
| 11 | MCP server / control channel over the commands (CLI done) | Agents | no | 10 | [cli](cli.md) |
| 12 | Translations in craft-i18n; translated core error texts; craft-ui theme | Suite | yes | 8 | [localization-parity](localization-parity.md) |
| 13 | Final app icon and its renders; macOS, Windows, flatpak packaging (Linux tar/deb/rpm done) | Packaging | yes | 8 | [assets/app-icon/README.md](../assets/app-icon/README.md) |
| 14 | Memory: panel layers as tiles, textures for visible panels only | Performance | no | 12 | [architecture](architecture.md) |
| 15 | Crash journal / autosave | Safety | yes | 6 | — |

### Board sheet export
Printable pages (N panels per page, panel number, dialogue, action, notes). Needs text rendering from
craft-fonts; craft-libs has `craft-pdf`.

### Camera moves exported as FilmCraft Motion keyframes
Today each panel is exported as a still of the camera's start view. FilmCraft keeps effect
keyframes in OTIO `metadata.filmcraft`; mapping the camera keys to Motion position/scale keys would
carry the moves into the edit.

### AI panel sketch
`crates/core/src/ai.rs` declares role `boardcraft-panel-image` (capability `image.generate`, inputs
prompt + reference) and builds the prompt; `generate_panel_image` returns "not supported yet".
Wiring needs `craft-ai-client` from storytold/craft-ai-integrations: status, plan, consent, start,
progress UI, result as a new layer.
