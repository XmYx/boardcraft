//! Timeline strip: panels as blocks sized by duration, scene bands, playhead, transport.
//! Drag a block's right edge to change the panel's duration.

use boardcraft_core::Id;

use crate::app::BoardApp;

const SCENE_COLORS: [egui::Color32; 4] = [
    egui::Color32::from_rgb(70, 130, 200),
    egui::Color32::from_rgb(200, 140, 60),
    egui::Color32::from_rgb(90, 170, 110),
    egui::Color32::from_rgb(170, 90, 160),
];

pub fn show(app: &mut BoardApp, ui: &mut egui::Ui) {
    let fps = app.project.settings.fps.max(1) as f64;
    ui.horizontal(|ui| {
        if ui.button(if app.playing { "Pause" } else { "Play" }).on_hover_text("Play the animatic (Space)").clicked() {
            app.toggle_play();
        }
        if ui.button("To Start").on_hover_text("Go to start").clicked() {
            app.playhead = 0.0;
            if let Some((id, _)) = app.project.panel_at(0) {
                app.select(Some(id));
            }
        }
        let t = app.playhead / fps;
        let total = app.project.total_frames() as f64 / fps;
        ui.monospace(format!("{:02}:{:05.2} / {:02}:{:05.2}", (t / 60.0) as u32, t % 60.0, (total / 60.0) as u32, total % 60.0));
        ui.separator();
        let l = ui.label("Zoom");
        ui.add(egui::Slider::new(&mut app.timeline_ppf, 0.5..=12.0).logarithmic(true).show_value(false)).labelled_by(l.id);
    });
    let ppf = app.timeline_ppf;
    let total = app.project.total_frames();
    let height = (ui.available_height() - 4.0).max(60.0);
    let mut seek: Option<f64> = None;
    let mut select: Option<Id> = None;
    let mut resize: Option<(Id, u32)> = None;
    let live = app.stroke.is_none();
    let selected = app.selected;
    let playhead = app.playhead;
    let project = &app.project;
    let textures = &mut app.textures;
    egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
        let width = (total as f32 * ppf + 40.0).max(ui.available_width());
        let (rect, bg) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
        bg.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, "Animatic timeline"));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        let band = 14.0;
        let block_top = rect.top() + band + 2.0;
        let block_h = rect.height() - band - 4.0;
        let mut x = rect.left();
        for (si, scene) in project.scenes.iter().enumerate() {
            let scene_x = x;
            for panel in &scene.panels {
                let w = panel.duration.max(1) as f32 * ppf;
                let r = egui::Rect::from_min_size(egui::pos2(x, block_top), egui::vec2(w, block_h));
                let resp = ui.interact(r, ui.scope_id().with(("tl-panel", panel.id.0)), egui::Sense::click());
                let label = project.panel_label(panel.id);
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected == Some(panel.id), format!("Panel {label} on the timeline")));
                if resp.clicked() {
                    select = Some(panel.id);
                }
                // Thumbnail, letterboxed into the block.
                if ui.is_rect_visible(r) {
                    let tex = textures.thumb(ui.ctx(), project, panel, live);
                    let aspect = project.settings.width as f32 / project.settings.height.max(1) as f32;
                    let tw = (block_h * aspect).min(w - 2.0).max(0.0);
                    let th = tw / aspect;
                    if tw > 4.0 {
                        let ir = egui::Rect::from_min_size(egui::pos2(x + 1.0, block_top + (block_h - th) / 2.0), egui::vec2(tw, th));
                        painter.image(tex, ir, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                    }
                    let stroke = if selected == Some(panel.id) {
                        egui::Stroke::new(2.0, ui.visuals().selection.bg_fill)
                    } else {
                        egui::Stroke::new(1.0, ui.visuals().widgets.inactive.bg_stroke.color)
                    };
                    painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
                    if w > 30.0 {
                        painter.text(
                            r.left_bottom() + egui::vec2(4.0, -3.0),
                            egui::Align2::LEFT_BOTTOM,
                            &label,
                            egui::FontId::proportional(11.0),
                            egui::Color32::from_gray(30),
                        );
                    }
                }
                // Duration handle on the right edge.
                let edge = egui::Rect::from_min_max(egui::pos2(r.right() - 4.0, r.top()), egui::pos2(r.right() + 3.0, r.bottom()));
                let er =
                    ui.interact(edge, ui.scope_id().with(("tl-edge", panel.id.0)), egui::Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                if er.dragged()
                    && let Some(pos) = er.interact_pointer_pos()
                {
                    let frames = ((pos.x - r.left()) / ppf).round().max(1.0) as u32;
                    resize = Some((panel.id, frames));
                }
                x += w;
            }
            let color = SCENE_COLORS[si % SCENE_COLORS.len()];
            let band_rect = egui::Rect::from_min_max(egui::pos2(scene_x, rect.top()), egui::pos2(x, rect.top() + band));
            painter.rect_filled(band_rect, 0.0, color);
            painter.text(
                band_rect.left_center() + egui::vec2(4.0, 0.0),
                egui::Align2::LEFT_CENTER,
                format!("{} {}", scene.number, scene.heading),
                egui::FontId::proportional(10.0),
                egui::Color32::WHITE,
            );
        }
        let px = rect.left() + playhead as f32 * ppf;
        painter.line_segment([egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())], egui::Stroke::new(2.0, egui::Color32::from_rgb(230, 60, 50)));
        if (bg.clicked() || bg.dragged())
            && let Some(pos) = bg.interact_pointer_pos()
        {
            seek = Some((((pos.x - rect.left()) / ppf) as f64).clamp(0.0, (total.max(1) - 1) as f64));
        }
    });
    if let Some((id, frames)) = resize
        && let Some(p) = app.project.panel_mut(id)
        && p.duration != frames
    {
        p.duration = frames;
        app.dirty = true;
    }
    if let Some(id) = select {
        app.select_and_seek(id);
    }
    if let Some(t) = seek {
        app.playhead = t;
        if let Some((id, _)) = app.project.panel_at(t as u64) {
            app.select(Some(id));
        }
    }
}
