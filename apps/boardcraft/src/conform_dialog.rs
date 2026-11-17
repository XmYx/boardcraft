//! Conform from an edit: after the user picks an edited timeline, a dialog shows what conforming
//! would change (computed on a copy of the board) with the options to also take the edit's order
//! and to mark panels the edit left out. Apply runs the `conform` command as one undo step.

use boardcraft_core::command::Command;
use boardcraft_core::timeline::{self, ConformOptions, ConformReport};

use crate::app::BoardApp;

/// Most unused panel labels listed by name.
const MAX_LISTED: usize = 12;

pub struct Dialog {
    /// The timeline's file name and text.
    pub name: String,
    pub text: String,
    pub options: ConformOptions,
    /// What the conform would do with `options`, recomputed when they change.
    preview: Option<(ConformOptions, Result<ConformReport, String>)>,
    focus: bool,
}

impl Dialog {
    pub fn new(name: String, text: String) -> Self {
        Self { name, text, options: ConformOptions::default(), preview: None, focus: true }
    }

    fn report(&mut self, app: &BoardApp) -> Result<ConformReport, String> {
        if let Some((o, r)) = &self.preview
            && *o == self.options
        {
            return r.clone();
        }
        // A copy shares every layer's pixels, so trying the conform costs little.
        let mut copy = app.project.clone();
        let r = timeline::conform_with(&mut copy, std::path::Path::new(&self.name), self.text.as_bytes(), &self.options).map_err(|e| e.to_string());
        self.preview = Some((self.options, r.clone()));
        r
    }
}

/// The summary lines for `r`.
fn summary(r: &ConformReport, options: &ConformOptions) -> Vec<String> {
    let mut lines = vec![
        tlf!("{n} clips matched to panels", n = r.matched),
        tlf!("{n} panels get a new duration", n = r.retimed),
        tlf!("{n} clips are not from this board", n = r.unmatched_clips),
    ];
    if r.unused_panels.is_empty() {
        lines.push(tl!("The edit uses every panel").into());
    } else {
        let mut names = r.unused_panels.iter().take(MAX_LISTED).cloned().collect::<Vec<_>>().join(", ");
        if r.unused_panels.len() > MAX_LISTED {
            names.push_str(", …");
        }
        lines.push(tlf!("{n} panels are not in the edit: {panels}", n = r.unused_panels.len(), panels = names));
    }
    if options.reorder {
        lines.push(tlf!("{n} panels change place, {m} of them to another scene", n = r.moved, m = r.changed_scene));
        if r.removed_scenes > 0 {
            lines.push(tlf!("{n} scenes are left empty and removed", n = r.removed_scenes));
        }
    }
    if options.mark_missing {
        lines.push(tlf!("{n} panels get the note “{mark}”, {m} lose it", n = r.marked, m = r.unmarked, mark = timeline::NOT_IN_EDIT));
    }
    lines
}

pub fn show(app: &mut BoardApp, ctx: &egui::Context) {
    let Some(mut d) = app.conform_dialog.take() else { return };
    let mut open = true;
    let mut apply = false;
    let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let report = d.report(app);
    let file = std::path::Path::new(&d.name).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    egui::Window::new(tl!("Conform to the Edit")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).open(&mut open).show(
        ctx,
        |ui| {
            ui.label(tlf!("Edit: {file}", file = file));
            let reorder = ui.checkbox(&mut d.options.reorder, tl!("Put panels in the edit's order"));
            if d.focus {
                reorder.request_focus();
                d.focus = false;
            }
            ui.label(
                egui::RichText::new(tl!(
                    "Panels follow the edit inside their scenes; a panel cut between two panels of another scene joins that scene. Panels the edit leaves out stay after the panel they followed."
                ))
                .weak(),
            );
            ui.checkbox(&mut d.options.mark_missing, tl!("Mark panels the edit leaves out (nothing is deleted)"));
            ui.separator();
            ui.strong(tl!("What changes"));
            match &report {
                Ok(r) => {
                    for line in summary(r, &d.options) {
                        ui.label(line);
                    }
                }
                Err(e) => {
                    ui.label(tlf!("Could not read the edit: {error}", error = e));
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_enabled(report.is_ok(), egui::Button::new(tl!("Apply"))).clicked() {
                    apply = true;
                }
                if ui.button(tl!("Cancel")).clicked() {
                    cancel = true;
                }
            });
        },
    );
    if apply {
        let cmd =
            Command::Conform { name: d.name.clone(), text: std::mem::take(&mut d.text), reorder: d.options.reorder, mark_missing: d.options.mark_missing };
        if let Some(out) = app.run(cmd) {
            app.status = tlf!("Conformed to {file}: {summary}", file = file, summary = out.message);
            let keep = app.selected.filter(|id| app.project.panel(*id).is_some());
            app.select(keep.or_else(|| app.project.panels().next().map(|(_, p)| p.id)));
            app.textures.retain(&app.project);
        }
        return;
    }
    if open && !cancel {
        app.conform_dialog = Some(d);
    }
}
