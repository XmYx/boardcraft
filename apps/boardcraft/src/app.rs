//! Application state, menus, file actions and keyboard shortcuts.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use boardcraft_core::command::{self, Command};
use boardcraft_core::timeline::TimelineFormat;
use boardcraft_core::{Brush, Id, Project, Raster, Tool, format, fountain, script, sheet, stills, timeline};

/// Write a file through the core's atomic save.
fn craft_atomic_write(path: &Path, bytes: &[u8]) -> boardcraft_core::Result<()> {
    boardcraft_core::format::write_atomic(path, bytes)
}

/// Screenplay files BoardCraft reads.
const SCRIPT_EXTENSIONS: &[&str] = &["fountain", "spmd", "txt", "fdx", "json"];

use crate::args::{Cli, StartView};
use crate::textures::Textures;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// Panel grid, grouped by scene.
    Board,
    /// One panel: drawing canvas and camera.
    Panel,
}

/// What a drag on the canvas does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasTool {
    Brush,
    Eraser,
    /// Move the selected camera key; the scroll wheel zooms it.
    Camera,
}

/// Which camera key the Camera tool edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyChoice {
    Start,
    End,
}

/// A stroke in progress.
pub struct StrokeState {
    pub panel: Id,
    pub layer: Id,
    pub last: boardcraft_core::StrokePoint,
}

pub struct BoardApp {
    pub project: Project,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub view: View,
    pub selected: Option<Id>,
    pub active_layer: Option<Id>,
    pub tool: CanvasTool,
    pub brush: Brush,
    pub eraser_radius: f32,
    pub key_choice: KeyChoice,
    pub playing: bool,
    /// Animatic playhead in frames.
    pub playhead: f64,
    /// Timeline zoom: pixels per frame.
    pub timeline_ppf: f32,
    pub textures: Textures,
    pub status: String,
    pub history: crate::history::History,
    /// Counts document edits; a frame that changes it becomes an undo step.
    pub edits: u64,
    /// Counts pointer presses, to group one press's edits into one undo step.
    press: u64,
    pub stroke: Option<StrokeState>,
    /// Last pen pressure seen from a touch/pen device (None: mouse, full pressure).
    pub pen_pressure: Option<f32>,
    confirm_close: bool,
    close_confirmed: bool,
    screenshot: Option<PathBuf>,
    frames_until_shot: u32,
    shot_done: Arc<AtomicBool>,
    last_title: String,
    /// Move keyboard focus to the selected panel's card or block on the next frame.
    pub focus_selected: bool,
    /// Interface language code, `None` = the system's.
    pub language: Option<String>,
    pub autosave: crate::autosave::Autosave,
    /// Show the previous panel faintly under the drawing (onion skin).
    pub onion: bool,
    /// Canvas view zoom (1 = fit) and pan in screen points.
    pub canvas_zoom: f32,
    pub canvas_pan: egui::Vec2,
    /// Board Settings dialog state, when open.
    pub settings_dialog: Option<crate::settings::Dialog>,
    /// Layers being edited in an external image editor.
    pub external: Vec<crate::external::Watch>,
    /// Where the last stroke ended (panel, layer, point), for Shift-click straight lines.
    pub last_point: Option<(Id, Id, boardcraft_core::StrokePoint)>,
    /// Boards a crashed session left; offered at start.
    pub recoverable: Vec<crate::autosave::Recoverable>,
    /// The audio track: decoded clips, sound output, strip state.
    pub audio: crate::audio_ui::AudioState,
    /// Export Movie dialog, when open, and the running export.
    pub movie_dialog: Option<crate::movie_ui::Dialog>,
    pub movie_job: Option<crate::movie_ui::Job>,
}

fn pick_layer(project: &Project, panel: Option<Id>) -> Option<Id> {
    let p = project.panel(panel?)?;
    p.layers.last().map(|l| l.id)
}

impl BoardApp {
    pub fn new(cc: &eframe::CreationContext<'_>, cli: Cli) -> Self {
        Self::with_context(&cc.egui_ctx, cli)
    }

