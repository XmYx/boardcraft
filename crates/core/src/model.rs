//! The document model: project → scenes → panels → layers.

use serde::{Deserialize, Serialize};

use crate::audio::AudioClip;
use crate::camera::Camera;
use crate::raster::{MAX_SIDE, Raster};

/// Default canvas: 1280 × 720 (16:9).
pub const DEFAULT_WIDTH: u32 = 1280;
pub const DEFAULT_HEIGHT: u32 = 720;
/// Default animatic rate.
pub const DEFAULT_FPS: u32 = 24;
/// Default panel duration: 2 seconds at 24 fps.
pub const DEFAULT_PANEL_FRAMES: u32 = 48;
/// Highest frame rate a project may use.
pub const MAX_FPS: u32 = 240;
/// Longest panel: one hour at the highest rate.
pub const MAX_PANEL_FRAMES: u32 = 3600 * MAX_FPS;
/// Smallest canvas side.
pub const MIN_SIDE: u32 = 16;

/// Identifier of a scene, panel or layer, unique within its project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(pub u64);

/// Canvas size and animatic rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    /// Frames per second: `fps`, or `fps × 1000/1001` when `ntsc` (23.976, 29.97, 59.94).
    pub fps: u32,
    #[serde(default)]
    pub ntsc: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { width: DEFAULT_WIDTH, height: DEFAULT_HEIGHT, fps: DEFAULT_FPS, ntsc: false }
    }
}

impl Settings {
    /// The exact rate as a fraction (`num / den` frames per second).
    pub fn rate(&self) -> (u32, u32) {
        let fps = self.fps.max(1);
        if self.ntsc { (fps * 1000, 1001) } else { (fps, 1) }
    }

    /// Frames per second as a float (display, playback, rounding).
    pub fn fps_f64(&self) -> f64 {
        let (n, d) = self.rate();
        n as f64 / d as f64
    }

    /// `23.976`, `24`, `29.97`.
    pub fn rate_label(&self) -> String {
        let f = self.fps_f64();
        if self.ntsc { format!("{:.3}", f).trim_end_matches('0').trim_end_matches('.').to_string() } else { self.fps.to_string() }
    }

    /// Whether every value is in range.
    pub fn is_valid(&self) -> bool {
        (MIN_SIDE..=MAX_SIDE).contains(&self.width) && (MIN_SIDE..=MAX_SIDE).contains(&self.height) && (1..=MAX_FPS).contains(&self.fps)
    }
}

/// One drawing layer of a panel. The pixels live beside the JSON in the project file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: Id,
    pub name: String,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(skip)]
    pub pixels: Raster,
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

/// One storyboard panel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub id: Id,
    /// Duration in frames at the project rate (at least 1).
    pub duration: u32,
    #[serde(default)]
    pub dialogue: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub notes: String,
    /// Bottom to top.
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub camera: Camera,
    /// Shot type or size (`WS`, `MS`, `CU`, `OTS`, …), free text.
    #[serde(default)]
    pub shot: String,
    /// Vector annotations drawn over the layers: strokes, arrows, motion and camera arrows
    /// (schema 2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<crate::annotate::Shape>,
    /// Bumped on every change to the pixels; lets a UI cache textures. Not saved.
    #[serde(skip)]
    pub revision: u64,
}

/// Paper colour under every panel.
pub const PAPER: [u8; 4] = [250, 250, 247, 255];

impl Panel {
    /// The visible layers flattened over paper at the canvas size.
    pub fn composite(&self, settings: &Settings) -> Raster {
        let mut out = Raster::filled(settings.width, settings.height, PAPER);
        for layer in self.layers.iter().filter(|l| l.visible && !l.pixels.is_empty()) {
            out.composite_over(&layer.pixels, layer.opacity);
        }
        crate::annotate::draw(&mut out, &self.annotations, 0.0, 0.0);
        out
    }

