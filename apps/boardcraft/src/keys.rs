//! Keyboard shortcuts through `craft-keymap` (craft-libs): BoardCraft's command catalog with its
//! default chords, dispatch of a frame's key presses, the shared Keyboard Shortcuts dialog, and
//! the user's overrides in `config.toml` in the per-user data folder (`craft-settings`).
//!
//! Single keys (B, Space, arrows) never fire while a text field has the keyboard (the keymap's
//! typing rule), and Space, Enter and the arrow keys stay with a focused control.

use craft_keymap::egui::Dispatcher;
use craft_keymap::ui::{DialogState, I18nText, Outcome, ShortcutsDialog};
use craft_keymap::{Active, Bindable, Binding, Chord, CommandRef, Key, Keymap, Overrides, Platform, Scope, Scopes};
use craft_settings::{Location, Settings, SettingsStore};
use serde::{Deserialize, Serialize};

use crate::app::{BoardApp, CanvasTool, View};

/// What BoardCraft keeps in `config.toml`.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Prefs {
    /// The user's shortcut changes (commands without an entry use their defaults).
    #[serde(with = "craft_keymap::keymap::overrides_list")]
    pub shortcuts: Overrides,
}

impl Settings for Prefs {
    const APP_ID: &'static str = "boardcraft";
    const SCHEMA_VERSION: u32 = 1;
}

/// A bindable command: id, label (English source text), menu, default chords.
struct Entry {
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    keys: &'static [&'static str],
}

