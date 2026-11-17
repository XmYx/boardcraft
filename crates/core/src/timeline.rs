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
//! Conform matches clips to panels by the panel id in the clip's `boardcraft` metadata, then by the
//! clip name `Panel 1.2`, then by the still's file name, and sets each matched panel's duration to
//! the clip's, rounded to whole frames at the board's rate.
//!
//! Two options ([`ConformOptions`]) take more from the edit; both are off by default, so a plain
//! conform only retimes:
//!
//! - **Reorder** puts panels in the edit's clip order. A panel used more than once counts where it
//!   is first used. The rule keeps scenes whole:
//!   1. A panel the edit cuts in between two panels of another scene (its previous and next used
//!      clips both belong to that scene) moves into that scene. Otherwise it stays in its scene.
//!   2. Inside each scene, the panels the edit uses follow the edit's order.
//!   3. A panel the edit does not use stays in its scene, right after the panel that came before
//!      it on the board (or first, when it was first).
//!   4. Scenes follow the edit order of their first used panel; scenes the edit does not use keep
//!      their place after the scene that came before them on the board. A scene left without
//!      panels by rule 1 is removed.
//! - **Mark missing** flags each panel the edit does not use with a first notes line
//!   [`NOT_IN_EDIT`]; nothing is deleted. A later conform that uses the panel again removes the
//!   line.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use craft_timeline::build::{Builder, MediaSpec, settings_for};
use craft_timeline::fx::Standard;
use craft_timeline::model::{Label, Marker, MarkerId, MarkerKind, MediaKind, TrackKind};
use craft_timeline::{FrameRate, Report, Tick};
use serde::{Deserialize, Serialize};
use serde_json::json;

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

fn rate(project: &Project) -> FrameRate {
    let (n, d) = project.settings.rate();
    FrameRate::new(n as i64, d as i64)
}

/// Board order: (scene index, panel index) of every panel.
fn board_order(project: &Project) -> Vec<(usize, usize)> {
    project.scenes.iter().enumerate().flat_map(|(si, s)| (0..s.panels.len()).map(move |pi| (si, pi))).collect()
}

/// The board as a `craft-timeline` project with one sequence (returned), stills in `stills_dir`.
pub fn to_model(project: &Project, stills_dir: &Path) -> (craft_timeline::model::Project, craft_timeline::model::ItemId) {
    to_model_in(project, stills_dir, &board_order(project))
}

