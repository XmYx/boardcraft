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
