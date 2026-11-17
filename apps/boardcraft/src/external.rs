//! Edit a layer in any image editor (GIMP, Krita, Photoshop, Affinity, Clip Studio…): the layer is
//! written as a PNG and opened with the system's handler for PNG files; when the editor saves the
//! file, BoardCraft reloads it into the layer (an undoable edit). Images of another size are
//! fitted into the canvas.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use boardcraft_core::{Id, Raster};

use crate::app::BoardApp;

const POLL: Duration = Duration::from_millis(800);

pub struct Watch {
    pub panel: Id,
    pub layer: Id,
    pub path: PathBuf,
    modified: Option<SystemTime>,
    checked: Instant,
}

fn safe(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).take(40).collect()
}

/// Write the layer and open it in the user's image editor.
pub fn start(app: &mut BoardApp, panel: Id, layer: Id) {
    let Some(l) = app.project.panel(panel).and_then(|p| p.layer(layer)) else { return };
    let dir = std::env::temp_dir().join("boardcraft-edit");
    let label = app.project.panel_label(panel);
    let path = dir.join(format!("{}-{}-{}-{}.png", safe(&app.project.name), safe(&label), safe(&l.name), layer.0));
    let written = std::fs::create_dir_all(&dir)
        .map_err(|e| e.to_string())
        .and_then(|_| l.pixels.to_png().map_err(|e| e.to_string()))
        .and_then(|png| boardcraft_core::format::write_atomic(&path, &png).map_err(|e| e.to_string()));
    if let Err(e) = written {
        app.status = tlf!("Could not write {file}: {error}", file = path.display(), error = e);
        return;
    }
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    app.external.retain(|w| !(w.panel == panel && w.layer == layer));
    app.external.push(Watch { panel, layer, path: path.clone(), modified, checked: Instant::now() });
    match open::that_detached(&path) {
        Ok(()) => app.status = tlf!("Editing {file} in your image editor. Save there to update the panel.", file = path.display()),
        Err(e) => app.status = tlf!("Wrote {file}; open it in your image editor ({error})", file = path.display(), error = e),
    }
}

/// Reload layers whose file changed. Runs inside the frame, so a reload is an undo step.
pub fn poll(app: &mut BoardApp, ctx: &egui::Context) {
    if app.external.is_empty() {
        return;
    }
    ctx.request_repaint_after(POLL);
    let (w, h) = (app.project.settings.width, app.project.settings.height);
    let mut changed = Vec::new();
    for watch in app.external.iter_mut() {
        if watch.checked.elapsed() < POLL {
            continue;
        }
        watch.checked = Instant::now();
        let m = std::fs::metadata(&watch.path).and_then(|m| m.modified()).ok();
        if m.is_some() && m != watch.modified {
            watch.modified = m;
            changed.push((watch.panel, watch.layer, watch.path.clone()));
        }
    }
    for (panel, layer, path) in changed {
        let decoded = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| Raster::decode(&b).map_err(|e| e.to_string()));
        match decoded {
            Ok(img) => {
                let pixels = if img.width() == w && img.height() == h {
                    img
                } else {
                    let mut r = Raster::new(w, h);
                    r.place_fit(&img);
                    r
                };
                if let Some(p) = app.project.panel_mut(panel)
                    && let Some(l) = p.layer_mut(layer)
                {
                    l.pixels = pixels;
                    p.touch();
                    app.edited();
                    app.status = tlf!("Updated the layer from {file}", file = path.display());
                }
            }
            // A half-written file: try again on the next poll.
            Err(_) => {
                if let Some(wch) = app.external.iter_mut().find(|x| x.path == path) {
                    wch.modified = None;
                }
            }
        }
    }
    let project = &app.project;
    app.external.retain(|wch| project.panel(wch.panel).is_some_and(|p| p.layer(wch.layer).is_some()));
}

#[cfg(test)]
mod tests {
    use super::safe;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe("../a b/é"), "___a_b_é");
        assert!(safe(&"x".repeat(100)).len() <= 40);
    }
}
