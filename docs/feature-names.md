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
| Import Screenplay (Fountain) | board a screenplay |
| Export Animatic Timeline (OTIO) | timeline + stills for an editor |
| Export Panel Images (PNG) | one image per panel |
| AI panel sketch | generate a sketch with the shared Craft AI tools (stub) |
| Key at start / Key at end | add a camera key on the panel's first / last frame |
| `boardcraft-cli` | the command-line tool |

## Name check

Checked 2026-11-17 against craft-contrib `standards/naming.md`: every label above is a generic
description (storyboard, panel, animatic, camera, brush, timeline are common nouns of the craft),
none is another company's product or feature name, and AI features are labelled "AI …" with no
vendor or model name. The reference app is named only in `docs/target-app-parity.md`. File formats
are named by their open specifications (Fountain, OpenTimelineIO, PNG).