    /// Set up the app on an egui context (also used by the UI tests).
    pub fn with_context(ctx: &egui::Context, cli: Cli) -> Self {
        // Light or dark follows the system (both themes meet 4.5:1 for text).
        crate::i18n::install_fonts(ctx);
        apply_contrast(ctx);
        crate::i18n::set_language(None);
        let mut app = Self {
            project: Project::blank("Untitled board"),
            path: None,
            dirty: false,
            view: if cli.view == StartView::Panel { View::Panel } else { View::Board },
            selected: None,
            active_layer: None,
            tool: CanvasTool::Brush,
            brush: Brush::default(),
            eraser_radius: 14.0,
            key_choice: KeyChoice::Start,
            playing: false,
            playhead: 0.0,
            timeline_ppf: 2.0,
            textures: Textures::default(),
            status: String::new(),
            history: Default::default(),
            edits: 0,
            press: 0,
            stroke: None,
            pen_pressure: None,
            confirm_close: false,
            close_confirmed: false,
            screenshot: cli.screenshot.clone(),
            frames_until_shot: 12,
            shot_done: Arc::new(AtomicBool::new(false)),
            last_title: String::new(),
            focus_selected: false,
            language: None,
            autosave: crate::autosave::Autosave::new(if cli.screenshot.is_some() {
                None
            } else {
                cli.data_dir.clone().or_else(crate::autosave::default_data_dir)
            }),
            recoverable: Vec::new(),
            onion: false,
            canvas_zoom: 1.0,
            canvas_pan: egui::Vec2::ZERO,
            settings_dialog: None,
            external: Vec::new(),
            last_point: None,
            audio: Default::default(),
            movie_dialog: None,
            movie_job: None,
        };
        app.recoverable = app.autosave.recoverable();
        if cli.demo {
            app.replace_project(boardcraft_core::demo::demo_project(), None);
            app.status = tl!("Demo board: Paper Boats").into();
        }
        if let Some(file) = &cli.file {
            app.open_path(file);
        }
        if app.selected.is_none() {
            let first = app.project.panels().next().map(|(_, p)| p.id);
            app.select(first);
        }
        app
    }

    /// Replace the open project (open, new, import as new).
    pub fn replace_project(&mut self, project: Project, path: Option<PathBuf>) {
        self.project = project;
        self.path = path;
        self.dirty = false;
        self.textures.clear();
        self.history.clear();
        self.stroke = None;
        self.playing = false;
        self.playhead = 0.0;
        self.audio = Default::default();
        let first = self.project.panels().next().map(|(_, p)| p.id);
        self.select(first);
    }

    pub fn select(&mut self, panel: Option<Id>) {
        self.selected = panel;
        let keep = self.active_layer.is_some_and(|l| panel.and_then(|p| self.project.panel(p)).is_some_and(|p| p.layer(l).is_some()));
        if !keep {
            self.active_layer = pick_layer(&self.project, panel);
        }
    }

    /// Select a panel and move the playhead to its first frame.
    pub fn select_and_seek(&mut self, panel: Id) {
        self.select(Some(panel));
        if let Some(t) = self.project.panel_start(panel) {
            self.playhead = t as f64;
        }
    }