    /// [`Panel::composite`] of the rectangle `x, y, w, h` only (fast updates while drawing).
    pub fn composite_region(&self, settings: &Settings, x: u32, y: u32, w: u32, h: u32) -> Raster {
        let x = x.min(settings.width.saturating_sub(1));
        let y = y.min(settings.height.saturating_sub(1));
        let mut out = Raster::filled(w.clamp(1, settings.width - x), h.clamp(1, settings.height - y), PAPER);
        for layer in self.layers.iter().filter(|l| l.visible && !l.pixels.is_empty()) {
            out.composite_over(&layer.pixels.crop(x, y, w, h), layer.opacity);
        }
        crate::annotate::draw(&mut out, &self.annotations, x as f32, y as f32);
        out
    }

    pub fn layer(&self, id: Id) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn layer_mut(&mut self, id: Id) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// Mark the pixels changed.
    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

/// One scene: a heading and its panels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub id: Id,
    /// Scene number as written in the script (`"12"`, `"12A"`), or assigned in order.
    #[serde(default)]
    pub number: String,
    /// Scene heading (`INT. KITCHEN - NIGHT`).
    #[serde(default)]
    pub heading: String,
    /// Stable id of the scene in the screenplay it came from, when the script gave one.
    #[serde(default)]
    pub source_scene_id: String,
    #[serde(default)]
    pub panels: Vec<Panel>,
}

/// A storyboard project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub settings: Settings,
    #[serde(default)]
    pub scenes: Vec<Scene>,
    /// The sound clips under the animatic (see [`crate::audio`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<AudioClip>,
    /// Next id to hand out.
    pub next_id: u64,
}

impl Default for Project {
    fn default() -> Self {
        Self::new("Untitled board")
    }
}

/// Where a panel sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelPos {
    pub scene: usize,
    pub panel: usize,
}