const fn e(id: &'static str, label: &'static str, menu: &'static str, keys: &'static [&'static str]) -> Entry {
    Entry { id, label, menu, keys }
}

/// Every command a shortcut can run, with the shortcuts BoardCraft has always had.
const CATALOG: &[Entry] = &[
    e("file.open", "Open…", "File", &["Cmd+O"]),
    e("file.save", "Save", "File", &["Cmd+S"]),
    e("file.save_as", "Save As…", "File", &["Cmd+Shift+S"]),
    e("file.import_script", "Import Screenplay…", "File", &[]),
    e("file.conform", "Conform Timing from Edit…", "File", &[]),
    e("file.export_sheet", "Export Board Sheet (PDF)…", "File", &[]),
    e("file.export_shot_list", "Export Shot List (CSV)…", "File", &[]),
    e("edit.undo", "Undo", "Edit", &["Cmd+Z"]),
    e("edit.redo", "Redo", "Edit", &["Cmd+Shift+Z", "Cmd+Y"]),
    e("edit.new_panel", "New Panel", "Edit", &["Cmd+N"]),
    e("edit.duplicate_panel", "Duplicate Panel", "Edit", &["Cmd+D"]),
    e("edit.new_scene", "New Scene", "Edit", &[]),
    e("edit.delete_panel", "Delete Panel", "Edit", &[]),
    e("edit.move_earlier", "Move Panel Earlier", "Edit", &[]),
    e("edit.move_later", "Move Panel Later", "Edit", &[]),
    e("edit.board_settings", "Board Settings…", "Edit", &["Cmd+,"]),
    e("edit.keyboard_shortcuts", "Keyboard Shortcuts…", "Edit", &[]),
    e("view.toggle_view", "Switch Board and Panel View", "View", &["V"]),
    e("view.play", "Play or Pause Animatic", "View", &["Space"]),
    e("view.onion", "Onion Skin", "View", &["O"]),
    e("view.fit", "Fit Canvas", "View", &["0"]),
    e("view.zoom_in", "Zoom In", "View", &["+", "="]),
    e("view.zoom_out", "Zoom Out", "View", &["-"]),
    e("playhead.back", "Previous Frame", "Playback", &[","]),
    e("playhead.forward", "Next Frame", "Playback", &["."]),
    e("playhead.start", "Go to Start", "Playback", &["Home"]),
    e("select.previous", "Previous Panel", "Playback", &["Left"]),
    e("select.next", "Next Panel", "Playback", &["Right"]),
    e("tool.brush", "Brush", "Tools", &["B"]),
    e("tool.eraser", "Eraser", "Tools", &["E"]),
    e("tool.camera", "Camera", "Tools", &["C"]),
    e("tool.smaller", "Smaller Brush", "Tools", &["["]),
    e("tool.bigger", "Bigger Brush", "Tools", &["]"]),
];

/// The catalog with labels and menu names in the current interface language.
fn catalog() -> Vec<Bindable> {
    CATALOG
        .iter()
        .map(|c| {
            let label = tl!(c.label).to_string();
            let mut b = Bindable::new(c.id, label.clone(), Scope::app()).with_path(vec![tl!(c.menu).to_string(), label]);
            if c.label != tl!(c.label) {
                // The English name still finds it in search and imports.
                b = b.with_alias(c.label);
            }
            for k in c.keys {
                match Binding::parse(k) {
                    Ok(binding) => b = b.with_default(binding),
                    Err(err) => log::warn!("default shortcut {k} for {}: {err}", c.id),
                }
            }
            b
        })
        .collect()
}

/// Shortcut state: the keymap, its dispatcher, the settings store and the open dialog.
pub struct Shortcuts {
    pub keymap: Keymap,
    dispatcher: Dispatcher,
    store: SettingsStore<Prefs>,
    /// The Keyboard Shortcuts dialog, while open.
    pub dialog: Option<DialogState>,
    /// The language the catalog's labels are in.
    lang: craft_i18n::Lang,
}

impl Shortcuts {
    /// Load the user's overrides from `config.toml` in `data_dir`; without one, keep them in memory.
    pub fn new(data_dir: Option<&std::path::Path>) -> Self {
        let location = match data_dir {
            Some(d) => Location::Dir(d.to_path_buf()),
            None => Location::Memory,
        };
        let (store, report) = SettingsStore::<Prefs>::open(location);
        log::debug!("settings: {report:?}");
        let keymap = Keymap::new(catalog(), Scopes::new(), Platform::current()).with_overrides(store.get().shortcuts.clone());
        for p in keymap.problems() {
            log::warn!("shortcut setting not used: {p:?}");
        }
        Self { keymap, dispatcher: Dispatcher::new(), store, dialog: None, lang: crate::i18n::current() }
    }

    /// The chord a menu shows for command `id` (the first binding), in this platform's form.
    pub fn chord(&self, id: &str) -> String {
        self.keymap.bindings(&CommandRef::from(id)).first().map(|b| b.trigger.display(self.keymap.platform())).unwrap_or_default()
    }

    /// Rebuild the catalog's labels after the interface language changed.
    fn follow_language(&mut self) {
        let lang = crate::i18n::current();
        if lang == self.lang {
            return;
        }
        self.lang = lang;
        let overrides = self.keymap.overrides().clone();
        self.keymap = Keymap::new(catalog(), Scopes::new(), Platform::current()).with_overrides(overrides);
    }

    /// Keep the keymap's overrides; the status line says when that failed.
    fn save(&mut self) -> Option<String> {
        let overrides = self.keymap.overrides().clone();
        self.store.update(|p| p.shortcuts = overrides);
        self.store.flush().err().map(|e| tlf!("Could not save the shortcuts: {error}", error = e))
    }
}

/// Keys that operate a focused control; with a control focused they stay with it.
fn operates_focused_control(chord: &Chord) -> bool {
    chord.is_plain() && matches!(chord.key, Key::Space | Key::Enter | Key::Left | Key::Right | Key::Up | Key::Down)
}

/// This frame's shortcuts: run what the key presses are bound to (or the dialog, while open).
pub fn handle(app: &mut BoardApp, ctx: &egui::Context) {
    app.shortcuts.follow_language();
    if app.shortcuts.dialog.is_some() {
        show_dialog(app, ctx);
        return;
    }
    let typing = ctx.egui_wants_keyboard_input();
    let focused = ctx.memory(|m| m.focused().is_some());
    let active = Active { typing, ..Active::NONE };
    let Shortcuts { keymap, dispatcher, .. } = &mut app.shortcuts;
    let hits = ctx.input_mut(|i| dispatcher.dispatch_filtered(i, keymap, &active, |hit| !(focused && operates_focused_control(&hit.trigger.chord))));
    for hit in hits {
        run(app, &hit.command.id);
    }
}

/// Run the command with id `id` (from a shortcut or a test).
pub fn run(app: &mut BoardApp, id: &str) {
    match id {
        "file.open" => app.open_dialog(),
        "file.save" => {
            app.save();
        }
        "file.save_as" => {
            app.save_as();
        }
        "file.import_script" => app.import_script(),
        "file.conform" => app.conform(),
        "file.export_sheet" => app.sheet_dialog = Some(crate::sheet_dialog::Dialog::new(app)),
        "file.export_shot_list" => app.export_shot_list(),
        "edit.undo" => app.undo(),
        "edit.redo" => app.redo(),
        "edit.new_panel" => app.add_panel(),
        "edit.duplicate_panel" => app.duplicate_panel(),
        "edit.new_scene" => app.add_scene(),
        "edit.delete_panel" => app.delete_panel(),
        "edit.move_earlier" => app.move_panel(false),
        "edit.move_later" => app.move_panel(true),
        "edit.board_settings" => app.settings_dialog = Some(crate::settings::Dialog::new(&app.project)),
        "edit.keyboard_shortcuts" => open_dialog(app),
        "view.toggle_view" => app.view = if app.view == View::Board { View::Panel } else { View::Board },
        "view.play" => app.toggle_play(),
        "view.onion" => app.onion = !app.onion,
        "view.fit" => {
            app.canvas_zoom = 1.0;
            app.canvas_pan = egui::Vec2::ZERO;
        }
        "view.zoom_in" => app.canvas_zoom = (app.canvas_zoom * 1.25).min(16.0),
        "view.zoom_out" => app.canvas_zoom = (app.canvas_zoom / 1.25).max(0.25),
        "playhead.back" | "playhead.forward" | "playhead.start" => {
            let last = app.project.total_frames().saturating_sub(1) as f64;
            app.playhead = match id {
                "playhead.start" => 0.0,
                "playhead.forward" => (app.playhead.floor() + 1.0).clamp(0.0, last),
                _ => (app.playhead.floor() - 1.0).clamp(0.0, last),
            };
            if let Some((panel, _)) = app.project.panel_at(app.playhead as u64) {
                app.select(Some(panel));
            }
        }
        "select.previous" => app.step_selection(-1),
        "select.next" => app.step_selection(1),
        "tool.brush" => app.tool = CanvasTool::Brush,
        "tool.eraser" => app.tool = CanvasTool::Eraser,
        "tool.camera" => app.tool = CanvasTool::Camera,
        "tool.smaller" | "tool.bigger" => {
            let r = if app.tool == CanvasTool::Eraser { &mut app.eraser_radius } else { &mut app.brush.radius };
            *r = if id == "tool.bigger" { (*r * 1.25).min(200.0) } else { (*r / 1.25).max(0.5) };
        }
        other => log::warn!("no command {other}"),
    }
}

pub fn open_dialog(app: &mut BoardApp) {
    app.shortcuts.dialog = Some(DialogState::new());
}

fn show_dialog(app: &mut BoardApp, ctx: &egui::Context) {
    let Shortcuts { keymap, dialog, .. } = &mut app.shortcuts;
    let Some(state) = dialog else { return };
    let text = I18nText { i18n: &crate::i18n::I18N, lang: crate::i18n::current() };
    let outcome = ShortcutsDialog::new(keymap, &text).show(ctx, state);
    let open = state.is_open();
    if !open {
        app.shortcuts.dialog = None;
    }
    match outcome {
        Some(Outcome::Ok) => {
            app.status = app.shortcuts.save().unwrap_or_else(|| tl!("Shortcuts saved").into());
        }
        Some(Outcome::Cancel) => app.status = tl!("Shortcut changes discarded").into(),
        _ => {}
    }
}
