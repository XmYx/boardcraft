//! Every edit is a command.
//!
//! A [`Command`] is a serializable description of one change to a [`Project`]; [`apply`] performs
//! it and returns what changed. The app, `boardcraft-cli` (`boardcraft-cli exec`) and tests drive
//! the board through the same commands, so every feature is scriptable by agents. Commands are JSON
//! with a `"cmd"` tag, for example `{"cmd": "set_duration", "panel": 3, "frames": 48}`.

use serde::{Deserialize, Serialize};

use crate::annotate::{MAX_SHAPES, Shape};
use crate::camera::CameraKey;
use crate::error::{Error, Result};
use crate::fountain;
use crate::model::{Id, MAX_PANEL_FRAMES, Project};
use crate::script::{ScriptFormat, parse_script_as, reimport};
use crate::select::{Mask, Transform};

/// A panel text field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextField {
    Dialogue,
    Action,
    Notes,
    Shot,
}

/// One change to a board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Append a scene with one blank panel.
    AddScene {
        heading: String,
    },
    /// Add a blank panel after `after` (or at the end of `scene`).
    AddPanel {
        scene: Option<Id>,
        after: Option<Id>,
    },
    DeletePanel {
        panel: Id,
    },
    /// Copy a panel (drawing, text, camera, timing) right after it.
    DuplicatePanel {
        panel: Id,
    },
    /// Change the canvas size without stretching (layers centred, cropped or extended).
    SetCanvas {
        width: u32,
        height: u32,
    },
    /// Change the frame rate; `keep_time` rescales durations so panels last as long as before.
    SetFrameRate {
        fps: u32,
        #[serde(default)]
        ntsc: bool,
        #[serde(default = "yes")]
        keep_time: bool,
    },
    /// Move a panel one step earlier or later in board order.
    MovePanel {
        panel: Id,
        later: bool,
    },
    SetDuration {
        panel: Id,
        frames: u32,
    },
    SetText {
        panel: Id,
        field: TextField,
        text: String,
    },
    SetSceneHeading {
        scene: Id,
        heading: String,
    },
    /// Add or replace the camera key on `key.frame`.
    SetCameraKey {
        panel: Id,
        key: CameraKey,
    },
    RemoveCameraKey {
        panel: Id,
        frame: u32,
    },
    ClearCamera {
        panel: Id,
    },
    AddLayer {
        panel: Id,
        name: String,
    },
    DeleteLayer {
        panel: Id,
        layer: Id,
    },
    SetLayerVisible {
        panel: Id,
        layer: Id,
        visible: bool,
    },
    SetLayerOpacity {
        panel: Id,
        layer: Id,
        opacity: f32,
    },
    /// Append the scenes of a Fountain screenplay.
    ImportFountain {
        text: String,
    },
    /// Import a screenplay (Fountain, Final Draft `.fdx` or ScriptCraft scene list JSON; detected
    /// from `name` and the text unless `format` is given). `merge` (the default) updates the board
    /// by stable scene id (see `script::reimport`); otherwise the scenes are appended.
    ImportScript {
        #[serde(default)]
        name: String,
        text: String,
        #[serde(default)]
        format: Option<ScriptFormat>,
        #[serde(default = "yes")]
        merge: bool,
    },
    /// Take panel timing from an edited timeline (OTIO, EDL, FCP7 XML or FCPXML; detected from
    /// `name` and the text). `reorder` also puts panels in the edit's order and `mark_missing`
    /// flags panels the edit leaves out (see `timeline::ConformOptions`); both default to false.
    Conform {
        name: String,
        text: String,
        #[serde(default)]
        reorder: bool,
        #[serde(default)]
        mark_missing: bool,
    },
    /// Add a sound file (WAV, AIFF, MP3, AAC/M4A, FLAC, Ogg, Opus, …) to the audio track. It starts
    /// at `start`, or at `panel`'s first frame and anchored to it, or at frame 0. `name` defaults to
    /// the file's name.
    AddAudio {
        path: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        start: Option<u64>,
        #[serde(default)]
        panel: Option<Id>,
    },
    /// Move a sound clip to start at frame `start` (an anchored clip keeps its panel).
    MoveAudio {
        clip: Id,
        start: u64,
    },
    RemoveAudio {
        clip: Id,
    },
    /// Anchor a sound clip to a panel so it moves with it when the board is retimed (`null` frees it).
    SetAudioAnchor {
        clip: Id,
        panel: Option<Id>,
    },
    SetAudioGain {
        clip: Id,
        gain_db: f32,
    },
    /// Skip `trim_in` frames at the start of the clip's source.
    SetAudioTrim {
        clip: Id,
        trim_in: u64,
    },
    /// Bucket fill on a layer at canvas pixel `x, y` with `color` (straight RGBA). The area is
    /// found on the layer itself, or on all visible layers with `all_layers`; `tolerance` (0..=255)
    /// and `gap` (pixels, closes openings in the line art) as in `fill::FillOptions`.
    Fill {
        panel: Id,
        layer: Id,
        x: u32,
        y: u32,
        color: [u8; 4],
        #[serde(default = "default_tolerance")]
        tolerance: u8,
        #[serde(default)]
        all_layers: bool,
        #[serde(default)]
        gap: u32,
    },
    /// Move, scale and rotate the pixels inside `polygon` (canvas pixels; a lasso or rectangle)
    /// about their centre.
    TransformSelection {
        panel: Id,
        layer: Id,
        polygon: Vec<[f32; 2]>,
        transform: Transform,
    },
    /// Clear the pixels inside `polygon` on a layer.
    ClearSelection {
        panel: Id,
        layer: Id,
        polygon: Vec<[f32; 2]>,
    },
    /// Move the camera key on frame `from` to frame `to` (replacing a key there).
    MoveCameraKey {
        panel: Id,
        from: u32,
        to: u32,
    },
    /// Add a vector annotation (stroke, arrow, motion or camera arrow) on top of the panel.
    AddShape {
        panel: Id,
        shape: Shape,
    },
    /// Delete the annotation at `index` (0 = drawn first).
    DeleteShape {
        panel: Id,
        index: usize,
    },
    /// Move an annotation by `dx, dy` canvas pixels.
    MoveShape {
        panel: Id,
        index: usize,
        dx: f32,
        dy: f32,
    },
    /// Recolour an annotation (straight-alpha RGBA) and optionally change its line width.
    StyleShape {
        panel: Id,
        index: usize,
        color: [u8; 4],
        #[serde(default)]
        width: Option<f32>,
    },
    /// Replace the panel's annotations with the shapes of an SVG document (text), or add them.
    ImportShapesSvg {
        panel: Id,
        svg: String,
        #[serde(default)]
        replace: bool,
    },
}

