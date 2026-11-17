//! The `.boardcraft` project file.
//!
//! A ZIP archive (written and read through `craft-zip`, saved through `craft-atomic`):
//!
//! | Entry | Contents |
//! |---|---|
//! | `mimetype` | `application/vnd.storytold.boardcraft+zip`, stored, first |
//! | `project.json` | the envelope below, UTF-8 JSON |
//! | `layers/<layer id>.png` | one RGBA PNG per layer that has paint; missing = transparent |
//! | `audio/<clip id>.<ext>` | the source file of each sound clip, unchanged |
//! | `thumbnail.png` | the first panel at 320 px wide, for file browsers (not read back) |
//!
//! ```json
//! { "format": "boardcraft.project", "schema_version": 2, "generator": "BoardCraft 0.1.0",
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
//! sorted and clamped, annotations cleaned, and layer PNGs of the wrong size are resampled.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Weak};

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
///
/// Schema 2 added camera key `rotation` and panel `annotations`. A schema-1 file reads as is (both
/// default to nothing); schema 1 builds refuse schema 2 files rather than drop those fields.
pub const SCHEMA_VERSION: u32 = 2;

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

/// Encoded layer PNGs kept between saves of the same board (incremental save).
///
/// Entries are keyed by layer id and remember the layer's pixel buffer through
/// [`Raster::buffer_key`]: pixels are copy-on-write, so a layer nobody painted since the last save
/// still has the same buffer and its PNG is reused instead of encoded again. The cache never keeps
/// pixels alive, only the encoded bytes (or the fact that the layer is blank).
#[derive(Default)]
pub struct EncodeCache {
    entries: HashMap<crate::model::Id, (Weak<Vec<u8>>, Option<Arc<Vec<u8>>>)>,
    /// Layers whose PNG was reused / encoded by the last save.
    pub reused: usize,
    pub encoded: usize,
}

impl EncodeCache {
    /// Number of layers remembered.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Bytes of encoded PNG held.
    pub fn bytes(&self) -> usize {
        self.entries.values().filter_map(|(_, png)| png.as_ref().map(|p| p.len())).sum()
    }

    /// The layer's PNG (`None` = blank layer, nothing to store), from the cache when its pixels are
    /// unchanged.
    fn png(&mut self, layer: &crate::model::Layer) -> Result<Option<Arc<Vec<u8>>>> {
        if let Some((key, png)) = self.entries.get(&layer.id)
            && layer.pixels.is_buffer(key)
        {
            self.reused += 1;
            return Ok(png.clone());
        }
        self.encoded += 1;
        let png = if layer.pixels.is_empty() || layer.pixels.is_blank() { None } else { Some(Arc::new(layer.pixels.to_png()?)) };
        self.entries.insert(layer.id, (layer.pixels.buffer_key(), png.clone()));
        Ok(png)
    }
}

/// Serialize a project to `.boardcraft` bytes.
pub fn to_bytes(project: &Project) -> Result<Vec<u8>> {
    to_bytes_cached(project, &mut EncodeCache::default())
}