/// [`to_model`] with the panels cut in `order` (scene index, panel index), as an editor may
/// rearrange them; clip names and stills keep the board's numbering.
fn to_model_in(project: &Project, stills_dir: &Path, order: &[(usize, usize)]) -> (craft_timeline::model::Project, craft_timeline::model::ItemId) {
    let r = rate(project);
    let (w, h) = (project.settings.width, project.settings.height);
    let mut b = Builder::new(&project.name, &Standard);
    let settings = settings_for(r, w, h, false);
    let seq_id = b.reserve_sequence(&project.name, settings.clone(), None);
    let mut seq = craft_timeline::build::empty_sequence(settings);
    b.ensure_tracks(&mut seq, TrackKind::Video, 1);
    let mut t = 0i64;
    for &(si, pi) in order {
        let Some(scene) = project.scenes.get(si) else { continue };
        let Some(panel) = scene.panels.get(pi) else { continue };
        if pi == 0 {
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
        {
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

fn edl(project: &Project, stills_dir: &Path, order: &[(usize, usize)]) -> String {
    use craft_timeline::edl::{OutEvent, OutHeader, OutLine, sanitize_reel, write};
    let header = OutHeader { title: project.name.to_uppercase(), rate: rate(project), drop_frame: false, reel_len: 8 };
    let mut events = Vec::new();
    let mut t = 0i64;
    for &(si, pi) in order {
        let Some(scene) = project.scenes.get(si) else { continue };
        let Some(panel) = scene.panels.get(pi) else { continue };
        {
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
    document_in(project, format, stills_dir, &board_order(project))
}

fn document_in(project: &Project, format: TimelineFormat, stills_dir: &Path, order: &[(usize, usize)]) -> Result<(String, Vec<String>)> {
    if format == TimelineFormat::Edl {
        return Ok((edl(project, stills_dir, order), Vec::new()));
    }
    let (model, seq) = to_model_in(project, stills_dir, order);
    let mut report = Report::default();
    let opts = export_options(&project.name);
    let text = match format {
        TimelineFormat::Otio => craft_timeline::otio::export(&model, seq, &opts, &mut report),
        TimelineFormat::Fcp7Xml => craft_timeline::fcp7::export(&model, seq, &opts, &mut report),
        TimelineFormat::Fcpxml => craft_timeline::fcpxml::export(&model, seq, &opts, &mut report),
        TimelineFormat::Edl => return Ok((edl(project, stills_dir, order), Vec::new())),
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
    write_stills(project, &stills)?;
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
    /// Panels the edit does not use (labels, before any reorder).
    pub unused_panels: Vec<String>,
    /// Panels whose place on the board changed (with [`ConformOptions::reorder`]).
    pub moved: usize,
    /// Panels that moved into another scene (with [`ConformOptions::reorder`]).
    pub changed_scene: usize,
    /// Scenes removed because every panel moved to another scene.
    pub removed_scenes: usize,
    /// Panels newly flagged as not in the edit (with [`ConformOptions::mark_missing`]).
    pub marked: usize,
    /// Panels whose flag was removed because the edit uses them again.
    pub unmarked: usize,
}

impl ConformReport {
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{} clips matched, {} panels retimed, {} clips not from this board, {} panels not in the edit",
            self.matched,
            self.retimed,
            self.unmatched_clips,
            self.unused_panels.len()
        );
        if self.moved > 0 || self.changed_scene > 0 {
            s.push_str(&format!(", {} panels moved ({} to another scene)", self.moved, self.changed_scene));
        }
        if self.removed_scenes > 0 {
            s.push_str(&format!(", {} empty scenes removed", self.removed_scenes));
        }
        if self.marked > 0 || self.unmarked > 0 {
            s.push_str(&format!(", {} panels marked not in the edit, {} unmarked", self.marked, self.unmarked));
        }
        s
    }
}

/// What a conform takes from the edit besides timing. The default retimes only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConformOptions {
    /// Put panels in the edit's order (see the module documentation for the rule).
    pub reorder: bool,
    /// Flag panels the edit does not use with a first notes line [`NOT_IN_EDIT`].
    pub mark_missing: bool,
}

/// The first notes line of a panel a conform found missing from the edit.
pub const NOT_IN_EDIT: &str = "[not in the edit]";

/// Whether a panel carries the [`NOT_IN_EDIT`] flag.
pub fn is_marked_missing(notes: &str) -> bool {
    notes.lines().next().is_some_and(|l| l.trim() == NOT_IN_EDIT)
}

fn mark(notes: &mut String) -> bool {
    if is_marked_missing(notes) {
        return false;
    }
    *notes = if notes.is_empty() { NOT_IN_EDIT.to_string() } else { format!("{NOT_IN_EDIT}\n{notes}") };
    true
}

fn unmark(notes: &mut String) -> bool {
    if !is_marked_missing(notes) {
        return false;
    }
    *notes = notes.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or_default();
    true
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
    conform_with(project, path, bytes, &ConformOptions::default())
}

/// Take panel timing, and with `options` the order and the panels left out, from an edited
/// timeline. On error the project is unchanged.
pub fn conform_with(project: &mut Project, path: &Path, bytes: &[u8], options: &ConformOptions) -> Result<ConformReport> {
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
    // Panels in the order the edit first uses them.
    let mut edit_order: Vec<Id> = Vec::new();
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
        if used.insert(id) {
            edit_order.push(id);
        }
        let frames = (c.seconds * fps).round().clamp(1.0, crate::model::MAX_PANEL_FRAMES as f64) as u32;
        if let Some(p) = project.panel_mut(id)
            && p.duration != frames
        {
            p.duration = frames;
            report.retimed += 1;
        }
    }
    report.unused_panels = project.panels().filter(|(_, p)| !used.contains(&p.id)).map(|(_, p)| project.panel_label(p.id)).collect();
    if options.mark_missing {
        for scene in &mut project.scenes {
            for p in &mut scene.panels {
                if used.contains(&p.id) {
                    report.unmarked += usize::from(unmark(&mut p.notes));
                } else {
                    report.marked += usize::from(mark(&mut p.notes));
                }
            }
        }
    }
    if options.reorder && !edit_order.is_empty() {
        reorder(project, &edit_order, &mut report);
    }
    Ok(report)
}

/// Rearrange the board to `edit_order` (panel ids in the order the edit first uses them) by the
/// rule in the module documentation.
fn reorder(project: &mut Project, edit_order: &[Id], report: &mut ConformReport) {
    let before: Vec<Id> = project.panels().map(|(_, p)| p.id).collect();
    let rank: HashMap<Id, usize> = edit_order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let home: HashMap<Id, usize> = project.scenes.iter().enumerate().flat_map(|(si, s)| s.panels.iter().map(move |p| (p.id, si))).collect();
    // Rule 1: the scene each used panel ends up in.
    let mut target: HashMap<Id, usize> = HashMap::new();
    for (i, id) in edit_order.iter().enumerate() {
        let own = home.get(id).copied().unwrap_or(0);
        let prev = i.checked_sub(1).and_then(|j| edit_order.get(j)).and_then(|p| home.get(p)).copied();
        let next = edit_order.get(i + 1).and_then(|n| home.get(n)).copied();
        let scene = match (prev, next) {
            (Some(a), Some(b)) if a == b && a != own => a,
            _ => own,
        };
        if scene != own {
            report.changed_scene += 1;
        }
        target.insert(*id, scene);
    }
    // Take every panel out, keeping the old scene lists for the anchors of unused panels.
    let old: Vec<Vec<Id>> = project.scenes.iter().map(|s| s.panels.iter().map(|p| p.id).collect()).collect();
    let mut pool: HashMap<Id, crate::model::Panel> = HashMap::new();
    for s in &mut project.scenes {
        for p in s.panels.drain(..) {
            pool.insert(p.id, p);
        }
    }
    let mut lists: Vec<Vec<Id>> = vec![Vec::new(); project.scenes.len()];
    // Rule 2: used panels in edit order within their target scene.
    for id in edit_order {
        if let Some(list) = target.get(id).and_then(|si| lists.get_mut(*si)) {
            list.push(*id);
        }
    }
    // Rule 3: unused panels after their board predecessor that stayed in the scene.
    for (si, ids) in old.iter().enumerate() {
        for (pi, id) in ids.iter().enumerate() {
            if rank.contains_key(id) {
                continue;
            }
            let Some(list) = lists.get_mut(si) else { continue };
            let anchor = ids[..pi].iter().rev().find_map(|prev| list.iter().position(|x| x == prev));
            match anchor {
                Some(at) => list.insert(at + 1, *id),
                None => list.insert(0, *id),
            }
        }
    }
    for (scene, list) in project.scenes.iter_mut().zip(&lists) {
        scene.panels = list.iter().filter_map(|id| pool.remove(id)).collect();
    }
    // Rule 4: scenes by their first used panel; unused scenes after their board predecessor.
    let first_use: Vec<Option<usize>> = project.scenes.iter().map(|s| s.panels.iter().filter_map(|p| rank.get(&p.id).copied()).min()).collect();
    let mut order: Vec<usize> = (0..project.scenes.len()).filter(|i| first_use[*i].is_some()).collect();
    order.sort_by_key(|i| first_use[*i]);
    for i in 0..project.scenes.len() {
        if first_use[i].is_some() {
            continue;
        }
        let at = (0..i).rev().find_map(|prev| order.iter().position(|x| *x == prev)).map(|p| p + 1).unwrap_or(0);
        order.insert(at, i);
    }
    let mut scenes: Vec<Option<crate::model::Scene>> = std::mem::take(&mut project.scenes).into_iter().map(Some).collect();
    project.scenes = order.into_iter().filter_map(|i| scenes.get_mut(i).and_then(Option::take)).collect();
    let n = project.scenes.len();
    project.scenes.retain(|s| !s.panels.is_empty());
    report.removed_scenes = n - project.scenes.len();
    // Defensive: a panel never goes missing.
    if !pool.is_empty()
        && let Some(last) = project.scenes.last_mut()
    {
        last.panels.extend(pool.into_values());
    }
    let after: Vec<Id> = project.panels().map(|(_, p)| p.id).collect();
    report.moved = before.iter().zip(&after).filter(|(a, b)| a != b).count();
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

    /// Scenes A [a1 a2 a3], B [b1 b2], C [c1]; panel text names each panel.
    fn three_scenes() -> Project {
        let mut p = Project::new("Order");
        p.settings = Settings { width: 64, height: 36, fps: 24, ntsc: false };
        for (heading, names) in [("INT. A", &["a1", "a2", "a3"][..]), ("INT. B", &["b1", "b2"][..]), ("INT. C", &["c1"][..])] {
            let s = p.add_scene(heading);
            let mut after = None;
            for (i, n) in names.iter().enumerate() {
                let id = p.add_panel(s, after).unwrap();
                let panel = p.panel_mut(id).unwrap();
                panel.action = n.to_string();
                panel.duration = 10 + i as u32;
                after = Some(id);
            }
        }
        p
    }

    fn names(p: &Project) -> Vec<Vec<String>> {
        p.scenes.iter().map(|s| s.panels.iter().map(|x| x.action.clone()).collect()).collect()
    }

    /// The edit: b1, c1, b2, a2, a1 (a3 cut out), as (scene, panel) indices of the board.
    const CUT: [(usize, usize); 5] = [(1, 0), (2, 0), (1, 1), (0, 1), (0, 0)];

    #[test]
    fn conform_reorders_and_marks_in_every_format() {
        let dir = Path::new("/tmp/boards/Order_stills");
        for f in TimelineFormat::ALL {
            let board = three_scenes();
            let (cut, _) = document_in(&board, f, dir, &CUT).unwrap();
            let name = format!("cut.{}", f.extension());
            // Default: retime only, order untouched.
            let mut plain = board.clone();
            let r = conform(&mut plain, Path::new(&name), cut.as_bytes()).unwrap();
            assert_eq!(names(&plain), names(&board), "{f:?}");
            assert_eq!((r.matched, r.moved, r.marked), (5, 0, 0), "{f:?}: {r:?}");
            assert_eq!(r.unused_panels, vec!["1.3".to_string()], "{f:?}");
            // Reorder and mark.
            let mut q = board.clone();
            let opts = ConformOptions { reorder: true, mark_missing: true };
            let r = conform_with(&mut q, Path::new(&name), cut.as_bytes(), &opts).unwrap();
            // c1 sits between b1 and b2, so it joins scene B; C is left empty and removed.
            // a3 is not in the edit: it stays after a2, its board predecessor, and is marked.
            assert_eq!(names(&q), vec![vec!["b1", "c1", "b2"], vec!["a2", "a3", "a1"]], "{f:?}: {r:?}");
            assert_eq!(q.scenes[0].heading, "INT. B");
            assert_eq!((r.changed_scene, r.removed_scenes, r.marked), (1, 1, 1), "{f:?}: {r:?}");
            assert_eq!(q.panel_count(), board.panel_count(), "nothing is deleted");
            let a3 = q.scenes[1].panels[1].clone();
            assert!(is_marked_missing(&a3.notes), "{f:?}");
            assert_eq!(a3.duration, 12, "unused panels keep their timing");
            assert!(r.summary().contains("moved"), "{}", r.summary());
        }
    }

    #[test]
    fn marks_are_idempotent_and_removed_when_the_panel_returns() {
        let board = three_scenes();
        let dir = Path::new("/x");
        let (cut, _) = document_in(&board, TimelineFormat::Otio, dir, &CUT).unwrap();
        let mut q = board.clone();
        let opts = ConformOptions { reorder: false, mark_missing: true };
        q.scenes[0].panels[2].notes = "Keep the rain.".into();
        conform_with(&mut q, Path::new("c.otio"), cut.as_bytes(), &opts).unwrap();
        let r = conform_with(&mut q, Path::new("c.otio"), cut.as_bytes(), &opts).unwrap();
        assert_eq!(r.marked, 0, "already marked");
        assert_eq!(q.scenes[0].panels[2].notes, format!("{NOT_IN_EDIT}\nKeep the rain."));
        // The full board as the edit: a3 is back, its flag goes.
        let (full, _) = document(&board, TimelineFormat::Otio, dir).unwrap();
        let r = conform_with(&mut q, Path::new("c.otio"), full.as_bytes(), &opts).unwrap();
        assert_eq!(r.unmarked, 1);
        assert_eq!(q.scenes[0].panels[2].notes, "Keep the rain.");
    }

    #[test]
    fn options_parse_with_defaults() {
        let o: ConformOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(o, ConformOptions::default());
        let o: ConformOptions = serde_json::from_str(r#"{"reorder":true}"#).unwrap();
        assert!(o.reorder && !o.mark_missing);
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
