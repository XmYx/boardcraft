# The `.boardcraft` file

An open, documented project format: a ZIP archive (PKWARE APPNOTE) any tool can unpack.

| Entry | Contents |
|---|---|
| `mimetype` | `application/vnd.storytold.boardcraft+zip` (stored, first entry) |
| `project.json` | UTF-8 JSON envelope, below |
| `layers/<layer id>.png` | RGBA PNG of one layer at the canvas size; absent = transparent |
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
    "next_id": 40
  }
}
```

| Field | Meaning |
|---|---|
| `settings.width`, `height` | canvas size in pixels, 16–8192 |
| `settings.fps` | whole frames per second, 1–240 |
| `scene.number` | as written in the script (`"12A"`), or assigned in order |
| `scene.source_scene_id` | stable id of the scene in the screenplay, or `""` |
| `panel.duration` | frames, ≥ 1 |
| `layers` | bottom to top; pixels in `layers/<id>.png` |
| `camera.keys[].frame` | frame offset inside the panel |
| `camera.keys[].center` | frame centre, 0–1 of canvas width/height |
| `camera.keys[].zoom` | 1 = whole canvas, up to 16 |

## Reading rules

- Unknown fields are ignored; missing optional fields take defaults.
- `schema_version` above this build's is refused with a message naming both versions.
- Values are validated and repaired (`Project::repair`): duplicate ids renumbered, durations and
  opacities clamped, camera keys sorted, de-duplicated and clamped, PNGs of the wrong size resampled.
- Archive limits: 200,000 entries, 256 MB of JSON, 512 MB per PNG, images up to 8192 px a side.

## Schema history

| Version | Since | Change |
|---|---|---|
| 1 | 0.1.0 | first version |

To change the schema, bump `format::SCHEMA_VERSION`, add an upgrade step in `format::from_bytes`, a
fixture of the old shape and a test; never edit a shipped step.
