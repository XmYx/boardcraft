# Interop: screenplays in, timelines out

BoardCraft sits between **ScriptCraft** (screenplay) and **FilmCraft** (edit), and speaks the
industry's interchange formats on both sides:

| Direction | Formats |
|---|---|
| Screenplay in | Fountain (`.fountain`), Final Draft (`.fdx`), ScriptCraft scene list (`.scenes.json`) |
| Re-import | any of the above, merged into the board by stable scene id |
| Timeline out | OpenTimelineIO (`.otio`), CMX 3600 EDL (`.edl`), Final Cut Pro 7 XML (`.xml`), FCPXML 1.10 (`.fcpxml`), each with panel stills |
| Timeline in (conform) | the same four: panel timing back from an editor's cut |
| Images out | one PNG per panel |
| Annotations | SVG out and in, per panel ([file-format](file-format.md#annotations)) |

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

### Other screenplay formats

- **Final Draft `.fdx`** (XML, read with craft-xml): `Scene Heading` paragraphs (their `Number`
  attribute is the scene number), `Action`, `Shot` and `General` as action panels,
  `Character` + `Parenthetical` + `Dialogue` as dialogue panels, `Transition` as notes; the first
  title-page paragraph is the title. Text runs with styles are joined.
- **ScriptCraft scene list** (`scriptcraft-scenes` version 1, from `scriptcraft-cli scenes` or
  File ▸ Export Scene List): scenes with `number`, `id`, `heading` and `beats`; dialogue uses the
  cue as printed (`TOMAS (V.O.)`). Other formats or versions are refused.

## Re-import by stable scene id

Importing a screenplay into a board that already has work **updates** it (File ▸ Import Screenplay,
`boardcraft-cli import-script`, command `import_script`):

1. Each script scene is matched to a board scene by its stable id (`[[scene-id: …]]`, or the
   scene list's `id`; ScriptCraft writes them with *Script ▸ Add Stable Scene IDs*), else by an
   identical heading (first unused match, so repeated headings pair up in order).
2. A matched scene keeps its panels' drawings, layers, cameras and durations, and takes the
   script's heading, number and id; panel *k* takes the dialogue and action of beat *k*; new beats
   add panels; panels beyond the script's beats are kept (they hold drawings) and counted.
3. Scenes new to the script are added in script order; board scenes the script no longer has are
   kept after the others and listed in the report. Nothing is ever deleted.
4. Re-importing the same script changes nothing.

`--append` (or `"merge": false`) appends the scenes instead.

**Checked against ScriptCraft:** its sample screenplay imports as the same 7 scenes, with one panel
per beat that `scriptcraft-cli scenes` lists (transitions become notes), and re-importing that
scene list's JSON into the board matches all 7 scenes and changes no panel. The shape ScriptCraft writes
(title page, section, synopsis and `FADE IN:` before the first heading, scene-id notes, cue
extensions, shots) is covered by the hand-written test `fountain::tests::scriptcraft_handoff_shape`.

## Out: the animatic as an editorial timeline

File ▸ Export Animatic Timeline ▸ *format*, or `boardcraft-cli export board.boardcraft cut.<ext>`.
Every format is written by `craft-timeline` (craft-libs) from one neutral model, and comes with
`<name>_stills/`: `s<scene>_p<panel>.png` at the project resolution, referenced by absolute path.
A static panel's still is the camera's view (annotations included); a panel whose camera moves
exports its **whole canvas** and the move as Motion keyframes (below). The EDL has no Motion, so
its stills are always the camera's first frame.

- One video track, one clip per panel in board order, named `Panel <scene>.<panel>`, lasting the
  panel's duration at the project rate.
- One marker per scene at its first panel: `<number> <heading>`, comment = the scene's stable id
  (EDL: a `* LOC:` locator).
- OTIO keeps the panel id, scene, dialogue, action, notes and camera keys in each clip's
  `metadata.boardcraft`. The EDL names each still in `* FROM CLIP NAME:` and `* SOURCE FILE:`.

| Format | Extension | Read by |
|---|---|---|
| OpenTimelineIO | `.otio` | FilmCraft, and most editors through OTIO adapters |
| CMX 3600 EDL | `.edl` | every editor |
| Final Cut Pro 7 XML (`xmeml` 5) | `.xml` | FilmCraft, Premiere Pro, DaVinci Resolve, Avid (via import) |
| FCPXML 1.10 | `.fcpxml` | FilmCraft, Final Cut Pro, DaVinci Resolve |

**Checked against FilmCraft:** each of the four exports of the demo board, opened with FilmCraft's
own importers (`filmcraft-interchange`), gives one 24 fps sequence of eight clips with the board's
durations, three scene markers and all eight stills found; the EDL's only note is that EDLs carry no
frame rate (FilmCraft assumes 24).

### Camera moves as Motion keyframes

A moving panel's clip carries craft-timeline's standard `motion` effect (`fx::MOTION`) with
keyframes, which each format writes in its own terms: OTIO in the clip's effect metadata, FCP7 XML
as Basic Motion (`center`, `scale`, `rotation` with `<keyframe>`s), FCPXML as `adjust-transform`
with `keyframeAnimation`. The sequence is the canvas size and the still is the whole canvas, so
for the camera at frame *f* (centre *c*, zoom *z*, rotation *r*, after fitting a turned frame to
the canvas):

| Motion parameter | Value |
|---|---|
| `scale` | 100 × *z* percent |
| `rotation` | −*r* degrees (the picture turns against the camera); written only when a key turns |
| `position` | frame centre − *z* · R(−*r*) · (*c* − canvas centre), in sequence pixels; anchor stays the still's centre |

Keys are sampled at every frame of the panel (at most 240 a clip; longer panels are sampled
evenly, always including each camera key and the last frame) with linear interpolation, so the
editor reproduces BoardCraft's eased, geometric-zoom move exactly at every frame without needing
any curve type of its own. FilmCraft reads OTIO effects from its own metadata namespace, so a
moving clip carries its effect list under `metadata.filmcraft` as well as `metadata.boardcraft`.

**Checked against FilmCraft:** the demo board (a push-in, and a pan over three keys ending in a
6° turn) exported to OTIO, FCP7 XML and FCPXML and opened with FilmCraft's importers gives the two
moving clips 92 and 76 `position` and `scale` keyframes (plus 76 `rotation` keyframes, 0 → 6°)
with the expected first and last values; static clips keep the default Motion, and all eight
stills are found. The EDL imports as before. The transform convention (position, scale, rotation
about the still's centre) is FilmCraft's Motion; `timeline::tests::motion_reproduces_the_camera_frame`
checks that it maps the camera frame's corners onto the sequence frame's corners.

## In: conform timing from an edit

When the editor retimes the animatic, File ▸ Conform Timing from Edit (`boardcraft-cli conform`,
command `conform`) reads the edited timeline in any of the four formats and sets each panel's
duration to its clip's, rounded to whole frames at the board's rate. Clips are matched by the panel
id in OTIO metadata, then by the clip name `Panel 1.2`, then by the still's file name. The report
lists clips that are not from this board and panels the edit no longer uses; panel order and
deleted panels are reported, not applied.

## Out: panel images

File ▸ Export Panel Images writes the same stills to a folder of your choice.