fn default_tolerance() -> u8 {
    crate::fill::FillOptions::default().tolerance
}

fn yes() -> bool {
    true
}

/// What a command did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    /// An id the command created (scene, panel or layer), if any.
    pub created: Option<Id>,
    /// A short description for a status bar or log.
    pub message: String,
}

fn panel_missing(id: Id) -> Error {
    Error::NotFound(format!("panel {}", id.0))
}

fn layer_missing(id: Id) -> Error {
    Error::NotFound(format!("layer {}", id.0))
}

fn shape_missing(index: usize) -> Error {
    Error::NotFound(format!("annotation {index}"))
}

fn ok(message: impl Into<String>) -> Result<Outcome> {
    Ok(Outcome { created: None, message: message.into() })
}

fn clip_missing(id: Id) -> Error {
    Error::NotFound(format!("sound clip {}", id.0))
}

/// Apply `cmd` to `project`. On error the project is unchanged. Afterwards sound clips anchored to
/// panels follow their panels ([`Project::sync_audio`]).
pub fn apply(project: &mut Project, cmd: &Command) -> Result<Outcome> {
    let out = apply_inner(project, cmd)?;
    project.sync_audio();
    Ok(out)
}

fn apply_inner(project: &mut Project, cmd: &Command) -> Result<Outcome> {
    match cmd {
        Command::AddScene { heading } => {
            let s = project.add_scene(heading);
            let p = project.add_panel(s, None)?;
            Ok(Outcome { created: Some(p), message: format!("added scene {}", project.scene(s).map(|x| x.number.as_str()).unwrap_or("")) })
        }
        Command::AddPanel { scene, after } => {
            let scene = match (scene, after) {
                (Some(s), _) => *s,
                (None, Some(a)) => project.scene_of(*a).map(|s| s.id).ok_or_else(|| panel_missing(*a))?,
                (None, None) => match project.scenes.last() {
                    Some(s) => s.id,
                    None => project.add_scene(""),
                },
            };
            let id = project.add_panel(scene, *after)?;
            Ok(Outcome { created: Some(id), message: format!("added panel {}", project.panel_label(id)) })
        }
        Command::DeletePanel { panel } => {
            if project.panel(*panel).is_none() {
                return Err(panel_missing(*panel));
            }
            if project.panel_count() <= 1 {
                return Err(Error::Unsupported("a board keeps at least one panel".into()));
            }
            project.delete_panel(*panel);
            project.scenes.retain(|s| !s.panels.is_empty());
            ok("deleted panel")
        }
        Command::DuplicatePanel { panel } => {
            let id = project.duplicate_panel(*panel)?;
            Ok(Outcome { created: Some(id), message: format!("duplicated as {}", project.panel_label(id)) })
        }
        Command::SetCanvas { width, height } => {
            project.set_canvas(*width, *height)?;
            ok(format!("canvas {width}×{height}"))
        }
        Command::SetFrameRate { fps, ntsc, keep_time } => {
            project.set_rate(*fps, *ntsc, *keep_time)?;
            ok(format!("{} fps", project.settings.rate_label()))
        }
        Command::MovePanel { panel, later } => {
            if project.panel(*panel).is_none() {
                return Err(panel_missing(*panel));
            }
            let moved = project.move_panel(*panel, *later);
            project.scenes.retain(|s| !s.panels.is_empty());
            ok(if moved { "moved panel" } else { "panel is already at the end" })
        }
        Command::SetDuration { panel, frames } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            p.duration = (*frames).clamp(1, MAX_PANEL_FRAMES);
            ok(format!("duration {} frames", p.duration))
        }
        Command::SetText { panel, field, text } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            let target = match field {
                TextField::Dialogue => &mut p.dialogue,
                TextField::Action => &mut p.action,
                TextField::Notes => &mut p.notes,
                TextField::Shot => &mut p.shot,
            };
            target.clone_from(text);
            ok("text set")
        }
        Command::SetSceneHeading { scene, heading } => {
            let s = project.scene_mut(*scene).ok_or_else(|| Error::NotFound(format!("scene {}", scene.0)))?;
            s.heading.clone_from(heading);
            ok("heading set")
        }
        Command::SetCameraKey { panel, key } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            p.camera.set_key(*key);
            ok("camera key set")
        }
        Command::RemoveCameraKey { panel, frame } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            if p.camera.remove_key(*frame) { ok("camera key removed") } else { Err(Error::NotFound(format!("camera key on frame {frame}"))) }
        }
        Command::ClearCamera { panel } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            p.camera.keys.clear();
            ok("camera cleared")
        }
        Command::MoveCameraKey { panel, from, to } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            if p.camera.move_key(*from, *to) {
                ok(format!("camera key moved to frame {to}"))
            } else {
                Err(Error::NotFound(format!("camera key on frame {from}")))
            }
        }
        Command::AddShape { panel, shape } => {
            let shape = shape.sanitized().ok_or_else(|| Error::Format("the shape has no drawable points".into()))?;
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            if p.annotations.len() >= MAX_SHAPES {
                return Err(Error::Unsupported(format!("a panel keeps at most {MAX_SHAPES} annotations")));
            }
            let name = shape.kind_name();
            p.annotations.push(shape);
            p.touch();
            ok(format!("added {name}"))
        }
        Command::DeleteShape { panel, index } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            if *index >= p.annotations.len() {
                return Err(shape_missing(*index));
            }
            let s = p.annotations.remove(*index);
            p.touch();
            ok(format!("deleted {}", s.kind_name()))
        }
        Command::MoveShape { panel, index, dx, dy } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            p.annotations.get_mut(*index).ok_or_else(|| shape_missing(*index))?.translate(*dx, *dy);
            p.touch();
            ok("moved annotation")
        }
        Command::StyleShape { panel, index, color, width } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            let s = p.annotations.get_mut(*index).ok_or_else(|| shape_missing(*index))?;
            s.color = *color;
            if let Some(w) = width.filter(|w| w.is_finite()) {
                s.width = w.clamp(crate::annotate::MIN_WIDTH, crate::annotate::MAX_WIDTH);
            }
            p.touch();
            ok("restyled annotation")
        }
        Command::ImportShapesSvg { panel, svg, replace } => {
            let shapes = crate::annotate::from_svg(svg.as_bytes())?;
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            if *replace {
                p.annotations.clear();
            }
            let room = MAX_SHAPES.saturating_sub(p.annotations.len());
            let n = shapes.len().min(room);
            p.annotations.extend(shapes.into_iter().take(room));
            p.touch();
            ok(format!("imported {n} annotations"))
        }
        Command::AddLayer { panel, name } => {
            if project.panel(*panel).is_none() {
                return Err(panel_missing(*panel));
            }
            let layer = project.new_layer(name);
            let id = layer.id;
            if let Some(p) = project.panel_mut(*panel) {
                p.layers.push(layer);
                p.touch();
            }
            Ok(Outcome { created: Some(id), message: "added layer".into() })
        }
        Command::DeleteLayer { panel, layer } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            if p.layer(*layer).is_none() {
                return Err(Error::NotFound(format!("layer {}", layer.0)));
            }
            if p.layers.len() <= 1 {
                return Err(Error::Unsupported("a panel keeps at least one layer".into()));
            }
            p.layers.retain(|l| l.id != *layer);
            p.touch();
            ok("deleted layer")
        }
        Command::SetLayerVisible { panel, layer, visible } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            p.layer_mut(*layer).ok_or_else(|| Error::NotFound(format!("layer {}", layer.0)))?.visible = *visible;
            p.touch();
            ok("layer visibility set")
        }
        Command::SetLayerOpacity { panel, layer, opacity } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            let o = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 1.0 };
            p.layer_mut(*layer).ok_or_else(|| Error::NotFound(format!("layer {}", layer.0)))?.opacity = o;
            p.touch();
            ok("layer opacity set")
        }
        Command::ImportScript { name, text, format, merge } => {
            let format = format.unwrap_or_else(|| ScriptFormat::detect(name, text.as_bytes()));
            let script = parse_script_as(format, name, text.as_bytes())?;
            if *merge {
                let r = reimport(project, &script);
                ok(r.summary())
            } else {
                let (s, n) = fountain::append_to_project(project, &script);
                ok(format!("added {s} scenes, {n} panels"))
            }
        }
        Command::Conform { name, text, reorder, mark_missing } => {
            let options = crate::timeline::ConformOptions { reorder: *reorder, mark_missing: *mark_missing };
            let r = crate::timeline::conform_with(project, std::path::Path::new(name), text.as_bytes(), &options)?;
            ok(r.summary())
        }
        Command::AddAudio { path, name, start, panel } => {
            let file = std::path::Path::new(path);
            if let Some(p) = panel
                && project.panel(*p).is_none()
            {
                return Err(panel_missing(*p));
            }
            let len = std::fs::metadata(file).map_err(|e| Error::io(file, e))?.len();
            if len > crate::audio::MAX_AUDIO_BYTES {
                return Err(Error::Audio(format!("{path} is larger than {} MB", crate::audio::MAX_AUDIO_BYTES >> 20)));
            }
            let bytes = std::fs::read(file).map_err(|e| Error::io(file, e))?;
            let ext = file.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
            let name =
                if name.is_empty() { file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Sound".into()) } else { name.clone() };
            let at = match (start, panel) {
                (Some(s), _) => *s,
                (None, Some(p)) => project.panel_start(*p).ok_or_else(|| panel_missing(*p))?,
                (None, None) => 0,
            };
            let id = project.add_audio(&name, &ext, bytes, at)?;
            if panel.is_some() {
                project.set_audio_anchor(id, *panel)?;
            }
            Ok(Outcome { created: Some(id), message: format!("added sound {name}") })
        }
        Command::MoveAudio { clip, start } => {
            project.move_audio(*clip, *start)?;
            ok(format!("sound starts at frame {start}"))
        }
        Command::RemoveAudio { clip } => {
            let before = project.audio.len();
            project.audio.retain(|c| c.id != *clip);
            if project.audio.len() == before {
                return Err(clip_missing(*clip));
            }
            ok("removed sound")
        }
        Command::SetAudioAnchor { clip, panel } => {
            project.set_audio_anchor(*clip, *panel)?;
            ok(if panel.is_some() { "sound anchored to panel" } else { "sound unanchored" })
        }
        Command::SetAudioGain { clip, gain_db } => {
            let c = project.audio_clip_mut(*clip).ok_or_else(|| clip_missing(*clip))?;
            c.gain_db = *gain_db;
            c.sanitize();
            ok(format!("gain {:.1} dB", c.gain_db))
        }
        Command::SetAudioTrim { clip, trim_in } => {
            let c = project.audio_clip_mut(*clip).ok_or_else(|| clip_missing(*clip))?;
            c.trim_in = *trim_in;
            c.sanitize();
            ok(format!("trim {} frames", c.trim_in))
        }
        Command::ImportFountain { text } => {
            let script = fountain::parse(text);
            let (s, n) = fountain::append_to_project(project, &script);
            ok(format!("added {s} scenes, {n} panels"))
        }
        Command::Fill { panel, layer, x, y, color, tolerance, all_layers, gap } => {
            let settings = project.settings;
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            let sample = if *all_layers {
                let mut out = crate::raster::Raster::new(settings.width, settings.height);
                for l in p.layers.iter().filter(|l| l.visible && !l.pixels.is_empty()) {
                    out.composite_over(&l.pixels, l.opacity);
                }
                out
            } else {
                p.layer(*layer).ok_or_else(|| layer_missing(*layer))?.pixels.clone()
            };
            let l = p.layer_mut(*layer).ok_or_else(|| layer_missing(*layer))?;
            let opts = crate::fill::FillOptions { tolerance: *tolerance, gap: *gap };
            let n = crate::fill::flood_fill(&mut l.pixels, &sample, *x, *y, *color, opts);
            if n == 0 {
                return Err(Error::Unsupported(format!("nothing to fill at {x}, {y}")));
            }
            p.touch();
            ok(format!("filled {n} pixels"))
        }
        Command::TransformSelection { panel, layer, polygon, transform } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            let l = p.layer_mut(*layer).ok_or_else(|| layer_missing(*layer))?;
            let mut pixels = l.pixels.clone();
            if !crate::select::transform_region(&mut pixels, polygon, transform) {
                return Err(Error::Unsupported("the selection is empty".into()));
            }
            l.pixels = pixels;
            p.touch();
            ok("selection transformed")
        }
        Command::ClearSelection { panel, layer, polygon } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            let l = p.layer_mut(*layer).ok_or_else(|| layer_missing(*layer))?;
            let mask = Mask::polygon(l.pixels.width(), l.pixels.height(), polygon);
            let mut pixels = l.pixels.clone();
            if !crate::select::clear_masked(&mut pixels, &mask) {
                return Err(Error::Unsupported("the selection is empty".into()));
            }
            l.pixels = pixels;
            p.touch();
            ok("selection cleared")
        }
    }
}

