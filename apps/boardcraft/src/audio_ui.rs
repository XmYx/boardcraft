//! The audio track in the app: File ▸ Import Audio, the waveform strip under the timeline's panel
//! blocks, the Sound section of the inspector (typed start, trim, gain and panel anchor: the
//! keyboard alternative to dragging), and sound during animatic playback.
//!
//! Clips on the strip are buttons: click selects, dragging moves the clip, and with a clip focused
//! Left/Right nudge it a frame (Shift: a second).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use boardcraft_core::audio::{AUDIO_EXTENSIONS, AudioBuffer, Mixer, Waveform};
use boardcraft_core::command::Command;
use boardcraft_core::{Id, Project};

use crate::app::BoardApp;

/// Clip fill on the strip; waveform and white text are at least 3:1 and 4.5:1 on it (checked in a test).
pub const CLIP_FILL: egui::Color32 = egui::Color32::from_rgb(24, 72, 84);
pub const WAVE: egui::Color32 = egui::Color32::from_rgb(130, 214, 226);
/// Height of the waveform strip.
pub const STRIP_HEIGHT: f32 = 34.0;

struct Cached {
    data: Arc<Vec<u8>>,
    decoded: Result<(Arc<AudioBuffer>, Arc<Waveform>), String>,
}

/// Decoded clips, the output stream and the strip's interaction state.
#[derive(Default)]
pub struct AudioState {
    cache: HashMap<Id, Cached>,
    pub out: crate::audio_out::AudioOut,
    /// The clip the inspector edits.
    pub selected: Option<Id>,
    /// A clip being dragged: (clip, grab offset in frames from its start).
    drag: Option<(Id, f32)>,
    /// Playback with sound: when it started, from which frame, for which track.
    since: Option<(Instant, f64, u64)>,
    /// Move keyboard focus to this clip's block on the next frame.
    pub focus_clip: Option<Id>,
}

impl AudioState {
    /// Decode what is new or changed; forget clips that are gone.
    fn refresh(&mut self, project: &Project) {
        self.cache.retain(|id, c| project.audio_clip(*id).is_some_and(|clip| Arc::ptr_eq(&clip.data, &c.data)));
        for clip in &project.audio {
            if self.cache.contains_key(&clip.id) {
                continue;
            }
            let decoded = boardcraft_core::audio::decode(clip).map(|b| {
                let w = Waveform::new(&b);
                (Arc::new(b), Arc::new(w))
            });
            self.cache.insert(clip.id, Cached { data: clip.data.clone(), decoded: decoded.map_err(|e| e.to_string()) });
        }
    }

    fn waveform(&self, id: Id) -> Option<Arc<Waveform>> {
        self.cache.get(&id).and_then(|c| c.decoded.as_ref().ok()).map(|(_, w)| w.clone())
    }

    fn mixer(&mut self, project: &Project) -> Mixer {
        self.refresh(project);
        let decoded: HashMap<Id, Arc<AudioBuffer>> = self.cache.iter().filter_map(|(id, c)| c.decoded.as_ref().ok().map(|(b, _)| (*id, b.clone()))).collect();
        Mixer::new(project, &decoded)
    }
}

/// What the track looks like, to notice edits during playback.
fn signature(project: &Project) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for c in &project.audio {
        (c.id, c.start, c.trim_in, c.gain_db.to_bits(), Arc::as_ptr(&c.data) as usize).hash(&mut h);
    }
    project.settings.fps.hash(&mut h);
    h.finish()
}

/// File ▸ Import Audio…
pub fn import(app: &mut BoardApp) {
    let Some(path) = rfd::FileDialog::new().add_filter(tl!("Audio"), AUDIO_EXTENSIONS).pick_file() else { return };
    import_path(app, &path);
}

/// Add a sound file: at the selected panel and anchored to it (it then follows the panel when the
/// board is retimed), or at the playhead.
pub fn import_path(app: &mut BoardApp, path: &Path) {
    let panel = app.selected.filter(|p| app.project.panel(*p).is_some());
    let start = if panel.is_some() { None } else { Some(app.playhead.max(0.0) as u64) };
    let cmd = Command::AddAudio { path: path.to_string_lossy().into_owned(), name: String::new(), start, panel };
    match boardcraft_core::command::apply(&mut app.project, &cmd) {
        Ok(out) => {
            app.edited();
            app.audio.selected = out.created;
            let name = out.created.and_then(|id| app.project.audio_clip(id)).map(|c| c.name.clone()).unwrap_or_default();
            app.status = match panel {
                Some(p) => tlf!("Added the sound {name} at panel {label}; it moves with the panel", name = name, label = app.project.panel_label(p)),
                None => tlf!("Added the sound {name}", name = name),
            };
        }
        Err(e) => app.status = tlf!("Could not import {file}: {error}", file = path.display(), error = e),
    }
}

