//! The animatic as an editorial timeline, through `craft-timeline` (craft-libs): export to
//! OpenTimelineIO, CMX 3600 EDL, Final Cut Pro 7 XML (`xmeml`, also read by Premiere Pro, Resolve
//! and Avid) and FCPXML; and **conform**: read an edited timeline back and take its panel timing.
//!
//! Every export writes the panel stills (see [`crate::stills::write_stills`]) next to the document
//! and references them by absolute path. One video track holds one clip per panel, named
//! `Panel <scene>.<panel>`; each scene starts with a marker (`<number> <heading>`, comment = the
//! scene's stable id). Formats that carry application data (OTIO metadata) keep the panel id, text
//! and camera keys under `boardcraft`.
//!
//! A panel whose camera moves exports its whole canvas as the still, and its clip gets the move as
//! Motion keyframes (craft-timeline's standard `motion` effect: position, scale and rotation,
//! sampled every frame up to [`MAX_MOTION_KEYS`] a clip, linear between samples), which OTIO, FCP7
//! XML and FCPXML carry and FilmCraft plays back. Static panels keep the camera's first frame as
//! their still and no Motion. The EDL, which has no Motion, always gets first-frame stills.
//!
//! Conform matches clips to panels by the panel id in the clip's `boardcraft` metadata, then by the
//! clip name `Panel 1.2`, then by the still's file name, and sets each matched panel's duration to
//! the clip's, rounded to whole frames at the board's rate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use craft_timeline::build::{Builder, MediaSpec, settings_for};
use craft_timeline::fx::Standard;
use craft_timeline::model::{Keyframe, Label, Marker, MarkerId, MarkerKind, MediaKind, Param, ParamValue, TrackItem, TrackKind, Vec2};
use craft_timeline::{FrameRate, Report, Tick};
use serde::Serialize;
use serde_json::json;

use crate::error::{Error, Result};
use crate::model::{Id, Panel, Project};
use crate::stills::{still_name, write_export_stills, write_stills};

/// Most Motion keyframes one parameter of one clip gets; longer moves are sampled more sparsely.
const MAX_MOTION_KEYS: u32 = 240;

/// The frames at which a panel's camera move is sampled: every frame (up to [`MAX_MOTION_KEYS`]),
/// always including the first and last frame and every key inside the panel.
fn motion_frames(panel: &Panel) -> Vec<u32> {
    let last = panel.duration.max(1) - 1;
    let step = last.div_ceil(MAX_MOTION_KEYS).max(1);
    let mut frames: Vec<u32> = (0..=last).step_by(step as usize).collect();
    frames.push(last);
    frames.extend(panel.camera.keys.iter().map(|k| k.frame).filter(|f| *f <= last));
    frames.sort_unstable();
    frames.dedup();
    frames
}

/// Motion (position in sequence pixels, scale in percent, rotation in degrees clockwise) that shows
/// the camera's view of the full-canvas still at `frame`, for a sequence the size of the canvas.
pub fn motion_at(project: &Project, panel: &Panel, frame: f32) -> (Vec2, f64, f64) {
    let (w, h) = (project.settings.width as f64, project.settings.height as f64);
    let k = panel.camera.at(frame).fitted((w / h.max(1.0)) as f32);
    let zoom = k.zoom as f64;
    // The picture turns against the camera.
    let rot = 0.0 - k.rotation as f64;
    let (s, c) = rot.to_radians().sin_cos();
    // The still's anchor is its centre; the camera's centre must land on the frame's centre.
    let (dx, dy) = ((k.center[0] as f64 - 0.5) * w, (k.center[1] as f64 - 0.5) * h);
    let pos = Vec2::new(w / 2.0 - zoom * (dx * c - dy * s), h / 2.0 - zoom * (dx * s + dy * c));
    (pos, zoom * 100.0, rot)
}

/// A parameter that holds `values` at `times`: animated only when the values differ.
fn param_of(times: &[Tick], values: Vec<ParamValue>) -> Option<Param> {
    let first = values.first()?.clone();
    if values.iter().all(|v| *v == first) {
        return Some(Param::new(first));
    }
    let mut p = Param::new(first);
    p.keyframes = times.iter().zip(values).map(|(t, v)| Keyframe::new(*t, v)).collect();
    Some(p)
}

