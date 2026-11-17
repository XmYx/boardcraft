# BoardCraft roadmap

**Stage: pre-alpha**

> **Status:** skeleton. The model, file format, screenplay import and FilmCraft export work and are
> tested; drawing and camera tools are minimal. · **Target:** a clean-room storyboarding and animatic
> app in the class of commercial storyboard tools (reference: Toon Boom Storyboard Pro, studied by use
> and public documentation only).

## Headline numbers

| Measure | Today |
|---|---|
| Overall parity with the target (estimate) | ~5% |
| Pipeline hand-offs | Fountain in, OTIO (FilmCraft) out, PNG panels out |
| Tests | 35 (31 core + 4 app UI tests with accessibility checks, run by `cargo xtask ci`) |

## By dimension

| Dimension | Today | Next |
|---|---|---|
| Board structure | scenes → panels, reorder, scene number + heading | sequences/acts, shots vs panels, renumbering rules |
| Drawing | round raster brush, pressure, eraser, layers, image import, pixel undo | vector strokes, stabilizer, fill, onion skin, light table, full undo |
| Camera | 2-key pan/zoom per panel, eased | multi-key editor, rotation, truck/camera paths, 3D-ish layers parallax |
| Timing | per-panel duration, timeline edge retiming | audio track (dialogue, scratch), transitions |
| Playback | panel durations through camera moves, captions | audio, loop range, frame-accurate scrubbing |
| Interchange | Fountain import, OTIO export, PNG stills | camera moves as FilmCraft Motion keyframes, FCPXML/EDL, PDF board sheets, re-import diff from ScriptCraft |
| Accessibility | AccessKit names for every control (checked in tests), keyboard shortcuts | full keyboard canvas navigation, high-contrast theme |
| Languages | English | craft-i18n catalogs |
| AI | role declared (`image.generate`), prompt builder, stub | wire craft-ai-client: consent, job progress, result as a new layer |

## Features

See [docs/target-app-parity.md](docs/target-app-parity.md) and the ranked [docs/gaps.md](docs/gaps.md).

## Languages

English only. Translations will come from [craft-i18n](https://github.com/storytold/craft-i18n)
(`CRAFT_I18N_DIR`), as in the other Craft apps.

## Upcoming

1. Board sheet export (PDF/PNG pages: panels with numbers, dialogue and action).
2. Camera moves into FilmCraft as Motion keyframes (OTIO `metadata.filmcraft` effects).
3. Audio: dialogue/scratch track under the animatic.
4. Full undo/redo (structure and text, not just pixels).
5. AI panel sketch through the shared Craft AI client.

## Agents: CLI and MCP

Not yet. The core crate is UI-free so a CLI (`boardcraft-cli`: import Fountain, export OTIO/PNG) and
an MCP server can be added on top of `boardcraft-core` like the other Craft apps.

## Progress log

| Date | What landed |
|---|---|
| 2026-11-17 | Skeleton: core model, `.boardcraft` format, Fountain import, OTIO + PNG export, egui app with board grid, panel canvas, camera keys, timeline and animatic playback, demo board |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First version |
