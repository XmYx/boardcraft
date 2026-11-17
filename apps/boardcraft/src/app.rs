//! Application state, menus, file actions and keyboard shortcuts.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use boardcraft_core::{Brush, Id, Project, Raster, Tool, format, fountain, otio};

use crate::args::{Cli, StartView};
use crate::textures::Textures;

/// Most pixel undo steps kept.
const UNDO_LIMIT: usize = 40;

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

/// One undoable change to a layer's pixels.
pub struct PixelUndo {
    pub panel: Id,
    pub layer: Id,
    pub pixels: Raster,
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
    pub undo: Vec<PixelUndo>,
    pub redo: Vec<PixelUndo>,
    pub stroke: Option<StrokeState>,
    /// Last pen pressure seen from a touch/pen device (None: mouse, full pressure).
    pub pen_pressure: Option<f32>,
    confirm_close: bool,
    close_confirmed: bool,
    screenshot: Option<PathBuf>,
    frames_until_shot: u32,
    shot_done: Arc<AtomicBool>,
    last_title: String,
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
        ctx.set_theme(egui::Theme::Dark);
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
            undo: Vec::new(),
            redo: Vec::new(),
            stroke: None,
            pen_pressure: None,
            confirm_close: false,
            close_confirmed: false,
            screenshot: cli.screenshot.clone(),
            frames_until_shot: 12,
            shot_done: Arc::new(AtomicBool::new(false)),
            last_title: String::new(),
        };
        if cli.demo {
            app.replace_project(boardcraft_core::demo::demo_project(), None);
            app.status = "Demo board: Paper Boats".into();
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
        self.undo.clear();
        self.redo.clear();
        self.stroke = None;
        self.playing = false;
        self.playhead = 0.0;
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
        let is_script = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("fountain") || e.eq_ignore_ascii_case("spmd") || e.eq_ignore_ascii_case("txt"));
        if is_script {
            match std::fs::read(path) {
                Ok(bytes) => {
                    let script = fountain::parse(&String::from_utf8_lossy(&bytes));
                    let project = fountain::project_from_script(&script, Default::default());
                    let n = project.panel_count();
                    self.replace_project(project, None);
                    self.dirty = true;
                    self.status = format!("Boarded {} scenes, {n} panels from {}", script.scenes.len(), path.display());
                }
                Err(e) => self.status = format!("Could not read {}: {e}", path.display()),
            }
            return;
        }
        match format::load(path) {
            Ok(p) => {
                self.replace_project(p, Some(path.to_path_buf()));
                self.status = format!("Opened {}", path.display());
            }
            Err(e) => self.status = format!("Could not open: {e}"),
        }
    }

    fn dialog() -> rfd::FileDialog {
        rfd::FileDialog::new()
    }

    pub fn open_dialog(&mut self) {
        if let Some(p) = Self::dialog().add_filter("BoardCraft project", &[format::EXTENSION]).add_filter("Fountain screenplay", &["fountain"]).pick_file() {
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
        match Self::dialog().add_filter("BoardCraft project", &[format::EXTENSION]).set_file_name(name).save_file() {
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
                self.status = format!("Saved {}", path.display());
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }

    pub fn import_fountain(&mut self) {
        let Some(path) = Self::dialog().add_filter("Fountain screenplay", &["fountain", "spmd", "txt"]).pick_file() else { return };
        match std::fs::read(&path) {
            Ok(bytes) => {
                let script = fountain::parse(&String::from_utf8_lossy(&bytes));
                // Into an untouched blank board: replace it; otherwise append the scenes.
                let untouched = !self.dirty && self.path.is_none() && self.project.panel_count() <= 1;
                if untouched {
                    let p = fountain::project_from_script(&script, self.project.settings);
                    self.replace_project(p, None);
                    self.dirty = true;
                    self.status = format!("Boarded {} scenes from {}", script.scenes.len(), path.display());
                } else {
                    let (s, n) = fountain::append_to_project(&mut self.project, &script);
                    self.dirty = true;
                    self.status = format!("Added {s} scenes, {n} panels from {}", path.display());
                }
            }
            Err(e) => self.status = format!("Could not read {}: {e}", path.display()),
        }
    }

    pub fn import_image(&mut self) {
        let Some(panel) = self.selected else {
            self.status = "Select a panel first".into();
            return;
        };
        let Some(path) = Self::dialog().add_filter("Images", &["png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp", "gif"]).pick_file() else { return };
        let decoded = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| Raster::decode(&b).map_err(|e| e.to_string()));
        match decoded {
            Ok(img) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Image".into());
                let mut layer = self.project.new_layer(&name);
                layer.pixels.place_fit(&img);
                let id = layer.id;
                if let Some(p) = self.project.panel_mut(panel) {
                    // Images go under the drawing layers.
                    p.layers.insert(0, layer);
                    p.touch();
                }
                self.active_layer = Some(id);
                self.dirty = true;
                self.status = format!("Imported {}", path.display());
            }
            Err(e) => self.status = format!("Could not import {}: {e}", path.display()),
        }
    }

    pub fn export_animatic(&mut self) {
        let Some(path) = Self::dialog().add_filter("OpenTimelineIO", &["otio"]).set_file_name(format!("{}.otio", self.project.name)).save_file() else {
            return;
        };
        match otio::export_animatic(&self.project, &path) {
            Ok(stills) => self.status = format!("Exported {} (stills in {}) — open it in FilmCraft with File ▸ Import", path.display(), stills.display()),
            Err(e) => self.status = format!("Export failed: {e}"),
        }
    }

    pub fn export_pngs(&mut self) {
        let Some(dir) = Self::dialog().pick_folder() else { return };
        match otio::write_stills(&self.project, &dir) {
            Ok(files) => self.status = format!("Wrote {} panel images to {}", files.len(), dir.display()),
            Err(e) => self.status = format!("Export failed: {e}"),
        }
    }

    pub fn ai_sketch(&mut self) {
        let Some(panel) = self.selected else { return };
        let result = boardcraft_core::ai::panel_request(&self.project, panel).and_then(|r| boardcraft_core::ai::generate_panel_image(&r));
        match result {
            Ok(img) => {
                let mut layer = self.project.new_layer("AI sketch");
                layer.pixels.place_fit(&img);
                if let Some(p) = self.project.panel_mut(panel) {
                    p.layers.insert(0, layer);
                    p.touch();
                }
                self.dirty = true;
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    // ---------------------------------------------------------------------------------------
    // Edits
    // ---------------------------------------------------------------------------------------

    pub fn add_panel(&mut self) {
        let (scene, after) = match self.selected.and_then(|id| self.project.scene_of(id).map(|s| (s.id, id))) {
            Some((s, id)) => (s, Some(id)),
            None => match self.project.scenes.last() {
                Some(s) => (s.id, None),
                None => (self.project.add_scene(""), None),
            },
        };
        match self.project.add_panel(scene, after) {
            Ok(id) => {
                self.select_and_seek(id);
                self.dirty = true;
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    pub fn add_scene(&mut self) {
        let s = self.project.add_scene("NEW SCENE");
        if let Ok(id) = self.project.add_panel(s, None) {
            self.select_and_seek(id);
        }
        self.dirty = true;
    }

    pub fn delete_panel(&mut self) {
        let Some(id) = self.selected else { return };
        if self.project.panel_count() <= 1 {
            self.status = "A board keeps at least one panel".into();
            return;
        }
        let next = {
            let ids: Vec<Id> = self.project.panels().map(|(_, p)| p.id).collect();
            let i = ids.iter().position(|x| *x == id).unwrap_or(0);
            ids.get(i + 1).or_else(|| i.checked_sub(1).and_then(|j| ids.get(j))).copied()
        };
        self.project.delete_panel(id);
        self.project.scenes.retain(|s| !s.panels.is_empty());
        self.undo.retain(|u| u.panel != id);
        self.redo.retain(|u| u.panel != id);
        self.textures.retain(&self.project);
        self.select(next);
        self.dirty = true;
    }

    pub fn move_panel(&mut self, later: bool) {
        if let Some(id) = self.selected
            && self.project.move_panel(id, later)
        {
            self.project.scenes.retain(|s| !s.panels.is_empty());
            self.dirty = true;
        }
    }

    pub fn step_selection(&mut self, delta: isize) {
        let ids: Vec<Id> = self.project.panels().map(|(_, p)| p.id).collect();
        let i = self.selected.and_then(|s| ids.iter().position(|x| *x == s)).unwrap_or(0) as isize;
        let j = (i + delta).clamp(0, ids.len() as isize - 1);
        if let Some(id) = ids.get(j as usize) {
            self.select_and_seek(*id);
        }
    }

    /// Record a layer's pixels before a change.
    pub fn push_undo(&mut self, panel: Id, layer: Id) {
        if let Some(l) = self.project.panel(panel).and_then(|p| p.layer(layer)) {
            self.undo.push(PixelUndo { panel, layer, pixels: l.pixels.clone() });
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    fn swap_pixels(&mut self, step: PixelUndo) -> Option<PixelUndo> {
        let p = self.project.panel_mut(step.panel)?;
        let l = p.layer_mut(step.layer)?;
        let old = std::mem::replace(&mut l.pixels, step.pixels);
        p.touch();
        Some(PixelUndo { panel: step.panel, layer: step.layer, pixels: old })
    }

    pub fn undo(&mut self) {
        if let Some(step) = self.undo.pop() {
            let panel = step.panel;
            if let Some(back) = self.swap_pixels(step) {
                self.redo.push(back);
                self.select(Some(panel));
                self.dirty = true;
            }
        }
    }

    pub fn redo(&mut self) {
        if let Some(step) = self.redo.pop()
            && let Some(back) = self.swap_pixels(step)
        {
            self.undo.push(back);
            self.dirty = true;
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
        self.playhead += dt * self.project.settings.fps.max(1) as f64;
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
        if space {
            self.toggle_play();
        }
        if left {
            self.step_selection(-1);
        }
        if right {
            self.step_selection(1);
        }
        if tab {
            self.view = if self.view == View::Board { View::Panel } else { View::Board };
        }
        if b {
            self.tool = CanvasTool::Brush;
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
            ui.menu_button("File", |ui| {
                if ui.button("New Board").clicked() {
                    self.replace_project(Project::blank("Untitled board"), None);
                }
                if ui.button("Open…").clicked() {
                    self.open_dialog();
                }
                ui.separator();
                if ui.button("Save").clicked() {
                    self.save();
                }
                if ui.button("Save As…").clicked() {
                    self.save_as();
                }
                ui.separator();
                if ui.button("Import Screenplay (Fountain)…").clicked() {
                    self.import_fountain();
                }
                if ui.button("Import Image to Panel…").clicked() {
                    self.import_image();
                }
                ui.separator();
                if ui.button("Export Animatic Timeline (OTIO)…").clicked() {
                    self.export_animatic();
                }
                if ui.button("Export Panel Images (PNG)…").clicked() {
                    self.export_pngs();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo Drawing")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo Drawing")).clicked() {
                    self.redo();
                }
                ui.separator();
                if ui.button("New Panel").clicked() {
                    self.add_panel();
                }
                if ui.button("New Scene").clicked() {
                    self.add_scene();
                }
                if ui.button("Delete Panel").clicked() {
                    self.delete_panel();
                }
                ui.separator();
                if ui.button("Move Panel Earlier").clicked() {
                    self.move_panel(false);
                }
                if ui.button("Move Panel Later").clicked() {
                    self.move_panel(true);
                }
            });
            ui.menu_button("View", |ui| {
                ui.radio_value(&mut self.view, View::Board, "Board (panel grid)");
                ui.radio_value(&mut self.view, View::Panel, "Panel (draw and camera)");
                ui.separator();
                if ui.button(if self.playing { "Pause Animatic" } else { "Play Animatic" }).clicked() {
                    self.toggle_play();
                }
            });
            ui.separator();
            ui.selectable_value(&mut self.view, View::Board, "Board");
            ui.selectable_value(&mut self.view, View::Panel, "Panel");
        });
    }

    fn close_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_close {
            return;
        }
        egui::Window::new("Unsaved changes").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("Save the board before quitting?");
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() && self.save() {
                    self.close_confirmed = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Don't Save").clicked() {
                    self.close_confirmed = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Cancel").clicked() {
                    self.confirm_close = false;
                }
            });
        });
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

impl eframe::App for BoardApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl BoardApp {
    /// Per-frame work outside the UI pass: close requests, playback, window title.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.close_confirmed && self.screenshot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        self.advance_playback(ctx);
        let title = self.title();
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    /// The whole window.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let fps = self.project.settings.fps.max(1) as f64;
                ui.label(format!(
                    "{} scenes · {} panels · {:.1} s",
                    self.project.scenes.len(),
                    self.project.panel_count(),
                    self.project.total_frames() as f64 / fps
                ));
                ui.separator();
                ui.label(&self.status);
            });
        });
        egui::Panel::bottom("timeline").resizable(true).default_size(150.0).min_size(110.0).show(ui, |ui| crate::timeline::show(self, ui));
        egui::Panel::right("inspector").resizable(true).default_size(330.0).min_size(260.0).show(ui, |ui| crate::inspector::show(self, ui));
        egui::CentralPanel::default().show(ui, |ui| match self.view {
            View::Board => crate::board::show(self, ui),
            View::Panel => crate::canvas::show(self, ui),
        });
        self.close_dialog(&ctx);
        self.screenshot_step(&ctx);
    }
}