/// Put a moving panel's camera on `clip` as Motion keyframes (media time = clip time: the still
/// starts at 0). Static panels keep the default Motion.
fn add_motion(project: &Project, panel: &Panel, clip: &mut TrackItem) {
    if !panel.camera.moves() {
        return;
    }
    let r = rate(project);
    let frames = motion_frames(panel);
    let times: Vec<Tick> = frames.iter().map(|f| r.tick_of(*f as i64)).collect();
    let samples: Vec<(Vec2, f64, f64)> = frames.iter().map(|f| motion_at(project, panel, *f as f32)).collect();
    let mut set = |id: &str, values: Vec<ParamValue>| {
        if let Some(p) = param_of(&times, values) {
            craft_timeline::build::set_param(&Standard, clip, craft_timeline::fx::MOTION, id, p);
        }
    };
    set("position", samples.iter().map(|s| ParamValue::Vec2(s.0)).collect());
    set("scale", samples.iter().map(|s| ParamValue::Float(s.1)).collect());
    if panel.camera.turns() {
        set("rotation", samples.iter().map(|s| ParamValue::Float(s.2)).collect());
    }
}

/// Namespace of BoardCraft's own data in timeline documents.
pub const NAMESPACE: &str = "boardcraft";

/// An editorial timeline format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TimelineFormat {
    /// OpenTimelineIO JSON (`.otio`).
    Otio,
    /// CMX 3600 edit decision list (`.edl`).
    Edl,
    /// Final Cut Pro 7 XML interchange (`.xml`).
    Fcp7Xml,
    /// FCPXML 1.10 (`.fcpxml`).
    Fcpxml,
}

impl TimelineFormat {
    pub const ALL: [TimelineFormat; 4] = [TimelineFormat::Otio, TimelineFormat::Edl, TimelineFormat::Fcp7Xml, TimelineFormat::Fcpxml];

    pub fn extension(self) -> &'static str {
        match self {
            TimelineFormat::Otio => "otio",
            TimelineFormat::Edl => "edl",
            TimelineFormat::Fcp7Xml => "xml",
            TimelineFormat::Fcpxml => "fcpxml",
        }
    }

    /// Name for menus and messages (format names, not product features).
    pub fn label(self) -> &'static str {
        match self {
            TimelineFormat::Otio => "OpenTimelineIO",
            TimelineFormat::Edl => "CMX 3600 EDL",
            TimelineFormat::Fcp7Xml => "Final Cut Pro 7 XML",
            TimelineFormat::Fcpxml => "FCPXML",
        }
    }

    /// From a file name's extension.
    pub fn from_path(path: &Path) -> Option<TimelineFormat> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        TimelineFormat::ALL.into_iter().find(|f| f.extension() == ext)
    }

    /// From content (and extension as a hint).
    pub fn detect(path: &Path, bytes: &[u8]) -> Option<TimelineFormat> {
        let ext = path.extension().and_then(|e| e.to_str());
        match craft_timeline::detect_text(bytes, ext) {
            Some(craft_timeline::Format::Otio) => Some(TimelineFormat::Otio),
            Some(craft_timeline::Format::Edl) => Some(TimelineFormat::Edl),
            Some(craft_timeline::Format::Fcp7Xml) => Some(TimelineFormat::Fcp7Xml),
            Some(craft_timeline::Format::Fcpxml) => Some(TimelineFormat::Fcpxml),
            _ => TimelineFormat::from_path(path),
        }
    }
}

fn rate(project: &Project) -> FrameRate {
    let (n, d) = project.settings.rate();
    FrameRate::new(n as i64, d as i64)
}

