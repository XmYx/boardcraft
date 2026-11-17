# Architecture

## Crates

| Crate | Path | What | Depends on UI? |
|---|---|---|---|
| `boardcraft-core` | `crates/core` | document model, brush, camera, `.boardcraft` format, screenplay import and re-import (`script`, `fountain`), timeline export and conform (`timeline`), stills, AI hook, demo board | no (`cargo xtask layers`) |
| `boardcraft` | `apps/boardcraft` | the egui desktop app | yes (egui/eframe 0.37, rfd) |
| `boardcraft-cli` | `apps/boardcraft-cli` | headless commands, info, export | no |
| `xtask` | `xtask` | `cargo xtask ci`, `layers`, `assets`, `licenses`, `progress`, `version`, `strings` | no |

Shared libraries from [storytold/craft-libs](https://github.com/storytold/craft-libs), pinned by
commit in the root `Cargo.toml`: `craft-zip` (container), `craft-image-codecs` (PNG/JPEG/…
decode, PNG encode), `craft-atomic` (crash-safe saves), `craft-i18n` (interface language),
`craft-fonts-build` (optional craft-fonts faces), `craft-timeline` (OTIO, EDL, FCP7 XML and FCPXML
read and write through one neutral model), `craft-xml` (Final Draft), `craft-pdf` (board sheets; its patched Krilla is pinned in
`[patch.crates-io]` as DeckCraft does), `craft-ai-core` (AI role
declaration).

### craft-libs API gaps (not filled by copying code)

- No raster paint canvas or brush crate: `boardcraft-core::raster` is local and a candidate to move.
- No Final Draft (`.fdx`) reader: `boardcraft-core::script::parse_fdx` is local (on craft-xml);
  ScriptCraft will need the same, so it is a candidate for craft-libs.
- No Fountain parser: `boardcraft-core::fountain` should move to craft-libs once ScriptCraft needs
  the same parser.
- No progress-log joiner yet (RFC-0017): `cargo xtask progress` is a minimal local joiner until the
  craft-libs one lands.

## Commands

`boardcraft-core::command` holds every board edit as a serializable `Command`; `apply` performs it
and leaves the project unchanged on error. The app routes structural edits (add/delete/move panel,
scenes, durations) through it; `boardcraft-cli exec` takes the same JSON ([cli.md](cli.md)).
Drawing strokes and in-place text editing write the model directly (pixel undo covers strokes).

## Data model (`boardcraft-core::model`)

```
Project { name, settings { width, height, fps }, scenes, next_id }
  Scene { id, number, heading, source_scene_id, panels }
    Panel { id, duration (frames), dialogue, action, notes, layers, camera, revision* }
      Layer { id, name, visible, opacity, pixels* }      * = not serialized as JSON
      Camera { keys: [CameraKey { frame, center [x, y], zoom }] }
```

- Ids are `u64`, unique per project, handed out by `Project::alloc`. `Project::repair` makes any
  loaded project consistent (unique ids, `next_id` past every id, clamped values, layer sizes).
- Time is whole frames at the project rate. A panel's camera keys are frame offsets inside it.
- `Panel::revision` changes whenever its pixels change; the app's texture cache rebuilds on it.

## Drawing (`raster`)

Straight-alpha RGBA8 canvases at the project size. A stroke is a run of round dabs spaced at a
fifth of the radius, with smoothstep edge falloff by `hardness`; pressure scales radius and/or
opacity. The eraser lowers alpha with the same dab. `render_view` resamples a normalized rectangle
(bilinear, supersampled when shrinking) for camera views, stills and thumbnails.

## Camera (`camera`)

A key frames a rectangle with the canvas aspect: centre (normalized) and zoom (1 = whole canvas).
Between keys position eases with smoothstep and zoom interpolates geometrically; keys are clamped so
the frame never leaves the canvas.

## App (`apps/boardcraft`)

- `app.rs`: state, menus, file actions, shortcuts, pixel undo, playback clock, screenshot mode.
- `board.rs`: Board view; `canvas.rs`: Panel view (drawing, camera tool, playback picture);
  `timeline.rs`: timeline strip; `inspector.rs`: panel properties, layers, camera keys.
- `textures.rs`: one full-size and one thumbnail texture per panel, rebuilt on `revision`.
- Playback renders nothing per frame on the CPU: it draws the panel's texture with UV coordinates
  from the camera view.

## Known limits

- Every layer is a full-canvas RGBA buffer in memory (1280×720 = 3.7 MB); large boards want tiles.
- Undo covers drawing (pixels) only.
