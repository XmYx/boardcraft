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
//! Sound clips go on audio tracks (overlapping clips on further tracks), each referencing its
//! source file, written unchanged beside the stills as `sound_<clip id>.<ext>`, with its trim and
//! gain; OTIO keeps the clip id and panel anchor under `boardcraft`. The EDL lists them as `A`/`AA`
//! events.
//!
//! Conform matches clips to panels by the panel id in the clip's `boardcraft` metadata, then by the
//! clip name `Panel 1.2`, then by the still's file name, and sets each matched panel's duration to
//! the clip's, rounded to whole frames at the board's rate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use craft_timeline::build::{Builder, MediaSpec, settings_for};
use craft_timeline::fx::Standard;
use craft_timeline::model::{Label, Marker, MarkerId, MarkerKind, MediaKind, TrackKind};
use craft_timeline::{FrameRate, Report, Tick};
use serde::Serialize;
use serde_json::json;

use crate::audio::AudioClip;
use crate::error::{Error, Result};
use crate::model::{Id, Project};
use crate::stills::{still_name, write_stills};

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

/// File name of a sound clip's source in an export's media folder: `sound_<clip id>.<ext>`.
pub fn audio_file_name(clip: &AudioClip) -> String {
    format!("sound_{}.{}", clip.id.0, clip.ext)
}

/// Write every sound clip's source into `dir` (created if needed). Returns the files written.
pub fn write_audio_files(project: &Project, dir: &Path) -> Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for c in project.audio.iter().filter(|c| !c.data.is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let path = dir.join(audio_file_name(c));
        craft_atomic::atomic_write(&path, &c.data).map_err(|e| Error::io(&path, e))?;
        written.push(path);
    }
    Ok(written)
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
            if let Some(track) = seq.video_tracks.first_mut() {
                track.items.push(clip);
            }
            t += dur;
        }
    }
    let fps = project.settings.fps_f64();
    for c in &project.audio {
        let len = c.length_frames(fps) as i64;
        if len <= 0 {
            continue;
        }
        let file = audio_file_name(c);
        let path = stills_dir.join(&file).to_string_lossy().into_owned();
        let source = len + c.trim_in as i64;
        let spec = MediaSpec {
            duration: Some(r.tick_of(source)),
            video: None,
            audio: Some((c.sample_rate, u32::from(c.channels.max(1)))),
            start_tc: None,
            kind: Some(MediaKind::AudioOnly),
        };
        let item = b.file_media(&path, &file, &path, &spec, None);
        let mut clip = b.clip(item, TrackKind::Audio, &c.name, r.tick_of(c.start as i64), r.tick_of(len), r.tick_of(c.trim_in as i64));
        clip.gain_db = f64::from(c.gain_db);
        clip.ext = Some(
            json!({
                "audio_id": c.id.0, "anchor_panel": c.anchor.map(|a| a.panel.0), "anchor_offset": c.anchor.map(|a| a.offset),
            })
            .to_string(),
        );
        b.place(&mut seq, TrackKind::Audio, 0, clip);
    }
    b.put_sequence(seq_id, seq);
    b.top.push(seq_id);
    (b.finish().project, seq_id)
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
    let fps = project.settings.fps_f64();
    for c in &project.audio {
        let len = c.length_frames(fps) as i64;
        if len <= 0 {
            continue;
        }
        let file = audio_file_name(c);
        let (trim, start) = (c.trim_in as i64, c.start as i64);
        events.push(OutEvent {
            lines: vec![OutLine {
                reel: sanitize_reel(&format!("SND{}", c.id.0), 8),
                chan: if c.channels >= 2 { "AA".into() } else { "A".into() },
                trans: "C".into(),
                dur: None,
                src: (trim, trim + len),
                rec: (start, start + len),
            }],
            m2: None,
            comments: vec![format!("FROM CLIP NAME: {}", c.name), format!("SOURCE FILE: {}", stills_dir.join(&file).display())],
            locators: Vec::new(),
        });
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
        TimelineFormat::Otio => craft_timeline::otio::export(&model, seq, &opts, &mut report),
        TimelineFormat::Fcp7Xml => craft_timeline::fcp7::export(&model, seq, &opts, &mut report),
        TimelineFormat::Fcpxml => craft_timeline::fcpxml::export(&model, seq, &opts, &mut report),
        TimelineFormat::Edl => return Ok((edl(project, stills_dir), Vec::new())),
    }
    .map_err(|e| Error::Format(format!("{}: {e}", format.label())))?;
    Ok((text, report.entries.iter().map(|e| e.message.clone()).collect()))
}

