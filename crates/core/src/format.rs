//! The `.boardcraft` project file.
//!
//! A ZIP archive (written and read through `craft-zip`, saved through `craft-atomic`):
//!
//! | Entry | Contents |
//! |---|---|
//! | `mimetype` | `application/vnd.storytold.boardcraft+zip`, stored, first |
//! | `project.json` | the envelope below, UTF-8 JSON |
//! | `layers/<layer id>.png` | one RGBA PNG per layer that has paint; missing = transparent |
//! | `thumbnail.png` | the first panel at 320 px wide, for file browsers (not read back) |
//!
//! ```json
//! { "format": "boardcraft.project", "schema_version": 1, "generator": "BoardCraft 0.1.0",
//!   "project": { "name": "…", "settings": { "width": 1280, "height": 720, "fps": 24 },
//!                "scenes": [ { "id": 2, "number": "1", "heading": "INT. …", "source_scene_id": "",
//!                              "panels": [ { "id": 3, "duration": 48, "dialogue": "…", "action": "…",
//!                                            "notes": "", "layers": [ { "id": 4, "name": "Sketch",
//!                                            "visible": true, "opacity": 1.0 } ],
//!                                            "camera": { "keys": [ { "frame": 0, "center": [0.5, 0.5], "zoom": 1.0 } ] } } ] } ],
//!                "next_id": 5 } }
//! ```
//!
//! The full description is `docs/file-format.md`. Every number read from a file is validated:
//! the canvas size and rate must be in range, ids are de-duplicated, durations clamped, camera keys
//! sorted and clamped, and layer PNGs of the wrong size are resampled.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::model::Project;
use crate::raster::Raster;

/// The `format` tag in `project.json`.
pub const FORMAT_ID: &str = "boardcraft.project";
/// MIME type, also the first (`mimetype`) entry of the archive.
pub const MIME_TYPE: &str = "application/vnd.storytold.boardcraft+zip";
/// File extension.
pub const EXTENSION: &str = "boardcraft";
/// Schema this build writes and the newest it reads.
pub const SCHEMA_VERSION: u32 = 1;

const MAX_ENTRIES: u64 = 200_000;
const MAX_TOTAL_BYTES: u64 = 8 << 30;
const MAX_JSON_BYTES: u64 = 256 << 20;
const MAX_PNG_BYTES: u64 = 512 << 20;

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format: &'static str,
    schema_version: u32,
    generator: &'static str,
    project: &'a Project,
}

#[derive(Deserialize)]
struct EnvelopeIn {
    format: String,
    schema_version: u32,
    project: Value,
}

fn layer_entry(id: crate::model::Id) -> String {
    format!("layers/{}.png", id.0)
}

fn zip_err(e: craft_zip::Error) -> Error {
    Error::Format(e.to_string())
}

/// Serialize a project to `.boardcraft` bytes.
pub fn to_bytes(project: &Project) -> Result<Vec<u8>> {
    let mut w = craft_zip::Writer::new();
    w.add("mimetype", MIME_TYPE.as_bytes(), craft_zip::Compression::Stored).map_err(zip_err)?;
    let json = serde_json::to_vec_pretty(&EnvelopeOut { format: FORMAT_ID, schema_version: SCHEMA_VERSION, generator: crate::GENERATOR, project })
        .map_err(|e| Error::Format(e.to_string()))?;
    w.add("project.json", &json, craft_zip::Compression::Deflated).map_err(zip_err)?;
    for (_, panel) in project.panels() {
        for layer in &panel.layers {
            if layer.pixels.is_empty() || layer.pixels.is_blank() {
                continue;
            }
            // PNG is compressed already.
            w.add(&layer_entry(layer.id), &layer.pixels.to_png()?, craft_zip::Compression::Stored).map_err(zip_err)?;
        }
    }
    if let Some((_, first)) = project.panels().next() {
        let thumb = first.composite(&project.settings);
        let h = (320.0 * thumb.height() as f32 / thumb.width().max(1) as f32).round().max(1.0) as u32;
        w.add("thumbnail.png", &thumb.resized(320, h).to_png()?, craft_zip::Compression::Stored).map_err(zip_err)?;
    }
    w.finish().map_err(zip_err)
}

/// Read a project from `.boardcraft` bytes.
pub fn from_bytes(bytes: &[u8]) -> Result<Project> {
    let mut zip = craft_zip::Archive::open(bytes, craft_zip::Limits { max_entries: MAX_ENTRIES, max_total_bytes: MAX_TOTAL_BYTES }).map_err(zip_err)?;
    if zip.has_duplicate_names() {
        return Err(Error::Format("two archive entries share a name".into()));
    }
    let json = zip.read("project.json", MAX_JSON_BYTES).map_err(zip_err)?.ok_or_else(|| Error::Format("project.json is missing".into()))?;
    let env: EnvelopeIn = serde_json::from_slice(&json).map_err(|e| Error::Format(format!("project.json: {e}")))?;
    if env.format != FORMAT_ID {
        return Err(Error::Format(format!("format is {:?}, expected {FORMAT_ID:?}", env.format)));
    }
    if env.schema_version > SCHEMA_VERSION {
        return Err(Error::NewerSchema { found: env.schema_version, supported: SCHEMA_VERSION });
    }
    if env.schema_version == 0 {
        return Err(Error::Format("schema_version 0".into()));
    }
    // Migrations from older schemas go here (none yet: schema 1 is the first).
    let mut project: Project = serde_json::from_value(env.project).map_err(|e| Error::Format(format!("project: {e}")))?;
    if !project.settings.is_valid() {
        let s = project.settings;
        return Err(Error::Format(format!("canvas {}×{} at {} fps is out of range", s.width, s.height, s.fps)));
    }
    let ids: Vec<crate::model::Id> = project.panels().flat_map(|(_, p)| p.layers.iter().map(|l| l.id)).collect();
    for id in ids {
        let Some(png) = zip.read(&layer_entry(id), MAX_PNG_BYTES).map_err(zip_err)? else { continue };
        let raster = Raster::decode(&png).map_err(|e| Error::Format(format!("{}: {e}", layer_entry(id))))?;
        if let Some(layer) = project.scenes.iter_mut().flat_map(|s| s.panels.iter_mut()).flat_map(|p| p.layers.iter_mut()).find(|l| l.id == id) {
            layer.pixels = raster;
        }
    }
    project.repair();
    Ok(project)
}

