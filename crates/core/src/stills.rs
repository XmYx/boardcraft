//! Rendered panel stills: what the camera sees at the first frame of each panel, at the project
//! resolution. Timeline exports ([`crate::timeline`]) reference them; File ▸ Export Panel Images
//! writes them on their own.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::{Panel, Project};
use crate::raster::Raster;

/// File name of a panel's still: `s<scene>_p<panel>.png` with 3-digit board positions.
pub fn still_name(scene_index: usize, panel_index: usize) -> String {
    format!("s{:03}_p{:03}.png", scene_index + 1, panel_index + 1)
}

/// Render what the camera sees at `frame` of `panel` at the project resolution.
pub fn render_frame(project: &Project, panel: &Panel, frame: f32) -> Raster {
    let composite = panel.composite(&project.settings);
    composite.render_view(panel.camera.view(frame), project.settings.width, project.settings.height)
}

/// `file://` URL of an absolute path, percent-encoding everything outside the RFC 3986 unreserved
/// set and `/`.
pub fn file_url(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Render every panel's still into `stills_dir` (created if needed). Returns the files written.
pub fn write_stills(project: &Project, stills_dir: &Path) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(stills_dir).map_err(|e| Error::io(stills_dir, e))?;
    let mut written = Vec::new();
    for (si, scene) in project.scenes.iter().enumerate() {
        for (pi, panel) in scene.panels.iter().enumerate() {
            let path = stills_dir.join(still_name(si, pi));
            let png = render_frame(project, panel, 0.0).to_png()?;
            craft_atomic::atomic_write(&path, &png).map_err(|e| Error::io(&path, e))?;
            written.push(path);
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fountain;
    use crate::model::Settings;

    #[test]
    fn writes_one_still_per_panel() {
        let p = fountain::project_from_script(&fountain::parse("INT. A\n\nOne.\n\nEXT. B\n\nTwo.\n"), Settings { width: 64, height: 36, fps: 24 });
        let dir = std::env::temp_dir().join(format!("boardcraft-stills-{}", std::process::id()));
        let files = write_stills(&p, &dir).unwrap();
        assert_eq!(files.len(), 2);
        assert!(dir.join("s002_p001.png").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn urls_on_windows_style_paths() {
        assert_eq!(file_url(Path::new("C:\\Boards\\a b.png")), "file:///C:/Boards/a%20b.png");
    }
}