/// Start, restart (after a seek or an edit) or stop sound to match playback.
pub fn sync_playback(app: &mut BoardApp) {
    let fps = app.project.settings.fps_f64().max(1e-6);
    if !app.playing || app.project.audio.is_empty() {
        if app.audio.since.take().is_some() || app.audio.out.is_playing() {
            app.audio.out.stop();
        }
        return;
    }
    let sig = signature(&app.project);
    let restart = match app.audio.since {
        None => true,
        Some((t0, f0, s)) => s != sig || (f0 + t0.elapsed().as_secs_f64() * fps - app.playhead).abs() > 3.0,
    };
    if !restart {
        return;
    }
    let mixer = app.audio.mixer(&app.project);
    app.audio.since = Some((Instant::now(), app.playhead, sig));
    if let Err(e) = app.audio.out.play(Arc::new(mixer), app.playhead / fps) {
        app.status = tlf!("Playing without sound: {error}", error = e);
    }
}

/// What the strip asks for.
pub enum StripAction {
    Select(Id),
    Move(Id, u64),
}

/// Draw the clips in `rect` (the strip under the panel blocks; x = 0 is frame 0).
pub fn strip(ui: &mut egui::Ui, rect: egui::Rect, project: &Project, ppf: f32, state: &mut AudioState) -> Vec<StripAction> {
    state.refresh(project);
    let mut actions = Vec::new();
    let painter = ui.painter_at(rect);
    let fps = project.settings.fps_f64().max(1e-6);
    let visible = ui.clip_rect().intersect(rect);
    let shift = ui.input(|i| i.modifiers.shift);
    for clip in &project.audio {
        let len = clip.length_frames(fps).max(1) as f32;
        let r = egui::Rect::from_min_size(
            egui::pos2(rect.left() + clip.start as f32 * ppf, rect.top() + 1.0),
            egui::vec2((len * ppf).max(4.0), rect.height() - 2.0),
        );
        let resp = ui.interact(r, ui.scope_id().with(("tl-sound", clip.id.0)), egui::Sense::click_and_drag());
        let secs = clip.start as f64 / fps;
        let name = match clip.anchor {
            Some(a) => tlf!(
                "Sound {name} at {seconds} s, moves with panel {label}",
                name = clip.name,
                seconds = format!("{secs:.2}"),
                label = project.panel_label(a.panel)
            ),
            None => tlf!("Sound {name} at {seconds} s", name = clip.name, seconds = format!("{secs:.2}")),
        };
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, state.selected == Some(clip.id), &name));
        if state.focus_clip == Some(clip.id) {
            resp.request_focus();
            resp.scroll_to_me(None);
            state.focus_clip = None;
        }
        if resp.clicked() || resp.drag_started() {
            actions.push(StripAction::Select(clip.id));
        }
        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            state.drag = Some((clip.id, (p.x - r.left()) / ppf));
        }
        if resp.dragged()
            && let (Some((id, grab)), Some(p)) = (state.drag, resp.interact_pointer_pos())
            && id == clip.id
        {
            let start = ((p.x - rect.left()) / ppf - grab).round().max(0.0) as u64;
            if start != clip.start {
                actions.push(StripAction::Move(clip.id, start));
            }
        }
        if resp.drag_stopped() {
            state.drag = None;
        }
        if resp.has_focus() {
            let (l, rt) = ui.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight)));
            let step = if shift { fps.round() as u64 } else { 1 };
            if l {
                actions.push(StripAction::Move(clip.id, clip.start.saturating_sub(step)));
            }
            if rt {
                actions.push(StripAction::Move(clip.id, clip.start.saturating_add(step)));
            }
            if l || rt {
                state.focus_clip = Some(clip.id);
            }
        }
        if !ui.is_rect_visible(r) {
            continue;
        }
        painter.rect_filled(r, 2.0, CLIP_FILL);
        // Waveform, one column a pixel over the visible part.
        let x0 = r.left().max(visible.left()).floor();
        let x1 = r.right().min(visible.right()).ceil();
        if let Some(w) = state.waveform(clip.id)
            && x1 > x0
        {
            let cols = (x1 - x0) as usize;
            let step = 1.0 / (ppf as f64 * fps);
            let from = clip.trim_in as f64 / fps + (x0 - r.left()) as f64 * step;
            let mid = r.center().y;
            let half = r.height() / 2.0 - 2.0;
            for (i, c) in w.columns(from, step, cols).into_iter().enumerate() {
                let Some((lo, hi)) = c else { continue };
                let x = x0 + i as f32 + 0.5;
                let (a, b) = (mid - hi.clamp(-1.0, 1.0) * half, mid - lo.clamp(-1.0, 1.0) * half);
                painter.line_segment([egui::pos2(x, a), egui::pos2(x, b.max(a + 1.0))], egui::Stroke::new(1.0, WAVE));
            }
        }
        let stroke = if state.selected == Some(clip.id) || resp.has_focus() {
            egui::Stroke::new(2.0, ui.visuals().selection.stroke.color)
        } else {
            egui::Stroke::new(1.0, WAVE)
        };
        painter.rect_stroke(r, 2.0, stroke, egui::StrokeKind::Inside);
        painter.text(r.left_top() + egui::vec2(4.0, 2.0), egui::Align2::LEFT_TOP, &clip.name, egui::FontId::proportional(10.0), egui::Color32::WHITE);
    }
    actions
}