/// Save atomically: a failed save leaves the previous file untouched.
pub fn save(project: &Project, path: &Path) -> Result<()> {
    let bytes = to_bytes(project)?;
    craft_atomic::atomic_write(path, &bytes).map_err(|e| Error::io(path, e))
}

/// Write any file atomically (exports): a failed write leaves the previous file untouched.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    craft_atomic::atomic_write(path, bytes).map_err(|e| Error::io(path, e))
}

/// Open a `.boardcraft` file.
pub fn load(path: &Path) -> Result<Project> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    from_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::CameraKey;
    use crate::raster::Brush;

    fn sample() -> Project {
        let mut p = Project::blank("Round trip");
        p.settings.width = 64;
        p.settings.height = 36;
        p.repair();
        let s = p.scenes[0].id;
        let id = p.add_panel(s, None).unwrap();
        let panel = p.panel_mut(id).unwrap();
        panel.dialogue = "BOB: Hi ünïcödé".into();
        panel.duration = 31;
        panel.camera.set_key(CameraKey { frame: 30, center: [0.6, 0.5], zoom: 2.0 });
        panel.layers[0].pixels.polyline(&[(4.0, 4.0), (60.0, 30.0)], &Brush::default());
        p
    }

    #[test]
    fn round_trip() {
        let p = sample();
        let back = from_bytes(&to_bytes(&p).unwrap()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn save_and_load_file() {
        let dir = std::env::temp_dir().join(format!("boardcraft-format-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.boardcraft");
        save(&sample(), &path).unwrap();
        assert_eq!(load(&path).unwrap(), sample());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_newer_and_foreign_files() {
        let mut w = craft_zip::Writer::new();
        w.add("project.json", br#"{"format":"boardcraft.project","schema_version":99,"project":{}}"#, craft_zip::Compression::Deflated).unwrap();
        assert!(matches!(from_bytes(&w.finish().unwrap()), Err(Error::NewerSchema { found: 99, .. })));
        let mut w = craft_zip::Writer::new();
        w.add("project.json", br#"{"format":"other","schema_version":1,"project":{}}"#, craft_zip::Compression::Deflated).unwrap();
        assert!(from_bytes(&w.finish().unwrap()).is_err());
        assert!(from_bytes(b"PK\x03\x04garbage").is_err());
        assert!(from_bytes(b"").is_err());
    }

    #[test]
    fn rejects_out_of_range_settings() {
        let mut p = sample();
        p.settings.width = 100_000;
        assert!(from_bytes(&to_bytes(&p).unwrap()).is_err());
    }

    #[test]
    fn mutated_files_never_panic() {
        let bytes = to_bytes(&sample()).unwrap();
        for cut in (0..bytes.len()).step_by(97) {
            let _ = std::panic::catch_unwind(|| from_bytes(&bytes[..cut])).expect("truncation panicked");
        }
        let mut seed = 0x9e37_79b9_u32;
        for _ in 0..300 {
            let mut m = bytes.clone();
            for _ in 0..4 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let i = seed as usize % m.len();
                m[i] ^= (seed >> 8) as u8 | 1;
            }
            let _ = std::panic::catch_unwind(|| from_bytes(&m)).expect("bit flip panicked");
        }
    }

    #[test]
    fn hostile_json_values_are_repaired() {
        let json = r#"{"format":"boardcraft.project","schema_version":1,"project":{"name":"x","settings":{"width":32,"height":32,"fps":24},
            "scenes":[{"id":1,"panels":[{"id":1,"duration":0,"layers":[{"id":1,"name":"a","opacity":-3}],
            "camera":{"keys":[{"frame":5,"center":[9,9],"zoom":1e30},{"frame":5,"center":[0,0],"zoom":0}]}}]}],"next_id":0}}"#;
        let mut w = craft_zip::Writer::new();
        w.add("project.json", json.as_bytes(), craft_zip::Compression::Deflated).unwrap();
        let p = from_bytes(&w.finish().unwrap()).unwrap();
        let panel = &p.scenes[0].panels[0];
        assert_eq!(panel.duration, 1);
        assert_eq!(panel.camera.keys.len(), 1);
        assert_eq!(panel.layers[0].opacity, 0.0);
        assert_ne!(p.scenes[0].id, panel.id);
        assert_eq!(panel.layers[0].pixels.width(), 32);
    }
}
