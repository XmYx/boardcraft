//! Animatic export to OpenTimelineIO (`.otio`), the timeline FilmCraft imports.
//!
//! Written from the public OpenTimelineIO JSON schema documentation. The export is:
//!
//! - one `Timeline.1` named after the project, at the project frame rate;
//! - one video `Track.1` with one `Clip.2` per panel, in board order, each lasting the panel's
//!   duration and referencing a rendered still (`ExternalReference.1`, absolute `file://` URL);
//! - one `Marker.2` per scene on the timeline's `Stack.1`, named `<number> <heading>`;
//! - the panel's text, scene and camera keys in each clip's `metadata.boardcraft` (OTIO keeps
//!   metadata it does not understand, so they survive a round trip through other tools).
//!
//! The stills are the camera's view at the first frame of each panel at the project resolution.
//! Camera moves play in BoardCraft's own animatic; carrying them into FilmCraft as Motion keyframes
//! is on the roadmap (`docs/gaps.md`).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

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

fn rt(frames: u64, fps: u32) -> Value {
    json!({"OTIO_SCHEMA": "RationalTime.1", "rate": fps as f64, "value": frames as f64})
}

fn range(start: u64, dur: u64, fps: u32) -> Value {
    json!({"OTIO_SCHEMA": "TimeRange.1", "start_time": rt(start, fps), "duration": rt(dur, fps)})
}

/// The OTIO document for `project`, with stills expected in `stills_dir` under [`still_name`]s.
pub fn timeline_json(project: &Project, stills_dir: &Path) -> Value {
    let fps = project.settings.fps.max(1);
    let mut clips = Vec::new();
    let mut markers = Vec::new();
    let mut t = 0u64;
    for (si, scene) in project.scenes.iter().enumerate() {
        let scene_start = t;
        let mut scene_len = 0u64;
        for (pi, panel) in scene.panels.iter().enumerate() {
            let dur = panel.duration.max(1) as u64;
            let label = project.panel_label(panel.id);
            let url = file_url(&stills_dir.join(still_name(si, pi)));
            clips.push(json!({
                "OTIO_SCHEMA": "Clip.2",
                "name": format!("Panel {label}"),
                "source_range": range(0, dur, fps),
                "media_references": {
                    "DEFAULT_MEDIA": {
                        "OTIO_SCHEMA": "ExternalReference.1",
                        "name": still_name(si, pi),
                        "target_url": url,
                        "available_range": null,
                        "available_image_bounds": null,
                        "metadata": {}
                    }
                },
                "active_media_reference_key": "DEFAULT_MEDIA",
                "effects": [],
                "markers": [],
                "enabled": true,
                "metadata": {"boardcraft": {
                    "panel_id": panel.id.0,
                    "panel": label,
                    "scene": scene.number,
                    "heading": scene.heading,
                    "dialogue": panel.dialogue,
                    "action": panel.action,
                    "notes": panel.notes,
                    "camera": panel.camera.keys,
                }}
            }));
            t += dur;
            scene_len += dur;
        }
        if scene_len > 0 {
            let name = if scene.heading.is_empty() { format!("Scene {}", scene.number) } else { format!("{} {}", scene.number, scene.heading) };
            markers.push(json!({
                "OTIO_SCHEMA": "Marker.2",
                "name": name.trim(),
                "marked_range": range(scene_start, 0, fps),
                "color": "GREEN",
                "comment": scene.source_scene_id,
                "metadata": {}
            }));
        }
    }
    json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": project.name,
        "global_start_time": null,
        "metadata": {"boardcraft": {"generator": crate::GENERATOR, "width": project.settings.width, "height": project.settings.height, "fps": fps}},
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "name": "Tracks",
            "source_range": null,
            "effects": [],
            "markers": markers,
            "enabled": true,
            "metadata": {},
            "children": [{
                "OTIO_SCHEMA": "Track.1",
                "name": "Boards",
                "kind": "Video",
                "source_range": null,
                "effects": [],
                "markers": [],
                "enabled": true,
                "metadata": {},
                "children": clips
            }]
        }
    })
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

/// Export the animatic: `<otio_path>` plus stills in `<otio stem>_stills/` beside it. The stills
/// folder is made absolute so FilmCraft finds the media wherever the `.otio` is opened from.
/// Returns the stills folder.
pub fn export_animatic(project: &Project, otio_path: &Path) -> Result<PathBuf> {
    let parent = otio_path.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&parent).map_err(|e| Error::io(&parent, e))?;
    let parent = parent.canonicalize().or_else(|_| std::path::absolute(&parent)).map_err(|e| Error::io(&parent, e))?;
    let stem = otio_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "animatic".into());
    let stills = parent.join(format!("{stem}_stills"));
    write_stills(project, &stills)?;
    let doc = serde_json::to_vec_pretty(&timeline_json(project, &stills)).map_err(|e| Error::Format(e.to_string()))?;
    craft_atomic::atomic_write(otio_path, &doc).map_err(|e| Error::io(otio_path, e))?;
    Ok(stills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fountain;
    use crate::model::Settings;

    fn board() -> Project {
        let script = fountain::parse("INT. A #1#\n\nA door opens.\n\nBOB\nHello there.\n\nEXT. B\n\nRain.\n");
        fountain::project_from_script(&script, Settings { width: 64, height: 36, fps: 24 })
    }

    #[test]
    fn timeline_shape() {
        let p = board();
        let v = timeline_json(&p, Path::new("/tmp/My Board_stills"));
        assert_eq!(v["OTIO_SCHEMA"], "Timeline.1");
        let track = &v["tracks"]["children"][0];
        assert_eq!(track["kind"], "Video");
        let clips = track["children"].as_array().unwrap();
        assert_eq!(clips.len(), 3);
        let total: f64 = clips.iter().map(|c| c["source_range"]["duration"]["value"].as_f64().unwrap()).sum();
        assert_eq!(total as u64, p.total_frames());
        assert_eq!(clips[0]["media_references"]["DEFAULT_MEDIA"]["target_url"], "file:///tmp/My%20Board_stills/s001_p001.png");
        assert_eq!(clips[1]["metadata"]["boardcraft"]["dialogue"], "BOB: Hello there.");
        let markers = v["tracks"]["markers"].as_array().unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0]["name"], "1 INT. A");
        assert_eq!(markers[1]["marked_range"]["start_time"]["value"].as_f64().unwrap() as u64, p.panel_start(p.scenes[1].panels[0].id).unwrap());
    }

    #[test]
    fn export_writes_files() {
        let dir = std::env::temp_dir().join(format!("boardcraft-otio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let stills = export_animatic(&board(), &dir.join("cut.otio")).unwrap();
        assert!(stills.join("s002_p001.png").exists());
        let text = std::fs::read_to_string(dir.join("cut.otio")).unwrap();
        assert!(text.contains("Timeline.1"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn urls_on_windows_style_paths() {
        assert_eq!(file_url(Path::new("C:\\Boards\\a b.png")), "file:///C:/Boards/a%20b.png");
    }
}
