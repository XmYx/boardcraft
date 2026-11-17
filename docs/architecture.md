# Architecture

## Crates

| Crate | Path | What | Depends on UI? |
|---|---|---|---|
| `boardcraft-core` | `crates/core` | document model, brush, camera, `.boardcraft` format, Fountain import, OTIO/PNG export, AI hook, demo board | no (`cargo xtask layers`) |
| `boardcraft` | `apps/boardcraft` | the egui desktop app | yes (egui/eframe 0.37, rfd) |
| `xtask` | `xtask` | `cargo xtask ci | layers | assets` | no |

Shared libraries from [storytold/craft-libs](https://github.com/storytold/craft-libs), pinned by
commit in the root `Cargo.toml`: `craft-zip` (container), `craft-image-codecs` (PNG/JPEG/…
decode, PNG encode), `craft-atomic` (crash-safe saves), `craft-ai-core` (AI role declaration).

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