    pub fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.project.name.clone());
        format!("{}{name} — BoardCraft", if self.dirty { "• " } else { "" })
    }

    // ---------------------------------------------------------------------------------------
    // Files
    // ---------------------------------------------------------------------------------------

    pub fn open_path(&mut self, path: &Path) {
        let is_script = path.extension().is_some_and(|e| SCRIPT_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)));
        if is_script {
            match std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| script::parse_script(&path.to_string_lossy(), &b).map_err(|e| e.to_string())) {
                Ok(script) => {
                    let project = fountain::project_from_script(&script, Default::default());
                    let n = project.panel_count();
                    self.replace_project(project, None);
                    self.edited();
                    self.status = tlf!("Boarded {scenes} scenes, {n} panels from {file}", scenes = script.scenes.len(), n = n, file = path.display());
                }
                Err(e) => self.status = tlf!("Could not read {file}: {error}", file = path.display(), error = e),
            }
            return;
        }
        match format::load(path) {
            Ok(p) => {
                self.replace_project(p, Some(path.to_path_buf()));
                self.status = tlf!("Opened {file}", file = path.display());
            }
            Err(e) => self.status = tlf!("Could not open: {error}", error = e),
        }
    }

    fn dialog() -> rfd::FileDialog {
        rfd::FileDialog::new()
    }

    pub fn open_dialog(&mut self) {
        if let Some(p) = Self::dialog().add_filter(tl!("BoardCraft project"), &[format::EXTENSION]).add_filter(tl!("Screenplay"), SCRIPT_EXTENSIONS).pick_file()
        {
            self.open_path(&p);
        }
    }

    pub fn save(&mut self) -> bool {
        match self.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    pub fn save_as(&mut self) -> bool {
        let name = format!("{}.{}", self.project.name, format::EXTENSION);
        match Self::dialog().add_filter(tl!("BoardCraft project"), &[format::EXTENSION]).set_file_name(name).save_file() {
            Some(mut p) => {
                if p.extension().is_none() {
                    p.set_extension(format::EXTENSION);
                }
                self.save_to(&p)
            }
            None => false,
        }
    }

    fn save_to(&mut self, path: &Path) -> bool {
        match format::save(&self.project, path) {
            Ok(()) => {
                self.path = Some(path.to_path_buf());
                self.dirty = false;
                self.autosave.clear();
                self.status = tlf!("Saved {file}", file = path.display());
                true
            }
            Err(e) => {
                self.status = tlf!("Save failed: {error}", error = e);
                false
            }
        }
    }

    /// Import a screenplay (Fountain, Final Draft, ScriptCraft scene list). Into an untouched blank
    /// board it starts the board; otherwise it updates the board by stable scene id, keeping every
    /// drawing (see `boardcraft_core::script::reimport`).
    pub fn import_script(&mut self) {
        let Some(path) = Self::dialog().add_filter(tl!("Screenplay"), SCRIPT_EXTENSIONS).pick_file() else { return };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                self.status = tlf!("Could not read {file}: {error}", file = path.display(), error = e);
                return;
            }
        };
        let untouched = !self.dirty && self.path.is_none() && self.project.panel_count() <= 1;
        if untouched {
            match script::parse_script(&path.to_string_lossy(), &bytes) {
                Ok(s) => {
                    let p = fountain::project_from_script(&s, self.project.settings);
                    self.replace_project(p, None);
                    self.edited();
                    self.status = tlf!("Boarded {scenes} scenes from {file}", scenes = s.scenes.len(), file = path.display());
                }
                Err(e) => self.status = tlf!("Could not import {file}: {error}", file = path.display(), error = e),
            }
            return;
        }
        let cmd =
            Command::ImportScript { name: path.to_string_lossy().into_owned(), text: String::from_utf8_lossy(&bytes).into_owned(), format: None, merge: true };
        if let Some(out) = self.run(cmd) {
            self.status = tlf!("Updated the board from {file}: {summary}", file = path.display(), summary = out.message);
            let keep = self.selected.filter(|id| self.project.panel(*id).is_some());
            self.select(keep.or_else(|| self.project.panels().next().map(|(_, p)| p.id)));
        }
    }

    /// Take panel timing from an editor's cut of an exported timeline.
    pub fn conform(&mut self) {
        let Some(path) = Self::dialog().add_filter(tl!("Timeline"), &["otio", "edl", "xml", "fcpxml"]).pick_file() else { return };
        match std::fs::read(&path) {
            Ok(bytes) => {
                let cmd = Command::Conform { name: path.to_string_lossy().into_owned(), text: String::from_utf8_lossy(&bytes).into_owned() };
                if let Some(out) = self.run(cmd) {
                    self.status = tlf!("Conformed timing from {file}: {summary}", file = path.display(), summary = out.message);
                }
            }
            Err(e) => self.status = tlf!("Could not read {file}: {error}", file = path.display(), error = e),
        }
    }

    pub fn import_image(&mut self) {
        let Some(panel) = self.selected else {
            self.status = tl!("Select a panel first").into();
            return;
        };
        let Some(path) = Self::dialog().add_filter(tl!("Images"), &["png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp", "gif"]).pick_file() else { return };
        let decoded = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| Raster::decode(&b).map_err(|e| e.to_string()));
        match decoded {
            Ok(img) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| tl!("Image").into());
                let mut layer = self.project.new_layer(&name);
                layer.pixels.place_fit(&img);
                let id = layer.id;
                if let Some(p) = self.project.panel_mut(panel) {
                    // Images go under the drawing layers.
                    p.layers.insert(0, layer);
                    p.touch();
                }
                self.active_layer = Some(id);
                self.edited();
                self.status = tlf!("Imported {file}", file = path.display());
            }
            Err(e) => self.status = tlf!("Could not import {file}: {error}", file = path.display(), error = e),
        }
    }

    /// Export a printable board sheet (PDF) on `paper` with `columns` × `rows` panels a page.
    pub fn export_sheet(&mut self, paper: sheet::Paper, columns: u32, rows: u32) {
        let Some(mut path) = Self::dialog().add_filter("PDF", &["pdf"]).set_file_name(format!("{}.pdf", self.project.name)).save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("pdf");
        }
        let labels = sheet::SheetLabels {
            dialogue: tl!("Dialogue").into(),
            action: tl!("Action").into(),
            notes: tl!("Notes").into(),
            page_of: tl!("Page {n} of {total}").into(),
            seconds: tl!("s").into(),
        };
        let opts = sheet::SheetOptions { paper, columns, rows, labels };
        let (font, index) = crate::i18n::sheet_font();
        let result = sheet::board_sheet_pdf(&self.project, &opts, font, index).and_then(|pdf| craft_atomic_write(&path, &pdf));
        match result {
            Ok(()) => self.status = tlf!("Wrote the board sheet {file} ({n} pages)", file = path.display(), n = sheet::page_count(&self.project, &opts)),
            Err(e) => self.status = tlf!("Export failed: {error}", error = e),
        }
    }

    /// Export the animatic as an editorial timeline plus panel stills.
    pub fn export_timeline(&mut self, format: TimelineFormat) {
        let ext = format.extension();
        let Some(mut path) = Self::dialog().add_filter(format.label(), &[ext]).set_file_name(format!("{}.{ext}", self.project.name)).save_file() else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension(ext);
        }
        match timeline::export(&self.project, &path) {
            Ok(stills) => {
                self.status = tlf!("Exported {file} (stills in {dir}). Open it in FilmCraft with File ▸ Import.", file = path.display(), dir = stills.display())
            }
            Err(e) => self.status = tlf!("Export failed: {error}", error = e),
        }
    }

    pub fn export_pngs(&mut self) {
        let Some(dir) = Self::dialog().pick_folder() else { return };
        match stills::write_stills(&self.project, &dir) {
            Ok(files) => self.status = tlf!("Wrote {n} panel images to {dir}", n = files.len(), dir = dir.display()),
            Err(e) => self.status = tlf!("Export failed: {error}", error = e),
        }
    }

    pub fn ai_sketch(&mut self) {
        let Some(panel) = self.selected else { return };
        let result = boardcraft_core::ai::panel_request(&self.project, panel).and_then(|r| boardcraft_core::ai::generate_panel_image(&r));
        match result {
            Ok(img) => {
                let mut layer = self.project.new_layer(tl!("AI sketch"));
                layer.pixels.place_fit(&img);
                if let Some(p) = self.project.panel_mut(panel) {
                    p.layers.insert(0, layer);
                    p.touch();
                }
                self.edited();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    // ---------------------------------------------------------------------------------------
    // Edits
    // ---------------------------------------------------------------------------------------

    /// Run a board command, reporting errors in the status bar. Returns its outcome.
    pub fn run(&mut self, cmd: Command) -> Option<boardcraft_core::command::Outcome> {
        match command::apply(&mut self.project, &cmd) {
            Ok(out) => {
                self.edited();
                self.textures.retain(&self.project);
                Some(out)
            }
            Err(e) => {
                self.status = e.to_string();
                None
            }
        }
    }

    pub fn add_panel(&mut self) {
        let scene = if self.selected.is_none() { self.project.scenes.last().map(|s| s.id) } else { None };
        if let Some(id) = self.run(Command::AddPanel { scene, after: self.selected }).and_then(|o| o.created) {
            self.select_and_seek(id);
        }
    }

    pub fn duplicate_panel(&mut self) {
        let Some(panel) = self.selected else { return };
        if let Some(id) = self.run(Command::DuplicatePanel { panel }).and_then(|o| o.created) {
            self.select_and_seek(id);
            self.focus_selected = true;
        }
    }

    pub fn export_shot_list(&mut self) {
        let Some(mut path) = Self::dialog().add_filter("CSV", &["csv"]).set_file_name(format!("{} shot list.csv", self.project.name)).save_file() else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension("csv");
        }
        match boardcraft_core::format::write_atomic(&path, boardcraft_core::shotlist::csv(&self.project).as_bytes()) {
            Ok(()) => self.status = tlf!("Wrote the shot list {file}", file = path.display()),
            Err(e) => self.status = tlf!("Export failed: {error}", error = e),
        }
    }

    pub fn add_scene(&mut self) {
        if let Some(id) = self.run(Command::AddScene { heading: "NEW SCENE".into() }).and_then(|o| o.created) {
            self.select_and_seek(id);
        }
    }

    pub fn delete_panel(&mut self) {
        let Some(id) = self.selected else { return };
        let next = {
            let ids: Vec<Id> = self.project.panels().map(|(_, p)| p.id).collect();
            let i = ids.iter().position(|x| *x == id).unwrap_or(0);
            ids.get(i + 1).or_else(|| i.checked_sub(1).and_then(|j| ids.get(j))).copied()
        };
        if self.run(Command::DeletePanel { panel: id }).is_some() {
            self.select(next);
        }
    }

    pub fn move_panel(&mut self, later: bool) {
        if let Some(id) = self.selected {
            self.run(Command::MovePanel { panel: id, later });
        }
    }

    pub fn step_selection(&mut self, delta: isize) {
        let ids: Vec<Id> = self.project.panels().map(|(_, p)| p.id).collect();
        let i = self.selected.and_then(|s| ids.iter().position(|x| *x == s)).unwrap_or(0) as isize;
        let j = (i + delta).clamp(0, ids.len() as isize - 1);
        if let Some(id) = ids.get(j as usize) {
            self.select_and_seek(*id);
            self.focus_selected = true;
        }
    }

    /// The document changed: mark it unsaved and count the edit (for undo).
    pub fn edited(&mut self) {
        // Sound anchored to panels follows edits made outside commands (typed durations).
        self.project.sync_audio();
        self.dirty = true;
        self.edits = self.edits.wrapping_add(1);
    }

    fn snapshot(&self) -> crate::history::Snapshot {
        crate::history::Snapshot { project: self.project.clone(), selected: self.selected, dirty: self.dirty }
    }

    fn restore(&mut self, s: crate::history::Snapshot) {
        self.project = s.project;
        self.dirty = s.dirty;
        self.stroke = None;
        // Restored panels may carry revisions the texture cache has seen with other pixels.
        self.textures.clear();
        let keep = s.selected.filter(|id| self.project.panel(*id).is_some());
        self.select(keep.or_else(|| self.project.panels().next().map(|(_, p)| p.id)));
    }

    pub fn undo(&mut self) {
        let current = self.snapshot();
        if let Some(s) = self.history.undo(current) {
            self.restore(s);
            self.status = tl!("Undone").into();
        }
    }

    pub fn redo(&mut self) {
        let current = self.snapshot();
        if let Some(s) = self.history.redo(current) {
            self.restore(s);
            self.status = tl!("Redone").into();
        }
    }

    pub fn active_brush(&self) -> Brush {
        match self.tool {
            CanvasTool::Eraser => Brush { tool: Tool::Eraser, radius: self.eraser_radius, color: [0, 0, 0, 255], hardness: 0.8, ..self.brush },
            _ => self.brush,
        }
    }

    // ---------------------------------------------------------------------------------------
    // Playback
    // ---------------------------------------------------------------------------------------

    pub fn toggle_play(&mut self) {
        self.playing = !self.playing;
        if self.playing && self.playhead >= self.project.total_frames() as f64 - 1.0 {
            self.playhead = 0.0;
        }
    }

    fn advance_playback(&mut self, ctx: &egui::Context) {
        if !self.playing {
            return;
        }
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.25) as f64;
        self.playhead += dt * self.project.settings.fps_f64();
        let total = self.project.total_frames() as f64;
        if self.playhead >= total {
            self.playhead = (total - 1.0).max(0.0);
            self.playing = false;
        }
        if let Some((id, _)) = self.project.panel_at(self.playhead as u64)
            && self.selected != Some(id)
        {
            self.select(Some(id));
        }
        ctx.request_repaint();
    }

    // ---------------------------------------------------------------------------------------
    // UI
    // ---------------------------------------------------------------------------------------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        use egui::{Key, KeyboardShortcut, Modifiers};
        let cmd = |k| KeyboardShortcut::new(Modifiers::COMMAND, k);
        let cmd_shift = |k| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, k);
        // Shift variants first: egui matches Ctrl+S logically, so it would also eat Ctrl+Shift+S.
        let (save_as, save, open, redo, undo, new_panel) = ctx.input_mut(|i| {
            let save_as = i.consume_shortcut(&cmd_shift(Key::S));
            let save = i.consume_shortcut(&cmd(Key::S));
            let open = i.consume_shortcut(&cmd(Key::O));
            let redo = i.consume_shortcut(&cmd_shift(Key::Z)) || i.consume_shortcut(&cmd(Key::Y));
            let undo = i.consume_shortcut(&cmd(Key::Z));
            (save_as, save, open, redo, undo, i.consume_shortcut(&cmd(Key::N)))
        });
        let (duplicate, settings) = ctx.input_mut(|i| (i.consume_shortcut(&cmd(Key::D)), i.consume_shortcut(&cmd(Key::Comma))));
        if duplicate {
            self.duplicate_panel();
        }
        if settings {
            self.settings_dialog = Some(crate::settings::Dialog::new(&self.project));
        }
        if save_as {
            self.save_as();
        } else if save {
            self.save();
        }
        if open {
            self.open_dialog();
        }
        if redo {
            self.redo();
        } else if undo {
            self.undo();
        }
        if new_panel {
            self.add_panel();
        }
        let keys = ctx.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::V),
                i.key_pressed(Key::B),
                i.key_pressed(Key::E),
                i.key_pressed(Key::C),
                i.key_pressed(Key::OpenBracket),
                i.key_pressed(Key::CloseBracket),
                i.modifiers.is_none(),
            )
        });
        let (space, left, right, tab, b, e, c, smaller, bigger, plain) = keys;
        if !plain {
            return;
        }
        // Space activates a focused button; it plays only when nothing has focus.
        if space && ctx.memory(|m| m.focused().is_none()) {
            self.toggle_play();
        }
        // Keyboard alternatives to dragging the playhead: , and . step a frame, Home goes to the start.
        let (back, fwd, home) = ctx.input(|i| (i.key_pressed(Key::Comma), i.key_pressed(Key::Period), i.key_pressed(Key::Home)));
        if back || fwd || home {
            let last = self.project.total_frames().saturating_sub(1) as f64;
            self.playhead = if home { 0.0 } else { (self.playhead.floor() + if fwd { 1.0 } else { -1.0 }).clamp(0.0, last) };
            if let Some((id, _)) = self.project.panel_at(self.playhead as u64) {
                self.select(Some(id));
            }
        }
        // With a widget focused, arrows are egui's focus navigation and selection follows focus.
        let nothing_focused = ctx.memory(|m| m.focused().is_none());
        if left && nothing_focused {
            self.step_selection(-1);
        }
        if right && nothing_focused {
            self.step_selection(1);
        }
        if tab {
            self.view = if self.view == View::Board { View::Panel } else { View::Board };
        }
        if b {
            self.tool = CanvasTool::Brush;
        }
        let (onion, fit, zoom_in, zoom_out) =
            ctx.input(|i| (i.key_pressed(Key::O), i.key_pressed(Key::Num0), i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals), i.key_pressed(Key::Minus)));
        if onion {
            self.onion = !self.onion;
        }
        if fit {
            self.canvas_zoom = 1.0;
            self.canvas_pan = egui::Vec2::ZERO;
        }
        if zoom_in {
            self.canvas_zoom = (self.canvas_zoom * 1.25).min(16.0);
        }
        if zoom_out {
            self.canvas_zoom = (self.canvas_zoom / 1.25).max(0.25);
        }
        if e {
            self.tool = CanvasTool::Eraser;
        }
        if c {
            self.tool = CanvasTool::Camera;
        }
        let r = if self.tool == CanvasTool::Eraser { &mut self.eraser_radius } else { &mut self.brush.radius };
        if smaller {
            *r = (*r / 1.25).max(0.5);
        }
        if bigger {
            *r = (*r * 1.25).min(200.0);
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tl!("File"), |ui| {
                if ui.button(tl!("New Board…")).clicked() {
                    self.replace_project(Project::blank("Untitled board"), None);
                    self.settings_dialog = Some(crate::settings::Dialog::new(&self.project));
                }
                if ui.button(tl!("Open…")).clicked() {
                    self.open_dialog();
                }
                ui.separator();
                if ui.button(tl!("Save")).clicked() {
                    self.save();
                }
                if ui.button(tl!("Save As…")).clicked() {
                    self.save_as();
                }
                ui.separator();
                if ui
                    .button(tl!("Import Screenplay…"))
                    .on_hover_text(tl!("Fountain, Final Draft (.fdx) or a ScriptCraft scene list (.json). Updates the board by scene, keeping drawings."))
                    .clicked()
                {
                    self.import_script();
                }
                if ui.button(tl!("Import Image to Panel…")).clicked() {
                    self.import_image();
                }
                if ui
                    .button(tl!("Import Audio…"))
                    .on_hover_text(tl!("Add a sound file at the selected panel; it moves with the panel when you retime"))
                    .clicked()
                {
                    crate::audio_ui::import(self);
                }
                ui.separator();
                ui.menu_button(tl!("Export Animatic Timeline"), |ui| {
                    for f in TimelineFormat::ALL {
                        if ui.button(tlf!("{format}…", format = f.label())).clicked() {
                            self.export_timeline(f);
                        }
                    }
                });
                if ui
                    .button(tl!("Conform Timing from Edit…"))
                    .on_hover_text(tl!("Take panel durations from an edited timeline (OTIO, EDL, FCP7 XML, FCPXML)"))
                    .clicked()
                {
                    self.conform();
                }
                ui.menu_button(tl!("Export Board Sheet (PDF)"), |ui| {
                    for (label, paper, c, r) in [
                        (tl!("A4, 6 panels a page…"), sheet::Paper::A4, 3, 2),
                        (tl!("A4, 3 panels a page…"), sheet::Paper::A4, 3, 1),
                        (tl!("Letter, 6 panels a page…"), sheet::Paper::Letter, 3, 2),
                        (tl!("Letter, 3 panels a page…"), sheet::Paper::Letter, 3, 1),
                    ] {
                        if ui.button(label).clicked() {
                            self.export_sheet(paper, c, r);
                        }
                    }
                });
                if ui.button(tl!("Export Shot List (CSV)…")).clicked() {
                    self.export_shot_list();
                }
                if ui.button(tl!("Export Panel Images (PNG)…")).clicked() {
                    self.export_pngs();
                }
                if ui.button(tl!("Export Movie (MP4)…")).clicked() {
                    self.movie_dialog = Some(Default::default());
                }
                ui.separator();
                if ui.button(tl!("Quit")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(tl!("Edit"), |ui| {
                if ui.add_enabled(self.history.can_undo(), egui::Button::new(tl!("Undo")).shortcut_text("Ctrl+Z")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(self.history.can_redo(), egui::Button::new(tl!("Redo")).shortcut_text("Ctrl+Shift+Z")).clicked() {
                    self.redo();
                }
                ui.separator();
                if ui.add(egui::Button::new(tl!("New Panel")).shortcut_text("Ctrl+N")).clicked() {
                    self.add_panel();
                }
                if ui.add(egui::Button::new(tl!("Duplicate Panel")).shortcut_text("Ctrl+D")).clicked() {
                    self.duplicate_panel();
                }
                if ui.button(tl!("New Scene")).clicked() {
                    self.add_scene();
                }
                if ui.button(tl!("Delete Panel")).clicked() {
                    self.delete_panel();
                }
                ui.separator();
                if ui.button(tl!("Move Panel Earlier")).clicked() {
                    self.move_panel(false);
                }
                if ui.button(tl!("Move Panel Later")).clicked() {
                    self.move_panel(true);
                }
                ui.separator();
                if ui.add(egui::Button::new(tl!("Board Settings…")).shortcut_text("Ctrl+,")).clicked() {
                    self.settings_dialog = Some(crate::settings::Dialog::new(&self.project));
                }
            });
            ui.menu_button(tl!("View"), |ui| {
                ui.radio_value(&mut self.view, View::Board, tl!("Board (panel grid)"));
                ui.radio_value(&mut self.view, View::Panel, tl!("Panel (draw and camera)"));
                ui.separator();
                if ui.button(if self.playing { tl!("Pause Animatic") } else { tl!("Play Animatic") }).clicked() {
                    self.toggle_play();
                }
                ui.checkbox(&mut self.onion, tl!("Onion Skin (O)"));
                if ui.button(tl!("Fit Canvas (0)")).clicked() {
                    self.canvas_zoom = 1.0;
                    self.canvas_pan = egui::Vec2::ZERO;
                }
                ui.separator();
                ui.menu_button(tl!("Language"), |ui| {
                    if ui.radio(self.language.is_none(), tl!("System")).clicked() {
                        self.language = None;
                        crate::i18n::set_language(None);
                    }
                    if ui.radio(self.language.as_deref() == Some("en"), "English").clicked() {
                        self.language = Some("en".into());
                        crate::i18n::CURRENT.set(craft_i18n::Lang::EN);
                    }
                    for lang in crate::i18n::I18N.languages() {
                        let code = crate::i18n::I18N.code(lang).to_string();
                        if code == "en" {
                            continue;
                        }
                        if ui.radio(self.language.as_deref() == Some(code.as_str()), crate::i18n::I18N.name(lang)).clicked() {
                            crate::i18n::set_language(Some(&code));
                            self.language = Some(code);
                        }
                    }
                });
            });
            ui.separator();
            ui.selectable_value(&mut self.view, View::Board, tl!("Board"));
            ui.selectable_value(&mut self.view, View::Panel, tl!("Panel"));
        });
    }

    fn close_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_close {
            return;
        }
        // Esc cancels, as the platform's dialogs do.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.confirm_close = false;
            return;
        }
        egui::Window::new(tl!("Unsaved changes")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(tl!("Save the board before quitting?"));
            ui.horizontal(|ui| {
                if ui.button(tl!("Save")).clicked() && self.save() {
                    self.close_confirmed = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button(tl!("Don't Save")).clicked() {
                    self.close_confirmed = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button(tl!("Cancel")).clicked() {
                    self.confirm_close = false;
                }
            });
        });
    }

    /// Offer boards a crashed session left unsaved.
    fn recovery_dialog(&mut self, ctx: &egui::Context) {
        if self.recoverable.is_empty() {
            return;
        }
        let mut action: Option<(usize, bool)> = None;
        let mut later = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new(tl!("Recover unsaved work")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(tl!("BoardCraft closed before these boards were saved. Recover them?"));
            ui.separator();
            for (i, r) in self.recoverable.iter().enumerate() {
                ui.horizontal(|ui| {
                    let name = if r.name.is_empty() { tl!("Untitled board").to_string() } else { r.name.clone() };
                    ui.label(tlf!("{name}, autosaved {time}", name = name, time = r.saved));
                    if ui.button(tlf!("Recover {name}", name = name)).clicked() {
                        action = Some((i, true));
                    }
                    if ui.button(tlf!("Discard {name}", name = name)).clicked() {
                        action = Some((i, false));
                    }
                });
            }
            ui.separator();
            if ui.button(tl!("Decide Later")).on_hover_text(tl!("Keep the autosaves and ask again next time")).clicked() {
                later = true;
            }
        });
        if later {
            self.recoverable.clear();
            return;
        }
        let Some((i, recover)) = action else { return };
        if i >= self.recoverable.len() {
            return;
        }
        let r = self.recoverable.remove(i);
        if recover {
            match self.autosave.recover(&r) {
                Ok(p) => {
                    self.replace_project(p, r.document.clone());
                    self.edited();
                    self.status = tl!("Recovered unsaved work. Save it to keep it.").into();
                }
                Err(e) => self.status = tlf!("Could not recover: {error}", error = e),
            }
        } else {
            crate::autosave::Autosave::discard(&r);
        }
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        if self.shot_done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        ctx.request_repaint();
        if self.frames_until_shot > 0 {
            self.frames_until_shot -= 1;
            if self.frames_until_shot == 0 {
                let done = self.shot_done.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                    let result = Raster::from_rgba(img.size[0] as u32, img.size[1] as u32, rgba)
                        .ok_or_else(|| "bad screenshot size".to_string())
                        .and_then(|r| r.to_png().map_err(|e| e.to_string()))
                        .and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string()));
                    match result {
                        Ok(()) => println!("screenshot written to {}", path.display()),
                        Err(e) => eprintln!("screenshot failed: {e}"),
                    }
                    done.store(true, Ordering::SeqCst);
                })));
            }
        }
    }
}

