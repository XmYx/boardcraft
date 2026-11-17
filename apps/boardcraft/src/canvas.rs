//! Panel view: the drawing canvas, the camera frame, annotations and animatic playback.

use boardcraft_core::{Id, StrokePoint, ViewQuad};

use crate::app::{BoardApp, CanvasTool, StrokeState};
use crate::tools::view::ViewXf;

/// Outline a camera frame (a quad in normalized canvas units) through the view transform; a turned
/// frame also marks its top edge so the turn reads.
fn frame_outline(painter: &egui::Painter, xf: &ViewXf, q: ViewQuad, stroke: egui::Stroke) {
    let pts: Vec<egui::Pos2> = q.iter().map(|p| xf.to_screen([p[0] * xf.size.x, p[1] * xf.size.y])).collect();
    painter.add(egui::Shape::closed_line(pts.clone(), stroke));
    if let [tl, tr, _, bl] = pts.as_slice() {
        let mid = egui::pos2((tl.x + tr.x) / 2.0, (tl.y + tr.y) / 2.0);
        let inward = (*bl - *tl).normalized() * 8.0;
        painter.line_segment([mid, mid + inward], stroke);
    }
}

fn toolbar(app: &mut BoardApp, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut app.tool, CanvasTool::Brush, tl!("Brush")).on_hover_text(tl!("Draw (B)"));
        ui.selectable_value(&mut app.tool, CanvasTool::Eraser, tl!("Eraser")).on_hover_text(tl!("Erase (E)"));
        ui.selectable_value(&mut app.tool, CanvasTool::Lasso, tl!("Lasso")).on_hover_text(tl!("Select a freehand area of the layer (L)"));
        ui.selectable_value(&mut app.tool, CanvasTool::RectSelect, tl!("Rectangle select")).on_hover_text(tl!("Select a rectangle of the layer (M)"));
        ui.selectable_value(&mut app.tool, CanvasTool::Fill, tl!("Fill")).on_hover_text(tl!("Fill an area with the colour (G)"));
        ui.selectable_value(&mut app.tool, CanvasTool::Camera, tl!("Camera"))
            .on_hover_text(tl!("Drag to frame the shot, scroll to zoom, Shift+scroll to turn (C)"));
        ui.separator();
        for (tool, label, hint) in crate::annotations::tools() {
            ui.selectable_value(&mut app.tool, tool, label).on_hover_text(hint);
        }
        ui.separator();
        crate::tools::rotation_field(app, ui);
    });
    ui.horizontal_wrapped(|ui| match app.tool {
        CanvasTool::Brush => {
            let l = ui.label(tl!("Size"));
            ui.add(egui::Slider::new(&mut app.brush.radius, 0.5..=60.0).logarithmic(true)).labelled_by(l.id);
            let mut c = egui::Color32::from_rgba_unmultiplied(app.brush.color[0], app.brush.color[1], app.brush.color[2], app.brush.color[3]);
            let l = ui.label(tl!("Colour"));
            if ui.color_edit_button_srgba(&mut c).labelled_by(l.id).changed() {
                app.brush.color = c.to_srgba_unmultiplied();
            }
            let l = ui.label(tl!("Hardness"));
            ui.add(egui::Slider::new(&mut app.brush.hardness, 0.0..=1.0).show_value(false)).labelled_by(l.id);
            ui.checkbox(&mut app.brush.pressure_size, tl!("Pressure: size"));
            ui.checkbox(&mut app.brush.pressure_opacity, tl!("Pressure: opacity"));
            ui.menu_button(tl!("Pen Settings"), |ui| crate::tools::pen::settings_ui(&mut app.tools.pen, ui));
        }
        CanvasTool::Eraser => {
            let l = ui.label(tl!("Size"));
            ui.add(egui::Slider::new(&mut app.eraser_radius, 1.0..=120.0).logarithmic(true)).labelled_by(l.id);
        }
        CanvasTool::Camera => {
            ui.label(tl!("Edits the selected camera key, or adds one at the playhead."));
        }
        CanvasTool::Lasso | CanvasTool::RectSelect => crate::tools::selection::toolbar(app, ui),
        CanvasTool::Fill => crate::tools::fill_toolbar(app, ui),
        CanvasTool::Select | CanvasTool::Pen | CanvasTool::Arrow | CanvasTool::MotionArrow | CanvasTool::CameraArrow => crate::annotations::toolbar(app, ui),
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
        ui.label(tl!("No panel selected."));
        return;
    };
    if !app.playing {
        egui::Panel::bottom("camera-keys").resizable(false).show(ui, |ui| crate::camera_keys::strip(app, ui, panel_id));
    }
    let settings = app.project.settings;
    let aspect = settings.width as f32 / settings.height.max(1) as f32;
    let avail = ui.available_rect_before_wrap().shrink(12.0);
    let (w, h) =
        if avail.width() / avail.height().max(1.0) > aspect { (avail.height() * aspect, avail.height()) } else { (avail.width(), avail.width() / aspect) };
    if w < 8.0 || h < 8.0 {
        return;
    }
    // View zoom (1 = fit) and pan; the whole area takes input so panning works off the canvas.
    let zoom = app.canvas_zoom.clamp(0.25, 16.0);
    let rect = egui::Rect::from_center_size(avail.center() + app.canvas_pan, egui::vec2(w * zoom, h * zoom));
    // Drag only: a press starts a stroke (or picks a shape) at once, so a click paints a dot.
    let resp = ui.allocate_rect(avail, egui::Sense::drag());
    if resp.hovered() {
        // Ctrl+wheel or pinch zooms about the pointer; middle-drag or Alt+drag pans.
        let dz = ui.input(|i| i.zoom_delta());
        if (dz - 1.0).abs() > f32::EPSILON
            && let Some(pos) = resp.hover_pos()
        {
            let new = (zoom * dz).clamp(0.25, 16.0);
            let k = new / zoom;
            let rel = pos - (avail.center() + app.canvas_pan);
            app.canvas_pan += rel - rel * k;
            app.canvas_zoom = new;
        }
    }
    let panning = ui.input(|i| i.pointer.middle_down() || (i.modifiers.alt && i.pointer.primary_down()));
    if panning && resp.dragged() {
        app.canvas_pan += resp.drag_delta();
    }
    // Hold R and drag to turn the view about the canvas centre.
    let turning = !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_down(egui::Key::R));
    if turning
        && !panning
        && resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let c = rect.center();
        let prev = pos - resp.drag_delta();
        let (a0, a1) = ((prev - c).angle(), (pos - c).angle());
        if a0.is_finite() && a1.is_finite() {
            crate::tools::rotate_view(app, (a1 - a0).to_degrees());
        }
    }
    let xf = ViewXf::new(rect, app.tools.view_angle, settings.width, settings.height);
    app.tools.last_view = Some(xf);
    // The canvas publishes what it shows: which panel, its layers, annotations and camera.
    let description = match app.project.panel(panel_id) {
        Some(p) => tlf!(
            "Panel canvas: panel {label}, {layers} layers, {shapes} annotations, {camera}",
            label = app.project.panel_label(panel_id),
            layers = p.layers.len(),
            shapes = p.annotations.len(),
            camera = if p.camera.moves() { tlf!("camera move with {n} keys", n = p.camera.keys.len()) } else { tl!("static camera").to_string() }
        ),
        None => tl!("Panel canvas").to_string(),
    };
    let mut description = description;
    if app.tools.view_angle != 0.0 {
        description = tlf!("{canvas}, view rotated {angle}°", canvas = description, angle = app.tools.view_angle.round());
    }
    if let Some(sel) = crate::tools::selection::describe(app) {
        description = tlf!("{canvas}, {selection}", canvas = description, selection = sel);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, description.clone()));

    let start = app.project.panel_start(panel_id).unwrap_or(0) as f64;
    let local = (app.playhead - start).max(0.0) as f32;
    let preview = if app.playing { None } else { crate::tools::selection::preview(app, ui.ctx(), panel_id) };
    let Some(panel) = app.project.panel(panel_id) else { return };
    let duration = panel.duration.max(1);
    let tex = app.textures.full(ui.ctx(), &app.project, panel);
    let painter = ui.painter_at(avail.expand(12.0));
    if app.playing {
        // The animatic: what the camera sees, turned frames included (a textured quad).
        let q = panel.camera.quad(local.min(duration as f32 - 1.0), aspect);
        painter.rect_filled(rect.expand(12.0), 0.0, egui::Color32::BLACK);
        let mut mesh = egui::Mesh::with_texture(tex);
        for (pos, uv) in [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()].into_iter().zip(q) {
            mesh.vertices.push(egui::epaint::Vertex { pos, uv: egui::pos2(uv[0], uv[1]), color: egui::Color32::WHITE });
        }
        mesh.indices = vec![0, 1, 2, 0, 2, 3];
        painter.add(egui::Shape::mesh(mesh));
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
    painter.rect_filled(avail.expand(12.0), 0.0, egui::Color32::from_gray(40));
    xf.image(&painter, preview.unwrap_or(tex), egui::Color32::WHITE);
    // Onion skin: the previous panel, faint and tinted, over the drawing.
    if app.onion
        && let Some(prev) = app.project.panels().map(|(_, p)| p.id).take_while(|id| *id != panel_id).last()
        && let Some(prev_panel) = app.project.panel(prev)
    {
        let ptex = app.textures.full(ui.ctx(), &app.project, prev_panel);
        xf.image(&painter, ptex, egui::Color32::from_rgba_unmultiplied(255, 110, 110, 60));
    }

    // Camera frames: every key (the selected one bold), and the frame at the playhead, all turned
    // with the view.
    if !panel.camera.keys.is_empty() || app.tool == CanvasTool::Camera {
        let selected = crate::camera_keys::selected_on(app, panel_id);
        let last = panel.camera.keys.len().saturating_sub(1);
        for (i, k) in panel.camera.keys.iter().enumerate() {
            let color = if i == 0 {
                egui::Color32::from_rgb(60, 170, 90)
            } else if i == last {
                egui::Color32::from_rgb(220, 120, 40)
            } else {
                egui::Color32::from_rgb(150, 110, 210)
            };
            let width = if selected == Some(k.frame) { 3.5 } else { 1.5 };
            frame_outline(&painter, &xf, k.quad(aspect), egui::Stroke::new(width, color));
        }
        if panel.camera.moves() || panel.camera.keys.is_empty() {
            frame_outline(&painter, &xf, panel.camera.quad(local, aspect), egui::Stroke::new(1.0, egui::Color32::from_rgb(230, 200, 60)));
        }
    }

    // Canvas pixels per screen point (the view rotation does not scale).
    let k = settings.width as f32 / rect.width().max(1.0);
    let to_canvas = |p: egui::Pos2| {
        let c = xf.to_canvas(p);
        (c[0], c[1])
    };
    let to_canvas_arr = move |p: egui::Pos2| xf.to_canvas(p);
    let to_screen = move |p: [f32; 2]| xf.to_screen(p);
    crate::annotations::overlay(app, &painter, panel_id, &to_screen, 1.0 / k);
    if panning || turning {
        crate::tools::selection::draw_overlay(app, &painter, &xf, panel_id);
        return;
    }
    match app.tool {
        CanvasTool::Brush | CanvasTool::Eraser => {
            if let Some(pos) = resp.hover_pos() {
                let r = app.active_brush().radius / settings.width as f32 * rect.width();
                painter.circle_stroke(pos, r.max(1.0), egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 120, 255)));
            }
            if let Some(p) = pressure_from_input(ui) {
                app.pen_pressure = Some(p);
            }
            let pressure = boardcraft_core::pen::apply_curve(app.pen_pressure.unwrap_or(1.0), app.tools.pen.pressure_curve);
            if resp.drag_started()
                && let Some(pos) = resp.interact_pointer_pos()
            {
                match app.active_layer.filter(|l| app.project.panel(panel_id).is_some_and(|p| p.layer(*l).is_some_and(|l| l.visible))) {
                    Some(layer) => {
                        let (x, y) = to_canvas(pos);
                        let pt = StrokePoint::new(x, y, pressure);
                        let brush = crate::tools::stroke_brush(app);
                        let mut stabilizer = boardcraft_core::pen::Stabilizer::new(app.tools.pen.stabilizer);
                        stabilizer.push(pt);
                        app.tools.stabilizer = Some(stabilizer);
                        // Shift: a straight line from where the last stroke on this layer ended.
                        let from = app.last_point.filter(|(p, l, _)| ui.input(|i| i.modifiers.shift) && *p == panel_id && *l == layer).map(|(_, _, q)| q);
                        match from {
                            Some(q) => draw_segment(app, panel_id, layer, q, pt, &brush),
                            None => {
                                if let Some(p) = app.project.panel_mut(panel_id) {
                                    if let Some(l) = p.layer_mut(layer) {
                                        l.pixels.dab(pt, &brush);
                                    }
                                    p.touch();
                                }
                                patch(app, panel_id, pt, pt, &brush);
                            }
                        }
                        app.stroke = Some(StrokeState { panel: panel_id, layer, last: pt });
                        app.edited();
                    }
                    None => app.status = tl!("Select a visible layer to draw on").into(),
                }
            } else if resp.dragged()
                && let Some(pos) = resp.interact_pointer_pos()
                && let Some(stroke) = app.stroke.as_mut()
            {
                let (x, y) = to_canvas(pos);
                let raw = StrokePoint::new(x, y, pressure);
                let pt = app.tools.stabilizer.as_mut().and_then(|s| s.push(raw)).unwrap_or(raw);
                let (pid, lid, last) = (stroke.panel, stroke.layer, stroke.last);
                stroke.last = pt;
                let brush = crate::tools::stroke_brush(app);
                draw_segment(app, pid, lid, last, pt, &brush);
            }
            if resp.drag_stopped() {
                // The smoothed stroke catches up with where the pen lifted.
                let tail = app.tools.stabilizer.take().map(|mut s| s.finish()).unwrap_or_default();
                if let Some(s) = app.stroke.as_mut() {
                    let (pid, lid, mut last) = (s.panel, s.layer, s.last);
                    let brush = crate::tools::stroke_brush(app);
                    for pt in tail {
                        draw_segment(app, pid, lid, last, pt, &brush);
                        last = pt;
                    }
                    if let Some(s) = app.stroke.as_mut() {
                        s.last = last;
                    }
                }
                if let Some(s) = app.stroke.take() {
                    app.last_point = Some((s.panel, s.layer, s.last));
                }
                app.pen_pressure = None;
            }
        }
        CanvasTool::Camera => {
            let drag = resp.drag_delta();
            let (scroll, shift) =
                if resp.hovered() { ui.input(|i| (i.smooth_scroll_delta.x + i.smooth_scroll_delta.y, i.modifiers.shift)) } else { (0.0, false) };
            if drag != egui::Vec2::ZERO || scroll != 0.0 {
                // The selected key, or a new one at the playhead.
                if let Some(mut key) = crate::camera_keys::edit_target(app, panel_id) {
                    let d = xf.delta_normalized(drag);
                    key.center[0] += d.x;
                    key.center[1] += d.y;
                    if shift {
                        key.rotation += scroll * 0.05;
                    } else {
                        key.zoom *= (scroll * 0.003).exp();
                    }
                    crate::camera_keys::set_selected(app, panel_id, key);
                }
            }
        }
        CanvasTool::Lasso | CanvasTool::RectSelect => crate::tools::selection::canvas_input(app, &resp, &xf),
        CanvasTool::Fill => crate::tools::fill_input(app, &resp, &xf),
        CanvasTool::Select | CanvasTool::Pen | CanvasTool::Arrow | CanvasTool::MotionArrow | CanvasTool::CameraArrow => {
            crate::annotations::canvas_input(app, &resp, panel_id, &to_canvas_arr, k)
        }
    }
    crate::tools::selection::draw_overlay(app, &painter, &xf, panel_id);
}

fn draw_segment(app: &mut BoardApp, panel: Id, layer: Id, from: StrokePoint, to: StrokePoint, brush: &boardcraft_core::Brush) {
    if let Some(p) = app.project.panel_mut(panel) {
        if let Some(l) = p.layer_mut(layer) {
            l.pixels.stroke(from, to, brush);
        }
        p.touch();
    }
    patch(app, panel, from, to, brush);
}

/// Update only the touched part of the panel's texture (no full re-upload per dab).
fn patch(app: &mut BoardApp, panel: Id, a: StrokePoint, b: StrokePoint, brush: &boardcraft_core::Brush) {
    let r = brush.radius.max(1.0) + 3.0;
    let fin = |v: f32| if v.is_finite() { v } else { 0.0 };
    let x0 = (fin(a.x.min(b.x)) - r).max(0.0) as u32;
    let y0 = (fin(a.y.min(b.y)) - r).max(0.0) as u32;
    let x1 = (fin(a.x.max(b.x)) + r).max(0.0).ceil() as u32;
    let y1 = (fin(a.y.max(b.y)) + r).max(0.0).ceil() as u32;
    if let Some(p) = app.project.panel(panel) {
        app.textures.patch(&app.project, p, [x0, y0, x1, y1]);
    }
}
