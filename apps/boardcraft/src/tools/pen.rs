//! Pen preferences: pressure curve, minimum size and stabilizer. They belong to the user, not the
//! board, so they live in the suite's `config.toml` through craft-settings.

use std::time::Instant;

use boardcraft_core::pen::{GAMMA_RANGE, MAX_STABILIZER, apply_curve};
use craft_settings::{Location, SaveStatus, SettingsStore};
use serde::{Deserialize, Serialize};

/// Everything BoardCraft keeps in `config.toml`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub pen: PenPrefs,
}

impl craft_settings::Settings for Prefs {
    const APP_ID: &'static str = "boardcraft";
    const SCHEMA_VERSION: u32 = 1;
}

/// How the pen draws.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PenPrefs {
    /// Pressure curve gamma: 1 linear, below 1 soft, above 1 firm.
    pub pressure_curve: f32,
    /// Dab size at the lightest touch, as a fraction of the brush size.
    pub min_size: f32,
    /// Stroke smoothing, 0 (off) to 10.
    pub stabilizer: u8,
}

impl Default for PenPrefs {
    fn default() -> Self {
        Self { pressure_curve: 1.0, min_size: boardcraft_core::raster::DEFAULT_MIN_SIZE, stabilizer: 0 }
    }
}

impl PenPrefs {
    /// Values in range (a hand-edited config file is hostile input).
    pub fn sanitized(self) -> Self {
        let d = Self::default();
        let g = if self.pressure_curve.is_finite() { self.pressure_curve.clamp(*GAMMA_RANGE.start(), *GAMMA_RANGE.end()) } else { d.pressure_curve };
        let m = if self.min_size.is_finite() { self.min_size.clamp(0.02, 1.0) } else { d.min_size };
        Self { pressure_curve: g, min_size: m, stabilizer: self.stabilizer.min(MAX_STABILIZER) }
    }

    /// Dab size (fraction of full) at raw pen pressure `p`, as the brush draws it.
    pub fn size_at(&self, p: f32) -> f32 {
        let m = self.min_size;
        m + (1.0 - m) * apply_curve(p, self.pressure_curve)
    }
}

/// The settings store, opened where the app keeps its data.
pub struct PrefsStore {
    store: SettingsStore<Prefs>,
}

impl PrefsStore {
    /// `data_dir` set (tests, `--data-dir`): settings live in its `Settings` folder; `memory`: kept in
    /// memory only (screenshots); otherwise the platform's config folder.
    pub fn open(data_dir: Option<&std::path::Path>, memory: bool) -> (Self, PenPrefs) {
        let location = match (memory, data_dir) {
            (true, _) => Location::Memory,
            (false, Some(d)) => Location::Dir(d.join("Settings")),
            (false, None) => Location::Auto,
        };
        let (store, _report) = SettingsStore::<Prefs>::open(location);
        let pen = store.get().pen.sanitized();
        (Self { store }, pen)
    }

    /// Keep the store in step with `pen` and save in the background; returns an error to report.
    pub fn sync(&mut self, pen: &PenPrefs) -> Option<String> {
        if self.store.get().pen != *pen {
            let pen = *pen;
            self.store.update(|p| p.pen = pen);
        }
        match self.store.tick(Instant::now()).status {
            SaveStatus::Failed(e) => Some(e.to_string()),
            _ => None,
        }
    }

    pub fn flush(&mut self) {
        let _ = self.store.flush();
    }
}

/// The Pen settings popup: curve, minimum size, stabilizer, with a preview of size against
/// pressure.
pub fn settings_ui(pen: &mut PenPrefs, ui: &mut egui::Ui) {
    ui.set_min_width(240.0);
    let l = ui.label(tl!("Pressure curve"));
    ui.add(egui::Slider::new(&mut pen.pressure_curve, GAMMA_RANGE).logarithmic(true).max_decimals(2))
        .labelled_by(l.id)
        .on_hover_text(tl!("1 is linear. Lower: a light touch already draws thick. Higher: press harder for thick lines."));
    let l = ui.label(tl!("Minimum size (%)"));
    let mut pct = (pen.min_size * 100.0).round() as u32;
    if ui.add(egui::Slider::new(&mut pct, 2..=100)).labelled_by(l.id).on_hover_text(tl!("Line size at the lightest touch")).changed() {
        pen.min_size = pct as f32 / 100.0;
    }
    let l = ui.label(tl!("Stabilizer"));
    ui.add(egui::Slider::new(&mut pen.stabilizer, 0..=MAX_STABILIZER))
        .labelled_by(l.id)
        .on_hover_text(tl!("Smooths strokes: 0 is off, 10 is strongest (the line trails the pen)"));
    preview(pen, ui);
    if ui.button(tl!("Reset Pen Settings")).clicked() {
        *pen = PenPrefs::default();
    }
    *pen = pen.sanitized();
}

/// A small graph of line size (up) against pen pressure (right).
fn preview(pen: &PenPrefs, ui: &mut egui::Ui) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(160.0, 100.0), egui::Sense::hover());
    let v = ui.visuals();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, v.extreme_bg_color);
    painter.rect_stroke(rect, 2.0, v.widgets.noninteractive.bg_stroke, egui::StrokeKind::Inside);
    let inner = rect.shrink(6.0);
    let at = |p: f32, s: f32| egui::pos2(inner.left() + p * inner.width(), inner.bottom() - s * inner.height());
    painter.line_segment([at(0.0, 0.0), at(1.0, 1.0)], egui::Stroke::new(1.0, v.weak_text_color()));
    let pts: Vec<egui::Pos2> = (0..=32).map(|i| i as f32 / 32.0).map(|p| at(p, pen.size_at(p))).collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0, v.text_color())));
    let half = (pen.size_at(0.5) * 100.0).round();
    let light = (pen.size_at(0.0) * 100.0).round();
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Image,
            true,
            tlf!("Pressure curve preview: the lightest touch draws {light}% of full size, half pressure {half}%", light = light, half = half),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_are_sanitized_and_persist() {
        let bad = PenPrefs { pressure_curve: f32::NAN, min_size: 7.0, stabilizer: 200 }.sanitized();
        assert_eq!(bad.pressure_curve, 1.0);
        assert_eq!(bad.min_size, 1.0);
        assert_eq!(bad.stabilizer, MAX_STABILIZER);
        let dir = std::env::temp_dir().join(format!("boardcraft-prefs-{}", std::process::id()));
        let (mut store, pen) = PrefsStore::open(Some(&dir), false);
        assert_eq!(pen, PenPrefs::default());
        let firm = PenPrefs { pressure_curve: 2.0, min_size: 0.1, stabilizer: 4 };
        assert!(store.sync(&firm).is_none());
        store.flush();
        let (_, back) = PrefsStore::open(Some(&dir), false);
        assert_eq!(back, firm);
        let _ = std::fs::remove_dir_all(&dir);
        // The preview's size curve follows the settings.
        assert!((firm.size_at(0.0) - 0.1).abs() < 1e-6);
        assert!(firm.size_at(0.5) < PenPrefs::default().size_at(0.5));
    }
}
