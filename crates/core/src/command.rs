//! Every edit is a command.
//!
//! A [`Command`] is a serializable description of one change to a [`Project`]; [`apply`] performs
//! it and returns what changed. The app, `boardcraft-cli` (`boardcraft-cli exec`) and tests drive
//! the board through the same commands, so every feature is scriptable by agents. Commands are JSON
//! with a `"cmd"` tag, for example `{"cmd": "set_duration", "panel": 3, "frames": 48}`.

use serde::{Deserialize, Serialize};

use crate::camera::CameraKey;
use crate::error::{Error, Result};
use crate::fountain;
use crate::model::{Id, MAX_PANEL_FRAMES, Project};
use crate::script::{ScriptFormat, parse_script_as, reimport};

/// A panel text field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextField {
    Dialogue,
    Action,
    Notes,
    Shot,
}

/// One change to a board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Append a scene with one blank panel.
    AddScene {
        heading: String,
    },
    /// Add a blank panel after `after` (or at the end of `scene`).
    AddPanel {
        scene: Option<Id>,
        after: Option<Id>,
    },
    DeletePanel {
        panel: Id,
    },
    /// Copy a panel (drawing, text, camera, timing) right after it.
    DuplicatePanel {
        panel: Id,
    },
    /// Change the canvas size without stretching (layers centred, cropped or extended).
    SetCanvas {
        width: u32,
        height: u32,
    },
    /// Change the frame rate; `keep_time` rescales durations so panels last as long as before.
    SetFrameRate {
        fps: u32,
        #[serde(default)]
        ntsc: bool,
        #[serde(default = "yes")]
        keep_time: bool,
    },
    /// Move a panel one step earlier or later in board order.
    MovePanel {
        panel: Id,
        later: bool,
    },
    SetDuration {
        panel: Id,
        frames: u32,
    },
    SetText {
        panel: Id,
        field: TextField,
        text: String,
    },
    SetSceneHeading {
        scene: Id,
        heading: String,
    },
    /// Add or replace the camera key on `key.frame`.
    SetCameraKey {
        panel: Id,
        key: CameraKey,
    },
    RemoveCameraKey {
        panel: Id,
        frame: u32,
    },
    ClearCamera {
        panel: Id,
    },
    AddLayer {
        panel: Id,
        name: String,
    },
    DeleteLayer {
        panel: Id,
        layer: Id,
    },
    SetLayerVisible {
        panel: Id,
        layer: Id,
        visible: bool,
    },
    SetLayerOpacity {
        panel: Id,
        layer: Id,
        opacity: f32,
    },
    /// Append the scenes of a Fountain screenplay.
    ImportFountain {
        text: String,
    },
    /// Import a screenplay (Fountain, Final Draft `.fdx` or ScriptCraft scene list JSON; detected
    /// from `name` and the text unless `format` is given). `merge` (the default) updates the board
    /// by stable scene id (see `script::reimport`); otherwise the scenes are appended.
    ImportScript {
        #[serde(default)]
        name: String,
        text: String,
        #[serde(default)]
        format: Option<ScriptFormat>,
        #[serde(default = "yes")]
        merge: bool,
    },
    /// Take panel timing from an edited timeline (OTIO, EDL, FCP7 XML or FCPXML; detected from
    /// `name` and the text).
    Conform {
        name: String,
        text: String,
    },
}

fn yes() -> bool {
    true
}

/// What a command did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    /// An id the command created (scene, panel or layer), if any.
    pub created: Option<Id>,
    /// A short description for a status bar or log.
    pub message: String,
}

fn panel_missing(id: Id) -> Error {
    Error::NotFound(format!("panel {}", id.0))
}

fn ok(message: impl Into<String>) -> Result<Outcome> {
    Ok(Outcome { created: None, message: message.into() })
}

