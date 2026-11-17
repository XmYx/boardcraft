# Interop: ScriptCraft in, FilmCraft out

BoardCraft sits between **ScriptCraft** (screenplay) and **FilmCraft** (edit).

## In: screenplays (Fountain)

File ▸ Import Screenplay, `boardcraft script.fountain`, or `boardcraft-cli new board.boardcraft --from
script.fountain`. The contract is ScriptCraft's
[`docs/interop.md`](https://github.com/storytold/scriptcraft/blob/main/docs/interop.md) (Fountain
from fountain.io); BoardCraft reads it as follows:

| Fountain | BoardCraft |
|---|---|
| title page `Title:` | project name |
| scene heading (`INT`, `EXT`, `EST`, `INT./EXT`, `INT/EXT`, `I/E` + `.` or space, any case, or forced `.HEADING`) | a scene; `#12A#` at the end is the scene number |
| `[[scene-id: <id>]]` among the notes and synopses after the heading (the first one wins) | `scene.source_scene_id` |
| action paragraph (shots such as `CLOSE ON …` included) | a panel with that action text |
| character cue + (parenthetical) + dialogue | a panel with dialogue `CUE (paren): line` |
| transition (`CUT TO:`, `> FADE OUT`) | a note on the previous panel |
| centered text (`> THE END <`) | a panel with that action text |
| text before the first heading (`FADE IN:`), notes, boneyard, sections, synopses, page breaks | ignored |

Panel durations are estimated from the text (about 2.5 spoken words a second, 1.5–8 s). Importing
into a board that already has work appends the scenes.

ScriptCraft's *Script ▸ Add Stable Scene IDs* writes the `[[scene-id: <16 hex>]]` notes; boards keep
the id per scene so a later re-import can update scenes in place (re-import matching itself is a
gap, see [gaps.md](gaps.md)). ScriptCraft's optional JSON scene list (`scriptcraft-cli scenes`,
format `scriptcraft-scenes` v1) carries the same scenes and beats; BoardCraft does not read it yet,
since Fountain carries everything it uses.

**Checked against ScriptCraft:** its sample screenplay imports as the same 7 scenes, with one panel
per beat that `scriptcraft-cli scenes` lists (transitions become notes). The shape ScriptCraft writes
(title page, section, synopsis and `FADE IN:` before the first heading, scene-id notes, cue
extensions, shots) is covered by the hand-written test `fountain::tests::scriptcraft_handoff_shape`.

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
