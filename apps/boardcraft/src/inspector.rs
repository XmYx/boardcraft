//! Inspector: the selected panel's scene, text, duration, layers and camera keys.

use boardcraft_core::Id;

use crate::app::BoardApp;

enum LayerAction {
    Add,
    Delete(Id),
    Up(Id),
    Down(Id),
    Clear(Id),
}

fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String, rows: usize) -> bool {
    let l = ui.label(label);
    ui.add(egui::TextEdit::multiline(value).desired_rows(rows).desired_width(f32::INFINITY)).labelled_by(l.id).changed()
}

pub fn show(app: &mut BoardApp, ui: &mut egui::Ui) {
    let Some(id) = app.selected else {
        ui.label("Select a panel.");
        return;
    };
    let label = app.project.panel_label(id);
    let fps = app.project.settings.fps.max(1);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.heading(format!("Panel {label}"));
        let mut changed = false;
        if let Some(pos) = app.project.find_panel(id)
            && let Some(scene) = app.project.scenes.get_mut(pos.scene)
        {
            ui.horizontal(|ui| {
                let l = ui.label("Scene");
                changed |= ui.add(egui::TextEdit::singleline(&mut scene.number).desired_width(40.0)).labelled_by(l.id).changed();
            });
            let l = ui.label("Heading");
            changed |= ui.add(egui::TextEdit::singleline(&mut scene.heading).desired_width(f32::INFINITY)).labelled_by(l.id).changed();
        }
        ui.separator();
        let mut layer_action = None;
        let mut active = app.active_layer;
        if let Some(panel) = app.project.panel_mut(id) {
            ui.horizontal(|ui| {
                let l = ui.label("Duration");
                changed |= ui
                    .add(egui::DragValue::new(&mut panel.duration).range(1..=boardcraft_core::model::MAX_PANEL_FRAMES).suffix(" fr"))
                    .labelled_by(l.id)
                    .changed();
                ui.label(format!("= {:.2} s at {fps} fps", panel.duration as f32 / fps as f32));
            });
            changed |= text_field(ui, "Dialogue", &mut panel.dialogue, 2);
            changed |= text_field(ui, "Action", &mut panel.action, 3);
            changed |= text_field(ui, "Notes", &mut panel.notes, 2);
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong("Layers");
                if ui.button("+ Layer").clicked() {
                    layer_action = Some(LayerAction::Add);
                }
            });
            let mut touched = false;
            // Top layer first, as in every paint app.
            for layer in panel.layers.iter_mut().rev() {
                ui.horizontal(|ui| {
                    touched |= ui.checkbox(&mut layer.visible, "").on_hover_text("Visible").changed();
                    if ui.selectable_label(active == Some(layer.id), &layer.name).clicked() {
                        active = Some(layer.id);
                    }
                    touched |= ui.add(egui::Slider::new(&mut layer.opacity, 0.0..=1.0).show_value(false)).on_hover_text("Opacity").changed();
                    ui.menu_button("…", |ui| {
                        if ui.button("Move Up").clicked() {
                            layer_action = Some(LayerAction::Up(layer.id));
                        }
                        if ui.button("Move Down").clicked() {
                            layer_action = Some(LayerAction::Down(layer.id));
                        }
                        if ui.button("Clear").clicked() {
                            layer_action = Some(LayerAction::Clear(layer.id));
                        }
                        if ui.button("Delete").clicked() {
                            layer_action = Some(LayerAction::Delete(layer.id));
                        }
                    });
                });
            }
            if touched {
                panel.touch();
                changed = true;
            }
            ui.separator();
            ui.strong("Camera moves");
            if panel.camera.keys.is_empty() {
                ui.label("Static: the whole canvas. Use the Camera tool (C) in the Panel view to frame a move.");
            }
            let mut remove = None;
            for k in &panel.camera.keys {
                ui.horizontal(|ui| {
                    ui.monospace(format!("f{:<4} x {:.2} y {:.2} zoom {:.2}", k.frame, k.center[0], k.center[1], k.zoom));
                    if ui.small_button("Delete").on_hover_text("Delete key").clicked() {
                        remove = Some(k.frame);
                    }
                });
            }
            if let Some(f) = remove {
                panel.camera.remove_key(f);
                changed = true;
            }
            if !panel.camera.keys.is_empty() && ui.button("Reset camera").clicked() {
                panel.camera.keys.clear();
                changed = true;
            }
        }
        app.active_layer = active;
        if changed {
            app.dirty = true;
        }
        if let Some(a) = layer_action {
            apply_layer_action(app, id, a);
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Import Image…").clicked() {
                app.import_image();
            }
            if ui.button("AI panel sketch").on_hover_text("Generate a sketch for this panel with the shared Craft AI tools (not connected yet)").clicked() {
                app.ai_sketch();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Move Earlier").clicked() {
                app.move_panel(false);
            }
            if ui.button("Move Later").clicked() {
                app.move_panel(true);
            }
            if ui.button("Delete Panel").clicked() {
                app.delete_panel();
            }
        });
    });
}

fn apply_layer_action(app: &mut BoardApp, panel_id: Id, action: LayerAction) {
    match action {
        LayerAction::Add => {
            let layer = app.project.new_layer("Layer");
            let lid = layer.id;
            if let Some(p) = app.project.panel_mut(panel_id) {
                p.layers.push(layer);
            }
            app.active_layer = Some(lid);
        }
        LayerAction::Clear(lid) => {
            app.push_undo(panel_id, lid);
            let (w, h) = (app.project.settings.width, app.project.settings.height);
            if let Some(l) = app.project.panel_mut(panel_id).and_then(|p| p.layer_mut(lid)) {
                l.pixels = boardcraft_core::Raster::new(w, h);
            }
        }
        LayerAction::Delete(lid) => {
            if let Some(p) = app.project.panel_mut(panel_id) {
                if p.layers.len() > 1 {
                    p.layers.retain(|l| l.id != lid);
                } else {
                    app.status = "A panel keeps at least one layer".into();
                }
            }
            app.undo.retain(|u| u.layer != lid);
            app.redo.retain(|u| u.layer != lid);
            if app.active_layer == Some(lid) {
                app.active_layer = app.project.panel(panel_id).and_then(|p| p.layers.last()).map(|l| l.id);
            }
        }
        LayerAction::Up(lid) | LayerAction::Down(lid) => {
            let up = matches!(action, LayerAction::Up(_));
            if let Some(p) = app.project.panel_mut(panel_id)
                && let Some(i) = p.layers.iter().position(|l| l.id == lid)
            {
                let j = if up { i + 1 } else { i.wrapping_sub(1) };
                if j < p.layers.len() {
                    p.layers.swap(i, j);
                }
            }
        }
    }
    if let Some(p) = app.project.panel_mut(panel_id) {
        p.touch();
    }
    app.dirty = true;
}
