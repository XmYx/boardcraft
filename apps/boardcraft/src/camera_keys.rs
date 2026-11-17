//! The camera-key strip under the Panel view's canvas: the panel's frames as a track with a marker
//! per camera key, and the selected key's values.
//!
//! Click a marker to select its key (the playhead jumps to it), drag it to move the key to another
//! frame. Without a pointer: K adds a key at the playhead, a focused marker moves to the previous or
//! next key with Left/Right, moves its key a frame with Shift+Left/Right and deletes it with
//! Delete; the buttons and typed fields do the same. Every change is a board command.

use boardcraft_core::command::Command;
use boardcraft_core::{CameraKey, Id};

use crate::app::{BoardApp, CanvasTool};

/// Degrees the Turn buttons rotate the selected key.
const TURN_STEP: f32 = 5.0;

/// The playhead's frame inside `panel`, clamped to the panel.
pub fn local_frame(app: &BoardApp, panel: Id) -> u32 {
    let start = app.project.panel_start(panel).unwrap_or(0) as f64;
    let d = app.project.panel(panel).map(|p| p.duration.max(1)).unwrap_or(1);
    ((app.playhead - start).max(0.0) as u32).min(d - 1)
}

/// The selected key's frame when it belongs to `panel` and still exists.
pub fn selected_on(app: &BoardApp, panel: Id) -> Option<u32> {
    app.camera_key.filter(|(p, f)| *p == panel && app.project.panel(*p).is_some_and(|x| x.camera.key(*f).is_some())).map(|(_, f)| f)
}

/// Select the key on `frame` and move the playhead to it.
pub fn select_key(app: &mut BoardApp, panel: Id, frame: u32) {
    app.camera_key = Some((panel, frame));
    if let Some(start) = app.project.panel_start(panel) {
        app.playhead = start as f64 + frame as f64;
    }
}

/// Add a key at the playhead (the camera as it is there), or select the one already there.
pub fn add_at_playhead(app: &mut BoardApp, panel: Id) {
    let frame = local_frame(app, panel);
    let Some(p) = app.project.panel(panel) else { return };
    if p.camera.key(frame).is_none() {
        let key = CameraKey { frame, ..p.camera.at(frame as f32) };
        if app.run(Command::SetCameraKey { panel, key }).is_none() {
            return;
        }
        app.status = tlf!("Camera key added at frame {f}", f = frame);
    }
    select_key(app, panel, frame);
}

/// The key the Camera tool edits on `panel`: the selected one, else a new key at the playhead.
pub fn edit_target(app: &mut BoardApp, panel: Id) -> Option<CameraKey> {
    if selected_on(app, panel).is_none() {
        add_at_playhead(app, panel);
    }
    let frame = selected_on(app, panel)?;
    app.project.panel(panel)?.camera.key(frame)
}

/// Replace the selected key's values (centre, zoom, rotation) with `key`'s.
pub fn set_selected(app: &mut BoardApp, panel: Id, key: CameraKey) {
    app.run(Command::SetCameraKey { panel, key });
}

fn move_key(app: &mut BoardApp, panel: Id, from: u32, to: u32) {
    let last = app.project.panel(panel).map(|p| p.duration.max(1) - 1).unwrap_or(0);
    let to = to.min(last);
    if from != to && app.run(Command::MoveCameraKey { panel, from, to }).is_some() {
        select_key(app, panel, to);
        app.status = tlf!("Camera key moved to frame {f}", f = to);
    }
}

fn delete_key(app: &mut BoardApp, panel: Id, frame: u32) {
    if app.run(Command::RemoveCameraKey { panel, frame }).is_some() {
        app.camera_key = None;
        app.status = tlf!("Camera key at frame {f} deleted", f = frame);
    }
}

fn step_key(app: &mut BoardApp, panel: Id, later: bool) -> Option<u32> {
    let frames: Vec<u32> = app.project.panel(panel)?.camera.keys.iter().map(|k| k.frame).collect();
    let cur = selected_on(app, panel).map(|f| f as i64).unwrap_or(if later { -1 } else { i64::MAX });
    let next = if later { frames.iter().find(|f| **f as i64 > cur) } else { frames.iter().rev().find(|f| (**f as i64) < cur) }.copied()?;
    select_key(app, panel, next);
    Some(next)
}

