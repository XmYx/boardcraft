# MCP server

`boardcraft-cli mcp <board.boardcraft>` is a [Model Context Protocol](https://modelcontextprotocol.io)
server over one board: newline-delimited JSON-RPC 2.0 on stdin and stdout (diagnostics go to
stderr). The protocol, lifecycle and the check of each call's arguments against the tool's schema
come from `craft-mcp-server` (craft-libs); BoardCraft only lists its tools and runs them
(`apps/boardcraft-cli/src/mcp.rs`).

A client configuration looks like this:

```json
{ "mcpServers": { "boardcraft": { "command": "boardcraft-cli", "args": ["mcp", "/path/to/board.boardcraft"] } } }
```

## Tools

| Tool | Arguments | Effect |
|---|---|---|
| `info` | none | scenes, panels (ids, labels, durations, text, shot, layers, camera keys) and settings as JSON |
| `render_panel` | `panel`, `frame?` | the panel's picture through its camera, as a PNG image |
| *every board command* | the command's fields ([cli.md](cli.md#commands)) | applies the command, e.g. `set_duration {"panel": 3, "frames": 48}` |
| `import_script_file` | `path`, `merge?` (default true) | a Fountain, Final Draft or scene-list file; updates scenes by id or appends |
| `conform_file` | `path`, `reorder?`, `mark_missing?`, `preview?` | timing (and with the options, order and flags) from an edited timeline; `preview` reports without changing anything |
| `export_timeline` | `path` | `.otio`, `.edl`, `.xml` (FCP7) or `.fcpxml`, with stills beside it |
| `export_sheet` | `path`, `paper?`, `columns?`, `rows?`, `fields?`, `header?`, `note_lines?` | board sheet PDF (options as `export-sheet`) |
| `export_shot_list` | `path` | shot list CSV |
| `export_png` | `dir` | one PNG per panel |

Every tool that changes the board saves it in place with an atomic write before it answers; when
the save fails the reply is an error that says the change is only in memory. Paths are resolved
against the server's working directory. The board is read once at start: do not edit the same file
in the app while the server runs.

The annotations tell clients what each tool may do: `info` and `render_panel` are read-only; the
others write files (the board or an export), so they are marked as such.

## Tests

`apps/boardcraft-cli/src/mcp.rs` drives the server in-process (initialize, list, call): commands by
id, schema errors before a tool runs, command errors as tool errors, every export, a conform preview
and apply, and a screenplay import, each checked against the board on disk.
