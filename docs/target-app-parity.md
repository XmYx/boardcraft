# Target app parity

## Headline

About **33%** of the professional reference (Toon Boom Storyboard Pro), up from about **28%** after
the polish pass and about **19%** before it (all 2026-11-17; the drawing-tools pass moved the drawing
dimension; the earlier "~5%" headline predated the interchange work and was not re-measured then).
Against the free tools users compare it with (Storyboarder, Boords' drawing, StudioBinder
storyboards), BoardCraft is near or ahead on data safety, interchange, timing and now everyday
drawing tools, and behind on audio, vector strokes and rulers.

## Target and how it was measured

Target: Toon Boom Storyboard Pro, with Storyboarder, Boords, FrameForge, StudioBinder and
Photoshop/Procreate board workflows as close competitors. Studied only through public documentation,
ordinary use and published reviews ([user-research.md](user-research.md)); nothing was copied
(AGENTS.md, clean room). Each dimension is a judgement of how much of the reference's everyday
feature set works today, weighted by everyday use. Re-measured 2026-11-17.

## By dimension

| Dimension | Weight | Before polish | Now | What moved it |
|---|---|---|---|---|
| Board structure and navigation | 15% | 25% | 40% | duplicate panel, Board Settings, keyboard focus and navigation |
| Drawing tools | 30% | 5% | 30% | canvas zoom/pan/rotate, straight lines, onion skin, edit in any image editor, faster strokes; pressure curve and stabilizer, lasso/rectangle select with transform and cut/copy/paste, bucket fill (drawing-tools pass) |
| Camera and staging | 15% | 10% | 15% | typed camera keys; still no rotation or 3D |
| Timing, audio, playback | 15% | 10% | 20% | 23.976–60 fps, rate change keeping timing; still no audio |
| Script import / export | 15% | 50% | 55% | shot list CSV, shot field |
| Printing and sharing | 10% | 35% | 45% | shot list; sheet field choice still missing |
| **Weighted total** | | **≈19%** | **≈33%** | |

Data safety (autosave, crash recovery, full undo, atomic saves) is not a feature dimension of the
reference but was its users' most serious complaint category; BoardCraft now covers it.

## By feature area

| Area | Works today | Missing |
|---|---|---|
| Scenes, panels | add, duplicate, delete, reorder, numbers, headings, shot type | acts/sequences, renumbering rules, panel split |
| Drawing | pressure brush with pressure curve, minimum size and stabilizer, eraser, layers, image import, external editor round trip, zoom/pan/rotate view, straight lines, onion skin, lasso and rectangle select with move/scale/rotate and cut/copy/paste, bucket fill with gap closing | vector strokes, rulers, textured brushes, feathered selections, free distort |
| Camera | pan/zoom keys, eased, typed values | rotation, multi-key timeline editor, 3D layers |
| Timeline | duration retiming, playhead, scene bands, 23.976–60 fps | audio tracks, transitions |
| Import | Fountain, Final Draft, ScriptCraft scene list; re-import by scene id; conform | PDF scripts, audio |
| Export | OTIO, EDL, FCP7 XML, FCPXML (+ stills), PDF board sheets, shot list CSV, PNG panels | movie render with burn-ins, sheet templates |
| Safety | autosave, crash recovery, undo/redo for every edit, atomic saves | version history |

## What works today (evidence)

- Polish-pass tests: autosave and recovery (unit and kittest), undo of strokes, typing and
  structural edits by keyboard, Board Settings by keyboard, duplicate/onion/zoom/shot presets.

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
