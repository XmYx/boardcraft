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
read and write through one neutral model), `craft-xml` (Final Draft, annotation SVG reading), `craft-svg` (annotation SVG writing and path data), `craft-pdf` (board sheets; its patched Krilla is pinned in
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
      Camera { keys: [CameraKey { frame, center [x, y], zoom, rotation }] }
    annotations: [Shape { type: stroke|arrow|motion_arrow|camera_arrow, points…, color, width }]
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

A key frames a rectangle with the canvas aspect: centre (normalized), zoom (1 = whole canvas) and
rotation (degrees clockwise). Between keys position and rotation ease with smoothstep and zoom
interpolates geometrically; a panel takes any number of keys. Keys store what was typed; when a
view is rendered, `CameraKey::fitted` raises the zoom to the least that keeps a turned frame on the
canvas and moves the centre inward, and `quad` gives the frame's four corners. `render_quad` samples
that parallelogram (stills, sheets), playback draws it as a textured quad (no CPU work per frame),
and timeline exports turn it into Motion keyframes ([interop](interop.md)).

## Annotations (`annotate`)

Vector shapes per panel, in canvas pixels, drawn over the raster layers by `annotate::draw` inside
`Panel::composite` and `composite_region`, so textures, thumbnails, stills, sheets and exports all
show them and painting keeps them. Shapes are edited by commands (`add_shape`, `move_shape`,
`style_shape`, `delete_shape`); hit tests and bounds come from the same flattened geometry.
SVG goes out through craft-svg's writer (a motion arrow stays a true quadratic curve, via kurbo)
and comes back through craft-svg's path-data parser and craft-xml. The app's annotation tools and
inspector live in `annotations.rs`, the camera-key strip in `camera_keys.rs`.

## App (`apps/boardcraft`)

- `app.rs`: state, menus, file actions, shortcuts, pixel undo, playback clock, screenshot mode.
- `board.rs`: Board view; `canvas.rs`: Panel view (drawing, camera tool, playback picture);
  `timeline.rs`: timeline strip; `inspector.rs`: panel properties, layers, camera keys.
- `textures.rs`: full-size and thumbnail textures for panels on screen, rebuilt on `revision`,
  dropped when not drawn for a while (below).
- `camera_keys.rs`: the camera-key strip; `annotations.rs`: annotation tools and inspector.
- Playback renders nothing per frame on the CPU: it draws the panel's texture with UV coordinates
  from the camera view.

## Large boards

- **Incremental saves.** Autosave and Save share a `format::EncodeCache`: each layer's encoded PNG
  is kept with a weak handle on its pixel buffer (`Raster::buffer_key`). Pixels are copy-on-write,
  so a layer nobody painted still has that buffer and its PNG is reused; a painted layer has a new
  buffer and is encoded again. The cache holds compressed bytes only, never pixels.
  `format::tests::large_board_saves_stay_bounded` builds a 300-panel, 600-layer board: the second
  save encodes nothing, one stroke re-encodes one layer, and the cache stays under a quarter of the
  raw pixel size.
- **Textures for what is on screen.** `Textures` stamps each texture with the frame it was drawn
  in; `end_frame` drops full-size textures not drawn for 120 frames (and keeps at most 6) and
  thumbnails not drawn for 180 frames. Views only ask for thumbnails of visible cards and blocks,
  so scrolling a 300-panel board keeps a screenful of thumbnails, not 300
  (`textures::tests::textures_follow_what_is_drawn`).

## Known limits

- Every layer is a full-canvas RGBA buffer in memory (1280×720 = 3.7 MB): a 300-panel board with
  two layers a panel holds about 2.2 GB of pixels. Tiles or loading layers on demand are the next
  step ([gaps](gaps.md) 14).
- A save still writes the whole archive (JSON and every PNG's bytes); only the encoding is skipped.
- Undo snapshots share unchanged pixel buffers; a stroke copies the layer it paints once.
- Annotations always draw above every raster layer.
