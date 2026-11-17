//! Drawing tools beyond the brush: pen tuning (pressure curve, minimum size, stabilizer),
//! selections (lasso, rectangle; move, scale, rotate, cut/copy/paste/delete), bucket fill and the
//! view rotation. The pixel work is in `boardcraft_core::{pen, select, fill}`; this module holds the
//! UI state, the keyboard shortcuts and the toolbar and menu entries.

pub mod pen;
pub mod selection;
pub mod view;

#[cfg(test)]
mod ui_tests;

use boardcraft_core::command::Command;
use boardcraft_core::pen::Stabilizer;

use crate::app::{BoardApp, CanvasTool, View};

/// One step of the view rotation keys, in degrees.
pub const ROTATE_STEP: f32 = 15.0;

/// What the area for a fill is found on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillSample {
    Layer,
    AllLayers,
}

/// Tool state and user preferences the app keeps.
pub struct ToolState {
    /// Pen preferences (persisted in `config.toml`).
    pub pen: pen::PenPrefs,
    prefs: pen::PrefsStore,
    /// Smoothing for the stroke in progress.
    pub stabilizer: Option<Stabilizer>,
    pub fill_tolerance: u8,
    pub fill_sample: FillSample,
    pub fill_gap: u32,
    /// View rotation in degrees (display only; pixels never change).
    pub view_angle: f32,
    pub sel: selection::State,
    /// The canvas view of the last frame (for mapping pointer positions in tests).
    pub last_view: Option<view::ViewXf>,
}

impl ToolState {
    pub fn new(data_dir: Option<&std::path::Path>, memory: bool) -> Self {
        let (prefs, pen) = pen::PrefsStore::open(data_dir, memory);
        Self {
            pen,
            prefs,
            stabilizer: None,
            fill_tolerance: 32,
            fill_sample: FillSample::AllLayers,
            fill_gap: 0,
            view_angle: 0.0,
            sel: selection::State::default(),
            last_view: None,
        }
    }

    /// Save the preferences now (on exit).
    pub fn flush(&mut self) {
        self.prefs.flush();
    }
}

/// The brush for a stroke, with the pen's minimum size.
pub fn stroke_brush(app: &BoardApp) -> boardcraft_core::Brush {
    boardcraft_core::Brush { min_size: app.tools.pen.min_size, ..app.active_brush() }
}

/// Per frame, before the UI: selection upkeep and saving preferences.
pub fn tick(app: &mut BoardApp) {
    selection::tick(app);
    let pen = app.tools.pen;
    if let Some(e) = app.tools.prefs.sync(&pen) {
        app.status = tlf!("Could not save preferences: {error}", error = e);
    }
}

/// Rotate the view by `degrees` (keys, menu, R+drag).
pub fn rotate_view(app: &mut BoardApp, degrees: f32) {
    let a = (app.tools.view_angle + degrees).rem_euclid(360.0);
    app.tools.view_angle = if a.is_finite() { a } else { 0.0 };
}

fn announce_rotation(app: &mut BoardApp) {
    app.status =
        if app.tools.view_angle == 0.0 { tl!("View rotation reset").into() } else { tlf!("View rotated to {angle}°", angle = app.tools.view_angle.round()) };
}

/// Shift with `[`, `]` or `0`, by the key's name or its physical key (layouts differ in what Shift
/// types there).
fn shifted(i: &egui::InputState, keys: &[egui::Key]) -> bool {
    i.events.iter().any(|e| match e {
        egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } => {
            modifiers.shift && !modifiers.command && !modifiers.alt && (keys.contains(key) || physical_key.is_some_and(|p| keys.contains(&p)))
        }
        _ => false,
    })
}

