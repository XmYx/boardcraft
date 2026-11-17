//! Rendered panel stills: what the camera sees at the first frame of each panel, at the project
//! resolution, annotations included. File ▸ Export Panel Images writes them; timeline exports
//! ([`crate::timeline`]) reference them, except that a panel whose camera moves exports its whole
//! canvas so the editor can replay the move.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::{Panel, Project};
use crate::raster::Raster;

/// File name of a panel's still: `s<scene>_p<panel>.png` with 3-digit board positions.
pub fn still_name(scene_index: usize, panel_index: usize) -> String {
    format!("s{:03}_p{:03}.png", scene_index + 1, panel_index + 1)
}

/// Render what the camera sees at `frame` of `panel` at the project resolution, turned frames
/// included.
pub fn render_frame(project: &Project, panel: &Panel, frame: f32) -> Raster {
    let composite = panel.composite(&project.settings);
    render_frame_from(&composite, panel, frame, project.settings.width, project.settings.height)
}

/// [`render_frame`] from a panel's [`Panel::composite`] made once, at any output size (movies),
/// turned frames included.
pub fn render_frame_from(composite: &Raster, panel: &Panel, frame: f32, width: u32, height: u32) -> Raster {
    let quad = panel.camera.quad(frame, width as f32 / height.max(1) as f32);
    let view = composite.render_quad(quad, width, height);
    // A frame turned past what the zoom limit can cover shows paper, never transparency.
    if quad.iter().flatten().any(|v| !(-1e-3..=1.001).contains(v)) {
        let mut paper = Raster::filled(width, height, crate::model::PAPER);
        paper.composite_over(&view, 1.0);
        return paper;
    }
    view
}

/// The still a timeline export uses for `panel`: the whole canvas when its camera moves (the
/// editor plays the move as Motion keyframes on it, see [`crate::timeline`]), else the camera's
/// view at the first frame.
pub fn export_still(project: &Project, panel: &Panel) -> Raster {
    if panel.camera.moves() { panel.composite(&project.settings) } else { render_frame(project, panel, 0.0) }
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

/// Render every panel's still (the camera's first frame) into `stills_dir` (created if needed).
/// Returns the files written.
pub fn write_stills(project: &Project, stills_dir: &Path) -> Result<Vec<PathBuf>> {
    write_with(project, stills_dir, |panel| render_frame(project, panel, 0.0))
}

/// The stills a timeline export references ([`export_still`]) into `stills_dir`.
pub fn write_export_stills(project: &Project, stills_dir: &Path) -> Result<Vec<PathBuf>> {
    write_with(project, stills_dir, |panel| export_still(project, panel))
}

fn write_with(project: &Project, stills_dir: &Path, render: impl Fn(&Panel) -> Raster) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(stills_dir).map_err(|e| Error::io(stills_dir, e))?;
    let mut written = Vec::new();
    for (si, scene) in project.scenes.iter().enumerate() {
        for (pi, panel) in scene.panels.iter().enumerate() {
            let path = stills_dir.join(still_name(si, pi));
            let png = render(panel).to_png()?;
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
        let p = fountain::project_from_script(&fountain::parse("INT. A\n\nOne.\n\nEXT. B\n\nTwo.\n"), Settings { width: 64, height: 36, fps: 24, ntsc: false });
        let dir = std::env::temp_dir().join(format!("boardcraft-stills-{}", std::process::id()));
        let files = write_stills(&p, &dir).unwrap();
        assert_eq!(files.len(), 2);
        assert!(dir.join("s002_p001.png").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn movie_frames_at_any_size_show_the_turned_camera() {
        // A canvas with a horizontal bar through the middle; a camera turned a quarter turn shows
        // it vertical, at the project size and at the smaller size a movie may use.
        let mut data = Vec::with_capacity(64 * 64 * 4);
        for y in 0..64 {
            for _ in 0..64 {
                data.extend_from_slice(if (30..34).contains(&y) { &[0, 0, 0, 255] } else { &[255, 255, 255, 255] });
            }
        }
        let r = Raster::from_rgba(64, 64, data).unwrap();
        let mut panel = Project::blank("t").new_panel();
        panel.camera.set_key(crate::camera::CameraKey { frame: 0, center: [0.5, 0.5], zoom: 1.0, rotation: 90.0 });
        for size in [64, 32] {
            let v = render_frame_from(&r, &panel, 0.0, size, size);
            let mid = size / 2;
            assert!(v.pixel(mid, 1)[0] < 80, "{size}: the bar runs top to bottom");
            assert!(v.pixel(1, mid)[0] > 200, "{size}: and not left to right");
        }
    }

    #[test]
    fn urls_on_windows_style_paths() {
        assert_eq!(file_url(Path::new("C:\\Boards\\a b.png")), "file:///C:/Boards/a%20b.png");
    }
}
