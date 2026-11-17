//! Panel view: the drawing canvas, the camera frame and animatic playback.

use boardcraft_core::{CameraKey, Id, StrokePoint};

use crate::app::{BoardApp, CanvasTool, KeyChoice, StrokeState};

const FULL_UV: egui::Rect = egui::Rect { min: egui::pos2(0.0, 0.0), max: egui::pos2(1.0, 1.0) };

fn view_rect(rect: egui::Rect, v: [f32; 4]) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.left() + v[0] * rect.width(), rect.top() + v[1] * rect.height()),
        egui::pos2(rect.left() + v[2] * rect.width(), rect.top() + v[3] * rect.height()),
    )
}

fn toolbar(app: &mut BoardApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.tool, CanvasTool::Brush, "Brush").on_hover_text("Draw (B)");
        ui.selectable_value(&mut app.tool, CanvasTool::Eraser, "Eraser").on_hover_text("Erase (E)");
        ui.selectable_value(&mut app.tool, CanvasTool::Camera, "Camera").on_hover_text("Drag to frame the shot, scroll to zoom (C)");
        ui.separator();
        match app.tool {
            CanvasTool::Brush => {
                let l = ui.label("Size");
                ui.add(egui::Slider::new(&mut app.brush.radius, 0.5..=60.0).logarithmic(true)).labelled_by(l.id);
                let mut c = egui::Color32::from_rgba_unmultiplied(app.brush.color[0], app.brush.color[1], app.brush.color[2], app.brush.color[3]);
                let l = ui.label("Colour");
                if ui.color_edit_button_srgba(&mut c).labelled_by(l.id).changed() {
                    app.brush.color = c.to_srgba_unmultiplied();
                }
                let l = ui.label("Hardness");
                ui.add(egui::Slider::new(&mut app.brush.hardness, 0.0..=1.0).show_value(false)).labelled_by(l.id);
                ui.checkbox(&mut app.brush.pressure_size, "Pressure: size");
                ui.checkbox(&mut app.brush.pressure_opacity, "Pressure: opacity");
            }
            CanvasTool::Eraser => {
                let l = ui.label("Size");
                ui.add(egui::Slider::new(&mut app.eraser_radius, 1.0..=120.0).logarithmic(true)).labelled_by(l.id);
            }
            CanvasTool::Camera => {
                ui.label("Edit key:");
                ui.radio_value(&mut app.key_choice, KeyChoice::Start, "Start");
                ui.radio_value(&mut app.key_choice, KeyChoice::End, "End");
            }
        }
    });
}

/// Latest pen pressure from touch / stylus events this frame.
fn pressure_from_input(ui: &egui::Ui) -> Option<f32> {
    ui.input(|i| {
        i.events.iter().rev().find_map(|e| match e {
            egui::Event::Touch { force: Some(f), .. } if f.is_finite() => Some(f.clamp(0.0, 1.0)),
            _ => None,
        })
    })
}

