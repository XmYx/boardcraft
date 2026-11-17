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
        ui.label(tl!("Select a panel."));
        return;
    };
    let label = app.project.panel_label(id);
    let fps = app.project.settings.fps.max(1);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.heading(tlf!("Panel {label}", label = label));
        let mut changed = false;
        if let Some(pos) = app.project.find_panel(id)
            && let Some(scene) = app.project.scenes.get_mut(pos.scene)
        {
            ui.horizontal(|ui| {
                let l = ui.label(tl!("Scene"));
                changed |= ui.add(egui::TextEdit::singleline(&mut scene.number).desired_width(40.0)).labelled_by(l.id).changed();
            });
            let l = ui.label(tl!("Heading"));
            changed |= ui.add(egui::TextEdit::singleline(&mut scene.heading).desired_width(f32::INFINITY)).labelled_by(l.id).changed();
        }
        ui.separator();
        let mut layer_action = None;
        let mut active = app.active_layer;
        if let Some(panel) = app.project.panel_mut(id) {
            ui.horizontal(|ui| {
                let l = ui.label(tl!("Duration"));
                changed |= ui
                    .add(egui::DragValue::new(&mut panel.duration).range(1..=boardcraft_core::model::MAX_PANEL_FRAMES).suffix(" fr"))
                    .labelled_by(l.id)
                    .changed();
                ui.label(tlf!("= {seconds} s at {fps} fps", seconds = format!("{:.2}", panel.duration as f32 / fps as f32), fps = fps));
            });
            changed |= text_field(ui, tl!("Dialogue"), &mut panel.dialogue, 2);
            changed |= text_field(ui, tl!("Action"), &mut panel.action, 3);
            changed |= text_field(ui, tl!("Notes"), &mut panel.notes, 2);
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong(tl!("Layers"));
                if ui.button(tl!("+ Layer")).clicked() {
                    layer_action = Some(LayerAction::Add);
                }
            });
            let mut touched = false;
            // Top layer first, as in every paint app.
            for layer in panel.layers.iter_mut().rev() {
                ui.horizontal(|ui| {
                    touched |= ui.checkbox(&mut layer.visible, "").on_hover_text(tlf!("{layer} visible", layer = layer.name)).changed();
                    if ui.selectable_label(active == Some(layer.id), &layer.name).clicked() {
                        active = Some(layer.id);
                    }
                    touched |= ui
                        .add(egui::Slider::new(&mut layer.opacity, 0.0..=1.0).show_value(false))
                        .on_hover_text(tlf!("{layer} opacity", layer = layer.name))
                        .changed();
                    ui.menu_button(tlf!("{layer} options", layer = layer.name), |ui| {
                        if ui.button(tl!("Move Up")).clicked() {
                            layer_action = Some(LayerAction::Up(layer.id));
                        }
                        if ui.button(tl!("Move Down")).clicked() {
                            layer_action = Some(LayerAction::Down(layer.id));
                        }
                        if ui.button(tl!("Clear")).clicked() {
                            layer_action = Some(LayerAction::Clear(layer.id));
                        }
                        if ui.button(tl!("Delete")).clicked() {
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
            ui.strong(tl!("Camera moves"));
            if panel.camera.keys.is_empty() {
                ui.label(tl!("Static: the whole canvas. Add keys below, or use the Camera tool (C) in the Panel view."));
            }
            // Keyboard / typed alternative to the Camera tool's drag and scroll.
            let mut remove = None;
            let mut edited = None;
            for k in &panel.camera.keys {
                let mut key = *k;
                ui.horizontal(|ui| {
                    ui.monospace(tlf!("frame {f}", f = k.frame));
                    let mut c = false;
                    let l = ui.label("x");
                    c |= ui.add(egui::DragValue::new(&mut key.center[0]).range(0.0..=1.0).speed(0.005).max_decimals(3)).labelled_by(l.id).changed();
                    let l = ui.label("y");
                    c |= ui.add(egui::DragValue::new(&mut key.center[1]).range(0.0..=1.0).speed(0.005).max_decimals(3)).labelled_by(l.id).changed();
                    let l = ui.label(tl!("zoom"));
                    c |= ui
                        .add(egui::DragValue::new(&mut key.zoom).range(1.0..=boardcraft_core::camera::MAX_ZOOM).speed(0.01).max_decimals(2))
                        .labelled_by(l.id)
                        .changed();
                    if c {
                        edited = Some(key);
                    }
                    if ui.small_button(tlf!("Delete key {f}", f = k.frame)).on_hover_text(tlf!("Delete the camera key at frame {f}", f = k.frame)).clicked() {
                        remove = Some(k.frame);
                    }
                });
            }
            if let Some(k) = edited {
                panel.camera.set_key(k);
                changed = true;
            }
            ui.horizontal(|ui| {
                let end = panel.duration.max(1) - 1;
                for (label, frame) in [(tl!("Key at start"), 0), (tl!("Key at end"), end)] {
                    if ui.button(label).clicked() {
                        let at = panel.camera.at(frame as f32);
                        panel.camera.set_key(boardcraft_core::CameraKey { frame, ..at });
                        changed = true;
                    }
                }
            });
            if let Some(f) = remove {
                panel.camera.remove_key(f);
                changed = true;
            }
            if !panel.camera.keys.is_empty() && ui.button(tl!("Reset camera")).clicked() {
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
            if ui.button(tl!("Import Image…")).clicked() {
                app.import_image();
            }
            if ui
                .button(tl!("AI panel sketch"))
                .on_hover_text(tl!("Generate a sketch for this panel with the shared Craft AI tools (not connected yet)"))
                .clicked()
            {
                app.ai_sketch();
            }
        });
        ui.horizontal(|ui| {
            if ui.button(tl!("Move Earlier")).clicked() {
                app.move_panel(false);
            }
            if ui.button(tl!("Move Later")).clicked() {
                app.move_panel(true);
            }
            if ui.button(tl!("Delete Panel")).clicked() {
                app.delete_panel();
            }
        });
    });
}

fn apply_layer_action(app: &mut BoardApp, panel_id: Id, action: LayerAction) {
    match action {
        LayerAction::Add => {
            let layer = app.project.new_layer(tl!("Layer"));
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
                    app.status = tl!("A panel keeps at least one layer").into();
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
