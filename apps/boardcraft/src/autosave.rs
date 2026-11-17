//! Autosave and crash recovery.
//!
//! While a board has unsaved changes it is written every [`INTERVAL`] to
//! `<data dir>/Autosave/<session>.boardcraft` on a background thread (atomic writes, so a crash
//! mid-save leaves the previous autosave). A sidecar `<session>.json` records the board's name, its
//! document path and a heartbeat the running app refreshes. At start, autosaves whose heartbeat is
//! older than [`STALE`] belong to a session that died: the app offers to recover them. Recovering
//! takes the files over for the new session, so they stay safe until the board is saved. A clean
//! save removes the autosave; quitting removes this session's files.
//!
//! Saves are incremental: autosaves and the user's own saves share one [`format::EncodeCache`], so
//! only layers painted since the last write are encoded to PNG again (pixels are copy-on-write, so
//! an unchanged layer still has the buffer the cache saw). On a large board an autosave then costs
//! the JSON and the archive, not hundreds of PNG encodes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use boardcraft_core::{Project, format};
use serde_json::{Value, json};

/// How often unsaved changes are written.
pub const INTERVAL: Duration = Duration::from_secs(30);
/// A heartbeat older than this means the session is gone.
pub const STALE: Duration = Duration::from_secs(120);
const FORMAT: &str = "boardcraft.autosave";

/// The per-user data folder: `BOARDCRAFT_DATA_DIR`, else the platform's application data folder.
pub fn default_data_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("BOARDCRAFT_DATA_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "windows") {
        return std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("BoardCraft"));
    }
    if cfg!(target_os = "macos") {
        return home.map(|h| h.join("Library/Application Support/BoardCraft"));
    }
    std::env::var_os("XDG_DATA_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".local/share")))
        .map(|d| d.join("boardcraft"))
}

/// A board a previous session left unsaved.
#[derive(Clone, Debug, PartialEq)]
pub struct Recoverable {
    pub board: PathBuf,
    pub sidecar: PathBuf,
    pub name: String,
    /// The document it belonged to, if it had been saved before.
    pub document: Option<PathBuf>,
    /// When it was last autosaved (local time, for display).
    pub saved: String,
}

pub struct Autosave {
    dir: Option<PathBuf>,
    session: String,
    last_write: Instant,
    last_edits: u64,
    last_heartbeat: Instant,
    busy: Arc<AtomicBool>,
    failed: Arc<std::sync::Mutex<Option<String>>>,
    /// Encoded layer PNGs shared by every save of this session's board.
    cache: Arc<std::sync::Mutex<format::EncodeCache>>,
}

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

impl Autosave {
    /// `dir` = None turns autosave off (screenshots, some tests).
    pub fn new(dir: Option<PathBuf>) -> Self {
        let session = format!("{}-{}", std::process::id(), chrono::Utc::now().timestamp_millis());
        Self {
            dir: dir.map(|d| d.join("Autosave")),
            session,
            last_write: Instant::now(),
            last_edits: 0,
            last_heartbeat: Instant::now() - STALE,
            busy: Arc::new(AtomicBool::new(false)),
            failed: Default::default(),
            cache: Default::default(),
        }
    }

    /// Save the board to the user's document, reusing the PNGs of unchanged layers.
    pub fn save_document(&self, project: &Project, path: &Path) -> boardcraft_core::Result<()> {
        let mut cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        format::save_cached(project, path, &mut cache)
    }