/// One command's JSON shape, for tools that drive the board (the MCP server, docs).
#[derive(Clone, Debug, PartialEq)]
pub struct CommandSchema {
    /// The `"cmd"` tag.
    pub name: &'static str,
    pub description: &'static str,
    /// A JSON Schema object of the command's fields, without the `cmd` tag.
    pub fields: serde_json::Value,
}

/// The JSON shape of every [`Command`], in the order of the enum.
pub fn schemas() -> Vec<CommandSchema> {
    use serde_json::{Value, json};
    let id = |d: &str| json!({"type": "integer", "minimum": 0, "description": d});
    let int = |d: &str| json!({"type": "integer", "minimum": 0, "description": d});
    let string = |d: &str| json!({"type": "string", "description": d});
    let boolean = |d: &str| json!({"type": "boolean", "description": d});
    let number = |d: &str| json!({"type": "number", "description": d});
    let obj = |props: Value, required: &[&str]| json!({"type": "object", "properties": props, "required": required, "additionalProperties": false});
    let panel = || id("Panel id (from info)");
    let layer = || id("Layer id (from info)");
    let schema = |name, description, fields| CommandSchema { name, description, fields };
    vec![
        schema("add_scene", "Append a scene with one blank panel.", obj(json!({"heading": string("Scene heading, e.g. INT. KITCHEN - NIGHT")}), &["heading"])),
        schema(
            "add_panel",
            "Add a blank panel after `after`, or at the end of `scene`, or at the end of the board.",
            obj(json!({"scene": id("Scene id"), "after": id("Panel id to add after")}), &[]),
        ),
        schema("delete_panel", "Delete a panel (a board keeps at least one).", obj(json!({"panel": panel()}), &["panel"])),
        schema("duplicate_panel", "Copy a panel (drawing, text, camera, timing) right after it.", obj(json!({"panel": panel()}), &["panel"])),
        schema(
            "set_canvas",
            "Change the canvas size without stretching (16–8192 px).",
            obj(json!({"width": int("Width in pixels"), "height": int("Height in pixels")}), &["width", "height"]),
        ),
        schema(
            "set_frame_rate",
            "Change the frame rate; keep_time (default true) keeps panel lengths in seconds.",
            obj(
                json!({"fps": int("Whole frames per second"), "ntsc": boolean("×1000/1001, e.g. 23.976"), "keep_time": boolean("Rescale durations")}),
                &["fps"],
            ),
        ),
        schema(
            "move_panel",
            "Move a panel one step earlier or later in board order.",
            obj(json!({"panel": panel(), "later": boolean("true: later, false: earlier")}), &["panel", "later"]),
        ),
        schema(
            "set_duration",
            "Set a panel's duration in frames.",
            obj(json!({"panel": panel(), "frames": int("Duration in frames (1 or more)")}), &["panel", "frames"]),
        ),
        schema(
            "set_text",
            "Set a panel text field.",
            obj(
                json!({"panel": panel(), "field": {"type": "string", "enum": ["dialogue", "action", "notes", "shot"]}, "text": string("The text")}),
                &["panel", "field", "text"],
            ),
        ),
        schema("set_scene_heading", "Set a scene's heading.", obj(json!({"scene": id("Scene id"), "heading": string("Heading")}), &["scene", "heading"])),
        schema(
            "set_camera_key",
            "Add or replace the camera key on key.frame (center in 0–1 of the canvas, zoom ≥ 1, rotation in degrees).",
            obj(
                json!({"panel": panel(), "key": obj(json!({"frame": int("Frame within the panel"), "center": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "zoom": number("Zoom, 1 = full frame"), "rotation": number("Degrees clockwise (default 0)")}), &["frame", "center", "zoom"])}),
                &["panel", "key"],
            ),
        ),
        schema(
            "remove_camera_key",
            "Remove the camera key on a frame.",
            obj(json!({"panel": panel(), "frame": int("Frame within the panel")}), &["panel", "frame"]),
        ),
        schema("clear_camera", "Remove every camera key of a panel.", obj(json!({"panel": panel()}), &["panel"])),
        schema("add_layer", "Add a drawing layer on top.", obj(json!({"panel": panel(), "name": string("Layer name")}), &["panel", "name"])),
        schema("delete_layer", "Delete a layer (a panel keeps at least one).", obj(json!({"panel": panel(), "layer": layer()}), &["panel", "layer"])),
        schema(
            "set_layer_visible",
            "Show or hide a layer.",
            obj(json!({"panel": panel(), "layer": layer(), "visible": boolean("Visible")}), &["panel", "layer", "visible"]),
        ),
        schema(
            "set_layer_opacity",
            "Set a layer's opacity (0–1).",
            obj(json!({"panel": panel(), "layer": layer(), "opacity": number("Opacity 0–1")}), &["panel", "layer", "opacity"]),
        ),
        schema("import_fountain", "Append the scenes of a Fountain screenplay.", obj(json!({"text": string("Fountain text")}), &["text"])),
        schema(
            "import_script",
            "Import a screenplay (Fountain, Final Draft .fdx, ScriptCraft scene list JSON). merge (default true) updates the board by scene id, keeping drawings; false appends.",
            obj(
                json!({"name": string("File name, used to detect the format"), "text": string("The file's text"), "format": {"type": "string", "enum": ["fountain", "final_draft", "scene_list"]}, "merge": boolean("Update by scene id (default true)")}),
                &["text"],
            ),
        ),
        schema(
            "conform",
            "Take panel timing from an edited timeline (OTIO, EDL, FCP7 XML, FCPXML). reorder puts panels in the edit's order; mark_missing flags panels the edit leaves out.",
            obj(
                json!({"name": string("File name, used to detect the format"), "text": string("The timeline's text"), "reorder": boolean("Apply the edit's order (default false)"), "mark_missing": boolean("Flag panels not in the edit (default false)")}),
                &["name", "text"],
            ),
        ),
        schema(
            "add_audio",
            "Add a sound file to the audio track, at `start`, or at `panel`'s first frame anchored to it.",
            obj(
                json!({"path": string("Sound file path (WAV, AIFF, MP3, AAC/M4A, FLAC, Ogg, Opus)"), "name": string("Display name"), "start": int("Start frame"), "panel": id("Panel to anchor to")}),
                &["path"],
            ),
        ),
        schema(
            "move_audio",
            "Move a sound clip to start at a frame.",
            obj(json!({"clip": id("Sound clip id"), "start": int("Start frame")}), &["clip", "start"]),
        ),
        schema("remove_audio", "Remove a sound clip.", obj(json!({"clip": id("Sound clip id")}), &["clip"])),
        schema(
            "set_audio_anchor",
            "Anchor a sound clip to a panel (null frees it); anchored clips follow retiming.",
            obj(json!({"clip": id("Sound clip id"), "panel": id("Panel id, or null")}), &["clip"]),
        ),
        schema(
            "set_audio_gain",
            "Set a sound clip's gain in dB.",
            obj(json!({"clip": id("Sound clip id"), "gain_db": number("Gain in dB")}), &["clip", "gain_db"]),
        ),
        schema(
            "set_audio_trim",
            "Skip frames at the start of a sound clip's source.",
            obj(json!({"clip": id("Sound clip id"), "trim_in": int("Frames to skip")}), &["clip", "trim_in"]),
        ),
        schema(
            "fill",
            "Bucket fill on a layer at canvas pixel x, y.",
            obj(
                json!({"panel": panel(), "layer": layer(), "x": int("Canvas x"), "y": int("Canvas y"), "color": {"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 255}, "minItems": 4, "maxItems": 4, "description": "Straight RGBA"}, "tolerance": int("0–255"), "all_layers": boolean("Find the area on all visible layers"), "gap": int("Close gaps up to this many pixels")}),
                &["panel", "layer", "x", "y", "color"],
            ),
        ),
        schema(
            "transform_selection",
            "Move, scale and rotate the pixels inside a polygon (canvas pixels) about their centre.",
            obj(
                json!({"panel": panel(), "layer": layer(), "polygon": {"type": "array", "items": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "description": "Polygon corners [x, y]"}, "transform": {"type": "object", "description": "dx, dy (px), scale_x, scale_y, angle (degrees clockwise)"}}),
                &["panel", "layer", "polygon", "transform"],
            ),
        ),
        schema(
            "clear_selection",
            "Clear the pixels inside a polygon on a layer.",
            obj(
                json!({"panel": panel(), "layer": layer(), "polygon": {"type": "array", "items": {"type": "array", "items": {"type": "number"}}, "description": "Polygon corners [x, y]"}}),
                &["panel", "layer", "polygon"],
            ),
        ),
        schema(
            "move_camera_key",
            "Move the camera key on frame `from` to frame `to` (replacing a key there).",
            obj(json!({"panel": panel(), "from": int("Frame of the key"), "to": int("New frame")}), &["panel", "from", "to"]),
        ),
        schema(
            "add_shape",
            "Add a vector annotation on top of the panel. shape.type is stroke (points), arrow (from, to), motion_arrow (from, via, to) or camera_arrow (from, to); points in canvas pixels.",
            obj(
                json!({"panel": panel(), "shape": {"type": "object", "description": "{\"type\": \"arrow\", \"from\": [x, y], \"to\": [x, y], \"color\": [r, g, b, a], \"width\": px}", "properties": {"type": {"type": "string", "enum": ["stroke", "arrow", "motion_arrow", "camera_arrow"]}}, "required": ["type"]}}),
                &["panel", "shape"],
            ),
        ),
        schema(
            "delete_shape",
            "Delete the annotation at index (0 = drawn first).",
            obj(json!({"panel": panel(), "index": int("Annotation index")}), &["panel", "index"]),
        ),
        schema(
            "move_shape",
            "Move an annotation by dx, dy canvas pixels.",
            obj(
                json!({"panel": panel(), "index": int("Annotation index"), "dx": number("Pixels right"), "dy": number("Pixels down")}),
                &["panel", "index", "dx", "dy"],
            ),
        ),
        schema(
            "style_shape",
            "Recolour an annotation (straight RGBA) and optionally change its line width.",
            obj(
                json!({"panel": panel(), "index": int("Annotation index"), "color": {"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 255}, "minItems": 4, "maxItems": 4, "description": "Straight RGBA"}, "width": number("Line width in pixels")}),
                &["panel", "index", "color"],
            ),
        ),
        schema(
            "import_shapes_svg",
            "Add the shapes of an SVG document (text) as annotations, or replace the panel's annotations with them.",
            obj(json!({"panel": panel(), "svg": string("SVG text"), "replace": boolean("Replace existing annotations (default false)")}), &["panel", "svg"]),
        ),
    ]
}

/// Parse a command from JSON.
pub fn parse(json: &str) -> Result<Command> {
    serde_json::from_str(json).map_err(|e| Error::Format(format!("command: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_commands_drive_a_board() {
        let mut p = Project::blank("t");
        let first = p.scenes[0].panels[0].id;
        let out = apply(&mut p, &parse(&format!(r#"{{"cmd":"add_panel","after":{}}}"#, first.0)).unwrap()).unwrap();
        let second = out.created.unwrap();
        apply(&mut p, &parse(&format!(r#"{{"cmd":"set_duration","panel":{},"frames":12}}"#, second.0)).unwrap()).unwrap();
        apply(&mut p, &parse(&format!(r#"{{"cmd":"set_text","panel":{},"field":"dialogue","text":"BOB: Hi"}}"#, second.0)).unwrap()).unwrap();
        apply(&mut p, &Command::SetCameraKey { panel: second, key: CameraKey { frame: 11, center: [0.6, 0.5], zoom: 2.0, rotation: 0.0 } }).unwrap();
        apply(&mut p, &Command::MovePanel { panel: second, later: false }).unwrap();
        assert_eq!(p.scenes[0].panels[0].id, second);
        assert_eq!(p.panel(second).unwrap().duration, 12);
        assert_eq!(p.panel(second).unwrap().dialogue, "BOB: Hi");
        assert_eq!(p.panel(second).unwrap().camera.keys.len(), 1);
        apply(&mut p, &Command::ImportFountain { text: "INT. A\n\nRain.\n".into() }).unwrap();
        assert_eq!(p.scenes.len(), 2);
        // Merging the same script again matches the scene by heading instead of adding it.
        let cmd = parse(r#"{"cmd":"import_script","name":"a.fountain","text":"INT. A\n\nRain, harder.\n"}"#).unwrap();
        let out = apply(&mut p, &cmd).unwrap();
        assert_eq!(p.scenes.len(), 2, "{}", out.message);
        let a = p.scenes.iter().find(|s| s.heading == "INT. A").unwrap();
        assert_eq!(a.panels[0].action, "Rain, harder.");
        assert_eq!(p.scenes[0].heading, "INT. A", "script scenes first, scenes the script lacks after");
    }

    #[test]
    fn errors_leave_the_board_alone() {
        let mut p = Project::blank("t");
        let before = p.clone();
        let only = p.scenes[0].panels[0].id;
        assert!(apply(&mut p, &Command::DeletePanel { panel: only }).is_err());
        assert!(apply(&mut p, &Command::SetDuration { panel: Id(999), frames: 3 }).is_err());
        let layer = p.scenes[0].panels[0].layers[0].id;
        assert!(apply(&mut p, &Command::DeleteLayer { panel: only, layer }).is_err());
        assert!(parse(r#"{"cmd":"explode"}"#).is_err());
        assert!(parse("not json").is_err());
        assert_eq!(p, before);
    }

    #[test]
    fn duplicate_canvas_and_rate() {
        let mut p = crate::demo::demo_project();
        let first = p.scenes[0].panels[0].id;
        let n = p.panel_count();
        let copy = apply(&mut p, &Command::DuplicatePanel { panel: first }).unwrap().created.unwrap();
        assert_eq!(p.panel_count(), n + 1);
        assert_eq!(p.scenes[0].panels[1].id, copy);
        assert_ne!(p.panel(copy).unwrap().layers[0].id, p.panel(first).unwrap().layers[0].id);
        assert!(p.panel(copy).unwrap().layers[1].pixels.shares_pixels(&p.panel(first).unwrap().layers[1].pixels));
        let total_s = p.total_frames() as f64 / p.settings.fps_f64();
        apply(&mut p, &parse(r#"{"cmd":"set_frame_rate","fps":24,"ntsc":true}"#).unwrap()).unwrap();
        assert_eq!(p.settings.rate_label(), "23.976");
        let after_s = p.total_frames() as f64 / p.settings.fps_f64();
        assert!((after_s - total_s).abs() < 0.2, "{total_s} → {after_s}");
        apply(&mut p, &Command::SetCanvas { width: 1920, height: 816 }).unwrap();
        assert!(p.panels().all(|(_, x)| x.layers.iter().all(|l| l.pixels.width() == 1920 && l.pixels.height() == 816)));
        assert!(apply(&mut p, &Command::SetCanvas { width: 1, height: 816 }).is_err());
        assert!(apply(&mut p, &Command::SetFrameRate { fps: 0, ntsc: false, keep_time: true }).is_err());
        // A file with ntsc reads back.
        let back = crate::format::from_bytes(&crate::format::to_bytes(&p).unwrap()).unwrap();
        assert!(back.settings.ntsc);
    }

    /// One value of every command; the match makes a new variant fail to compile here until it has
    /// a sample (and so a schema).
    fn samples() -> Vec<Command> {
        let p = Id(1);
        let all = vec![
            Command::AddScene { heading: "H".into() },
            Command::AddPanel { scene: Some(p), after: Some(p) },
            Command::DeletePanel { panel: p },
            Command::DuplicatePanel { panel: p },
            Command::SetCanvas { width: 64, height: 36 },
            Command::SetFrameRate { fps: 24, ntsc: true, keep_time: false },
            Command::MovePanel { panel: p, later: true },
            Command::SetDuration { panel: p, frames: 3 },
            Command::SetText { panel: p, field: TextField::Shot, text: "CU".into() },
            Command::SetSceneHeading { scene: p, heading: "H".into() },
            Command::SetCameraKey { panel: p, key: CameraKey::default() },
            Command::RemoveCameraKey { panel: p, frame: 0 },
            Command::ClearCamera { panel: p },
            Command::AddLayer { panel: p, name: "L".into() },
            Command::DeleteLayer { panel: p, layer: p },
            Command::SetLayerVisible { panel: p, layer: p, visible: false },
            Command::SetLayerOpacity { panel: p, layer: p, opacity: 0.5 },
            Command::ImportFountain { text: "INT. A".into() },
            Command::ImportScript { name: "a.fdx".into(), text: "x".into(), format: Some(ScriptFormat::FinalDraft), merge: false },
            Command::Conform { name: "a.otio".into(), text: "{}".into(), reorder: true, mark_missing: true },
            Command::AddAudio { path: "a.wav".into(), name: "A".into(), start: Some(1), panel: Some(p) },
            Command::MoveAudio { clip: p, start: 2 },
            Command::RemoveAudio { clip: p },
            Command::SetAudioAnchor { clip: p, panel: Some(p) },
            Command::SetAudioGain { clip: p, gain_db: -3.0 },
            Command::SetAudioTrim { clip: p, trim_in: 4 },
            Command::Fill { panel: p, layer: p, x: 1, y: 1, color: [0, 0, 0, 255], tolerance: 32, all_layers: false, gap: 0 },
            Command::TransformSelection {
                panel: p,
                layer: p,
                polygon: vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0]],
                transform: crate::select::Transform::default(),
            },
            Command::ClearSelection { panel: p, layer: p, polygon: vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0]] },
            Command::MoveCameraKey { panel: p, from: 0, to: 4 },
            Command::AddShape { panel: p, shape: Shape::new(crate::annotate::ShapeKind::Arrow { from: [0.0, 0.0], to: [9.0, 9.0] }, [0, 0, 0, 255], 3.0) },
            Command::DeleteShape { panel: p, index: 0 },
            Command::MoveShape { panel: p, index: 0, dx: 1.0, dy: 2.0 },
            Command::StyleShape { panel: p, index: 0, color: [9, 9, 9, 255], width: Some(2.0) },
            Command::ImportShapesSvg { panel: p, svg: "<svg/>".into(), replace: true },
        ];
        for c in &all {
            match c {
                Command::AddScene { .. }
                | Command::AddPanel { .. }
                | Command::DeletePanel { .. }
                | Command::DuplicatePanel { .. }
                | Command::SetCanvas { .. }
                | Command::SetFrameRate { .. }
                | Command::MovePanel { .. }
                | Command::SetDuration { .. }
                | Command::SetText { .. }
                | Command::SetSceneHeading { .. }
                | Command::SetCameraKey { .. }
                | Command::RemoveCameraKey { .. }
                | Command::ClearCamera { .. }
                | Command::AddLayer { .. }
                | Command::DeleteLayer { .. }
                | Command::SetLayerVisible { .. }
                | Command::SetLayerOpacity { .. }
                | Command::ImportFountain { .. }
                | Command::ImportScript { .. }
                | Command::Conform { .. }
                | Command::AddAudio { .. }
                | Command::MoveAudio { .. }
                | Command::RemoveAudio { .. }
                | Command::SetAudioAnchor { .. }
                | Command::SetAudioGain { .. }
                | Command::SetAudioTrim { .. }
                | Command::Fill { .. }
                | Command::TransformSelection { .. }
                | Command::ClearSelection { .. }
                | Command::MoveCameraKey { .. }
                | Command::AddShape { .. }
                | Command::DeleteShape { .. }
                | Command::MoveShape { .. }
                | Command::StyleShape { .. }
                | Command::ImportShapesSvg { .. } => {}
            }
        }
        all
    }

    #[test]
    fn every_command_has_a_schema_that_fits_it() {
        let schemas = schemas();
        let samples = samples();
        assert_eq!(schemas.len(), samples.len());
        for (schema, sample) in schemas.iter().zip(&samples) {
            let mut v = serde_json::to_value(sample).unwrap();
            assert_eq!(v["cmd"], schema.name, "schemas follow the enum order");
            let obj = v.as_object_mut().unwrap();
            obj.remove("cmd");
            let props = schema.fields["properties"].as_object().unwrap();
            for k in obj.keys() {
                assert!(props.contains_key(k), "{}: `{k}` missing from the schema", schema.name);
            }
            for r in schema.fields["required"].as_array().unwrap() {
                assert!(obj.contains_key(r.as_str().unwrap()), "{}: required `{r}` is no field", schema.name);
            }
            // The required fields alone parse.
            let mut minimal = serde_json::Map::new();
            minimal.insert("cmd".into(), schema.name.into());
            for r in schema.fields["required"].as_array().unwrap() {
                let k = r.as_str().unwrap();
                minimal.insert(k.into(), obj[k].clone());
            }
            assert!(serde_json::from_value::<Command>(serde_json::Value::Object(minimal)).is_ok(), "{}", schema.name);
        }
    }

    #[test]
    fn annotation_and_camera_key_commands() {
        let mut p = Project::blank("t");
        let id = p.scenes[0].panels[0].id;
        let add =
            parse(&format!(r#"{{"cmd":"add_shape","panel":{},"shape":{{"type":"arrow","from":[10,10],"to":[100,40],"color":[0,0,0,255],"width":3}}}}"#, id.0))
                .unwrap();
        apply(&mut p, &add).unwrap();
        apply(&mut p, &Command::MoveShape { panel: id, index: 0, dx: 5.0, dy: 5.0 }).unwrap();
        apply(&mut p, &Command::StyleShape { panel: id, index: 0, color: [9, 9, 9, 255], width: Some(1e9) }).unwrap();
        let s = &p.panel(id).unwrap().annotations[0];
        assert_eq!(s.kind, crate::annotate::ShapeKind::Arrow { from: [15.0, 15.0], to: [105.0, 45.0] });
        assert_eq!((s.color, s.width), ([9, 9, 9, 255], crate::annotate::MAX_WIDTH));
        // Annotations show in the composite.
        assert_ne!(p.panel(id).unwrap().composite(&p.settings).pixel(60, 30), crate::model::PAPER);
        let svg = crate::annotate::to_svg(&p.panel(id).unwrap().annotations, 1280, 720);
        apply(&mut p, &Command::ImportShapesSvg { panel: id, svg, replace: false }).unwrap();
        assert_eq!(p.panel(id).unwrap().annotations.len(), 2);
        assert!(apply(&mut p, &Command::DeleteShape { panel: id, index: 5 }).is_err());
        apply(&mut p, &Command::DeleteShape { panel: id, index: 1 }).unwrap();
        let bad = Shape::new(crate::annotate::ShapeKind::Stroke { points: vec![[f32::NAN, 0.0]] }, [0; 4], 1.0);
        assert!(apply(&mut p, &Command::AddShape { panel: id, shape: bad }).is_err());
        // Camera keys move between frames.
        apply(&mut p, &Command::SetCameraKey { panel: id, key: CameraKey { frame: 4, zoom: 2.0, ..Default::default() } }).unwrap();
        apply(&mut p, &parse(&format!(r#"{{"cmd":"move_camera_key","panel":{},"from":4,"to":9}}"#, id.0)).unwrap()).unwrap();
        assert_eq!(p.panel(id).unwrap().camera.keys[0].frame, 9);
        assert!(apply(&mut p, &Command::MoveCameraKey { panel: id, from: 4, to: 1 }).is_err());
        // Annotations survive a save and follow a canvas resize.
        let back = crate::format::from_bytes(&crate::format::to_bytes(&p).unwrap()).unwrap();
        assert_eq!(back.panel(id).unwrap().annotations, p.panel(id).unwrap().annotations);
        apply(&mut p, &Command::SetCanvas { width: 1300, height: 720 }).unwrap();
        assert_eq!(p.panel(id).unwrap().annotations[0].kind, crate::annotate::ShapeKind::Arrow { from: [25.0, 15.0], to: [115.0, 45.0] });
    }

    #[test]
    fn hostile_values_are_clamped() {
        let mut p = Project::blank("t");
        let id = p.scenes[0].panels[0].id;
        let layer = p.scenes[0].panels[0].layers[0].id;
        apply(&mut p, &Command::SetDuration { panel: id, frames: u32::MAX }).unwrap();
        apply(&mut p, &Command::SetLayerOpacity { panel: id, layer, opacity: f32::NAN }).unwrap();
        assert_eq!(p.panel(id).unwrap().duration, MAX_PANEL_FRAMES);
        assert_eq!(p.panel(id).unwrap().layers[0].opacity, 1.0);
    }
    #[test]
    fn fill_and_selection_commands() {
        let mut p = Project::blank("t");
        let id = p.scenes[0].panels[0].id;
        let lines = p.scenes[0].panels[0].layers[0].id;
        let colour = apply(&mut p, &Command::AddLayer { panel: id, name: "colour".into() }).unwrap().created.unwrap();
        let b = crate::raster::Brush { radius: 1.5, hardness: 1.0, ..crate::raster::Brush::default() };
        p.panel_mut(id).unwrap().layer_mut(lines).unwrap().pixels.polyline(&[(10.0, 10.0), (60.0, 10.0), (60.0, 60.0), (10.0, 60.0), (10.0, 10.0)], &b);
        // Fill the colour layer inside the outline drawn on the line layer.
        let fill = parse(&format!(r#"{{"cmd":"fill","panel":{},"layer":{},"x":30,"y":30,"color":[200,0,0,255],"all_layers":true}}"#, id.0, colour.0)).unwrap();
        apply(&mut p, &fill).unwrap();
        let px = |p: &Project, l: Id, x: u32, y: u32| p.panel(id).unwrap().layer(l).unwrap().pixels.pixel(x, y);
        assert_eq!(px(&p, colour, 30, 30), [200, 0, 0, 255]);
        assert_eq!(px(&p, colour, 100, 100)[3], 0);
        // Move the filled square 100 px right.
        let poly = vec![[0.0, 0.0], [80.0, 0.0], [80.0, 80.0], [0.0, 80.0]];
        let t = Transform { dx: 100.0, ..Transform::default() };
        apply(&mut p, &Command::TransformSelection { panel: id, layer: colour, polygon: poly.clone(), transform: t }).unwrap();
        assert_eq!(px(&p, colour, 30, 30)[3], 0);
        assert_eq!(px(&p, colour, 130, 30), [200, 0, 0, 255]);
        // Clear it; an empty selection is an error and leaves the layer alone.
        let moved = vec![[100.0, 0.0], [180.0, 0.0], [180.0, 80.0], [100.0, 80.0]];
        apply(&mut p, &Command::ClearSelection { panel: id, layer: colour, polygon: moved }).unwrap();
        assert!(p.panel(id).unwrap().layer(colour).unwrap().pixels.is_blank());
        assert!(apply(&mut p, &Command::ClearSelection { panel: id, layer: colour, polygon: vec![[1.0, 1.0]] }).is_err());
        assert!(apply(&mut p, &Command::Fill { panel: id, layer: colour, x: u32::MAX, y: 0, color: [0; 4], tolerance: 0, all_layers: false, gap: 0 }).is_err());
    }
}
