//! Shot list export: one CSV row per panel (RFC 4180 quoting, UTF-8), for production schedules
//! and spreadsheets. Columns: scene, scene id, heading, panel, shot, start timecode, duration in
//! frames and seconds, dialogue, action, notes, still file (as the timeline exports name it).

use crate::model::Project;
use crate::stills::still_name;

/// CSV field quoting: always quoted, quotes doubled, line breaks kept.
fn field(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// `HH:MM:SS:FF` at the board's rate (whole frames, non-drop).
pub fn timecode(frame: u64, fps: u32) -> String {
    let fps = fps.max(1) as u64;
    let f = frame % fps;
    let s = frame / fps;
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60, f)
}

/// The shot list as CSV text.
pub fn csv(project: &Project) -> String {
    let fps = project.settings.fps_f64();
    let mut out = String::from("scene,scene_id,heading,panel,shot,start_tc,frames,seconds,dialogue,action,notes,still\r\n");
    let mut t = 0u64;
    for (si, scene) in project.scenes.iter().enumerate() {
        for (pi, p) in scene.panels.iter().enumerate() {
            let d = p.duration.max(1) as u64;
            let row = [
                field(&scene.number),
                field(&scene.source_scene_id),
                field(&scene.heading),
                field(&project.panel_label(p.id)),
                field(&p.shot),
                field(&timecode(t, project.settings.fps)),
                d.to_string(),
                format!("{:.3}", d as f64 / fps),
                field(&p.dialogue),
                field(&p.action),
                field(&p.notes),
                field(&still_name(si, pi)),
            ];
            out.push_str(&row.join(","));
            out.push_str("\r\n");
            t += d;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_and_quoting() {
        let mut p = crate::demo::demo_project();
        let id = p.scenes[0].panels[0].id;
        p.panel_mut(id).unwrap().shot = "WS".into();
        p.panel_mut(id).unwrap().notes = "say \"hi\",\nthen go".into();
        let text = csv(&p);
        let lines: Vec<&str> = text.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert!(lines[0].starts_with("scene,scene_id,heading,panel,shot"));
        assert!(lines[1].contains("\"1.1\",\"WS\",\"00:00:00:00\",92,3.833"));
        assert!(text.contains("\"say \"\"hi\"\",\nthen go\""));
        assert_eq!(text.matches("s003_p003.png").count(), 1);
        assert_eq!(timecode(24 * 3661 + 5, 24), "01:01:01:05");
    }
}