/// The board as a `craft-timeline` project with one sequence (returned), stills in `stills_dir`.
pub fn to_model(project: &Project, stills_dir: &Path) -> (craft_timeline::model::Project, craft_timeline::model::ItemId) {
    let r = rate(project);
    let (w, h) = (project.settings.width, project.settings.height);
    let mut b = Builder::new(&project.name, &Standard);
    let settings = settings_for(r, w, h, false);
    let seq_id = b.reserve_sequence(&project.name, settings.clone(), None);
    let mut seq = craft_timeline::build::empty_sequence(settings);
    b.ensure_tracks(&mut seq, TrackKind::Video, 1);
    let mut t = 0i64;
    for (si, scene) in project.scenes.iter().enumerate() {
        if !scene.panels.is_empty() {
            let name = if scene.heading.is_empty() { format!("Scene {}", scene.number) } else { format!("{} {}", scene.number, scene.heading) };
            seq.markers.push(Marker {
                id: MarkerId(b.alloc()),
                start: r.tick_of(t),
                duration: Tick::ZERO,
                name: name.trim().to_string(),
                comment: scene.source_scene_id.clone(),
                kind: MarkerKind::default(),
                color: Label::Forest,
                url: String::new(),
                author: String::new(),
                ext: None,
            });
        }
        for (pi, panel) in scene.panels.iter().enumerate() {
            let dur = panel.duration.max(1) as i64;
            let file = still_name(si, pi);
            let path = stills_dir.join(&file).to_string_lossy().into_owned();
            let spec = MediaSpec { duration: Some(r.tick_of(dur)), video: Some((w, h, r)), audio: None, start_tc: None, kind: Some(MediaKind::Still) };
            let item = b.file_media(&path, &file, &path, &spec, None);
            let label = project.panel_label(panel.id);
            let mut clip = b.clip(item, TrackKind::Video, &format!("Panel {label}"), r.tick_of(t), r.tick_of(dur), Tick::ZERO);
            clip.ext = Some(
                json!({
                    "panel_id": panel.id.0, "panel": label, "scene": scene.number, "heading": scene.heading,
                    "scene_id": scene.source_scene_id, "dialogue": panel.dialogue, "action": panel.action,
                    "notes": panel.notes, "shot": panel.shot, "camera": panel.camera.keys,
                })
                .to_string(),
            );
            add_motion(project, panel, &mut clip);
            if let Some(track) = seq.video_tracks.first_mut() {
                track.items.push(clip);
            }
            t += dur;
        }
    }
    b.put_sequence(seq_id, seq);
    b.top.push(seq_id);
    (b.finish().project, seq_id)
}

/// Namespace FilmCraft reads clip effects from in OTIO metadata.
const FILMCRAFT_NAMESPACE: &str = "filmcraft";

/// OTIO has no schema for keyframed effects; each application keeps them in its own metadata
/// namespace. FilmCraft reads `metadata.filmcraft.effects`, so a clip with an animated Motion
/// carries the same effect list there as well as under `boardcraft`. Static clips are left alone.
/// Text that does not parse is returned unchanged.
fn mirror_motion_for_filmcraft(text: &str) -> String {
    fn animated(effects: &serde_json::Value) -> bool {
        effects.as_array().is_some_and(|list| {
            list.iter().any(|e| {
                e.get("params")
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|ps| ps.values().any(|p| p.get("keyframes").and_then(serde_json::Value::as_array).is_some_and(|k| !k.is_empty())))
            })
        })
    }
    fn walk(v: &mut serde_json::Value, depth: usize) {
        if depth > 64 {
            return;
        }
        match v {
            serde_json::Value::Object(o) => {
                let is_clip = o.get("OTIO_SCHEMA").and_then(serde_json::Value::as_str).is_some_and(|s| s.starts_with("Clip."));
                if is_clip
                    && let Some(meta) = o.get_mut("metadata").and_then(serde_json::Value::as_object_mut)
                    && let Some(effects) = meta.get(NAMESPACE).and_then(|m| m.get("effects")).filter(|e| animated(e)).cloned()
                    && !meta.contains_key(FILMCRAFT_NAMESPACE)
                {
                    meta.insert(FILMCRAFT_NAMESPACE.to_string(), json!({ "effects": effects }));
                }
                for x in o.values_mut() {
                    walk(x, depth + 1);
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|x| walk(x, depth + 1)),
            _ => {}
        }
    }
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(text) else { return text.to_string() };
    walk(&mut v, 0);
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| text.to_string())
}