/// [`to_bytes`], reusing the PNGs of layers unchanged since the last save through `cache`.
pub fn to_bytes_cached(project: &Project, cache: &mut EncodeCache) -> Result<Vec<u8>> {
    cache.reused = 0;
    cache.encoded = 0;
    let mut w = craft_zip::Writer::new();
    w.add("mimetype", MIME_TYPE.as_bytes(), craft_zip::Compression::Stored).map_err(zip_err)?;
    let json = serde_json::to_vec_pretty(&EnvelopeOut { format: FORMAT_ID, schema_version: SCHEMA_VERSION, generator: crate::GENERATOR, project })
        .map_err(|e| Error::Format(e.to_string()))?;
    w.add("project.json", &json, craft_zip::Compression::Deflated).map_err(zip_err)?;
    let mut live = std::collections::HashSet::new();
    for (_, panel) in project.panels() {
        for layer in &panel.layers {
            live.insert(layer.id);
            let Some(png) = cache.png(layer)? else { continue };
            // PNG is compressed already.
            w.add(&layer_entry(layer.id), &png, craft_zip::Compression::Stored).map_err(zip_err)?;
        }
    }
    for clip in &project.audio {
        if clip.data.is_empty() {
            continue;
        }
        // Uncompressed PCM deflates; coded audio does not.
        let how = if matches!(clip.ext.as_str(), "wav" | "wave" | "aif" | "aiff" | "aifc") {
            craft_zip::Compression::Deflated
        } else {
            craft_zip::Compression::Stored
        };
        w.add(&clip.entry_name(), &clip.data, how).map_err(zip_err)?;
    }
    cache.entries.retain(|id, _| live.contains(id));
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
    // Migrations from older schemas go here. 1 → 2 needs no step: schema 2 only added optional
    // fields (camera key `rotation`, panel `annotations`) that default to nothing.
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
    // Sound clips: the source from the archive; a clip whose file is missing is dropped.
    let mut audio = std::mem::take(&mut project.audio);
    for clip in &mut audio {
        clip.sanitize();
        if let Some(bytes) = zip.read(&clip.entry_name(), crate::audio::MAX_AUDIO_BYTES).map_err(zip_err)? {
            clip.data = std::sync::Arc::new(bytes);
        }
    }
    audio.retain(|c| !c.data.is_empty());
    project.audio = audio;
    project.repair();
    Ok(project)
}

/// Save atomically: a failed save leaves the previous file untouched.
pub fn save(project: &Project, path: &Path) -> Result<()> {
    save_cached(project, path, &mut EncodeCache::default())
}

