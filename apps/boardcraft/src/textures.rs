//! GPU textures for panel drawings, rebuilt only when a panel's pixels change, and kept only for
//! panels that are on screen.
//!
//! Every texture remembers the frame it was last drawn in. [`Textures::end_frame`] drops full-size
//! textures not drawn for [`FULL_IDLE_FRAMES`] frames (and all but the [`MAX_FULL`] most recent),
//! and thumbnails not drawn for [`THUMB_IDLE_FRAMES`] frames, so a 300-panel board holds textures
//! for the panels in view, not for the whole board. A dropped texture is rebuilt from the panel the
//! next time it is drawn.

use std::collections::HashMap;

use boardcraft_core::{Id, Panel, Project};

/// Thumbnail size (16:9 boards; other aspects are fitted by the views).
const THUMB_W: u32 = 320;
/// Frames a full-size texture survives without being drawn.
pub const FULL_IDLE_FRAMES: u64 = 120;
/// Most full-size textures kept at once (the Panel view shows one, onion skin a second).
pub const MAX_FULL: usize = 6;
/// Frames a thumbnail survives without being drawn.
pub const THUMB_IDLE_FRAMES: u64 = 180;

struct Tex {
    revision: u64,
    handle: egui::TextureHandle,
    used: u64,
}

#[derive(Default)]
pub struct Textures {
    full: HashMap<Id, Tex>,
    thumbs: HashMap<Id, Tex>,
    frame: u64,
}

fn image(r: &boardcraft_core::Raster) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied([r.width() as usize, r.height() as usize], r.data())
}

impl Textures {
    /// Full-resolution composite of `panel`.
    pub fn full(&mut self, ctx: &egui::Context, project: &Project, panel: &Panel) -> egui::TextureId {
        let frame = self.frame;
        if let Some(t) = self.full.get_mut(&panel.id) {
            t.used = frame;
            if t.revision != panel.revision {
                t.handle.set(image(&panel.composite(&project.settings)), egui::TextureOptions::LINEAR);
                t.revision = panel.revision;
            }
            return t.handle.id();
        }
        let handle = ctx.load_texture(format!("panel-{}", panel.id.0), image(&panel.composite(&project.settings)), egui::TextureOptions::LINEAR);
        let id = handle.id();
        self.full.insert(panel.id, Tex { revision: panel.revision, handle, used: frame });
        id
    }

    /// Re-composite only the rectangle `x0..x1, y0..y1` (canvas pixels) into the full texture, after
    /// a brush dab: keeps drawing fast on large canvases.
    pub fn patch(&mut self, project: &Project, panel: &Panel, rect: [u32; 4]) {
        let Some(t) = self.full.get_mut(&panel.id) else { return };
        let s = &project.settings;
        let x0 = rect[0].min(s.width.saturating_sub(1));
        let y0 = rect[1].min(s.height.saturating_sub(1));
        let x1 = rect[2].clamp(x0 + 1, s.width);
        let y1 = rect[3].clamp(y0 + 1, s.height);
        let region = panel.composite_region(s, x0, y0, x1 - x0, y1 - y0);
        t.handle.set_partial([x0 as usize, y0 as usize], image(&region), egui::TextureOptions::LINEAR);
        t.revision = panel.revision;
    }

    /// Downscaled composite for the board grid and timeline. `live` = false skips rebuilding while
    /// a stroke is in progress (the thumbnail catches up when it ends).
    pub fn thumb(&mut self, ctx: &egui::Context, project: &Project, panel: &Panel, live: bool) -> egui::TextureId {
        let frame = self.frame;
        let render = || {
            let s = &project.settings;
            let h = (THUMB_W as f32 * s.height as f32 / s.width.max(1) as f32).round().max(1.0) as u32;
            image(&panel.composite(s).resized(THUMB_W, h))
        };
        if let Some(t) = self.thumbs.get_mut(&panel.id) {
            t.used = frame;
            if t.revision != panel.revision && live {
                t.handle.set(render(), egui::TextureOptions::LINEAR);
                t.revision = panel.revision;
            }
            return t.handle.id();
        }
        let handle = ctx.load_texture(format!("thumb-{}", panel.id.0), render(), egui::TextureOptions::LINEAR);
        let id = handle.id();
        self.thumbs.insert(panel.id, Tex { revision: panel.revision, handle, used: frame });
        id
    }

    /// Call once a frame after drawing: drops textures that have not been drawn for a while.
    pub fn end_frame(&mut self) {
        let now = self.frame;
        self.full.retain(|_, t| now.saturating_sub(t.used) <= FULL_IDLE_FRAMES);
        if self.full.len() > MAX_FULL {
            let mut ages: Vec<(u64, Id)> = self.full.iter().map(|(id, t)| (t.used, *id)).collect();
            ages.sort_unstable();
            for (_, id) in ages.iter().take(self.full.len() - MAX_FULL) {
                self.full.remove(id);
            }
        }
        self.thumbs.retain(|_, t| now.saturating_sub(t.used) <= THUMB_IDLE_FRAMES);
        self.frame = self.frame.wrapping_add(1);
    }

    /// Textures held now: (full size, thumbnails).
    #[cfg(test)]
    pub fn counts(&self) -> (usize, usize) {
        (self.full.len(), self.thumbs.len())
    }

    /// Bytes of texture memory held (RGBA8).
    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.full.values().chain(self.thumbs.values()).map(|t| t.handle.size()[0] * t.handle.size()[1] * 4).sum()
    }

    /// Drop textures of panels that no longer exist.
    pub fn retain(&mut self, project: &Project) {
        let live: std::collections::HashSet<Id> = project.panels().map(|(_, p)| p.id).collect();
        self.full.retain(|id, _| live.contains(id));
        self.thumbs.retain(|id, _| live.contains(id));
    }

    pub fn clear(&mut self) {
        self.full.clear();
        self.thumbs.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 300-panel board: drawing a few panels a frame keeps only those textures alive.
    #[test]
    fn textures_follow_what_is_drawn() {
        let ctx = egui::Context::default();
        let mut p = Project::new("Large");
        p.settings.width = 64;
        p.settings.height = 36;
        let s = p.add_scene("");
        for _ in 0..300 {
            p.add_panel(s, None).unwrap();
        }
        let ids: Vec<Id> = p.panels().map(|(_, x)| x.id).collect();
        let mut t = Textures::default();
        // Scroll through the whole board, 12 thumbnails in view at a time.
        for window in ids.chunks(12) {
            for _ in 0..30 {
                for id in window {
                    if let Some(panel) = p.panel(*id) {
                        t.thumb(&ctx, &p, panel, true);
                    }
                }
                t.end_frame();
            }
        }
        let (_, thumbs) = t.counts();
        assert!(thumbs <= 12 * (THUMB_IDLE_FRAMES as usize / 30 + 1), "{thumbs} thumbnails kept");
        assert!(thumbs < 300);
        // Idle long enough and everything not drawn goes.
        for _ in 0..=THUMB_IDLE_FRAMES {
            t.end_frame();
        }
        assert_eq!(t.counts().1, 0);
        // Full textures: at most MAX_FULL, the most recently drawn.
        for id in ids.iter().take(20) {
            if let Some(panel) = p.panel(*id) {
                t.full(&ctx, &p, panel);
            }
            t.end_frame();
        }
        assert_eq!(t.counts().0, MAX_FULL);
        assert!(t.bytes() <= MAX_FULL * 64 * 36 * 4);
    }
}
