//! Export Board Sheet (PDF): paper, grid, which fields print, a header line and ruled note lines
//! (`boardcraft_core::sheet::SheetOptions`). The choices are kept for the next export this session.

use std::path::PathBuf;

use boardcraft_core::sheet::{self, Paper, SheetOptions};

use crate::app::BoardApp;

pub struct Dialog {
    pub options: SheetOptions,
    /// Write here without asking (set by tests); `None` asks for a file on Export.
    pub path: Option<PathBuf>,
    /// Move focus into the dialog on its first frame.
    focus: bool,
}

impl Dialog {
    pub fn new(app: &BoardApp) -> Self {
        Self { options: app.sheet_options.clone(), path: None, focus: true }
    }
}

/// The sheet's printed words in the interface language.
pub fn labels() -> sheet::SheetLabels {
    sheet::SheetLabels {
        dialogue: tl!("Dialogue").into(),
        action: tl!("Action").into(),
        notes: tl!("Notes").into(),
        page_of: tl!("Page {n} of {total}").into(),
        seconds: tl!("s").into(),
    }
}

pub fn show(app: &mut BoardApp, ctx: &egui::Context) {
    let Some(mut d) = app.sheet_dialog.take() else { return };
    let mut open = true;
    let mut export = false;
    let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let pages = sheet::page_count(&app.project, &d.options);
    egui::Window::new(tl!("Export Board Sheet")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).open(&mut open).show(
        ctx,
        |ui| {
            let o = &mut d.options;
            ui.horizontal(|ui| {
                ui.label(tl!("Paper"));
                let a4 = ui.radio_value(&mut o.paper, Paper::A4, "A4");
                if d.focus {
                    a4.request_focus();
                    d.focus = false;
                }
                ui.radio_value(&mut o.paper, Paper::Letter, tl!("Letter"));
            });
            ui.horizontal(|ui| {
                let l = ui.label(tl!("Columns"));
                ui.add(egui::DragValue::new(&mut o.columns).range(1..=6)).labelled_by(l.id);
                let l = ui.label(tl!("Rows"));
                ui.add(egui::DragValue::new(&mut o.rows).range(1..=6)).labelled_by(l.id);
            });
            ui.label(egui::RichText::new(tlf!("{n} panels a page, {pages} pages", n = o.columns * o.rows, pages = pages)).weak());
            ui.separator();
            ui.strong(tl!("Print under each panel"));
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut o.fields.shot, tl!("Shot"));
                ui.checkbox(&mut o.fields.dialogue, tl!("Dialogue"));
                ui.checkbox(&mut o.fields.action, tl!("Action"));
                ui.checkbox(&mut o.fields.notes, tl!("Notes"));
            });
            ui.checkbox(&mut o.note_lines, tl!("Ruled lines for handwritten notes"));
            ui.horizontal(|ui| {
                let l = ui.label(tl!("Header text"));
                ui.add(egui::TextEdit::singleline(&mut o.header).char_limit(sheet::MAX_HEADER).hint_text(tl!("Production, episode, draft"))).labelled_by(l.id);
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(tl!("Export…")).clicked() {
                    export = true;
                }
                if ui.button(tl!("Cancel")).clicked() {
                    cancel = true;
                }
            });
        },
    );
    app.sheet_options = d.options.clone();
    if export {
        let opts = SheetOptions { labels: labels(), ..d.options.clone() };
        match d.path.take() {
            Some(path) => app.export_sheet_to(&path, &opts),
            None => app.export_sheet(&opts),
        }
        return;
    }
    if open && !cancel {
        app.sheet_dialog = Some(d);
    }
}
