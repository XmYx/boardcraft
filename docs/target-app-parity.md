# Target app parity

## Headline

About 5% of a professional storyboard tool, by the features below weighted by everyday use. The
pipeline hand-offs (script in, edit out) are the part that is furthest along.

## Target and how it was measured

Target: Toon Boom Storyboard Pro, as a reference class for storyboarding and animatic tools. Studied
only through public documentation and ordinary use; nothing was copied (see AGENTS.md, clean room).
The estimate is a judgement per feature area, not a measurement.

## By dimension

| Dimension | Weight | Parity |
|---|---|---|
| Board structure and navigation | 15% | 25% |
| Drawing tools | 30% | 5% |
| Camera and staging | 15% | 10% |
| Timing, audio, playback | 15% | 10% |
| Script import / export | 15% | 25% |
| Printing and sharing | 10% | 5% |

## By feature area

| Area | Works today | Missing |
|---|---|---|
| Scenes, panels | add, delete, reorder, numbers, headings | acts/sequences, shots, renumbering rules, panel duplication |
| Drawing | raster brush + pressure, eraser, layers, image import | vector, fill, selection/transform, onion skin, stabilizer |
| Camera | pan/zoom keys, eased, overlay | rotation, multiple keys UI, 3D layer parallax |
| Timeline | duration retiming, playhead, scene bands | audio tracks, transitions, scrubbing audio |
| Import | Fountain | Final Draft XML, PDF scripts |
| Export | OTIO + stills, PNG panels | PDF board sheets, movie render, FCPXML/EDL |

## What works today (evidence)

- Tests: `cargo test --workspace` (format round trip and fuzzing, Fountain parsing, OTIO shape,
  camera interpolation, brush, app UI tests with accessibility checks).
- The OTIO export was opened with FilmCraft's own importer (`filmcraft-interchange`): one 24 fps
  sequence, one clip per panel at the right times, scene markers, stills linked, empty report.
- Screenshots: `docs/images/boardcraft-board.png`, `docs/images/boardcraft-panel.png`.

## Calibration

First estimate; recalibrate after the first user test with a real script.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First version |