fn export_options<'a>(name: &'a str) -> craft_timeline::ExportOptions<'a> {
    craft_timeline::ExportOptions {
        relative_to: None,
        name: Some(name),
        catalog: &Standard,
        namespace: NAMESPACE,
        xmeml_version: 5,
        fcpxml_version: craft_timeline::FcpxmlVersion::default(),
    }
}

fn edl(project: &Project, stills_dir: &Path) -> String {
    use craft_timeline::edl::{OutEvent, OutHeader, OutLine, sanitize_reel, write};
    let header = OutHeader { title: project.name.to_uppercase(), rate: rate(project), drop_frame: false, reel_len: 8 };
    let mut events = Vec::new();
    let mut t = 0i64;
    for (si, scene) in project.scenes.iter().enumerate() {
        for (pi, panel) in scene.panels.iter().enumerate() {
            let dur = panel.duration.max(1) as i64;
            let file = still_name(si, pi);
            let mut locators = Vec::new();
            if pi == 0 {
                let name = if scene.heading.is_empty() { format!("SCENE {}", scene.number) } else { format!("{} {}", scene.number, scene.heading) };
                locators.push((t, "GREEN".to_string(), name));
            }
            events.push(OutEvent {
                lines: vec![OutLine {
                    reel: sanitize_reel(file.trim_end_matches(".png"), 8),
                    chan: "V".into(),
                    trans: "C".into(),
                    dur: None,
                    src: (0, dur),
                    rec: (t, t + dur),
                }],
                m2: None,
                comments: vec![format!("FROM CLIP NAME: {file}"), format!("SOURCE FILE: {}", stills_dir.join(&file).display())],
                locators,
            });
            t += dur;
        }
    }
    write(&header, &events)
}

/// The document text of `project` in `format`, with stills expected in `stills_dir`. Returns the
/// text and what the format could not carry.
pub fn document(project: &Project, format: TimelineFormat, stills_dir: &Path) -> Result<(String, Vec<String>)> {
    if format == TimelineFormat::Edl {
        return Ok((edl(project, stills_dir), Vec::new()));
    }
    let (model, seq) = to_model(project, stills_dir);
    let mut report = Report::default();
    let opts = export_options(&project.name);
    let text = match format {
        TimelineFormat::Otio => craft_timeline::otio::export(&model, seq, &opts, &mut report).map(|t| mirror_motion_for_filmcraft(&t)),
        TimelineFormat::Fcp7Xml => craft_timeline::fcp7::export(&model, seq, &opts, &mut report),
        TimelineFormat::Fcpxml => craft_timeline::fcpxml::export(&model, seq, &opts, &mut report),
        TimelineFormat::Edl => return Ok((edl(project, stills_dir), Vec::new())),
    }
    .map_err(|e| Error::Format(format!("{}: {e}", format.label())))?;
    Ok((text, report.entries.iter().map(|e| e.message.clone()).collect()))
}

/// Export the animatic: `path` (format from its extension) plus stills in `<stem>_stills/` beside
/// it. Returns the stills folder.
pub fn export(project: &Project, path: &Path) -> Result<PathBuf> {
    let format = TimelineFormat::from_path(path).ok_or_else(|| Error::Unsupported(format!("{}: use .otio, .edl, .xml or .fcpxml", path.display())))?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&parent).map_err(|e| Error::io(&parent, e))?;
    let parent = parent.canonicalize().or_else(|_| std::path::absolute(&parent)).map_err(|e| Error::io(&parent, e))?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "animatic".into());
    let stills = parent.join(format!("{stem}_stills"));
    // An EDL carries no Motion: its stills are each panel's first camera frame. The other formats
    // get the whole canvas of moving panels and replay the move as keyframes.
    if format == TimelineFormat::Edl {
        write_stills(project, &stills)?;
    } else {
        write_export_stills(project, &stills)?;
    }
    let (text, _) = document(project, format, &stills)?;
    craft_atomic::atomic_write(path, text.as_bytes()).map_err(|e| Error::io(path, e))?;
    Ok(stills)
}