/// Dim ("weak") text at 4.5:1 or better on panel backgrounds in both themes; egui's dark default
/// is below that.
pub fn readable_visuals(mut v: egui::Visuals) -> egui::Visuals {
    v.weak_text_color = Some(if v.dark_mode { egui::Color32::from_gray(160) } else { egui::Color32::from_gray(90) });
    v
}

fn apply_contrast(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Dark, readable_visuals(egui::Visuals::dark()));
    ctx.set_visuals_of(egui::Theme::Light, readable_visuals(egui::Visuals::light()));
}

impl eframe::App for BoardApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn on_exit(&mut self) {
        // A clean exit (saved, or the user chose Don't Save) leaves nothing to recover.
        if !self.dirty || self.close_confirmed {
            self.autosave.clear();
        }
    }
}

impl BoardApp {
    /// Per-frame work outside the UI pass: close requests, playback, window title.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if let Some(e) = self.autosave.tick(&self.project, self.path.as_deref(), self.dirty, self.edits) {
            self.status = tlf!("Autosave failed: {error}", error = e);
        }
        if self.dirty {
            // Keep frames coming so autosave runs while the user is idle.
            ctx.request_repaint_after(crate::autosave::INTERVAL);
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.close_confirmed && self.screenshot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        self.advance_playback(ctx);
        crate::audio_ui::sync_playback(self);
        crate::movie_ui::poll(self, ctx);
        let title = self.title();
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    /// The whole window.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.pointer.any_pressed()) {
            self.press = self.press.wrapping_add(1);
        }
        self.history.begin_frame(&self.project, self.selected, self.dirty, self.edits);
        self.draw_inner(ui, &ctx);
        let gesture = if ctx.input(|i| i.pointer.any_down() || i.pointer.any_released()) {
            crate::history::Gesture::Press(self.press)
        } else if let Some(id) = ctx.memory(|m| m.focused()) {
            crate::history::Gesture::Focus(id)
        } else {
            crate::history::Gesture::Other
        };
        self.history.end_frame(self.edits, gesture);
    }

    fn draw_inner(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let ctx = ctx.clone();
        self.shortcuts(&ctx);
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let fps = self.project.settings.fps_f64();
                ui.label(tlf!(
                    "{scenes} scenes · {panels} panels · {seconds} s",
                    scenes = self.project.scenes.len(),
                    panels = self.project.panel_count(),
                    seconds = format!("{:.1}", self.project.total_frames() as f64 / fps)
                ));
                ui.separator();
                // The status line is a polite live region: results of actions are announced.
                let status = ui.label(&self.status);
                ui.ctx().accesskit_node_builder(status.id, |n| n.set_live(egui::accesskit::Live::Polite));
                crate::movie_ui::status(self, ui);
            });
        });
        egui::Panel::bottom("timeline").resizable(true).default_size(150.0).min_size(110.0).show(ui, |ui| crate::timeline::show(self, ui));
        egui::Panel::right("inspector").resizable(true).default_size(330.0).min_size(260.0).show(ui, |ui| crate::inspector::show(self, ui));
        egui::CentralPanel::default().show(ui, |ui| match self.view {
            View::Board => crate::board::show(self, ui),
            View::Panel => crate::canvas::show(self, ui),
        });
        self.close_dialog(&ctx);
        self.recovery_dialog(&ctx);
        crate::settings::show(self, &ctx);
        crate::movie_ui::show(self, &ctx);
        crate::external::poll(self, &ctx);
        self.screenshot_step(&ctx);
    }
}
