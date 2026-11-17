# `boardcraft-cli` and the command reference

Everything the app does to a board is a command (`boardcraft_core::command::Command`). The CLI runs
them headless, for scripts and agents.

```bash
boardcraft-cli new board.boardcraft --from script.fountain   # board a screenplay (.fountain, .fdx, .scenes.json)
boardcraft-cli new demo.boardcraft --demo                    # the demo board
boardcraft-cli info board.boardcraft                         # JSON: scenes, panels, layers, camera
boardcraft-cli exec board.boardcraft '{"cmd":"set_duration","panel":3,"frames":48}'
boardcraft-cli import-script board.boardcraft script.fdx     # update the board by scene id (--append to append)
boardcraft-cli export board.boardcraft cut.otio              # .otio, .edl, .xml (FCP7) or .fcpxml, + cut_stills/
boardcraft-cli conform board.boardcraft edited.xml           # panel timing from an editor's cut
boardcraft-cli export-png board.boardcraft stills/
boardcraft-cli export-sheet board.boardcraft board.pdf --paper letter --grid 3x2   # printable board sheet
boardcraft-cli export-shotlist board.boardcraft shots.csv    # shot list
boardcraft-cli export-movie board.boardcraft cut.mp4         # MP4 with burn-ins (--no-burn-in, --half)
boardcraft-cli exec board.boardcraft '{"cmd":"add_audio","path":"dialogue.wav","panel":3}'
boardcraft-cli commands                                      # command names
```

`exec` applies its commands in order and saves only if all succeed. Exit codes: 0 success, 1 a
failed operation (message on stderr), 2 a usage error.

## Commands

Ids come from `info`. Every command is JSON with a `"cmd"` tag.

| `cmd` | Fields | Effect |
|---|---|---|
| `add_scene` | `heading` | new scene with one blank panel; `created` = the panel |
| `add_panel` | `scene?`, `after?` | blank panel after `after`, or at the end of `scene` / the last scene |
| `delete_panel` | `panel` | refuses to delete the last panel |
| `duplicate_panel` | `panel` | copy after it; `created` = the copy |
| `set_canvas` | `width`, `height` | frame size without stretching (16–8192) |
| `set_frame_rate` | `fps`, `ntsc?`, `keep_time?` (default true) | 23.976 = `{"fps":24,"ntsc":true}` |
| `move_panel` | `panel`, `later` | one step earlier/later in board order, across scenes |
| `set_duration` | `panel`, `frames` | clamped to 1 … one hour at 240 fps |
| `set_text` | `panel`, `field` (`dialogue`/`action`/`notes`/`shot`), `text` | |
| `set_scene_heading` | `scene`, `heading` | |
| `set_camera_key` | `panel`, `key` {`frame`, `center` [x, y], `zoom`} | add or replace; values clamped |
| `remove_camera_key` | `panel`, `frame` | |
| `clear_camera` | `panel` | |
| `add_layer` | `panel`, `name` | `created` = the layer |
| `delete_layer` | `panel`, `layer` | refuses to delete the last layer |
| `set_layer_visible` | `panel`, `layer`, `visible` | |
| `set_layer_opacity` | `panel`, `layer`, `opacity` | clamped to 0–1 |
| `import_fountain` | `text` | append the screenplay's scenes |
| `import_script` | `text`, `name?`, `format?` (`fountain`/`final_draft`/`scene_list`), `merge?` (default true) | update the board by scene id ([interop](interop.md)), or append |
| `conform` | `name`, `text` | panel durations from an edited timeline (format from `name` and content) |
| `fill` | `panel`, `layer`, `x`, `y`, `color` [r, g, b, a], `tolerance?` (default 32), `all_layers?`, `gap?` | bucket fill at a canvas pixel; the area is found on the layer or on all visible layers |
| `transform_selection` | `panel`, `layer`, `polygon` [[x, y], …], `transform` {`dx`, `dy`, `scale_x`, `scale_y`, `angle`} | move, scale and rotate (degrees, about the selection's centre) the pixels inside the polygon |
| `clear_selection` | `panel`, `layer`, `polygon` | clear the pixels inside the polygon |
| `add_audio` | `path`, `name?`, `start?`, `panel?` | add a sound file (WAV, AIFF, MP3, AAC/M4A, FLAC, Ogg, Opus) at `start`, or at `panel`'s first frame anchored to it; `created` = the clip |
| `move_audio` | `clip`, `start` | start frame; an anchored clip keeps its panel and takes the new offset |
| `remove_audio` | `clip` | |
| `set_audio_anchor` | `clip`, `panel` (`null` to free) | anchored clips follow their panel when the board is retimed |
| `set_audio_gain` | `clip`, `gain_db` | clamped to −60 … +24 dB |
| `set_audio_trim` | `clip`, `trim_in` | frames skipped at the start of the source |

After every command, sound clips anchored to a panel move to that panel's start plus their offset;
a clip whose panel was deleted keeps its place and loses the anchor. `info` lists the clips under
`audio`.

Drawing strokes are not commands yet (pixels are edited directly, with pixel undo); see
[gaps.md](gaps.md).
