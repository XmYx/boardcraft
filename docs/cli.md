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
boardcraft-cli conform board.boardcraft edited.xml --reorder --mark-missing --dry-run   # also order, flags; report only
boardcraft-cli export-png board.boardcraft stills/
boardcraft-cli export-sheet board.boardcraft board.pdf --paper letter --grid 3x2   # printable board sheet
boardcraft-cli export-sheet board.boardcraft board.pdf --fields dialogue,notes --header "Ep. 4, draft 2" --note-lines
boardcraft-cli export-shotlist board.boardcraft shots.csv    # shot list
boardcraft-cli mcp board.boardcraft                          # MCP server on stdio (docs/mcp.md)
boardcraft-cli commands                                      # command names
```

### `conform` options

- `--reorder`: put panels in the edit's clip order. The rule (inside scenes, a panel cut between
  two panels of another scene joins it, unused panels stay after their board predecessor, scenes
  follow their first used panel) is in [interop.md](interop.md#in-conform-from-an-edit).
- `--mark-missing`: give each panel the edit leaves out the first notes line `[not in the edit]`;
  nothing is deleted. A later conform that uses the panel again removes the line.
- `--dry-run`: print the report without saving the board.

Without options, conform only retimes (as before). The report is JSON: `matched`, `retimed`,
`unmatched_clips`, `unused_panels`, `moved`, `changed_scene`, `removed_scenes`, `marked`,
`unmarked`.

### `export-sheet` options

| Option | Default | Effect |
|---|---|---|
| `--paper a4\|letter` | `a4` | landscape paper |
| `--grid <cols>x<rows>` | `3x2` | panels a page, each 1–6 |
| `--fields <list>` | all | which of `shot,dialogue,action,notes` print (`none` for pictures only) |
| `--header <text>` | none | a line in the page header after the board name (200 characters at most) |
| `--note-lines` | off | ruled lines for handwritten notes in the space under each panel's text |

The app's File ▸ Export Board Sheet (PDF)… dialog has the same options.

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
| `conform` | `name`, `text`, `reorder?`, `mark_missing?` (both default false) | panel durations, and optionally order and flags for left-out panels, from an edited timeline (format from `name` and content) |

`boardcraft_core::command::schemas()` gives each command's fields as a JSON Schema; the MCP server
lists them as its tools ([mcp.md](mcp.md)) and a test checks they match the commands.

## MCP server

`boardcraft-cli mcp <board.boardcraft>` serves the board to an MCP client (an agent) over stdio:
board info, every command above as a tool, screenplay import, conform, and the exports. Tools that
change the board save it. See [mcp.md](mcp.md).

Drawing strokes are not commands yet (pixels are edited directly, with pixel undo); see
[gaps.md](gaps.md).
