# Feature names

User-visible names in BoardCraft are our own (craft-contrib `standards/naming.md`). No other
company's product or feature names appear as labels.

| BoardCraft name | What it is |
|---|---|
| Board view | grid of panel thumbnails grouped by scene |
| Panel view | canvas for one panel with the drawing tools and camera |
| Timeline | strip of panels sized by duration, with the playhead |
| Animatic | playback of the panels for their durations through their camera moves |
| Camera tool, camera keys | pan/zoom keyframes of a panel |
| Brush, Eraser | raster drawing tools |
| Import Screenplay | board a screenplay, or update the board from it by scene id |
| Export Animatic Timeline ▸ *format* | timeline + stills for an editor (formats named by their specifications) |
| Conform Timing from Edit | take panel durations, and optionally the order, from an editor's cut |
| Conform to the Edit (dialog) | what a conform changes, with Put panels in the edit's order and Mark panels the edit leaves out |
| Export Board Sheet (PDF) | printable pages of panels with their text; options for paper, grid, fields, header text and ruled note lines |
| Keyboard Shortcuts | the shared craft-keymap dialog: change, clear or reset any shortcut |
| `boardcraft-cli mcp` | the MCP server for agents |
| Export Shot List (CSV) | one row per panel for schedules |
| Board Settings | frame size, aspect ratio, frame rate |
| Duplicate Panel | copy a panel after itself |
| Onion Skin | the previous panel shown faintly over the drawing |
| Edit in Image Editor | round trip a layer through the user's image editor |
| Recover unsaved work | crash recovery from autosave |
| Shot (field) | shot size/angle; presets use generic film terms (WS, MS, CU…) |
| Export Panel Images (PNG) | one image per panel |
| AI panel sketch | generate a sketch with the shared Craft AI tools (stub) |
| Key at start / Key at end | add a camera key on the panel's first / last frame |
| `boardcraft-cli` | the command-line tool |

## Name check

Checked 2026-11-17 against craft-contrib `standards/naming.md`: every label above is a generic
description (storyboard, panel, animatic, camera, brush, timeline are common nouns of the craft),
none is another company's product or feature name, and AI features are labelled "AI …" with no
vendor or model name. The reference app is named only in `docs/target-app-parity.md`. File formats
are named as formats, which naming.md rule 3 keeps accurate: Fountain, OpenTimelineIO, CMX 3600 EDL,
Final Cut Pro 7 XML, FCPXML, Final Draft (`.fdx`), PNG; these appear as format names in menus and
dialogs, never as the name of a BoardCraft feature.