/// Apply `cmd` to `project`. On error the project is unchanged.
pub fn apply(project: &mut Project, cmd: &Command) -> Result<Outcome> {
    match cmd {
        Command::AddScene { heading } => {
            let s = project.add_scene(heading);
            let p = project.add_panel(s, None)?;
            Ok(Outcome { created: Some(p), message: format!("added scene {}", project.scene(s).map(|x| x.number.as_str()).unwrap_or("")) })
        }
        Command::AddPanel { scene, after } => {
            let scene = match (scene, after) {
                (Some(s), _) => *s,
                (None, Some(a)) => project.scene_of(*a).map(|s| s.id).ok_or_else(|| panel_missing(*a))?,
                (None, None) => match project.scenes.last() {
                    Some(s) => s.id,
                    None => project.add_scene(""),
                },
            };
            let id = project.add_panel(scene, *after)?;
            Ok(Outcome { created: Some(id), message: format!("added panel {}", project.panel_label(id)) })
        }
        Command::DeletePanel { panel } => {
            if project.panel(*panel).is_none() {
                return Err(panel_missing(*panel));
            }
            if project.panel_count() <= 1 {
                return Err(Error::Unsupported("a board keeps at least one panel".into()));
            }
            project.delete_panel(*panel);
            project.scenes.retain(|s| !s.panels.is_empty());
            ok("deleted panel")
        }
        Command::DuplicatePanel { panel } => {
            let id = project.duplicate_panel(*panel)?;
            Ok(Outcome { created: Some(id), message: format!("duplicated as {}", project.panel_label(id)) })
        }
        Command::SetCanvas { width, height } => {
            project.set_canvas(*width, *height)?;
            ok(format!("canvas {width}×{height}"))
        }
        Command::SetFrameRate { fps, ntsc, keep_time } => {
            project.set_rate(*fps, *ntsc, *keep_time)?;
            ok(format!("{} fps", project.settings.rate_label()))
        }
        Command::MovePanel { panel, later } => {
            if project.panel(*panel).is_none() {
                return Err(panel_missing(*panel));
            }
            let moved = project.move_panel(*panel, *later);
            project.scenes.retain(|s| !s.panels.is_empty());
            ok(if moved { "moved panel" } else { "panel is already at the end" })
        }
        Command::SetDuration { panel, frames } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            p.duration = (*frames).clamp(1, MAX_PANEL_FRAMES);
            ok(format!("duration {} frames", p.duration))
        }
        Command::SetText { panel, field, text } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            let target = match field {
                TextField::Dialogue => &mut p.dialogue,
                TextField::Action => &mut p.action,
                TextField::Notes => &mut p.notes,
                TextField::Shot => &mut p.shot,
            };
            target.clone_from(text);
            ok("text set")
        }
        Command::SetSceneHeading { scene, heading } => {
            let s = project.scene_mut(*scene).ok_or_else(|| Error::NotFound(format!("scene {}", scene.0)))?;
            s.heading.clone_from(heading);
            ok("heading set")
        }
        Command::SetCameraKey { panel, key } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            p.camera.set_key(*key);
            ok("camera key set")
        }
        Command::RemoveCameraKey { panel, frame } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            if p.camera.remove_key(*frame) { ok("camera key removed") } else { Err(Error::NotFound(format!("camera key on frame {frame}"))) }
        }
        Command::ClearCamera { panel } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            p.camera.keys.clear();
            ok("camera cleared")
        }
        Command::AddLayer { panel, name } => {
            if project.panel(*panel).is_none() {
                return Err(panel_missing(*panel));
            }
            let layer = project.new_layer(name);
            let id = layer.id;
            if let Some(p) = project.panel_mut(*panel) {
                p.layers.push(layer);
                p.touch();
            }
            Ok(Outcome { created: Some(id), message: "added layer".into() })
        }
        Command::DeleteLayer { panel, layer } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            if p.layer(*layer).is_none() {
                return Err(Error::NotFound(format!("layer {}", layer.0)));
            }
            if p.layers.len() <= 1 {
                return Err(Error::Unsupported("a panel keeps at least one layer".into()));
            }
            p.layers.retain(|l| l.id != *layer);
            p.touch();
            ok("deleted layer")
        }
        Command::SetLayerVisible { panel, layer, visible } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            p.layer_mut(*layer).ok_or_else(|| Error::NotFound(format!("layer {}", layer.0)))?.visible = *visible;
            p.touch();
            ok("layer visibility set")
        }
        Command::SetLayerOpacity { panel, layer, opacity } => {
            let p = project.panel_mut(*panel).ok_or_else(|| panel_missing(*panel))?;
            let o = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 1.0 };
            p.layer_mut(*layer).ok_or_else(|| Error::NotFound(format!("layer {}", layer.0)))?.opacity = o;
            p.touch();
            ok("layer opacity set")
        }
        Command::ImportScript { name, text, format, merge } => {
            let format = format.unwrap_or_else(|| ScriptFormat::detect(name, text.as_bytes()));
            let script = parse_script_as(format, name, text.as_bytes())?;
            if *merge {
                let r = reimport(project, &script);
                ok(r.summary())
            } else {
                let (s, n) = fountain::append_to_project(project, &script);
                ok(format!("added {s} scenes, {n} panels"))
            }
        }
        Command::Conform { name, text } => {
            let r = crate::timeline::conform(project, std::path::Path::new(name), text.as_bytes())?;
            ok(r.summary())
        }
        Command::ImportFountain { text } => {
            let script = fountain::parse(text);
            let (s, n) = fountain::append_to_project(project, &script);
            ok(format!("added {s} scenes, {n} panels"))
        }
    }
}

