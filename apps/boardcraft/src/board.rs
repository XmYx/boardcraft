//! Board view: every panel as a thumbnail card, grouped by scene.

use boardcraft_core::Id;

use crate::app::{BoardApp, View};

const CARD_W: f32 = 232.0;
/// Camera badge ink: 5.9:1 on panel paper.
pub const CAM_INK: egui::Color32 = egui::Color32::from_rgb(170, 60, 20);

enum Action {
    Select(Id),
    Open(Id),
    AddPanel(Id),
}

fn snippet(s: &str, max: usize) -> String {
    let one = s.replace('\n', " ");
    if one.chars().count() <= max { one } else { format!("{}…", one.chars().take(max).collect::<String>()) }
}

pub fn show(app: &mut BoardApp, ui: &mut egui::Ui) {
    let mut action = None;
    let fps = app.project.settings.fps.max(1) as f32;
    let aspect = app.project.settings.height as f32 / app.project.settings.width.max(1) as f32;
    let live = app.stroke.is_none();
    let selected = app.selected;
    let focus_selected = std::mem::take(&mut app.focus_selected);
    let mut card_focused = false;
    let project = &app.project;
    let textures = &mut app.textures;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for scene in &project.scenes {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(tlf!("Scene {number}", number = scene.number));
                ui.label(egui::RichText::new(&scene.heading).strong());
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
                for panel in &scene.panels {
                    let label = project.panel_label(panel.id);
                    let thumb_h = CARD_W * aspect;
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(CARD_W, thumb_h + 48.0), egui::Sense::click());
                    let resp = resp.on_hover_text(if panel.action.is_empty() { &panel.dialogue } else { &panel.action });
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected == Some(panel.id), tlf!("Panel {label}", label = label)));
                    card_focused |= resp.has_focus();
                    if focus_selected && selected == Some(panel.id) {
                        resp.request_focus();
                        resp.scroll_to_me(None);
                    }
                    let enter = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if enter && selected == Some(panel.id) {
                        // Enter on the selected card opens it, as a double-click does.
                        action = Some(Action::Open(panel.id));
                    } else if resp.clicked() || (resp.gained_focus() && selected != Some(panel.id)) {
                        action = Some(Action::Select(panel.id));
                    }
                    if resp.double_clicked() {
                        action = Some(Action::Open(panel.id));
                    }
                    if ui.is_rect_visible(rect) {
                        let tex = textures.thumb(ui.ctx(), project, panel, live);
                        let img_rect = egui::Rect::from_min_size(rect.min, egui::vec2(CARD_W, thumb_h));
                        let p = ui.painter();
                        p.image(tex, img_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                        let is_sel = selected == Some(panel.id);
                        let stroke = if is_sel {
                            egui::Stroke::new(3.0, ui.visuals().selection.bg_fill)
                        } else {
                            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
                        };
                        p.rect_stroke(img_rect, 2.0, stroke, egui::StrokeKind::Outside);
                        if panel.camera.moves() {
                            p.text(
                                img_rect.right_top() + egui::vec2(-6.0, 4.0),
                                egui::Align2::RIGHT_TOP,
                                tl!("CAM"),
                                egui::FontId::proportional(11.0),
                                CAM_INK,
                            );
                        }
                        let text = ui.visuals().text_color();
                        let weak = ui.visuals().weak_text_color();
                        p.text(
                            egui::pos2(rect.left(), img_rect.bottom() + 4.0),
                            egui::Align2::LEFT_TOP,
                            format!("{label}   {:.1}s", panel.duration as f32 / fps),
                            egui::FontId::proportional(13.0),
                            text,
                        );
                        let line = if !panel.dialogue.is_empty() { snippet(&panel.dialogue, 34) } else { snippet(&panel.action, 34) };
                        p.text(egui::pos2(rect.left(), img_rect.bottom() + 22.0), egui::Align2::LEFT_TOP, line, egui::FontId::proportional(12.0), weak);
                    }
                }
                if ui.add_sized([60.0, CARD_W * aspect], egui::Button::new(tl!("+ Panel"))).on_hover_text(tl!("New panel at the end of this scene")).clicked() {
                    action = Some(Action::AddPanel(scene.id));
                }
            });
            ui.separator();
        }
    });
    // Arrow keys on a focused card step through the panels (focus follows the selection).
    if card_focused {
        let (l, r) = ui.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight)));
        if l || r {
            app.step_selection(if r { 1 } else { -1 });
        }
    }
    match action {
        Some(Action::Select(id)) => app.select_and_seek(id),
        Some(Action::Open(id)) => {
            app.select_and_seek(id);
            app.view = View::Panel;
        }
        Some(Action::AddPanel(scene)) => {
            let last = app.project.scene(scene).and_then(|s| s.panels.last()).map(|p| p.id);
            if let Ok(id) = app.project.add_panel(scene, last) {
                app.select_and_seek(id);
                app.dirty = true;
            }
        }
        None => {}
    }
}