/// The Sound section of the inspector: every clip, and the selected one's start, trim, gain and
/// anchor as typed values.
pub fn inspector(app: &mut BoardApp, ui: &mut egui::Ui) {
    ui.heading(tl!("Sound"));
    if app.project.audio.is_empty() {
        ui.label(tl!("No sound yet. Import Audio adds a clip at the selected panel."));
        if ui.button(tl!("Import Audio…")).clicked() {
            import(app);
        }
        return;
    }
    if app.audio.selected.is_none_or(|id| app.project.audio_clip(id).is_none()) {
        app.audio.selected = app.project.audio.first().map(|c| c.id);
    }
    for c in &app.project.audio {
        if ui.selectable_label(app.audio.selected == Some(c.id), &c.name).clicked() {
            app.audio.selected = Some(c.id);
        }
    }
    let Some(clip) = app.audio.selected.and_then(|id| app.project.audio_clip(id)).cloned() else { return };
    let fps = app.project.settings.fps_f64();
    let mut cmds = Vec::new();
    egui::Grid::new("sound-fields").num_columns(2).show(ui, |ui| {
        let mut start = clip.start;
        let l = ui.label(tl!("Start"));
        if ui.add(egui::DragValue::new(&mut start).range(0..=boardcraft_core::audio::MAX_START).suffix(" fr")).labelled_by(l.id).changed() {
            cmds.push(Command::MoveAudio { clip: clip.id, start });
        }
        ui.end_row();
        let mut trim = clip.trim_in;
        let l = ui.label(tl!("Trim start"));
        if ui.add(egui::DragValue::new(&mut trim).range(0..=boardcraft_core::audio::MAX_START).suffix(" fr")).labelled_by(l.id).changed() {
            cmds.push(Command::SetAudioTrim { clip: clip.id, trim_in: trim });
        }
        ui.end_row();
        let mut gain = clip.gain_db;
        let l = ui.label(tl!("Gain"));
        if ui
            .add(egui::DragValue::new(&mut gain).range(boardcraft_core::audio::MIN_GAIN_DB..=boardcraft_core::audio::MAX_GAIN_DB).speed(0.1).suffix(" dB"))
            .labelled_by(l.id)
            .changed()
        {
            cmds.push(Command::SetAudioGain { clip: clip.id, gain_db: gain });
        }
        ui.end_row();
    });
    ui.label(tlf!("Length {seconds} s", seconds = format!("{:.2}", clip.length_frames(fps) as f64 / fps.max(1e-6))));
    match clip.anchor {
        Some(a) => {
            ui.label(tlf!("Moves with panel {label}", label = app.project.panel_label(a.panel)));
            if ui.button(tl!("Unanchor")).on_hover_text(tl!("Keep the sound where it is when panels are retimed")).clicked() {
                cmds.push(Command::SetAudioAnchor { clip: clip.id, panel: None });
            }
        }
        None => {
            ui.label(tl!("Stays at its time when panels are retimed"));
        }
    }
    if let Some(p) = app.selected
        && clip.anchor.map(|a| a.panel) != Some(p)
        && ui.button(tlf!("Anchor to Panel {label}", label = app.project.panel_label(p))).clicked()
    {
        cmds.push(Command::SetAudioAnchor { clip: clip.id, panel: Some(p) });
    }
    ui.horizontal(|ui| {
        if ui.button(tl!("Start at Playhead")).clicked() {
            cmds.push(Command::MoveAudio { clip: clip.id, start: app.playhead.max(0.0) as u64 });
        }
        if ui.button(tl!("Remove Sound")).clicked() {
            cmds.push(Command::RemoveAudio { clip: clip.id });
        }
        if ui.button(tl!("Import Audio…")).clicked() {
            import(app);
        }
    });
    for c in cmds {
        app.run(c);
    }
}