/// [`save`], reusing unchanged layers' PNGs through `cache` (autosave, repeated saves).
pub fn save_cached(project: &Project, path: &Path, cache: &mut EncodeCache) -> Result<()> {
    let bytes = to_bytes_cached(project, cache)?;
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
        panel.camera.set_key(CameraKey { frame: 30, center: [0.6, 0.5], zoom: 2.0, rotation: 12.5 });
        panel.annotations.push(crate::annotate::Shape::new(
            crate::annotate::ShapeKind::MotionArrow { from: [5.0, 30.0], via: [30.0, 2.0], to: [58.0, 28.0] },
            [200, 30, 30, 255],
            2.0,
        ));
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
    fn sound_round_trips() {
        let mut p = sample();
        let first = p.scenes[0].panels[0].id;
        let wav = crate::audio::tone_wav(440.0, 0.2, 8_000, 1);
        let id = p.add_audio("Line", "wav", wav.clone(), 5).unwrap();
        p.set_audio_anchor(id, Some(first)).unwrap();
        let bytes = to_bytes(&p).unwrap();
        let back = from_bytes(&bytes).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.audio[0].data.as_slice(), wav.as_slice());
        // A clip whose file is missing from the archive is dropped, not an error.
        let mut no_file = p.clone();
        no_file.audio[0].data = Default::default();
        assert!(from_bytes(&to_bytes(&no_file).unwrap()).unwrap().audio.is_empty());
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
    fn schema_1_files_read_as_schema_2() {
        // A schema-1 envelope as 0.1.0 wrote it: camera keys without rotation, no annotations.
        let json = r#"{"format":"boardcraft.project","schema_version":1,"generator":"BoardCraft 0.1.0","project":{"name":"Old","settings":{"width":32,"height":18,"fps":24},
            "scenes":[{"id":1,"number":"1","heading":"INT. A","source_scene_id":"","panels":[{"id":2,"duration":24,"dialogue":"","action":"","notes":"",
            "layers":[{"id":3,"name":"Sketch","visible":true,"opacity":1.0}],"camera":{"keys":[{"frame":0,"center":[0.5,0.5],"zoom":1.0},{"frame":23,"center":[0.6,0.5],"zoom":2.0}]}}]}],"next_id":4}}"#;
        let mut w = craft_zip::Writer::new();
        w.add("project.json", json.as_bytes(), craft_zip::Compression::Deflated).unwrap();
        let p = from_bytes(&w.finish().unwrap()).unwrap();
        let panel = &p.scenes[0].panels[0];
        assert_eq!(panel.camera.keys.len(), 2);
        assert!(panel.camera.keys.iter().all(|k| k.rotation == 0.0));
        assert!(panel.annotations.is_empty());
        // Saved again, it is schema 2.
        let again = to_bytes(&p).unwrap();
        let mut zip = craft_zip::Archive::open(&again, craft_zip::Limits { max_entries: 10, max_total_bytes: 1 << 20 }).unwrap();
        let text = String::from_utf8(zip.read("project.json", 1 << 20).unwrap().unwrap()).unwrap();
        assert!(text.contains("\"schema_version\": 2"), "{text}");
        assert!(!text.contains("annotations"), "empty annotations are not written");
    }

    #[test]
    fn incremental_save_reuses_unchanged_layers() {
        let mut p = sample();
        let mut cache = EncodeCache::default();
        let first = to_bytes_cached(&p, &mut cache).unwrap();
        assert_eq!(cache.reused, 0);
        let layers = cache.encoded;
        assert!(layers >= 2);
        // Nothing changed: every layer is reused and the file is the same.
        assert_eq!(to_bytes_cached(&p, &mut cache).unwrap(), first);
        assert_eq!((cache.reused, cache.encoded), (layers, 0));
        // A snapshot (as autosave takes) shares the buffers, so it reuses too.
        let snapshot = p.clone();
        to_bytes_cached(&snapshot, &mut cache).unwrap();
        assert_eq!(cache.encoded, 0);
        // Paint one layer: only it is encoded again, and the file reads back with the new paint.
        let id = p.scenes[0].panels[1].id;
        p.panel_mut(id).unwrap().layers[0].pixels.polyline(&[(1.0, 30.0), (60.0, 2.0)], &Brush::default());
        let bytes = to_bytes_cached(&p, &mut cache).unwrap();
        assert_eq!((cache.reused, cache.encoded), (layers - 1, 1));
        assert_eq!(from_bytes(&bytes).unwrap(), p);
        // Deleted layers leave the cache.
        p.scenes[0].panels.truncate(1);
        to_bytes_cached(&p, &mut cache).unwrap();
        assert_eq!(cache.len(), p.panels().map(|(_, x)| x.layers.len()).sum::<usize>());
    }

    /// A 300-panel board with two painted layers a panel: the second save of an unchanged board
    /// encodes nothing, an edit re-encodes one layer, and the cache holds only compressed bytes.
    #[test]
    fn large_board_saves_stay_bounded() {
        let mut p = Project::new("Large");
        p.settings.width = 160;
        p.settings.height = 90;
        let s = p.add_scene("INT. LONG");
        for i in 0..300 {
            let id = p.add_panel(s, None).unwrap();
            let layer = p.new_layer("Ink");
            let panel = p.panel_mut(id).unwrap();
            panel.layers.push(layer);
            let y = (i % 80) as f32 + 5.0;
            for l in &mut panel.layers {
                l.pixels.polyline(&[(5.0, y), (150.0, 90.0 - y)], &Brush::default());
            }
        }
        let mut cache = EncodeCache::default();
        let t = std::time::Instant::now();
        let full = to_bytes_cached(&p, &mut cache).unwrap();
        let first = t.elapsed();
        assert_eq!(cache.encoded, 600);
        let t = std::time::Instant::now();
        let again = to_bytes_cached(&p, &mut cache).unwrap();
        let second = t.elapsed();
        assert_eq!(cache.encoded, 0);
        assert_eq!(again.len(), full.len());
        // Reusing must be much cheaper than encoding (generous bound for slow CI machines).
        assert!(second < first, "incremental save {second:?} vs full {first:?}");
        let id = p.scenes[0].panels[150].id;
        p.panel_mut(id).unwrap().layers[1].pixels.polyline(&[(1.0, 1.0), (100.0, 80.0)], &Brush::default());
        to_bytes_cached(&p, &mut cache).unwrap();
        assert_eq!(cache.encoded, 1);
        // The cache keeps encoded bytes, never pixels: far below the raw layer memory.
        let raw = 600 * 160 * 90 * 4;
        assert!(cache.bytes() < raw / 4, "cache {} bytes vs {raw} raw", cache.bytes());
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
