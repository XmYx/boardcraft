<p align="center">
  <img src="assets/app-icon/boardcraft.svg" width="160" alt="BoardCraft icon">
</p>

<h1 align="center">BoardCraft</h1>

<p align="center"><b>Storyboards and animatics, in pure Rust.</b></p>

<p align="center">
BoardCraft is an open-source storyboarding and animatic app, one of the Crafting Apps. Board a
script into scenes and panels, sketch each panel with a pressure-aware brush, frame camera moves,
time the panels and play the animatic, then send the cut to FilmCraft.
</p>

<p align="center">
  <img alt="Status: pre-alpha" src="https://img.shields.io/badge/status-pre--alpha-orange">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue">
  <img alt="Rust 1.95+" src="https://img.shields.io/badge/rust-1.95%2B-orange">
  <img alt="Platforms" src="https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey">
</p>

> [!NOTE]
> BoardCraft is a skeleton proposed to the ArtCraft team as the storyboarding step of the suite's
> film pipeline: **ScriptCraft** (screenplay) → **BoardCraft** (storyboards, animatic) →
> **FilmCraft** (edit). It is pre-alpha: the document model, file format and pipeline hand-offs
> work and are tested; the drawing tools are deliberately minimal.

![BoardCraft: the demo board in the panel grid, with the animatic timeline below](docs/images/boardcraft-board.png)

[What works today](#what-works-today) · [Getting started](#getting-started) · [Pipeline](#pipeline) · [Downloads](#downloads) ·
[Roadmap](#roadmap) · [The Crafting Apps](#the-crafting-apps) · [License and credits](#license-and-credits)

## What works today

- **Scenes and panels.** A board is scenes (number, heading) holding panels. Add, delete and reorder
  panels (across scene boundaries too).
- **Drawing.** Round brush with soft edge, size, colour and opacity; pen pressure (from tablets
  that report it) can drive size and opacity; eraser; layers with visibility and opacity; image
  import onto a layer (PNG, JPEG, WebP, TIFF, BMP, GIF through `craft-image-codecs`); pixel undo.
- **Panel text.** Dialogue, action and notes fields per panel.
- **Timing.** Each panel has a duration in frames; drag a panel's edge on the timeline to retime it.
- **Camera.** Pan and zoom keyframes per panel (start and end), eased, edited by dragging and
  scrolling in the Panel view; the camera frames are drawn over the drawing.
- **Views.** Board view (thumbnail grid grouped by scene), Panel view (canvas, tools, camera), and
  a timeline strip with scene bands, thumbnails and a playhead.
- **Animatic playback.** Plays the panels for their durations at the project rate through each
  panel's camera move, with the dialogue as a caption.
- **Screenplay import.** Fountain, Final Draft (`.fdx`) and ScriptCraft's scene list (JSON) become
  scenes and panels with text and an estimated duration.
- **Re-import by scene id.** Importing a revised script updates the board in place: scenes matched
  by their stable id keep their drawings, cameras and timing and take the new text; new scenes are
  added, removed ones kept and reported.
- **Timeline export.** The animatic as OpenTimelineIO, CMX 3600 EDL, Final Cut Pro 7 XML or FCPXML,
  with rendered panel stills and scene markers (through craft-timeline). All four verified with
  FilmCraft's own importers.
- **Conform.** Read an editor's cut back (any of the four) and take its panel timing.
- **Panel images.** Export every panel as a PNG.
- **Open project format.** `.boardcraft` is a ZIP of `project.json` plus one PNG per layer,
  documented in [docs/file-format.md](docs/file-format.md). Saves are atomic.

- **Commands and CLI.** Every board edit is a command; `boardcraft-cli` runs them headless and
  exports, for scripts and agents ([docs/cli.md](docs/cli.md)).
- **Accessible.** Every control has an AccessKit name; panels, timeline blocks and the canvas
  publish nodes; every drag has a keyboard or typed alternative; checked by kittest tests.
- **Languages.** Interface text goes through craft-i18n; translations arrive with
  `CRAFT_I18N_DIR` (English until BoardCraft's catalog exists).

Stubbed: **AI panel sketch** declares its role to the shared Craft AI layer and builds the prompt,
but does not call a model yet ([docs/gaps.md](docs/gaps.md)).

![The Panel view: canvas with the start and end camera frames, inspector and timeline](docs/images/boardcraft-panel.png)

## Getting started

You need Rust 1.95 or newer (`rustup`).

```bash
cargo run --release -p boardcraft -- --demo          # the demo board
cargo run --release -p boardcraft -- script.fountain # board a screenplay
cargo run --release -p boardcraft -- my.boardcraft   # open a project
```

| Keys | |
|---|---|
| Space | play / pause the animatic |
| ← → | previous / next panel (moves keyboard focus to it) |
| Enter | open the focused panel in the Panel view |
| , . / Home | playhead one frame back / forward / to the start |
| V | Board view ↔ Panel view |
| B / E / C | brush / eraser / camera tool |
| [ ] | smaller / larger brush |
| Ctrl+S, Ctrl+Shift+S, Ctrl+O | save, save as, open |
| Ctrl+Z, Ctrl+Shift+Z | undo / redo drawing |
| Ctrl+N | new panel after the selected one |

Optional build inputs (never Cargo dependencies), as in the other Craft apps:

```bash
CRAFT_I18N_DIR=$PWD/../craft-i18n CRAFT_FONTS_DIR=$PWD/../craft-fonts cargo run --release -p boardcraft
```

Headless:

```bash
cargo run -p boardcraft-cli -- new board.boardcraft --from script.fountain
```

Checks before every commit:

```bash
cargo xtask ci     # fmt, clippy -D warnings, tests, layering, assets, licences, progress fragments
```

## Pipeline

- **In: ScriptCraft (or any screenwriting app).** File ▸ Import Screenplay reads Fountain, Final
  Draft or ScriptCraft's scene list; on a board with work it updates scenes by their stable id.
- **Out: FilmCraft (or any editor).** File ▸ Export Animatic Timeline ▸ OTIO / EDL / FCP7 XML /
  FCPXML writes the timeline and `<name>_stills/`. File ▸ Conform Timing from Edit brings the
  editor's timing back.

Details: [docs/interop.md](docs/interop.md).

## Downloads

None yet: BoardCraft has not had a release. Packages will be named
`boardcraft-<version>-<platform>-<arch>.<ext>` (Linux tar.gz/deb/rpm from `packaging/linux/package.sh`).

## Roadmap

Stage and plans are in [ROADMAP.md](ROADMAP.md); the ranked work list is [docs/gaps.md](docs/gaps.md).

## The Crafting Apps

| | App | What it's for | Code |
|---|---|---|---|
| | ScriptCraft | Screenwriting | storytold/scriptcraft (proposed) |
| | **BoardCraft** | Storyboards and animatics | this repository |
| | FilmCraft | Video editing | [storytold/filmcraft](https://github.com/storytold/filmcraft) |
| | AniCraft | 2D vector animation | [storytold/anicraft](https://github.com/storytold/anicraft) |
| | PhotoCraft | Photo editing | [storytold/photocraft](https://github.com/storytold/photocraft) |

## License and credits

BoardCraft is licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option. Assets and their licences are listed in
[ATTRIBUTION.md](ATTRIBUTION.md); see also [NOTICE](NOTICE). Built on the shared Craft libraries
([storytold/craft-libs](https://github.com/storytold/craft-libs)) and egui.
