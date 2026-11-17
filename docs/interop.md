# Interop: ScriptCraft in, FilmCraft out

BoardCraft sits between **ScriptCraft** (screenplay) and **FilmCraft** (edit).

## In: screenplays (Fountain)

File ▸ Import Screenplay, or `boardcraft script.fountain`. Fountain syntax from fountain.io:

| Fountain | BoardCraft |
|---|---|
| title page `Title:` | project name |
| scene heading (`INT.`, `EXT.`, `EST.`, `INT./EXT.`, `I/E`, or forced `.HEADING`) | a scene; `#12#` at the end is the scene number |
| `[[scene-id: <id>]]` in the scene | `scene.source_scene_id` |
| action paragraph | a panel with that action text |
| character + (parenthetical) + dialogue | a panel with dialogue `NAME (paren): line` |
| transition (`CUT TO:`, `> FADE OUT`) | a note on the previous panel |
| notes, boneyard, sections, synopses, page breaks | ignored |

Panel durations are estimated from the text (about 2.5 spoken words a second, 1.5–8 s). Importing
into a board that already has work appends the scenes.

**Dependency on ScriptCraft:** ScriptCraft's own `docs/interop.md` did not exist when this was
written, so BoardCraft assumes plain Fountain, plus the optional `[[scene-id: …]]` note proposed to
ScriptCraft so a later re-import can update boards scene by scene instead of duplicating them. If
ScriptCraft settles on a different hand-off, BoardCraft follows it.

## Out: animatic timeline for FilmCraft (OpenTimelineIO)

File ▸ Export Animatic Timeline (OTIO) writes `<name>.otio` and `<name>_stills/`:

- one `Timeline.1` at the project rate, one video `Track.1`, one `Clip.2` per panel in board order
  with the panel's duration;
- each clip references `s<scene>_p<panel>.png` (the camera's view at the panel's first frame, at the
  project resolution) by absolute `file://` URL;
- one `Marker.2` per scene on the timeline stack (`"<number> <heading>"`, comment = scene id);
- the panel's dialogue, action, notes, scene and camera keys in the clip's `metadata.boardcraft`.

FilmCraft imports OTIO (`filmcraft-interchange`). The export was checked against that importer:
the demo board loads as one 24 fps sequence with eight clips at the right times, three scene markers,
eight still media items and an empty import report. To repeat the check, point a scratch crate at a
FilmCraft checkout's `crates/interchange` and call
`filmcraft_interchange::import(bytes, Format::Otio, None)` on the exported file.

Not carried yet: camera moves (they play in BoardCraft's animatic; FilmCraft gets the start frame).

## Out: panel images

File ▸ Export Panel Images writes the same stills to a folder of your choice.
