//! A small demo board: a short original screenplay imported through [`crate::fountain`], with a
//! procedurally drawn rough sketch on every panel and a few camera moves. Used by `--demo`, the
//! README screenshot and tests. All of it is original work under the project licence.

use crate::camera::CameraKey;
use crate::fountain;
use crate::model::{Project, Settings};
use crate::raster::{Brush, Raster};

/// The demo screenplay (original text, written for BoardCraft).
pub const DEMO_SCRIPT: &str = "Title: Paper Boats
Author: BoardCraft contributors

EXT. HARBOUR WALL - DAWN #1#
[[scene-id: demo-harbour]]

Grey water. A girl, JUNE (9), sits on the wall folding a paper boat.

JUNE
(whispering)
This one goes all the way.

She leans out and sets the boat on the water.

EXT. OPEN SEA - MORNING #2#
[[scene-id: demo-sea]]

The tiny boat rides a long swell. Gulls wheel overhead.

A huge freighter slides past, blotting out the sun.

CUT TO:

EXT. HARBOUR WALL - DUSK #3#
[[scene-id: demo-dusk]]

June waits. Her grandfather, OTTO, sits beside her.

OTTO
Boats always come back. Just not always to us.

June smiles and folds another.
";

fn ink() -> Brush {
    Brush { radius: 2.6, color: [28, 28, 34, 255], hardness: 0.6, ..Brush::default() }
}

fn blue() -> Brush {
    Brush { radius: 1.6, color: [90, 140, 220, 150], hardness: 0.3, ..Brush::default() }
}

fn circle(r: &mut Raster, cx: f32, cy: f32, rad: f32, b: &Brush) {
    let pts: Vec<(f32, f32)> = (0..=48).map(|i| i as f32 / 48.0 * std::f32::consts::TAU).map(|a| (cx + rad * a.cos(), cy + rad * a.sin())).collect();
    r.polyline(&pts, b);
}

fn wave_line(r: &mut Raster, y: f32, amp: f32, w: f32, b: &Brush) {
    let pts: Vec<(f32, f32)> = (0..=64).map(|i| i as f32 / 64.0 * w).map(|x| (x, y + amp * (x / 37.0).sin())).collect();
    r.polyline(&pts, b);
}

/// A stick figure standing (or sitting) with its feet/hips at `(x, y)`; `s` is its height.
fn figure(r: &mut Raster, x: f32, y: f32, s: f32, sitting: bool, b: &Brush) {
    let head = s * 0.11;
    let neck = y - s + head * 2.0;
    circle(r, x, y - s + head, head, b);
    let hip = if sitting { y } else { y - s * 0.45 };
    r.polyline(&[(x, neck), (x, hip)], b);
    r.polyline(&[(x - s * 0.18, neck + s * 0.25), (x, neck + s * 0.08), (x + s * 0.2, neck + s * 0.22)], b);
    if sitting {
        r.polyline(&[(x, hip), (x + s * 0.28, hip), (x + s * 0.3, hip + s * 0.3)], b);
    } else {
        r.polyline(&[(x - s * 0.15, y), (x, hip), (x + s * 0.15, y)], b);
    }
}

fn boat(r: &mut Raster, x: f32, y: f32, s: f32, b: &Brush) {
    r.polyline(&[(x - s, y - s * 0.3), (x - s * 0.6, y), (x + s * 0.6, y), (x + s, y - s * 0.3), (x - s, y - s * 0.3)], b);
    r.polyline(&[(x - s * 0.4, y - s * 0.3), (x, y - s), (x + s * 0.4, y - s * 0.3)], b);
}

fn gull(r: &mut Raster, x: f32, y: f32, s: f32, b: &Brush) {
    r.polyline(&[(x - s, y), (x - s * 0.5, y - s * 0.4), (x, y), (x + s * 0.5, y - s * 0.4), (x + s, y)], b);
}

