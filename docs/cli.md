# `boardcraft-cli` and the command reference

Everything the app does to a board is a command (`boardcraft_core::command::Command`). The CLI runs
them headless, for scripts and agents.

```bash
boardcraft-cli new board.boardcraft --from script.fountain   # board a screenplay
boardcraft-cli new demo.boardcraft --demo                    # the demo board
boardcraft-cli info board.boardcraft                         # JSON: scenes, panels, layers, camera
boardcraft-cli exec board.boardcraft '{"cmd":"set_duration","panel":3,"frames":48}'
boardcraft-cli export-otio board.boardcraft cut.otio         # + cut_stills/
boardcraft-cli export-png board.boardcraft stills/
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
| `move_panel` | `panel`, `later` | one step earlier/later in board order, across scenes |
| `set_duration` | `panel`, `frames` | clamped to 1 … one hour at 240 fps |
| `set_text` | `panel`, `field` (`dialogue`/`action`/`notes`), `text` | |
| `set_scene_heading` | `scene`, `heading` | |
| `set_camera_key` | `panel`, `key` {`frame`, `center` [x, y], `zoom`} | add or replace; values clamped |
| `remove_camera_key` | `panel`, `frame` | |
| `clear_camera` | `panel` | |
| `add_layer` | `panel`, `name` | `created` = the layer |
| `delete_layer` | `panel`, `layer` | refuses to delete the last layer |
| `set_layer_visible` | `panel`, `layer`, `visible` | |
| `set_layer_opacity` | `panel`, `layer`, `opacity` | clamped to 0–1 |
| `import_fountain` | `text` | append the screenplay's scenes |

Drawing strokes are not commands yet (pixels are edited directly, with pixel undo); see
[gaps.md](gaps.md).
