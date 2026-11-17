//! GPU textures for panel drawings, rebuilt only when a panel's pixels change.

use std::collections::HashMap;

use boardcraft_core::{Id, Panel, Project};

/// Thumbnail size (16:9 boards; other aspects are fitted by the views).
const THUMB_W: u32 = 320;

struct Entry {
    revision: u64,
    full: Option<egui::TextureHandle>,
    thumb: Option<(u64, egui::TextureHandle)>,
}

#[derive(Default)]
pub struct Textures {
    map: HashMap<Id, Entry>,
}

fn image(r: &boardcraft_core::Raster) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied([r.width() as usize, r.height() as usize], r.data())
}

impl Textures {
    /// Full-resolution composite of `panel`.
    pub fn full(&mut self, ctx: &egui::Context, project: &Project, panel: &Panel) -> egui::TextureId {
        let e = self.map.entry(panel.id).or_insert(Entry { revision: u64::MAX, full: None, thumb: None });
        let stale = e.revision != panel.revision || e.full.is_none();
        if stale {
            let img = image(&panel.composite(&project.settings));
            match &mut e.full {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => e.full = Some(ctx.load_texture(format!("panel-{}", panel.id.0), img, egui::TextureOptions::LINEAR)),
            }
            e.revision = panel.revision;
        }
        e.full.as_ref().map(|t| t.id()).unwrap_or_default()
    }

    /// Re-composite only the rectangle `x0..x1, y0..y1` (canvas pixels) into the full texture, after
    /// a brush dab: keeps drawing fast on large canvases.
    pub fn patch(&mut self, project: &Project, panel: &Panel, rect: [u32; 4]) {
        let Some(e) = self.map.get_mut(&panel.id) else { return };
        let Some(tex) = e.full.as_mut() else { return };
        let s = &project.settings;
        let x0 = rect[0].min(s.width.saturating_sub(1));
        let y0 = rect[1].min(s.height.saturating_sub(1));
        let x1 = rect[2].clamp(x0 + 1, s.width);
        let y1 = rect[3].clamp(y0 + 1, s.height);
        let region = panel.composite_region(s, x0, y0, x1 - x0, y1 - y0);
        tex.set_partial([x0 as usize, y0 as usize], image(&region), egui::TextureOptions::LINEAR);
        e.revision = panel.revision;
    }

    /// Downscaled composite for the board grid and timeline. `live` = false skips rebuilding while
    /// a stroke is in progress (the thumbnail catches up when it ends).
    pub fn thumb(&mut self, ctx: &egui::Context, project: &Project, panel: &Panel, live: bool) -> egui::TextureId {
        let e = self.map.entry(panel.id).or_insert(Entry { revision: u64::MAX, full: None, thumb: None });
        let stale = e.thumb.as_ref().is_none_or(|(rev, _)| *rev != panel.revision);
        if stale && (live || e.thumb.is_none()) {
            let s = &project.settings;
            let h = (THUMB_W as f32 * s.height as f32 / s.width.max(1) as f32).round().max(1.0) as u32;
            let img = image(&panel.composite(s).resized(THUMB_W, h));
            match &mut e.thumb {
                Some((rev, t)) => {
                    t.set(img, egui::TextureOptions::LINEAR);
                    *rev = panel.revision;
                }
                None => e.thumb = Some((panel.revision, ctx.load_texture(format!("thumb-{}", panel.id.0), img, egui::TextureOptions::LINEAR))),
            }
        }
        e.thumb.as_ref().map(|(_, t)| t.id()).unwrap_or_default()
    }

    /// Drop textures of panels that no longer exist.
    pub fn retain(&mut self, project: &Project) {
        let live: std::collections::HashSet<Id> = project.panels().map(|(_, p)| p.id).collect();
        self.map.retain(|id, _| live.contains(id));
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}