    fn board_path(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(format!("{}.boardcraft", self.session)))
    }

    fn sidecar_path(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(format!("{}.json", self.session)))
    }

    /// Called every frame. Writes when the board has had unsaved edits for [`INTERVAL`] and
    /// refreshes the heartbeat. Returns an error message once if a background save failed.
    pub fn tick(&mut self, project: &Project, document: Option<&Path>, dirty: bool, edits: u64) -> Option<String> {
        let (Some(board), Some(sidecar)) = (self.board_path(), self.sidecar_path()) else { return None };
        let failure = self.failed.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        let due = dirty && edits != self.last_edits && self.last_write.elapsed() >= INTERVAL;
        if due && !self.busy.swap(true, Ordering::SeqCst) {
            self.last_write = Instant::now();
            self.last_edits = edits;
            let snapshot = project.clone();
            let meta = json!({"format": FORMAT, "version": 1, "name": project.name, "document": document.map(|d| d.to_string_lossy().into_owned()), "saved": now(), "heartbeat": now()});
            let (busy, failed, cache) = (self.busy.clone(), self.failed.clone(), self.cache.clone());
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(move || -> Result<(), String> {
                    if let Some(dir) = board.parent() {
                        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                    }
                    let mut cache = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    format::save_cached(&snapshot, &board, &mut cache).map_err(|e| e.to_string())?;
                    drop(cache);
                    format::write_atomic(&sidecar, meta.to_string().as_bytes()).map_err(|e| e.to_string())
                });
                let err = match result {
                    Ok(Ok(())) => None,
                    Ok(Err(e)) => Some(e),
                    Err(_) => Some("autosave stopped unexpectedly".to_string()),
                };
                if let Some(e) = err {
                    *failed.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(e);
                }
                busy.store(false, Ordering::SeqCst);
            });
            self.last_heartbeat = Instant::now();
        } else if self.last_heartbeat.elapsed() >= INTERVAL && sidecar.exists() {
            self.last_heartbeat = Instant::now();
            if let Ok(text) = std::fs::read_to_string(&sidecar)
                && let Ok(mut v) = serde_json::from_str::<Value>(&text)
            {
                v["heartbeat"] = json!(now());
                let _ = format::write_atomic(&sidecar, v.to_string().as_bytes());
            }
        }
        failure
    }

    /// The board was saved (or discarded): this session's autosave is no longer needed.
    pub fn clear(&mut self) {
        if self.busy.load(Ordering::SeqCst) {
            // Let a running write finish before removing what it writes.
            let t = Instant::now();
            while self.busy.load(Ordering::SeqCst) && t.elapsed() < Duration::from_secs(5) {
                std::thread::yield_now();
            }
        }
        for p in [self.board_path(), self.sidecar_path()].into_iter().flatten() {
            let _ = std::fs::remove_file(p);
        }
        self.last_edits = 0;
    }

    /// Autosaves left by sessions that are gone, newest first.
    pub fn recoverable(&self) -> Vec<Recoverable> {
        let Some(dir) = &self.dir else { return Vec::new() };
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut out: Vec<(String, Recoverable)> = Vec::new();
        for e in entries.flatten() {
            let sidecar = e.path();
            if sidecar.extension().is_none_or(|x| x != "json") || sidecar.file_stem().is_some_and(|s| s == self.session.as_str()) {
                continue;
            }
            let board = sidecar.with_extension("boardcraft");
            let Ok(text) = std::fs::read_to_string(&sidecar) else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
            if v.get("format").and_then(Value::as_str) != Some(FORMAT) || !board.exists() {
                continue;
            }
            let beat = v.get("heartbeat").and_then(Value::as_str).and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok());
            let alive = beat.is_some_and(|t| chrono::Utc::now().signed_duration_since(t).to_std().is_ok_and(|age| age < STALE));
            if alive {
                continue; // another BoardCraft window is running
            }
            let saved_raw = v.get("saved").and_then(Value::as_str).unwrap_or("").to_string();
            let saved = chrono::DateTime::parse_from_rfc3339(&saved_raw)
                .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default();
            out.push((
                saved_raw,
                Recoverable {
                    board,
                    sidecar,
                    name: v.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    document: v.get("document").and_then(Value::as_str).map(PathBuf::from),
                    saved,
                },
            ));
        }
        out.sort_by(|a, b| b.0.cmp(&a.0));
        out.into_iter().map(|(_, r)| r).collect()
    }

    /// Open a recoverable board and take its files over for this session.
    pub fn recover(&mut self, r: &Recoverable) -> boardcraft_core::Result<Project> {
        let project = format::load(&r.board)?;
        if let (Some(board), Some(sidecar)) = (self.board_path(), self.sidecar_path()) {
            let _ = std::fs::rename(&r.board, &board);
            if let Ok(text) = std::fs::read_to_string(&r.sidecar)
                && let Ok(mut v) = serde_json::from_str::<Value>(&text)
            {
                v["heartbeat"] = json!(now());
                let _ = format::write_atomic(&sidecar, v.to_string().as_bytes());
            }
            let _ = std::fs::remove_file(&r.sidecar);
        }
        self.last_heartbeat = Instant::now();
        Ok(project)
    }

    /// Delete a recoverable board the user chose to discard.
    pub fn discard(r: &Recoverable) {
        let _ = std::fs::remove_file(&r.board);
        let _ = std::fs::remove_file(&r.sidecar);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("boardcraft-autosave-{name}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    #[test]
    fn autosave_then_recover_after_a_crash() {
        let dir = tmp("crash");
        let mut a = Autosave::new(Some(dir.clone()));
        a.last_write = Instant::now() - INTERVAL;
        let mut p = boardcraft_core::demo::demo_project();
        p.name = "Crashed board".into();
        a.tick(&p, Some(Path::new("/boards/x.boardcraft")), true, 1);
        while a.busy.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        // The session "dies": its heartbeat goes stale.
        let side = a.sidecar_path().unwrap();
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&side).unwrap()).unwrap();
        v["heartbeat"] = json!("2000-01-01T00:00:00+00:00");
        std::fs::write(&side, v.to_string()).unwrap();
        // A new session finds it.
        let mut b = Autosave::new(Some(dir.clone()));
        let found = b.recoverable();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Crashed board");
        assert_eq!(found[0].document.as_deref(), Some(Path::new("/boards/x.boardcraft")));
        let back = b.recover(&found[0]).unwrap();
        assert_eq!(back.panel_count(), p.panel_count());
        assert!(b.board_path().unwrap().exists(), "taken over by the new session");
        assert!(b.recoverable().is_empty());
        b.clear();
        assert!(!b.board_path().unwrap().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn live_sessions_and_clean_boards_are_left_alone() {
        let dir = tmp("live");
        let mut a = Autosave::new(Some(dir.clone()));
        a.last_write = Instant::now() - INTERVAL;
        let p = boardcraft_core::Project::blank("x");
        a.tick(&p, None, false, 1);
        assert!(!a.board_path().unwrap().exists(), "nothing to save when clean");
        a.tick(&p, None, true, 2);
        while a.busy.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        assert!(Autosave::new(Some(dir.clone())).recoverable().is_empty(), "a fresh heartbeat means another window is running");
        assert!(Autosave::new(None).recoverable().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