/// Keyboard shortcuts of the drawing tools. Runs before the app's own, so it can take the arrow
/// keys, Enter and Esc while a selection is active.
pub fn shortcuts(app: &mut BoardApp, ctx: &egui::Context) {
    use egui::{Key, KeyboardShortcut, Modifiers};
    let cmd = |k| KeyboardShortcut::new(Modifiers::COMMAND, k);
    // View rotation: Shift+[ and Shift+] turn, Shift+0 resets.
    let (left, right, reset) = ctx.input(|i| {
        (shifted(i, &[Key::OpenBracket, Key::OpenCurlyBracket]), shifted(i, &[Key::CloseBracket, Key::CloseCurlyBracket]), shifted(i, &[Key::Num0]))
    });
    if left || right {
        rotate_view(app, if left { -ROTATE_STEP } else { ROTATE_STEP });
        announce_rotation(app);
    }
    if reset {
        app.tools.view_angle = 0.0;
        announce_rotation(app);
    }
    // Tool keys.
    let (l, m, g) = ctx.input(|i| (i.key_pressed(Key::L), i.key_pressed(Key::M), i.key_pressed(Key::G)));
    if ctx.input(|i| i.modifiers.is_none()) {
        if l {
            app.tool = CanvasTool::Lasso;
        }
        if m {
            app.tool = CanvasTool::RectSelect;
        }
        if g {
            app.tool = CanvasTool::Fill;
        }
    }
    if app.view != View::Panel {
        return;
    }
    // Clipboard: the platform turns Ctrl+X/C/V into clipboard events; tests send the keys.
    let has_sel = app.tools.sel.selection.is_some();
    let has_clip = app.tools.sel.clipboard.is_some();
    let (mut cut, mut copy, mut paste, mut image) = (false, false, false, None);
    ctx.input_mut(|i| {
        i.events.retain(|e| match e {
            egui::Event::Cut if has_sel => {
                cut = true;
                false
            }
            egui::Event::Copy if has_sel => {
                copy = true;
                false
            }
            egui::Event::Paste(_) if has_clip => {
                paste = true;
                false
            }
            egui::Event::PasteImage(img) => {
                image = Some(img.clone());
                false
            }
            _ => true,
        });
        cut |= has_sel && i.consume_shortcut(&cmd(Key::X));
        copy |= has_sel && i.consume_shortcut(&cmd(Key::C));
        paste |= i.consume_shortcut(&cmd(Key::V));
    });
    let select_all = ctx.input_mut(|i| i.consume_shortcut(&cmd(Key::A)));
    if cut {
        selection::cut(app, ctx);
    } else if copy {
        selection::copy(app, ctx);
    }
    if let Some(img) = image {
        selection::paste_image(app, &img);
    } else if paste {
        selection::paste(app);
    }
    if select_all {
        selection::select_all(app);
    }
    if app.tools.sel.selection.is_none() {
        return;
    }
    // With a selection: arrows move it (Shift: 10 px), Enter applies, Esc cancels, Delete clears,
    // Ctrl+Z first undoes the transform.
    let (undo, enter, esc, delete) = ctx.input_mut(|i| {
        (
            !app.tools.sel.selection.as_ref().is_some_and(|s| s.transform.is_identity()) && i.consume_shortcut(&cmd(Key::Z)),
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Escape),
            i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace),
        )
    });
    let mut step = |k: Key, dx: f32, dy: f32| {
        let (big, small) = ctx.input_mut(|i| (i.consume_key(Modifiers::SHIFT, k), i.consume_key(Modifiers::NONE, k)));
        if big {
            selection::nudge(app, dx * 10.0, dy * 10.0);
        } else if small {
            selection::nudge(app, dx, dy);
        }
    };
    step(Key::ArrowLeft, -1.0, 0.0);
    step(Key::ArrowRight, 1.0, 0.0);
    step(Key::ArrowUp, 0.0, -1.0);
    step(Key::ArrowDown, 0.0, 1.0);
    if undo {
        selection::reset_transform(app);
    }
    if enter {
        selection::apply(app);
    } else if esc {
        selection::cancel(app);
    } else if delete {
        selection::delete(app);
    }
}