/// What a conform changed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ConformReport {
    pub format: Option<TimelineFormat>,
    /// Clips matched to panels.
    pub matched: usize,
    /// Panels whose duration changed.
    pub retimed: usize,
    /// Clips that are no panel of this board.
    pub unmatched_clips: usize,
    /// Panels the edit does not use (labels).
    pub unused_panels: Vec<String>,
}

impl ConformReport {
    pub fn summary(&self) -> String {
        format!(
            "{} clips matched, {} panels retimed, {} clips not from this board, {} panels not in the edit",
            self.matched,
            self.retimed,
            self.unmatched_clips,
            self.unused_panels.len()
        )
    }
}

/// One clip of an edit: its name, still file name, BoardCraft data and duration in seconds.
struct EditClip {
    name: String,
    file: String,
    ext: Option<String>,
    seconds: f64,
}

fn edit_clips(path: &Path, bytes: &[u8]) -> Result<(TimelineFormat, Vec<EditClip>)> {
    let format = TimelineFormat::detect(path, bytes).ok_or_else(|| Error::Unsupported(format!("{}: not a timeline BoardCraft reads", path.display())))?;
    let text = craft_timeline::decode_text(bytes);
    let mut report = Report::default();
    if format == TimelineFormat::Edl {
        let doc = craft_timeline::edl::parse(&text, &mut report);
        let r = craft_timeline::edl::guess_rate(&doc);
        let fps = r.as_f64().max(1.0);
        let mut clips = Vec::new();
        for ev in &doc.events {
            let Some(line) = ev.lines.first() else { continue };
            let rec_in = line.tcs[2].frames(r, line.tcs[2].df);
            let rec_out = line.tcs[3].frames(r, line.tcs[3].df);
            let file = ev.from_clip.clone().unwrap_or_default();
            clips.push(EditClip { name: file.clone(), file, ext: None, seconds: (rec_out - rec_in).max(0) as f64 / fps });
        }
        return Ok((format, clips));
    }
    let opts = craft_timeline::ImportOptions { base_dir: None, name: None, catalog: &Standard, namespace: NAMESPACE };
    let imported = match format {
        TimelineFormat::Otio => craft_timeline::otio::import(&text, &opts, &mut report),
        TimelineFormat::Fcp7Xml => craft_timeline::fcp7::import(&text, &opts, &mut report),
        TimelineFormat::Fcpxml => craft_timeline::fcpxml::import(&text, &opts, &mut report),
        TimelineFormat::Edl => return Err(Error::Unsupported("EDL".into())),
    }
    .map_err(|e| Error::Format(format!("{}: {e}", format.label())))?;
    let p = &imported.project;
    let seq = imported.sequences.first().and_then(|id| p.sequence(*id)).ok_or_else(|| Error::Format("the timeline has no sequence".into()))?;
    let mut items: Vec<_> = seq.video_tracks.iter().flat_map(|t| t.items.iter()).filter(|i| i.enabled).collect();
    items.sort_by_key(|i| i.start);
    let clips = items
        .into_iter()
        .map(|i| {
            let file = craft_timeline::build::item_path(p, i.item)
                .map(|path| path.rsplit(['/', '\\']).next().unwrap_or(path).to_string())
                .or_else(|| p.item(i.item).map(|it| it.name.clone()))
                .unwrap_or_default();
            EditClip { name: i.name.clone(), file, ext: i.ext.clone(), seconds: i.duration.seconds() }
        })
        .collect();
    Ok((format, clips))
}

