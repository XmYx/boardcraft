//! File ▸ Export Movie: a small dialog (size, burn-ins), then the export runs on a background
//! thread with its progress and a Cancel button in the status bar.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};

use boardcraft_core::movie::{MovieOptions, MovieReport, MovieSize, movie_size, write_movie};

use crate::app::BoardApp;

/// The export dialog's choices.
pub struct Dialog {
    pub size: MovieSize,
    pub burn_in: bool,
    focus: bool,
}

impl Default for Dialog {
    fn default() -> Self {
        Self { size: MovieSize::Board, burn_in: true, focus: true }
    }
}

/// A running export.
pub struct Job {
    path: PathBuf,
    done: Arc<AtomicU64>,
    total: u64,
    cancel: Arc<AtomicBool>,
    rx: Receiver<Result<MovieReport, String>>,
}

impl Job {
    /// Fraction done (0..=1).
    pub fn fraction(&self) -> f32 {
        self.done.load(Ordering::Relaxed) as f32 / self.total.max(1) as f32
    }
}

/// Start exporting the board to `path` on a background thread.
pub fn start(app: &mut BoardApp, path: PathBuf, size: MovieSize, burn_in: bool) {
    if app.movie_job.is_some() {
        app.status = tl!("A movie export is already running").into();
        return;
    }
    let project = app.project.clone();
    let opts = MovieOptions { size, burn_in, panel_label: tl!("Panel {label}").to_string(), ..Default::default() };
    let (font, index) = crate::i18n::sheet_font();
    let done = Arc::new(AtomicU64::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let total = project.total_frames();
    let (d, c, p) = (done.clone(), cancel.clone(), path.clone());
    let spawned = std::thread::Builder::new().name("movie-export".into()).spawn(move || {
        let r = write_movie(&project, &p, &opts, font, index, &mut |n, _| {
            d.store(n, Ordering::Relaxed);
            !c.load(Ordering::Relaxed)
        });
        let _ = tx.send(r.map_err(|e| match e {
            boardcraft_core::Error::Cancelled => String::new(),
            e => e.to_string(),
        }));
    });
    match spawned {
        Ok(_) => {
            app.status = tlf!("Exporting the movie {file}…", file = path.display());
            app.movie_job = Some(Job { path, done, total, cancel, rx });
        }
        Err(e) => app.status = tlf!("Export failed: {error}", error = e),
    }
}

/// Report a finished export; keep frames coming while one runs.
pub fn poll(app: &mut BoardApp, ctx: &egui::Context) {
    let Some(job) = &app.movie_job else { return };
    match job.rx.try_recv() {
        Err(TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        Ok(result) => {
            let path = job.path.clone();
            app.movie_job = None;
            app.status = match result {
                Ok(r) => {
                    let mut s = tlf!("Wrote the movie {file} ({w}×{h}, {n} frames)", file = path.display(), w = r.width, h = r.height, n = r.frames);
                    if !r.audio_errors.is_empty() {
                        s.push(' ');
                        s.push_str(&tlf!("Sound left out: {list}", list = r.audio_errors.join("; ")));
                    }
                    s
                }
                Err(e) if e.is_empty() => tl!("Movie export cancelled").into(),
                Err(e) => tlf!("Export failed: {error}", error = e),
            };
        }
        Err(TryRecvError::Disconnected) => {
            app.movie_job = None;
            app.status = tl!("The movie export stopped unexpectedly").into();
        }
    }
}

/// Progress and Cancel in the status bar.
pub fn status(app: &mut BoardApp, ui: &mut egui::Ui) {
    let Some(job) = &app.movie_job else { return };
    let f = job.fraction();
    ui.add(egui::ProgressBar::new(f).desired_width(140.0).text(tlf!("Movie {percent}%", percent = (f * 100.0).round() as u32)));
    if ui.button(tl!("Cancel Export")).clicked() {
        job.cancel.store(true, Ordering::Relaxed);
    }
}

/// The export dialog, when open.
pub fn show(app: &mut BoardApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.movie_dialog.take() else { return };
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.status = tl!("Movie export cancelled").into();
        return;
    }
    let (w, h) = movie_size(&app.project, MovieSize::Board);
    let (hw, hh) = movie_size(&app.project, MovieSize::Half);
    let mut go = false;
    let mut close = false;
    egui::Window::new(tl!("Export Movie")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        ui.label(tl!("An MP4 (H.264 video, AAC sound) of the animatic with its camera moves and sound track."));
        ui.label(tl!("Size"));
        ui.horizontal(|ui| {
            let full = ui.radio_value(&mut dialog.size, MovieSize::Board, tlf!("Board size ({w}×{h})", w = w, h = h));
            if std::mem::take(&mut dialog.focus) {
                full.request_focus();
            }
            ui.radio_value(&mut dialog.size, MovieSize::Half, tlf!("Half size ({w}×{h})", w = hw, h = hh));
        });
        ui.checkbox(&mut dialog.burn_in, tl!("Burn-ins: panel number, timecode and dialogue"));
        ui.horizontal(|ui| {
            if ui.button(tl!("Export…")).clicked() {
                go = true;
            }
            if ui.button(tl!("Cancel")).clicked() {
                close = true;
            }
        });
    });
    if close {
        return;
    }
    if go {
        let name = format!("{}.mp4", app.project.name);
        if let Some(mut path) = rfd::FileDialog::new().add_filter("MP4", &["mp4"]).set_file_name(name).save_file() {
            if path.extension().is_none() {
                path.set_extension("mp4");
            }
            start(app, path, dialog.size, dialog.burn_in);
        }
        return;
    }
    app.movie_dialog = Some(dialog);
}
