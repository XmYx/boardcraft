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
| Movie out | MP4: H.264 video and AAC sound, optional burn-ins |

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
`<name>_stills/`: `s<scene>_p<panel>.png`, the camera's view at the panel's first frame at the
project resolution, referenced by absolute path.

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

Camera moves are not carried yet (the stills are each panel's first camera frame).

### Sound in the timeline exports

The board's sound clips travel with the cut. Each clip's source file is copied unchanged into the
same `<name>_stills/` folder as `sound_<clip id>.<ext>` and referenced by absolute path:

- OTIO, FCP7 XML and FCPXML: one audio track (more when clips overlap), one clip per sound clip
  at its start frame, with its trim as the source in-point and its gain; OTIO keeps the clip id
  and the panel it is anchored to (`anchor_panel`, `anchor_offset`) in `metadata.boardcraft`.
- EDL: one `A` (mono) or `AA` (stereo) event per clip, reel `SND<id>`, with `* FROM CLIP NAME:`
  and `* SOURCE FILE:`.
- Conform reads only the video events and tracks, so sound never counts as an unknown clip.

## In: conform timing from an edit
## In: conform from an edit

When the editor retimes the animatic, File ▸ Conform Timing from Edit (`boardcraft-cli conform`,
command `conform`) reads the edited timeline in any of the four formats and sets each panel's
duration to its clip's, rounded to whole frames at the board's rate. Clips are matched by the panel
id in OTIO metadata, then by the clip name `Panel 1.2`, then by the still's file name. The report
lists clips that are not from this board and panels the edit no longer uses.

By default conform only retimes. Two options take more from the edit; the app shows a summary of
what would change (worked out on a copy of the board) and applies nothing until Apply, as one undo
step:

- **Put panels in the edit's order** (`reorder`). A panel used more than once counts where it is
  first used. Scenes stay whole:
  1. A panel the edit cuts in between two panels of another scene (the used clips just before and
     after it both belong to that scene) moves into that scene; otherwise it stays in its scene.
  2. Inside each scene, the panels the edit uses follow the edit's order.
  3. A panel the edit does not use stays in its scene, right after the panel that came before it
     on the board (first, when it was first).
  4. Scenes follow the edit order of their first used panel; a scene the edit does not use keeps its
     place after the scene before it. A scene left without panels by rule 1 is removed (its panels
     live on in the other scene).
- **Mark panels the edit leaves out** (`mark_missing`). Nothing is deleted: each such panel gets the
  first notes line `[not in the edit]` (in the inspector, and on sheets that print notes). A later
  conform in which the edit uses the panel again removes the line; marking twice adds nothing.

Tests (`crates/core/src/timeline.rs`) cut a three-scene board in a new order, with one panel
moved into another scene and one left out, in each of the four formats, and check the result.

## Out: movie (MP4)

File ▸ Export Movie, or `boardcraft-cli export-movie board.boardcraft cut.mp4 [--no-burn-in]
[--half]`. Every frame is rendered as playback shows it (camera moves applied) at the board size or
half of it, encoded to H.264 (craft-video) with the board's exact rate (23.976 included) and muxed
into MP4 by craft-media with the index first. The sound track is mixed at 48 kHz stereo and encoded
to AAC-LC (craft-audio). Burn-ins: the panel number top left, the timecode (`HH:MM:SS:FF`,
non-drop) top right, and up to two lines of the panel's dialogue at the bottom, drawn with the
board-sheet font. A sound file that cannot be decoded is left out and reported; the movie is still
written.

## Out: panel images

File ▸ Export Panel Images writes the same stills to a folder of your choice.
