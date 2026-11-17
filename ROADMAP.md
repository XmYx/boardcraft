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
| Overall parity with the target (estimate) | ~5% |
| Pipeline hand-offs | Fountain / FDX / scene-list JSON in (with re-import), OTIO / EDL / FCP7 XML / FCPXML out and conform back, PNG panels |
| Tests | run `cargo xtask ci`: core, CLI, app UI (kittest, accessibility checks), xtask |

## By dimension

| Dimension | Today | Next |
|---|---|---|
| Board structure | scenes → panels, reorder, scene number + heading | sequences/acts, shots vs panels, renumbering rules |
| Drawing | round raster brush, pressure, eraser, layers, image import, pixel undo | vector strokes, stabilizer, fill, onion skin, light table, full undo |
| Camera | pan/zoom keys per panel, eased, typed key values | rotation, timeline key editor, layer parallax |
| Timing | per-panel duration, timeline edge retiming | audio track (dialogue, scratch), transitions |
| Playback | panel durations through camera moves, captions | audio, loop range |
| Interchange | Fountain, Final Draft and ScriptCraft JSON import; re-import by scene id; OTIO, EDL, FCP7 XML, FCPXML export; conform timing; PNG stills | camera moves as FilmCraft Motion keyframes, PDF board sheets |
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

1. Board sheet export (PDF/PNG pages: panels with numbers, dialogue and action).
2. Camera moves into FilmCraft as Motion keyframes.
3. Audio: dialogue/scratch track under the animatic.
4. Full undo/redo through the command layer.
5. AI panel sketch through the shared Craft AI client.

## Agents: CLI and MCP

`boardcraft-cli` runs every board command headless ([docs/cli.md](docs/cli.md)). An MCP server
over the same commands is planned.

## Progress log

New entries are fragments in `docs/progress.d/` (RFC-0017); `cargo xtask progress` shows the joined
log.

| Date | What landed |
|---|---|

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First version |
