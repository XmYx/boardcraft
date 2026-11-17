//! Board Settings: canvas size and aspect ratio, frame rate. Changing either after work has begun
//! is safe: the canvas grows or crops around the centre without stretching drawings, and a new
//! frame rate keeps every panel's length in seconds unless asked not to.

use boardcraft_core::Project;
use boardcraft_core::command::Command;

use crate::app::BoardApp;

/// Aspect ratios offered (generic formats).
const ASPECTS: [(&str, u32, u32); 7] =
    [("16:9", 16, 9), ("1.85:1", 185, 100), ("2.39:1", 239, 100), ("4:3", 4, 3), ("1:1", 1, 1), ("9:16", 9, 16), ("4:5", 4, 5)];

/// Frame rates offered: label, whole rate, NTSC (×1000/1001).
const RATES: [(&str, u32, bool); 9] = [
    ("23.976", 24, true),
    ("24", 24, false),
    ("25", 25, false),
    ("29.97", 30, true),
    ("30", 30, false),
    ("48", 48, false),
    ("50", 50, false),
    ("59.94", 60, true),
    ("60", 60, false),
];

pub struct Dialog {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub ntsc: bool,
    pub keep_time: bool,
}

impl Dialog {
    pub fn new(p: &Project) -> Self {
        Self { width: p.settings.width, height: p.settings.height, fps: p.settings.fps, ntsc: p.settings.ntsc, keep_time: true }
    }
}

fn even(v: f32) -> u32 {
    ((v / 2.0).round() as u32 * 2).clamp(16, 8192)
}

pub fn show(app: &mut BoardApp, ctx: &egui::Context) {
    let Some(mut d) = app.settings_dialog.take() else { return };
    let mut open = true;
    let mut apply = false;
    let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    egui::Window::new(tl!("Board Settings")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).open(&mut open).show(
        ctx,
        |ui| {
            ui.strong(tl!("Frame"));
            ui.horizontal_wrapped(|ui| {
                ui.label(tl!("Aspect ratio"));
                for (name, w, h) in ASPECTS {
                    if ui.button(name).on_hover_text(tlf!("Set the frame to {ratio}, keeping its width", ratio = name)).clicked() {
                        d.height = even(d.width as f32 * h as f32 / w as f32);
                    }
                }
            });
            ui.horizontal(|ui| {
                let l = ui.label(tl!("Width"));
                ui.add(egui::DragValue::new(&mut d.width).range(16..=8192).suffix(" px")).labelled_by(l.id);
                let l = ui.label(tl!("Height"));
                ui.add(egui::DragValue::new(&mut d.height).range(16..=8192).suffix(" px")).labelled_by(l.id);
            });
            ui.label(egui::RichText::new(tl!("Drawings keep their size: the frame grows or crops around the centre.")).weak());
            ui.separator();
            ui.strong(tl!("Timing"));
            ui.horizontal_wrapped(|ui| {
                ui.label(tl!("Frame rate"));
                for (name, fps, ntsc) in RATES {
                    if ui.radio((d.fps, d.ntsc) == (fps, ntsc), name).clicked() {
                        d.fps = fps;
                        d.ntsc = ntsc;
                    }
                }
            });
            ui.checkbox(&mut d.keep_time, tl!("Keep panel lengths in seconds"));
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(tl!("Apply")).clicked() {
                    apply = true;
                }
                if ui.button(tl!("Cancel")).clicked() {
                    cancel = true;
                }
            });
        },
    );
    if apply {
        let s = app.project.settings;
        if (d.width, d.height) != (s.width, s.height) {
            app.run(Command::SetCanvas { width: d.width, height: d.height });
            app.textures.clear();
        }
        if (d.fps, d.ntsc) != (s.fps, s.ntsc) {
            app.run(Command::SetFrameRate { fps: d.fps, ntsc: d.ntsc, keep_time: d.keep_time });
        }
        app.status =
            tlf!("Board is {w}×{h} at {fps} fps", w = app.project.settings.width, h = app.project.settings.height, fps = app.project.settings.rate_label());
        return;
    }
    if open && !cancel {
        app.settings_dialog = Some(d);
    }
}