/// Parse a command from JSON.
pub fn parse(json: &str) -> Result<Command> {
    serde_json::from_str(json).map_err(|e| Error::Format(format!("command: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_commands_drive_a_board() {
        let mut p = Project::blank("t");
        let first = p.scenes[0].panels[0].id;
        let out = apply(&mut p, &parse(&format!(r#"{{"cmd":"add_panel","after":{}}}"#, first.0)).unwrap()).unwrap();
        let second = out.created.unwrap();
        apply(&mut p, &parse(&format!(r#"{{"cmd":"set_duration","panel":{},"frames":12}}"#, second.0)).unwrap()).unwrap();
        apply(&mut p, &parse(&format!(r#"{{"cmd":"set_text","panel":{},"field":"dialogue","text":"BOB: Hi"}}"#, second.0)).unwrap()).unwrap();
        apply(&mut p, &Command::SetCameraKey { panel: second, key: CameraKey { frame: 11, center: [0.6, 0.5], zoom: 2.0 } }).unwrap();
        apply(&mut p, &Command::MovePanel { panel: second, later: false }).unwrap();
        assert_eq!(p.scenes[0].panels[0].id, second);
        assert_eq!(p.panel(second).unwrap().duration, 12);
        assert_eq!(p.panel(second).unwrap().dialogue, "BOB: Hi");
        assert_eq!(p.panel(second).unwrap().camera.keys.len(), 1);
        apply(&mut p, &Command::ImportFountain { text: "INT. A\n\nRain.\n".into() }).unwrap();
        assert_eq!(p.scenes.len(), 2);
        // Merging the same script again matches the scene by heading instead of adding it.
        let cmd = parse(r#"{"cmd":"import_script","name":"a.fountain","text":"INT. A\n\nRain, harder.\n"}"#).unwrap();
        let out = apply(&mut p, &cmd).unwrap();
        assert_eq!(p.scenes.len(), 2, "{}", out.message);
        let a = p.scenes.iter().find(|s| s.heading == "INT. A").unwrap();
        assert_eq!(a.panels[0].action, "Rain, harder.");
        assert_eq!(p.scenes[0].heading, "INT. A", "script scenes first, scenes the script lacks after");
    }

    #[test]
    fn errors_leave_the_board_alone() {
        let mut p = Project::blank("t");
        let before = p.clone();
        let only = p.scenes[0].panels[0].id;
        assert!(apply(&mut p, &Command::DeletePanel { panel: only }).is_err());
        assert!(apply(&mut p, &Command::SetDuration { panel: Id(999), frames: 3 }).is_err());
        let layer = p.scenes[0].panels[0].layers[0].id;
        assert!(apply(&mut p, &Command::DeleteLayer { panel: only, layer }).is_err());
        assert!(parse(r#"{"cmd":"explode"}"#).is_err());
        assert!(parse("not json").is_err());
        assert_eq!(p, before);
    }

    #[test]
    fn duplicate_canvas_and_rate() {
        let mut p = crate::demo::demo_project();
        let first = p.scenes[0].panels[0].id;
        let n = p.panel_count();
        let copy = apply(&mut p, &Command::DuplicatePanel { panel: first }).unwrap().created.unwrap();
        assert_eq!(p.panel_count(), n + 1);
        assert_eq!(p.scenes[0].panels[1].id, copy);
        assert_ne!(p.panel(copy).unwrap().layers[0].id, p.panel(first).unwrap().layers[0].id);
        assert!(p.panel(copy).unwrap().layers[1].pixels.shares_pixels(&p.panel(first).unwrap().layers[1].pixels));
        let total_s = p.total_frames() as f64 / p.settings.fps_f64();
        apply(&mut p, &parse(r#"{"cmd":"set_frame_rate","fps":24,"ntsc":true}"#).unwrap()).unwrap();
        assert_eq!(p.settings.rate_label(), "23.976");
        let after_s = p.total_frames() as f64 / p.settings.fps_f64();
        assert!((after_s - total_s).abs() < 0.2, "{total_s} → {after_s}");
        apply(&mut p, &Command::SetCanvas { width: 1920, height: 816 }).unwrap();
        assert!(p.panels().all(|(_, x)| x.layers.iter().all(|l| l.pixels.width() == 1920 && l.pixels.height() == 816)));
        assert!(apply(&mut p, &Command::SetCanvas { width: 1, height: 816 }).is_err());
        assert!(apply(&mut p, &Command::SetFrameRate { fps: 0, ntsc: false, keep_time: true }).is_err());
        // A file with ntsc reads back.
        let back = crate::format::from_bytes(&crate::format::to_bytes(&p).unwrap()).unwrap();
        assert!(back.settings.ntsc);
    }

    #[test]
    fn hostile_values_are_clamped() {
        let mut p = Project::blank("t");
        let id = p.scenes[0].panels[0].id;
        let layer = p.scenes[0].panels[0].layers[0].id;
        apply(&mut p, &Command::SetDuration { panel: id, frames: u32::MAX }).unwrap();
        apply(&mut p, &Command::SetLayerOpacity { panel: id, layer, opacity: f32::NAN }).unwrap();
        assert_eq!(p.panel(id).unwrap().duration, MAX_PANEL_FRAMES);
        assert_eq!(p.panel(id).unwrap().layers[0].opacity, 1.0);
    }
}
