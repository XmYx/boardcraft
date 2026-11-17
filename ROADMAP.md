# BoardCraft roadmap

**Last updated:** 2026-11-17 · **Change:** major

**Stage: pre-alpha**

> **Status:** skeleton. The model, file format, commands and CLI, screenplay import and FilmCraft
> export work and are tested; drawing and camera tools are minimal. · **Target:** a clean-room
> storyboarding and animatic app in the class of professional storyboard tools (the reference app
> is named in [docs/target-app-parity.md](docs/target-app-parity.md)).

## Headline numbers

| Measure | Today |
|---|---|
| Overall parity with the target (estimate) | ~28% (re-measured 2026-11-17; ~19% before the polish pass) |
| Pipeline hand-offs | Fountain / FDX / scene-list JSON in (with re-import), OTIO / EDL / FCP7 XML / FCPXML out and conform back, PNG panels |
| Tests | run `cargo xtask ci`: core, CLI, app UI (kittest, accessibility checks), xtask |

## Safety

Autosave every 30 s with crash recovery, undo/redo for every edit, atomic saves.

## By dimension

| Dimension | Today | Next |
|---|---|---|
| Board structure | scenes → panels, reorder, scene number + heading | sequences/acts, shots vs panels, renumbering rules |
| Drawing | pressure brush, eraser, layers, image import, edit in any image editor, onion skin, zoom/pan, straight lines | pressure curve, stabilizer, lasso/transform, fill, vector |
| Camera | pan/zoom keys per panel, eased, typed key values | rotation, timeline key editor, layer parallax |
| Timing | per-panel duration, timeline edge retiming, 23.976–60 fps | audio track, transitions |
| Playback | panel durations through camera moves, captions | audio, loop range |
| Interchange | Fountain, Final Draft and ScriptCraft JSON import; re-import by scene id; OTIO, EDL, FCP7 XML, FCPXML export; conform timing; PDF board sheets; PNG stills | camera moves as FilmCraft Motion keyframes, sheet templates |
| Agents | every edit a command; `boardcraft-cli` (new, info, exec, export) | MCP server / control channel |
| Accessibility | named nodes, keyboard flows, live status, typed alternatives to drags | canvas content nodes per layer, high-contrast theme |
| Languages | English; craft-i18n wiring ready | translations in craft-i18n |
| AI | role declared (`image.generate`), prompt builder, stub | wire craft-ai-client |

## Features

See [docs/target-app-parity.md](docs/target-app-parity.md) and the ranked [docs/gaps.md](docs/gaps.md);
plans in [docs/roadmap.md](docs/roadmap.md).

## Languages

English only until craft-i18n has `locales/<locale>/boardcraft.tsv`. See
[docs/localization-parity.md](docs/localization-parity.md).

## Upcoming

1. Audio track under the animatic.
2. Movie export with burn-ins.
3. Pen tablet tuning and a stabilizer.
4. Lasso, transform, rotation, fill.
5. Remappable shortcuts.

## Agents: CLI and MCP

`boardcraft-cli` runs every board command headless ([docs/cli.md](docs/cli.md)), and
`boardcraft-cli mcp` serves them, with import, conform and the exports, as MCP tools
([docs/mcp.md](docs/mcp.md)). A live channel into the running app is planned.

## Progress log

New entries are fragments in `docs/progress.d/` (RFC-0017); `cargo xtask progress` shows the joined
log.

| Date | What landed |
|---|---|

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First version |