pub fn show(app: &mut BoardApp, ui: &mut egui::Ui) {
    toolbar(app, ui);
    ui.separator();
    let Some(panel_id) = app.selected else {
        ui.label("No panel selected.");
        return;
    };
    let settings = app.project.settings;
    let avail = ui.available_rect_before_wrap().shrink(12.0);
    let aspect = settings.width as f32 / settings.height.max(1) as f32;
    let (w, h) =
        if avail.width() / avail.height().max(1.0) > aspect { (avail.height() * aspect, avail.height()) } else { (avail.width(), avail.width() / aspect) };
    if w < 8.0 || h < 8.0 {
        return;
    }
    let rect = egui::Rect::from_center_size(avail.center(), egui::vec2(w, h));
    let resp = ui.allocate_rect(rect, egui::Sense::drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, "Panel canvas"));

    let start = app.project.panel_start(panel_id).unwrap_or(0) as f64;
    let local = (app.playhead - start).max(0.0) as f32;
    let Some(panel) = app.project.panel(panel_id) else { return };
    let duration = panel.duration.max(1);
    let tex = app.textures.full(ui.ctx(), &app.project, panel);
    let painter = ui.painter_at(ui.max_rect());
    if app.playing {
        // The animatic: what the camera sees.
        let v = panel.camera.view(local.min(duration as f32 - 1.0));
        let uv = egui::Rect::from_min_max(egui::pos2(v[0], v[1]), egui::pos2(v[2], v[3]));
        painter.rect_filled(rect.expand(12.0), 0.0, egui::Color32::BLACK);
        painter.image(tex, rect, uv, egui::Color32::WHITE);
        painter.text(
            rect.left_top() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            app.project.panel_label(panel_id),
            egui::FontId::proportional(14.0),
            egui::Color32::from_gray(90),
        );
        if !panel.dialogue.is_empty() {
            let galley = painter.layout(panel.dialogue.clone(), egui::FontId::proportional(18.0), egui::Color32::WHITE, rect.width() * 0.8);
            let pos = egui::pos2(rect.center().x - galley.size().x / 2.0, rect.bottom() - galley.size().y - 18.0);
            painter.rect_filled(egui::Rect::from_min_size(pos, galley.size()).expand(6.0), 4.0, egui::Color32::from_black_alpha(170));
            painter.galley(pos, galley, egui::Color32::WHITE);
        }
        return;
    }
    painter.rect_filled(rect.expand(12.0), 0.0, egui::Color32::from_gray(40));
    painter.image(tex, rect, FULL_UV, egui::Color32::WHITE);

    // Camera frames: start key, end key, and the frame at the playhead.
    let has_keys = !panel.camera.keys.is_empty();
    if has_keys || app.tool == CanvasTool::Camera {
        let first = panel.camera.view(0.0);
        let last = panel.camera.view(duration as f32 - 1.0);
        painter.rect_stroke(view_rect(rect, first), 0.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 170, 90)), egui::StrokeKind::Middle);
        painter.rect_stroke(view_rect(rect, last), 0.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(220, 120, 40)), egui::StrokeKind::Middle);
        if panel.camera.moves() {
            let now = panel.camera.view(local);
            painter.rect_stroke(view_rect(rect, now), 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(230, 200, 60)), egui::StrokeKind::Middle);
        }
    }

    let to_canvas = |p: egui::Pos2| ((p.x - rect.left()) / rect.width() * settings.width as f32, (p.y - rect.top()) / rect.height() * settings.height as f32);
    match app.tool {
        CanvasTool::Brush | CanvasTool::Eraser => {
            if let Some(pos) = resp.hover_pos() {
                let r = app.active_brush().radius / settings.width as f32 * rect.width();
                painter.circle_stroke(pos, r.max(1.0), egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 120, 255)));
            }
            if let Some(p) = pressure_from_input(ui) {
                app.pen_pressure = Some(p);
            }
            let pressure = app.pen_pressure.unwrap_or(1.0);
            if resp.drag_started()
                && let Some(pos) = resp.interact_pointer_pos()
            {
                match app.active_layer.filter(|l| app.project.panel(panel_id).is_some_and(|p| p.layer(*l).is_some_and(|l| l.visible))) {
                    Some(layer) => {
                        app.push_undo(panel_id, layer);
                        let (x, y) = to_canvas(pos);
                        let pt = StrokePoint::new(x, y, pressure);
                        let brush = app.active_brush();
                        if let Some(p) = app.project.panel_mut(panel_id) {
                            if let Some(l) = p.layer_mut(layer) {
                                l.pixels.dab(pt, &brush);
                            }
                            p.touch();
                        }
                        app.stroke = Some(StrokeState { panel: panel_id, layer, last: pt });
                        app.dirty = true;
                    }
                    None => app.status = "Select a visible layer to draw on".into(),
                }
            } else if resp.dragged()
                && let Some(pos) = resp.interact_pointer_pos()
                && let Some(stroke) = app.stroke.as_mut()
            {
                let (x, y) = to_canvas(pos);
                let pt = StrokePoint::new(x, y, pressure);
                let (pid, lid, last) = (stroke.panel, stroke.layer, stroke.last);
                stroke.last = pt;
                let brush = app.active_brush();
                draw_segment(app, pid, lid, last, pt, &brush);
            }
            if resp.drag_stopped() {
                app.stroke = None;
                app.pen_pressure = None;
            }
        }
        CanvasTool::Camera => {
            let frame = if app.key_choice == KeyChoice::Start { 0 } else { duration - 1 };
            let drag = resp.drag_delta();
            let scroll = if resp.hovered() { ui.input(|i| i.smooth_scroll_delta.y) } else { 0.0 };
            if drag != egui::Vec2::ZERO || scroll != 0.0 {
                if let Some(p) = app.project.panel_mut(panel_id) {
                    // A move needs both ends: pin the other end where the camera is before this edit.
                    let other = if frame == 0 { duration - 1 } else { 0 };
                    if other != frame && !p.camera.keys.iter().any(|k| k.frame == other) {
                        let at = p.camera.at(other as f32);
                        p.camera.set_key(CameraKey { frame: other, ..at });
                    }
                    let mut key = p.camera.keys.iter().find(|k| k.frame == frame).copied().unwrap_or_else(|| CameraKey { frame, ..p.camera.at(frame as f32) });
                    key.center[0] += drag.x / rect.width();
                    key.center[1] += drag.y / rect.height();
                    key.zoom *= (scroll * 0.003).exp();
                    p.camera.set_key(key);
                }
                app.dirty = true;
            }
        }
    }
}

fn draw_segment(app: &mut BoardApp, panel: Id, layer: Id, from: StrokePoint, to: StrokePoint, brush: &boardcraft_core::Brush) {
    if let Some(p) = app.project.panel_mut(panel) {
        if let Some(l) = p.layer_mut(layer) {
            l.pixels.stroke(from, to, brush);
        }
        p.touch();
    }
}