/// Take panel timing from an edited timeline (any [`TimelineFormat`]).
pub fn conform(project: &mut Project, path: &Path, bytes: &[u8]) -> Result<ConformReport> {
    let (format, clips) = edit_clips(path, bytes)?;
    let fps = project.settings.fps_f64();
    // Lookups by the names this board's exports use.
    let mut by_label: HashMap<String, Id> = HashMap::new();
    let mut by_file: HashMap<String, Id> = HashMap::new();
    for (si, scene) in project.scenes.iter().enumerate() {
        for (pi, panel) in scene.panels.iter().enumerate() {
            by_label.insert(format!("Panel {}", project.panel_label(panel.id)), panel.id);
            by_file.insert(still_name(si, pi), panel.id);
        }
    }
    let mut report = ConformReport { format: Some(format), ..Default::default() };
    let mut used = std::collections::HashSet::new();
    for c in clips {
        let from_ext = c
            .ext
            .as_deref()
            .and_then(|e| serde_json::from_str::<serde_json::Value>(e).ok())
            .and_then(|v| v.get("panel_id").and_then(serde_json::Value::as_u64))
            .map(Id)
            .filter(|id| project.panel(*id).is_some());
        let Some(id) = from_ext.or_else(|| by_label.get(&c.name).copied()).or_else(|| by_file.get(&c.file).copied()) else {
            report.unmatched_clips += 1;
            continue;
        };
        report.matched += 1;
        used.insert(id);
        let frames = (c.seconds * fps).round().clamp(1.0, crate::model::MAX_PANEL_FRAMES as f64) as u32;
        if let Some(p) = project.panel_mut(id)
            && p.duration != frames
        {
            p.duration = frames;
            report.retimed += 1;
        }
    }
    report.unused_panels = project.panels().filter(|(_, p)| !used.contains(&p.id)).map(|(_, p)| project.panel_label(p.id)).collect();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fountain;
    use crate::model::Settings;

    fn board() -> Project {
        let script = fountain::parse("INT. A #1#\n\n[[scene-id: s1]]\n\nA door opens.\n\nBOB\nHello there.\n\nEXT. B\n\nRain.\n");
        fountain::project_from_script(&script, Settings { width: 64, height: 36, fps: 24, ntsc: false })
    }

    #[test]
    fn every_format_round_trips_through_conform() {
        let dir = Path::new("/tmp/boards/My Board_stills");
        for f in TimelineFormat::ALL {
            let p = board();
            let (text, notes) = document(&p, f, dir).unwrap();
            assert!(!text.is_empty(), "{f:?}");
            assert!(notes.is_empty(), "{f:?}: {notes:?}");
            // An editor trims the second panel to 10 frames.
            let mut edited = p.clone();
            let second = edited.scenes[0].panels[1].id;
            edited.panel_mut(second).unwrap().duration = 10;
            let (cut, _) = document(&edited, f, dir).unwrap();
            let mut q = p.clone();
            let name = format!("cut.{}", f.extension());
            let r = conform(&mut q, Path::new(&name), cut.as_bytes()).unwrap();
            assert_eq!(r.matched, 3, "{f:?}: {r:?}");
            assert_eq!(r.retimed, 1, "{f:?}: {r:?}");
            assert_eq!(q.panel(second).unwrap().duration, 10, "{f:?}");
            assert!(r.unused_panels.is_empty());
        }
    }

    #[test]
    fn otio_carries_board_data() {
        let (text, _) = document(&board(), TimelineFormat::Otio, Path::new("/x")).unwrap();
        assert!(text.contains("\"boardcraft\""));
        assert!(text.contains("BOB: Hello there."));
        assert!(text.contains("Timeline.1"));
    }

    #[test]
    fn edl_shape() {
        let (text, _) = document(&board(), TimelineFormat::Edl, Path::new("/x")).unwrap();
        assert!(text.starts_with("TITLE: "));
        assert!(text.contains("* FROM CLIP NAME: s001_p002.png"));
        assert!(text.contains("FCM: NON-DROP FRAME"));
    }

    /// The Motion a clip gets maps the camera frame's corners onto the sequence frame's corners.
    #[test]
    fn motion_reproduces_the_camera_frame() {
        let mut p = board();
        p.settings.width = 1280;
        p.settings.height = 720;
        let id = p.scenes[0].panels[0].id;
        let key = crate::camera::CameraKey { frame: 10, center: [0.62, 0.4], zoom: 2.2, rotation: 17.0 };
        p.panel_mut(id).unwrap().camera.set_key(key);
        let panel = p.panel(id).unwrap().clone();
        let (w, h) = (1280.0f64, 720.0f64);
        let (pos, scale, rot) = motion_at(&p, &panel, 10.0);
        let quad = panel.camera.quad(10.0, (w / h) as f32);
        let (s, c) = rot.to_radians().sin_cos();
        let z = scale / 100.0;
        let map = |q: [f32; 2]| {
            let (x, y) = (q[0] as f64 * w - w / 2.0, q[1] as f64 * h - h / 2.0);
            (pos.x + z * (x * c - y * s), pos.y + z * (x * s + y * c))
        };
        for (corner, want) in quad.iter().zip([(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]) {
            let got = map(*corner);
            assert!((got.0 - want.0).abs() < 0.5 && (got.1 - want.1).abs() < 0.5, "{corner:?} → {got:?}, want {want:?}");
        }
        // No camera: the identity.
        let still = p.scenes[0].panels[1].clone();
        let (pos, scale, rot) = motion_at(&p, &still, 0.0);
        assert_eq!((pos.x, pos.y, scale, rot), (640.0, 360.0, 100.0, 0.0));
    }

    #[test]
    fn moving_panels_export_motion_keyframes() {
        let mut p = board();
        let id = p.scenes[0].panels[0].id;
        let d = p.panel(id).unwrap().duration;
        let cam = &mut p.panel_mut(id).unwrap().camera;
        cam.set_key(crate::camera::CameraKey { frame: 0, ..Default::default() });
        cam.set_key(crate::camera::CameraKey { frame: d / 2, center: [0.3, 0.5], zoom: 2.0, rotation: 0.0 });
        cam.set_key(crate::camera::CameraKey { frame: d - 1, center: [0.6, 0.6], zoom: 2.0, rotation: 30.0 });
        let (model, seq) = to_model(&p, Path::new("/x"));
        let s = model.sequence(seq).unwrap();
        let clips = &s.video_tracks[0].items;
        let m = clips[0].effect("motion").unwrap();
        for id in ["position", "scale", "rotation"] {
            let param = m.param(id).unwrap();
            assert_eq!(param.keyframes.len(), d as usize, "{id}: one key a frame");
        }
        // A static panel keeps the default Motion.
        assert!(!clips[1].effect("motion").unwrap().is_animated());
        let (otio, _) = document(&p, TimelineFormat::Otio, Path::new("/x")).unwrap();
        assert!(otio.contains("\"keyframes\""));
        let v: serde_json::Value = serde_json::from_str(&otio).unwrap();
        let clip = &v["tracks"]["children"][0]["children"][0];
        assert!(clip["metadata"]["filmcraft"]["effects"].is_array(), "FilmCraft reads Motion from its namespace: {clip}");
        assert!(clip["metadata"]["boardcraft"]["panel_id"].is_u64());
        assert!(v["tracks"]["children"][0]["children"][1]["metadata"].get("filmcraft").is_none());
        let (xml, notes) = document(&p, TimelineFormat::Fcp7Xml, Path::new("/x")).unwrap();
        assert!(xml.contains("<keyframe>") && xml.contains("Basic Motion"), "{notes:?}");
        let (fcpxml, _) = document(&p, TimelineFormat::Fcpxml, Path::new("/x")).unwrap();
        assert!(fcpxml.contains("adjust-transform") && fcpxml.contains("keyframeAnimation"));
        // Long panels are sampled, not keyed every frame.
        p.panel_mut(id).unwrap().duration = 5000;
        assert!(motion_frames(p.panel(id).unwrap()).len() <= MAX_MOTION_KEYS as usize + 4);
    }

    #[test]
    fn conform_rejects_garbage() {
        let mut p = board();
        assert!(conform(&mut p, Path::new("x.bin"), b"\x00\x01").is_err());
        assert!(conform(&mut p, Path::new("x.otio"), b"{not json").is_err());
        let r = conform(&mut p, Path::new("x.edl"), b"TITLE: X\n").unwrap();
        assert_eq!(r.matched, 0);
    }
}