/// The strip: buttons, the key track and the selected key's fields.
pub fn strip(app: &mut BoardApp, ui: &mut egui::Ui, panel: Id) {
    let Some(p) = app.project.panel(panel) else { return };
    let duration = p.duration.max(1);
    let keys: Vec<CameraKey> = p.camera.keys.clone();
    let aspect = app.project.settings.width as f32 / app.project.settings.height.max(1) as f32;
    let selected = selected_on(app, panel);
    let now = local_frame(app, panel);
    ui.horizontal_wrapped(|ui| {
        ui.strong(tl!("Camera keys"));
        if ui.button(tl!("Add Key at Playhead")).on_hover_text(tl!("Key the camera at the playhead's frame (K)")).clicked() {
            add_at_playhead(app, panel);
        }
        if ui.add_enabled(!keys.is_empty(), egui::Button::new(tl!("Previous Key"))).clicked() {
            step_key(app, panel, false);
        }
        if ui.add_enabled(!keys.is_empty(), egui::Button::new(tl!("Next Key"))).clicked() {
            step_key(app, panel, true);
        }
        if ui.add_enabled(selected.is_some(), egui::Button::new(tl!("Delete Key"))).clicked()
            && let Some(f) = selected
        {
            delete_key(app, panel, f);
        }
        ui.label(tlf!("{n} keys · playhead at frame {f}", n = keys.len(), f = now));
    });

    // The track: one marker per key along the panel's frames.
    let width = ui.available_width().max(60.0);
    let (track, _) = ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::hover());
    let painter = ui.painter_at(track.expand(2.0));
    let x_of = |f: u32| track.left() + 8.0 + (track.width() - 16.0) * f as f32 / (duration - 1).max(1) as f32;
    let frame_of =
        |x: f32| (((x - track.left() - 8.0) / (track.width() - 16.0).max(1.0)) * (duration - 1) as f32).round().clamp(0.0, (duration - 1) as f32) as u32;
    let line = ui.visuals().widgets.noninteractive.fg_stroke.color;
    painter.line_segment([egui::pos2(x_of(0), track.center().y), egui::pos2(x_of(duration - 1), track.center().y)], egui::Stroke::new(1.0, line));
    painter.line_segment(
        [egui::pos2(x_of(now), track.top()), egui::pos2(x_of(now), track.bottom())],
        egui::Stroke::new(2.0, egui::Color32::from_rgb(220, 50, 40)),
    );
    let mut action: Option<(u32, KeyAction)> = None;
    for k in &keys {
        let c = egui::pos2(x_of(k.frame), track.center().y);
        let r = egui::Rect::from_center_size(c, egui::vec2(16.0, 22.0));
        let resp = ui.interact(r, ui.scope_id().with(("cam-key", panel.0, k.frame)), egui::Sense::click_and_drag());
        let is_sel = selected == Some(k.frame);
        let name = tlf!("Camera key at frame {f}", f = k.frame);
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, is_sel, &name));
        if app.focus_camera_key == Some(k.frame) && is_sel {
            resp.request_focus();
            app.focus_camera_key = None;
        }
        let fill = if is_sel { ui.visuals().selection.bg_fill } else { ui.visuals().widgets.inactive.fg_stroke.color };
        let ring = if resp.has_focus() { ui.visuals().selection.stroke } else { egui::Stroke::new(1.0, line) };
        let d = 7.0;
        painter.add(egui::Shape::convex_polygon(
            vec![c + egui::vec2(0.0, -d), c + egui::vec2(d, 0.0), c + egui::vec2(0.0, d), c + egui::vec2(-d, 0.0)],
            fill,
            ring,
        ));
        if resp.clicked() || resp.drag_started() || (resp.gained_focus() && !is_sel) {
            action = Some((k.frame, KeyAction::Select));
        }
        if resp.drag_stopped()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            action = Some((k.frame, KeyAction::MoveTo(frame_of(pos.x))));
        }
        if resp.has_focus() {
            let (l, r, shift, del) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowLeft),
                    i.key_pressed(egui::Key::ArrowRight),
                    i.modifiers.shift,
                    i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                )
            });
            if shift && (l || r) {
                let to = if r { k.frame.saturating_add(1) } else { k.frame.saturating_sub(1) };
                action = Some((k.frame, KeyAction::MoveTo(to)));
            } else if l || r {
                action = Some((k.frame, KeyAction::Step(r)));
            } else if del {
                action = Some((k.frame, KeyAction::Delete));
            }
        }
    }
    match action {
        Some((f, KeyAction::Select)) => select_key(app, panel, f),
        Some((f, KeyAction::MoveTo(to))) => {
            move_key(app, panel, f, to);
            app.focus_camera_key = Some(to.min(duration - 1));
        }
        Some((f, KeyAction::Step(later))) => {
            select_key(app, panel, f);
            if let Some(next) = step_key(app, panel, later) {
                app.focus_camera_key = Some(next);
            }
        }
        Some((f, KeyAction::Delete)) => delete_key(app, panel, f),
        None => {}
    }

    // The selected key's values, typed.
    let Some(frame) = selected_on(app, panel) else {
        if keys.is_empty() {
            ui.label(tl!("No keys: the camera shows the whole canvas. Add a key, then frame it with the Camera tool (C) or the fields."));
        }
        return;
    };
    let Some(mut key) = app.project.panel(panel).and_then(|p| p.camera.key(frame)) else { return };
    ui.horizontal_wrapped(|ui| {
        let mut f = frame;
        let l = ui.label(tl!("Frame"));
        if ui.add(egui::DragValue::new(&mut f).range(0..=duration - 1)).labelled_by(l.id).changed() {
            move_key(app, panel, frame, f);
            return;
        }
        let mut changed = false;
        let l = ui.label(tl!("Centre x"));
        changed |= ui.add(egui::DragValue::new(&mut key.center[0]).range(0.0..=1.0).speed(0.005).max_decimals(3)).labelled_by(l.id).changed();
        let l = ui.label(tl!("Centre y"));
        changed |= ui.add(egui::DragValue::new(&mut key.center[1]).range(0.0..=1.0).speed(0.005).max_decimals(3)).labelled_by(l.id).changed();
        let l = ui.label(tl!("Zoom"));
        changed |=
            ui.add(egui::DragValue::new(&mut key.zoom).range(1.0..=boardcraft_core::camera::MAX_ZOOM).speed(0.01).max_decimals(2)).labelled_by(l.id).changed();
        let l = ui.label(tl!("Rotation"));
        let max = boardcraft_core::camera::MAX_ROTATION;
        changed |= ui.add(egui::DragValue::new(&mut key.rotation).range(-max..=max).speed(0.5).max_decimals(1).suffix("°")).labelled_by(l.id).changed();
        if ui.button(tl!("Turn Left")).on_hover_text(tl!("Turn the frame 5° counter-clockwise")).clicked() {
            key.rotation -= TURN_STEP;
            changed = true;
        }
        if ui.button(tl!("Turn Right")).on_hover_text(tl!("Turn the frame 5° clockwise")).clicked() {
            key.rotation += TURN_STEP;
            changed = true;
        }
        if changed {
            set_selected(app, panel, key);
        }
        let shown = key.fitted(aspect);
        if (shown.zoom - key.sanitized().zoom).abs() > 1e-3 {
            ui.label(tlf!("Shown at zoom {z} so the turned frame stays on the canvas", z = format!("{:.2}", shown.zoom)));
        }
    });
    if app.tool != CanvasTool::Camera {
        ui.weak(tl!("Camera tool (C): drag to move the frame, scroll to zoom, Shift+scroll to turn."));
    }
}

#[derive(Clone, Copy)]
enum KeyAction {
    Select,
    MoveTo(u32),
    Step(bool),
    Delete,
}