impl Project {
    /// An empty project.
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), settings: Settings::default(), scenes: Vec::new(), audio: Vec::new(), next_id: 1 }
    }

    /// A project with one scene and one blank panel, ready to draw.
    pub fn blank(name: &str) -> Self {
        let mut p = Self::new(name);
        let s = p.add_scene("");
        let _ = p.add_panel(s, None);
        p
    }

    /// Hand out a fresh id.
    pub fn alloc(&mut self) -> Id {
        let id = Id(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    /// A blank layer at the canvas size.
    pub fn new_layer(&mut self, name: &str) -> Layer {
        Layer { id: self.alloc(), name: name.to_string(), visible: true, opacity: 1.0, pixels: Raster::new(self.settings.width, self.settings.height) }
    }

    /// A panel with one blank layer.
    pub fn new_panel(&mut self) -> Panel {
        let id = self.alloc();
        let layer = self.new_layer("Sketch");
        Panel {
            id,
            duration: DEFAULT_PANEL_FRAMES * self.settings.fps / DEFAULT_FPS,
            dialogue: String::new(),
            action: String::new(),
            notes: String::new(),
            shot: String::new(),
            layers: vec![layer],
            camera: Camera::default(),
            annotations: Vec::new(),
            revision: 0,
        }
    }

    /// Append a scene; its number is its position.
    pub fn add_scene(&mut self, heading: &str) -> Id {
        let id = self.alloc();
        let number = (self.scenes.len() + 1).to_string();
        self.scenes.push(Scene { id, number, heading: heading.to_string(), source_scene_id: String::new(), panels: Vec::new() });
        id
    }

    /// Add a blank panel to `scene`, after `after` (or at the end). Returns the new panel's id.
    pub fn add_panel(&mut self, scene: Id, after: Option<Id>) -> crate::Result<Id> {
        let panel = self.new_panel();
        let id = panel.id;
        let s = self.scenes.iter_mut().find(|s| s.id == scene).ok_or_else(|| crate::Error::NotFound(format!("scene {}", scene.0)))?;
        let at = after.and_then(|a| s.panels.iter().position(|p| p.id == a)).map(|i| i + 1).unwrap_or(s.panels.len());
        s.panels.insert(at, panel);
        Ok(id)
    }

    pub fn scene(&self, id: Id) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.id == id)
    }

    pub fn scene_mut(&mut self, id: Id) -> Option<&mut Scene> {
        self.scenes.iter_mut().find(|s| s.id == id)
    }

    /// Position of a panel.
    pub fn find_panel(&self, id: Id) -> Option<PanelPos> {
        self.scenes.iter().enumerate().find_map(|(si, s)| s.panels.iter().position(|p| p.id == id).map(|pi| PanelPos { scene: si, panel: pi }))
    }

    pub fn panel(&self, id: Id) -> Option<&Panel> {
        self.panels().find(|(_, p)| p.id == id).map(|(_, p)| p)
    }

    pub fn panel_mut(&mut self, id: Id) -> Option<&mut Panel> {
        self.scenes.iter_mut().flat_map(|s| s.panels.iter_mut()).find(|p| p.id == id)
    }

    /// The scene that holds a panel.
    pub fn scene_of(&self, panel: Id) -> Option<&Scene> {
        self.find_panel(panel).and_then(|pos| self.scenes.get(pos.scene))
    }

    /// Every panel in board order with its scene.
    pub fn panels(&self) -> impl Iterator<Item = (&Scene, &Panel)> {
        self.scenes.iter().flat_map(|s| s.panels.iter().map(move |p| (s, p)))
    }

    /// Number of panels.
    pub fn panel_count(&self) -> usize {
        self.scenes.iter().map(|s| s.panels.len()).sum()
    }

    /// Remove a panel; returns it.
    pub fn delete_panel(&mut self, id: Id) -> Option<Panel> {
        let pos = self.find_panel(id)?;
        let scene = self.scenes.get_mut(pos.scene)?;
        (pos.panel < scene.panels.len()).then(|| scene.panels.remove(pos.panel))
    }

    /// Move a panel one step earlier (`-1`) or later (`+1`) in board order, crossing into the
    /// neighbouring scene at a scene boundary.
    pub fn move_panel(&mut self, id: Id, later: bool) -> bool {
        let Some(pos) = self.find_panel(id) else { return false };
        let len = self.scenes.get(pos.scene).map(|s| s.panels.len()).unwrap_or(0);
        if later && pos.panel + 1 < len {
            if let Some(s) = self.scenes.get_mut(pos.scene) {
                s.panels.swap(pos.panel, pos.panel + 1);
            }
            return true;
        }
        if !later && pos.panel > 0 {
            if let Some(s) = self.scenes.get_mut(pos.scene) {
                s.panels.swap(pos.panel, pos.panel - 1);
            }
            return true;
        }
        let target = if later { pos.scene + 1 } else { pos.scene.wrapping_sub(1) };
        if target >= self.scenes.len() {
            return false;
        }
        let Some(panel) = self.delete_panel(id) else { return false };
        if let Some(s) = self.scenes.get_mut(target) {
            if later {
                s.panels.insert(0, panel);
            } else {
                s.panels.push(panel);
            }
            return true;
        }
        false
    }

    /// Total animatic length in frames.
    pub fn total_frames(&self) -> u64 {
        self.panels().map(|(_, p)| p.duration.max(1) as u64).sum()
    }

    /// First frame of a panel on the animatic timeline.
    pub fn panel_start(&self, id: Id) -> Option<u64> {
        let mut t = 0u64;
        for (_, p) in self.panels() {
            if p.id == id {
                return Some(t);
            }
            t += p.duration.max(1) as u64;
        }
        None
    }

    /// The panel showing at timeline `frame` and the frame offset into it. Past the end: the last
    /// panel's last frame.
    pub fn panel_at(&self, frame: u64) -> Option<(Id, u32)> {
        let mut t = 0u64;
        let mut last = None;
        for (_, p) in self.panels() {
            let d = p.duration.max(1) as u64;
            if frame < t + d {
                return Some((p.id, (frame - t) as u32));
            }
            t += d;
            last = Some((p.id, d as u32 - 1));
        }
        last
    }

    /// `"<scene number>.<panel number>"`, e.g. `3.2`.
    pub fn panel_label(&self, id: Id) -> String {
        match self.find_panel(id) {
            Some(pos) => {
                let n =
                    self.scenes.get(pos.scene).map(|s| if s.number.is_empty() { (pos.scene + 1).to_string() } else { s.number.clone() }).unwrap_or_default();
                format!("{n}.{}", pos.panel + 1)
            }
            None => String::new(),
        }
    }

    /// A copy of a panel with fresh ids for it and its layers, inserted right after it.
    pub fn duplicate_panel(&mut self, id: Id) -> crate::Result<Id> {
        let pos = self.find_panel(id).ok_or_else(|| crate::Error::NotFound(format!("panel {}", id.0)))?;
        let mut copy = self.panel(id).cloned().ok_or_else(|| crate::Error::NotFound(format!("panel {}", id.0)))?;
        copy.id = self.alloc();
        for l in &mut copy.layers {
            l.id = self.alloc();
        }
        let new_id = copy.id;
        if let Some(scene) = self.scenes.get_mut(pos.scene) {
            scene.panels.insert(pos.panel + 1, copy);
        }
        Ok(new_id)
    }

    /// Change the canvas size without stretching: every layer keeps its pixels, centred, cropped or
    /// extended with transparency. Camera keys keep their normalized positions.
    pub fn set_canvas(&mut self, width: u32, height: u32) -> crate::Result<()> {
        if !(MIN_SIDE..=MAX_SIDE).contains(&width) || !(MIN_SIDE..=MAX_SIDE).contains(&height) {
            return Err(crate::Error::Format(format!("canvas {width}×{height} is out of range ({MIN_SIDE}–{MAX_SIDE})")));
        }
        // Layers stay centred (`Raster::recanvas`); annotations shift with them.
        let dx = ((width as i64 - self.settings.width as i64) / 2) as f32;
        let dy = ((height as i64 - self.settings.height as i64) / 2) as f32;
        for p in self.scenes.iter_mut().flat_map(|s| s.panels.iter_mut()) {
            for l in &mut p.layers {
                l.pixels = l.pixels.recanvas(width, height);
            }
            for a in &mut p.annotations {
                a.translate(dx, dy);
            }
            p.touch();
        }
        self.settings.width = width;
        self.settings.height = height;
        Ok(())
    }

    /// Change the frame rate. With `keep_time`, durations and camera keys are rescaled so every
    /// panel lasts as long in seconds as before.
    pub fn set_rate(&mut self, fps: u32, ntsc: bool, keep_time: bool) -> crate::Result<()> {
        if !(1..=MAX_FPS).contains(&fps) {
            return Err(crate::Error::Format(format!("{fps} fps is out of range (1–{MAX_FPS})")));
        }
        let old = self.settings.fps_f64();
        self.settings.fps = fps;
        self.settings.ntsc = ntsc;
        let k = self.settings.fps_f64() / old.max(1e-6);
        if keep_time {
            for p in self.scenes.iter_mut().flat_map(|s| s.panels.iter_mut()) {
                p.duration = ((p.duration as f64 * k).round() as u32).clamp(1, MAX_PANEL_FRAMES);
                for key in &mut p.camera.keys {
                    key.frame = (key.frame as f64 * k).round() as u32;
                }
                p.camera.normalize();
            }
        }
        // Sound keeps its time in seconds either way.
        let scale = |v: u64| (v as f64 * k).round() as u64;
        for c in &mut self.audio {
            c.start = scale(c.start);
            c.trim_in = scale(c.trim_in);
            if let Some(a) = &mut c.anchor {
                a.offset = (a.offset as f64 * k).round() as i64;
            }
            c.sanitize();
        }
        self.sync_audio();
        Ok(())
    }

    /// Repair a loaded project: unique ids, `next_id` past every id, durations ≥ 1, sorted
    /// camera keys, layer pixels at the canvas size.
    pub fn repair(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut max = 0u64;
        let mut collect = |id: Id| {
            max = max.max(id.0);
        };
        for s in &self.scenes {
            collect(s.id);
            for p in &s.panels {
                collect(p.id);
                for l in &p.layers {
                    collect(l.id);
                }
            }
        }
        for c in &self.audio {
            collect(c.id);
        }
        self.next_id = self.next_id.max(max.saturating_add(1));
        let (w, h) = (self.settings.width, self.settings.height);
        let mut fresh = self.next_id;
        let mut renumber = |id: &mut Id| {
            if !seen.insert(*id) {
                *id = Id(fresh);
                seen.insert(*id);
                fresh = fresh.saturating_add(1);
            }
        };
        for s in &mut self.scenes {
            renumber(&mut s.id);
            for p in &mut s.panels {
                renumber(&mut p.id);
                p.duration = p.duration.clamp(1, MAX_PANEL_FRAMES);
                p.camera.normalize();
                crate::annotate::sanitize(&mut p.annotations);
                for l in &mut p.layers {
                    renumber(&mut l.id);
                    if !l.opacity.is_finite() {
                        l.opacity = 1.0;
                    }
                    l.opacity = l.opacity.clamp(0.0, 1.0);
                    if l.pixels.is_empty() {
                        l.pixels = Raster::new(w, h);
                    } else if l.pixels.width() != w || l.pixels.height() != h {
                        l.pixels = l.pixels.resized(w, h);
                    }
                }
            }
        }
        self.audio.truncate(crate::audio::MAX_CLIPS);
        for c in &mut self.audio {
            renumber(&mut c.id);
            c.sanitize();
        }
        self.next_id = fresh;
        self.sync_audio();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three_panels() -> (Project, Vec<Id>) {
        let mut p = Project::new("t");
        let s1 = p.add_scene("INT. A");
        let s2 = p.add_scene("EXT. B");
        let a = p.add_panel(s1, None).unwrap();
        let b = p.add_panel(s1, Some(a)).unwrap();
        let c = p.add_panel(s2, None).unwrap();
        (p, vec![a, b, c])
    }

    #[test]
    fn timeline_lookup() {
        let (mut p, ids) = three_panels();
        p.panel_mut(ids[1]).unwrap().duration = 10;
        assert_eq!(p.total_frames(), 48 + 10 + 48);
        assert_eq!(p.panel_at(0), Some((ids[0], 0)));
        assert_eq!(p.panel_at(50), Some((ids[1], 2)));
        assert_eq!(p.panel_at(58), Some((ids[2], 0)));
        assert_eq!(p.panel_at(10_000), Some((ids[2], 47)));
        assert_eq!(p.panel_start(ids[2]), Some(58));
        assert_eq!(p.panel_label(ids[1]), "1.2");
        assert_eq!(p.panel_label(ids[2]), "2.1");
    }

    #[test]
    fn move_panel_crosses_scenes() {
        let (mut p, ids) = three_panels();
        assert!(p.move_panel(ids[1], true));
        assert_eq!(p.find_panel(ids[1]), Some(PanelPos { scene: 1, panel: 0 }));
        assert!(!p.move_panel(ids[0], false), "first panel cannot move earlier");
        assert!(!p.move_panel(ids[2], true), "last panel cannot move later");
    }

    #[test]
    fn repair_fixes_duplicate_ids_and_bad_values() {
        let (mut p, ids) = three_panels();
        p.scenes[1].panels[0].id = ids[0];
        p.scenes[1].panels[0].duration = 0;
        p.scenes[0].panels[0].layers[0].opacity = f32::NAN;
        p.next_id = 0;
        p.repair();
        let mut all: Vec<u64> = p.panels().map(|(_, x)| x.id.0).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 3);
        assert!(p.panels().all(|(_, x)| x.duration >= 1));
        assert_eq!(p.scenes[0].panels[0].layers[0].opacity, 1.0);
        let fresh = p.alloc();
        assert!(p.panels().all(|(_, x)| x.id != fresh));
    }

    #[test]
    fn region_matches_full_composite() {
        let (mut p, ids) = three_panels();
        p.settings.width = 64;
        p.settings.height = 36;
        p.repair();
        let panel = p.panel_mut(ids[0]).unwrap();
        panel.layers[0].pixels.polyline(&[(3.0, 3.0), (60.0, 30.0)], &crate::raster::Brush::default());
        let s = p.settings;
        let full = p.panel(ids[0]).unwrap().composite(&s);
        let part = p.panel(ids[0]).unwrap().composite_region(&s, 10, 5, 20, 12);
        assert_eq!(part, full.crop(10, 5, 20, 12));
    }

    #[test]
    fn composite_is_paper_when_empty() {
        let (p, ids) = three_panels();
        let img = p.panel(ids[0]).unwrap().composite(&p.settings);
        assert_eq!(img.pixel(10, 10), PAPER);
    }
}