/// Draw sketch number `n` onto `ink_layer` / `rough_layer` (canvas `w × h`).
fn sketch(n: usize, rough: &mut Raster, inkl: &mut Raster, w: f32, h: f32) {
    let (k, b) = (ink(), blue());
    match n {
        0 => {
            // Wide: harbour wall, sea, the girl.
            wave_line(rough, h * 0.55, 3.0, w, &b);
            inkl.polyline(&[(0.0, h * 0.62), (w * 0.7, h * 0.62), (w * 0.7, h)], &k);
            figure(inkl, w * 0.35, h * 0.62, h * 0.28, true, &k);
            wave_line(inkl, h * 0.72, 4.0, w, &Brush { radius: 1.8, ..k });
            circle(rough, w * 0.82, h * 0.2, h * 0.07, &b);
        }
        1 => {
            // Close-up: hands and boat.
            circle(rough, w * 0.5, h * 0.5, h * 0.35, &b);
            boat(inkl, w * 0.5, h * 0.62, h * 0.22, &k);
            inkl.polyline(&[(w * 0.18, h), (w * 0.3, h * 0.66), (w * 0.38, h * 0.62)], &k);
            inkl.polyline(&[(w * 0.82, h), (w * 0.7, h * 0.66), (w * 0.62, h * 0.62)], &k);
        }
        2 => {
            // The boat set on the water, low angle.
            wave_line(inkl, h * 0.7, 6.0, w, &k);
            boat(inkl, w * 0.55, h * 0.68, h * 0.1, &k);
            inkl.polyline(&[(w * 0.62, 0.0), (w * 0.58, h * 0.55)], &Brush { radius: 2.0, ..k });
            wave_line(rough, h * 0.8, 3.0, w, &b);
        }
        3 => {
            // Open sea, swell, gulls.
            for i in 0..4 {
                wave_line(inkl, h * (0.55 + 0.1 * i as f32), 8.0 - i as f32, w, &Brush { radius: 1.6 + i as f32 * 0.3, ..k });
            }
            boat(inkl, w * 0.4, h * 0.53, h * 0.06, &k);
            gull(inkl, w * 0.6, h * 0.2, h * 0.05, &k);
            gull(inkl, w * 0.72, h * 0.28, h * 0.035, &k);
            circle(rough, w * 0.2, h * 0.18, h * 0.08, &b);
        }
        4 => {
            // The freighter blots out the sun.
            inkl.polyline(&[(w * 0.05, h * 0.25), (w * 0.95, h * 0.2), (w * 0.9, h * 0.7), (w * 0.12, h * 0.72), (w * 0.05, h * 0.25)], &k);
            for i in 0..6 {
                let x = w * (0.2 + 0.12 * i as f32);
                inkl.polyline(&[(x, h * 0.28), (x, h * 0.68)], &Brush { radius: 1.4, ..k });
            }
            boat(inkl, w * 0.5, h * 0.86, h * 0.05, &k);
            wave_line(rough, h * 0.82, 4.0, w, &b);
        }
        5 => {
            // Dusk: two figures on the wall.
            inkl.polyline(&[(0.0, h * 0.64), (w, h * 0.64)], &k);
            figure(inkl, w * 0.4, h * 0.64, h * 0.26, true, &k);
            figure(inkl, w * 0.56, h * 0.64, h * 0.34, true, &k);
            circle(rough, w * 0.5, h * 0.58, h * 0.12, &b);
            wave_line(rough, h * 0.75, 3.0, w, &b);
        }
        _ => {
            // Over-the-shoulder: Otto, June beyond.
            circle(inkl, w * 0.22, h * 0.45, h * 0.22, &k);
            inkl.polyline(&[(0.0, h * 0.9), (w * 0.22, h * 0.67), (w * 0.45, h)], &k);
            figure(inkl, w * 0.65, h * 0.8, h * 0.4, true, &k);
            boat(rough, w * 0.75, h * 0.62, h * 0.05, &b);
        }
    }
}

/// The demo board.
pub fn demo_project() -> Project {
    let script = fountain::parse(DEMO_SCRIPT);
    let mut project = fountain::project_from_script(&script, Settings::default());
    let (w, h) = (project.settings.width as f32, project.settings.height as f32);
    let ids: Vec<_> = project.panels().map(|(_, p)| p.id).collect();
    for (n, id) in ids.iter().enumerate() {
        let rough = project.new_layer("Rough");
        let Some(panel) = project.panel_mut(*id) else { continue };
        panel.layers.insert(0, rough);
        panel.layers[1].name = "Ink".into();
        let (lo, hi) = panel.layers.split_at_mut(1);
        if let (Some(r), Some(i)) = (lo.first_mut(), hi.first_mut()) {
            sketch(n, &mut r.pixels, &mut i.pixels, w, h);
        }
        panel.touch();
        let end = panel.duration.saturating_sub(1);
        match n {
            0 => {
                panel.camera.set_key(CameraKey { frame: 0, center: [0.5, 0.5], zoom: 1.0 });
                panel.camera.set_key(CameraKey { frame: end, center: [0.38, 0.55], zoom: 1.6 });
            }
            3 => {
                panel.camera.set_key(CameraKey { frame: 0, center: [0.3, 0.45], zoom: 1.5 });
                panel.camera.set_key(CameraKey { frame: end, center: [0.62, 0.45], zoom: 1.5 });
            }
            _ => {}
        }
    }
    project
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_has_drawings_and_moves() {
        let p = demo_project();
        assert_eq!(p.name, "Paper Boats");
        assert_eq!(p.scenes.len(), 3);
        assert!(p.panel_count() >= 7);
        assert!(p.panels().all(|(_, x)| x.layers.len() == 2 && !x.layers[1].pixels.is_blank()));
        assert!(p.panels().filter(|(_, x)| x.camera.moves()).count() == 2);
    }
}