/// Fill at the pointer (Fill tool).
pub fn fill_input(app: &mut BoardApp, resp: &egui::Response, xf: &view::ViewXf) {
    if !resp.drag_started() {
        return;
    }
    let Some(pos) = resp.interact_pointer_pos() else { return };
    let [x, y] = xf.to_canvas(pos);
    let s = app.project.settings;
    if !(x >= 0.0 && y >= 0.0 && x < s.width as f32 && y < s.height as f32) {
        app.status = tl!("Click inside the canvas to fill").into();
        return;
    }
    let (Some(panel), Some(layer)) = (app.selected, app.active_layer) else {
        app.status = tl!("Select a visible layer to fill on").into();
        return;
    };
    if !app.project.panel(panel).and_then(|p| p.layer(layer)).is_some_and(|l| l.visible) {
        app.status = tl!("Select a visible layer to fill on").into();
        return;
    }
    let cmd = Command::Fill {
        panel,
        layer,
        x: x as u32,
        y: y as u32,
        color: app.brush.color,
        tolerance: app.tools.fill_tolerance,
        all_layers: app.tools.fill_sample == FillSample::AllLayers,
        gap: app.tools.fill_gap,
    };
    if app.run(cmd).is_some() {
        app.status = tl!("Filled").into();
    }
}

/// Toolbar options of the Fill tool.
pub fn fill_toolbar(app: &mut BoardApp, ui: &mut egui::Ui) {
    let mut c = egui::Color32::from_rgba_unmultiplied(app.brush.color[0], app.brush.color[1], app.brush.color[2], app.brush.color[3]);
    let l = ui.label(tl!("Colour"));
    if ui.color_edit_button_srgba(&mut c).labelled_by(l.id).changed() {
        app.brush.color = c.to_srgba_unmultiplied();
    }
    let l = ui.label(tl!("Tolerance"));
    ui.add(egui::Slider::new(&mut app.tools.fill_tolerance, 0..=255)).labelled_by(l.id);
    let l = ui.label(tl!("Close gaps (px)"));
    ui.add(egui::Slider::new(&mut app.tools.fill_gap, 0..=boardcraft_core::fill::MAX_GAP)).labelled_by(l.id);
    ui.radio_value(&mut app.tools.fill_sample, FillSample::AllLayers, tl!("All visible layers"));
    ui.radio_value(&mut app.tools.fill_sample, FillSample::Layer, tl!("This layer"));
}

/// The typed view rotation, at the end of the canvas toolbar.
pub fn rotation_field(app: &mut BoardApp, ui: &mut egui::Ui) {
    let l = ui.label(tl!("View rotation"));
    let r = ui
        .add(egui::DragValue::new(&mut app.tools.view_angle).speed(1.0).range(0.0..=359.0).suffix(" °"))
        .labelled_by(l.id)
        .on_hover_text(tl!("Turn the view (Shift+[ and Shift+], or hold R and drag); Shift+0 resets. Drawings are not changed."));
    if r.changed() && !app.tools.view_angle.is_finite() {
        app.tools.view_angle = 0.0;
    }
}

/// Edit menu entries for selections.
pub fn edit_menu(app: &mut BoardApp, ui: &mut egui::Ui) {
    selection::edit_menu(app, ui);
}

/// View menu entries for the view rotation.
pub fn view_menu(app: &mut BoardApp, ui: &mut egui::Ui) {
    if ui.add(egui::Button::new(tl!("Rotate View Left")).shortcut_text("Shift+[")).clicked() {
        rotate_view(app, -ROTATE_STEP);
        announce_rotation(app);
    }
    if ui.add(egui::Button::new(tl!("Rotate View Right")).shortcut_text("Shift+]")).clicked() {
        rotate_view(app, ROTATE_STEP);
        announce_rotation(app);
    }
    if ui.add(egui::Button::new(tl!("Reset View Rotation")).shortcut_text("Shift+0")).clicked() {
        app.tools.view_angle = 0.0;
        announce_rotation(app);
    }
}