/// Export the animatic: `path` (format from its extension) plus stills and sound files in
/// `<stem>_stills/` beside it. Returns that folder.
pub fn export(project: &Project, path: &Path) -> Result<PathBuf> {
    let format = TimelineFormat::from_path(path).ok_or_else(|| Error::Unsupported(format!("{}: use .otio, .edl, .xml or .fcpxml", path.display())))?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&parent).map_err(|e| Error::io(&parent, e))?;
    let parent = parent.canonicalize().or_else(|_| std::path::absolute(&parent)).map_err(|e| Error::io(&parent, e))?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "animatic".into());
    let stills = parent.join(format!("{stem}_stills"));
    write_stills(project, &stills)?;
    write_audio_files(project, &stills)?;
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
            // Sound events are not panels.
            if !line.chans.is_empty() && !line.chans.contains(&craft_timeline::edl::Channel::V) {
                continue;
            }
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
    fn sound_goes_on_audio_tracks_in_every_format() {
        let mut p = board();
        let second = p.scenes[0].panels[1].id;
        let a = p.add_audio("Dialogue take 3", "wav", crate::audio::tone_wav(440.0, 1.0, 48_000, 2), 0).unwrap();
        p.set_audio_anchor(a, Some(second)).unwrap();
        p.add_audio("Rain", "wav", crate::audio::tone_wav(110.0, 0.5, 8_000, 1), 2).unwrap();
        let (model, seq) = to_model(&p, Path::new("/x"));
        let seq = model.sequence(seq).unwrap();
        assert_eq!(seq.audio_tracks.len(), 2, "overlapping clips go on two tracks");
        let first = &seq.audio_tracks[0].items[0];
        assert_eq!(first.name, "Dialogue take 3");
        assert_eq!(first.duration, rate(&p).tick_of(24));
        for f in TimelineFormat::ALL {
            let (text, notes) = document(&p, f, Path::new("/x")).unwrap();
            assert!(text.contains(&format!("sound_{}.wav", a.0)), "{f:?}");
            assert!(notes.is_empty(), "{f:?}: {notes:?}");
            // Conform ignores the sound and still matches every panel.
            let mut q = p.clone();
            let r = conform(&mut q, Path::new(&format!("cut.{}", f.extension())), text.as_bytes()).unwrap();
            assert_eq!((r.matched, r.unmatched_clips), (3, 0), "{f:?}: {r:?}");
        }
        let (edl, _) = document(&p, TimelineFormat::Edl, Path::new("/x")).unwrap();
        assert!(edl.contains(" AA "), "{edl}");
        // The export writes the sources next to the stills.
        let dir = std::env::temp_dir().join(format!("boardcraft-tl-audio-{}", std::process::id()));
        let folder = export(&p, &dir.join("cut.otio")).unwrap();
        assert!(folder.join(format!("sound_{}.wav", a.0)).exists());
        std::fs::remove_dir_all(&dir).ok();
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

    #[test]
    fn conform_rejects_garbage() {
        let mut p = board();
        assert!(conform(&mut p, Path::new("x.bin"), b"\x00\x01").is_err());
        assert!(conform(&mut p, Path::new("x.otio"), b"{not json").is_err());
        let r = conform(&mut p, Path::new("x.edl"), b"TITLE: X\n").unwrap();
        assert_eq!(r.matched, 0);
    }
}
