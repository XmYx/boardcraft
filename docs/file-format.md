# The `.boardcraft` file

An open, documented project format: a ZIP archive (PKWARE APPNOTE) any tool can unpack.

| Entry | Contents |
|---|---|
| `mimetype` | `application/vnd.storytold.boardcraft+zip` (stored, first entry) |
| `project.json` | UTF-8 JSON envelope, below |
| `layers/<layer id>.png` | RGBA PNG of one layer at the canvas size; absent = transparent |
| `audio/<clip id>.<ext>` | the source file of one sound clip, byte for byte (WAV and AIFF deflated, others stored) |
| `thumbnail.png` | first panel, 320 px wide, for file browsers; ignored when reading |

## `project.json`

```json
{
  "format": "boardcraft.project",
  "schema_version": 1,
  "generator": "BoardCraft 0.1.0",
  "project": {
    "name": "Paper Boats",
    "settings": { "width": 1280, "height": 720, "fps": 24 },
    "scenes": [
      {
        "id": 2, "number": "1", "heading": "EXT. HARBOUR WALL - DAWN", "source_scene_id": "demo-harbour",
        "panels": [
          {
            "id": 3, "duration": 92,
            "dialogue": "", "action": "Grey water. …", "notes": "",
            "layers": [ { "id": 30, "name": "Rough", "visible": true, "opacity": 1.0 },
                        { "id": 4, "name": "Ink", "visible": true, "opacity": 1.0 } ],
            "camera": { "keys": [ { "frame": 0, "center": [0.5, 0.5], "zoom": 1.0 },
                                  { "frame": 91, "center": [0.38, 0.55], "zoom": 1.6 } ] }
          }
        ]
      }
    ],
    "audio": [
      { "id": 41, "name": "Harbour ambience", "ext": "wav", "start": 0, "trim_in": 12, "gain_db": -6.0,
        "anchor": { "panel": 3, "offset": 0 }, "sample_rate": 48000, "channels": 2, "source_frames": 480000 }
    ],
    "next_id": 42
  }
}
```

| Field | Meaning |
|---|---|
| `settings.width`, `height` | canvas size in pixels, 16–8192 |
| `settings.fps` | whole frames per second, 1–240 |
| `settings.ntsc` | optional; `true` = `fps × 1000/1001` (23.976, 29.97, 59.94) |
| `panel.shot` | optional shot type/size text |
| `scene.number` | as written in the script (`"12A"`), or assigned in order |
| `scene.source_scene_id` | stable id of the scene in the screenplay, or `""` |
| `panel.duration` | frames, ≥ 1 |
| `layers` | bottom to top; pixels in `layers/<id>.png` |
| `camera.keys[].frame` | frame offset inside the panel |
| `camera.keys[].center` | frame centre, 0–1 of canvas width/height |
| `camera.keys[].zoom` | 1 = whole canvas, up to 16 |
| `audio` | optional (omitted when empty); the sound clips under the animatic, mixed |
| `audio[].ext` | the source's file extension (lower-case letters and digits): the entry `audio/<id>.<ext>` |
| `audio[].start` | first frame on the animatic |
| `audio[].trim_in` | frames of the source skipped at its start |
| `audio[].gain_db` | −60 to +24 |
| `audio[].anchor` | optional `{ panel, offset }`: the clip starts `offset` frames after that panel's first frame and moves with it when the board is retimed |
| `audio[].sample_rate`, `channels`, `source_frames` | the source as measured on import (for exports and drawing without decoding) |

## Reading rules

- Unknown fields are ignored; missing optional fields take defaults.
- `schema_version` above this build's is refused with a message naming both versions.
- Values are validated and repaired (`Project::repair`): duplicate ids renumbered, durations and
  opacities clamped, camera keys sorted, de-duplicated and clamped, PNGs of the wrong size resampled.
- A sound clip whose `audio/<id>.<ext>` entry is missing is dropped; an anchor to a panel that does
  not exist is removed (the clip keeps its `start`); at most 256 clips are read.
- Archive limits: 200,000 entries, 256 MB of JSON, 512 MB per PNG, 1 GB per sound file, images up
  to 8192 px a side.

## Schema history

| Version | Since | Change |
|---|---|---|
| 1 | 0.1.0 | first version; the optional `audio` list and `audio/` entries were added without a bump (older builds ignore them and drop the sound) |

To change the schema, bump `format::SCHEMA_VERSION`, add an upgrade step in `format::from_bytes`, a
fixture of the old shape and a test; never edit a shipped step.
