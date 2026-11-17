//! Printable board sheets (PDF): the storyboard as pages of panels, each with its number,
//! duration, scene heading, dialogue, action and notes.
//!
//! Written with `craft-pdf` (craft-libs; Krilla backend). Text is real PDF text (selectable,
//! searchable) in the font the caller passes: the app passes an embedded craft-fonts face when the
//! build has one, else egui's bundled UI face. Panel pictures are the camera's view at each panel's
//! first frame. Labels ("Dialogue", "Page 1 of 3") come from the caller so the app can translate
//! them.

use craft_pdf::write::backend as krilla;
use krilla::color::rgb::Color;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::surface::Surface;
use krilla::text::{Font, TextDirection};
use serde::{Deserialize, Serialize};
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size as FontSize};

use crate::error::{Error, Result};
use crate::model::{Panel, Project, Scene};
use crate::stills::render_frame;

/// Paper size, landscape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paper {
    #[default]
    A4,
    Letter,
}

impl Paper {
    /// Width and height in points, landscape.
    pub fn size(self) -> (f32, f32) {
        match self {
            Paper::A4 => (841.89, 595.28),
            Paper::Letter => (792.0, 612.0),
        }
    }
}

/// Interface words printed on the sheet, translated by the caller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetLabels {
    pub dialogue: String,
    pub action: String,
    pub notes: String,
    /// `{n}` and `{total}` are replaced.
    pub page_of: String,
    /// Seconds suffix, e.g. `s`.
    pub seconds: String,
}

impl Default for SheetLabels {
    fn default() -> Self {
        Self { dialogue: "Dialogue".into(), action: "Action".into(), notes: "Notes".into(), page_of: "Page {n} of {total}".into(), seconds: "s".into() }
    }
}

/// Board sheet layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetOptions {
    pub paper: Paper,
    /// Panels per row (1–6).
    pub columns: u32,
    /// Rows per page (1–6).
    pub rows: u32,
    #[serde(default)]
    pub labels: SheetLabels,
}

impl Default for SheetOptions {
    fn default() -> Self {
        Self { paper: Paper::A4, columns: 3, rows: 2, labels: SheetLabels::default() }
    }
}

const MARGIN: f32 = 32.0;
const HEADER: f32 = 26.0;
const GUTTER: f32 = 14.0;
const LABEL_PT: f32 = 9.0;
const TEXT_PT: f32 = 7.5;
const INK: (u8, u8, u8) = (0x20, 0x20, 0x24);
const MUTED: (u8, u8, u8) = (0x60, 0x60, 0x66);

/// Glyph advances for wrapping text in the sheet's font.
struct Measure<'a> {
    font: skrifa::FontRef<'a>,
}

impl Measure<'_> {
    fn width(&self, text: &str, pt: f32) -> f32 {
        let charmap = self.font.charmap();
        let metrics = self.font.glyph_metrics(FontSize::new(pt), LocationRef::default());
        text.chars().map(|c| charmap.map(c).and_then(|g| metrics.advance_width(g)).unwrap_or(pt * 0.5)).sum()
    }

    /// Greedy word wrap to `width`; at most `max_lines`, the last one ending in `…` when cut.
    fn wrap(&self, text: &str, pt: f32, width: f32, max_lines: usize) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut cut = false;
        'paragraphs: for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split_whitespace() {
                let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if self.width(&candidate, pt) <= width || line.is_empty() {
                    line = candidate;
                } else {
                    if lines.len() + 1 >= max_lines {
                        lines.push(line);
                        cut = true;
                        break 'paragraphs;
                    }
                    lines.push(std::mem::replace(&mut line, word.to_string()));
                }
            }
            if lines.len() >= max_lines {
                cut = true;
                break;
            }
            lines.push(line);
        }
        lines.truncate(max_lines);
        if cut && let Some(last) = lines.last_mut() {
            while !last.is_empty() && self.width(&format!("{last}…"), pt) > width {
                last.pop();
            }
            last.push('…');
        }
        // Single words wider than the box are cut too.
        for l in &mut lines {
            while l.chars().count() > 1 && self.width(l, pt) > width {
                l.pop();
            }
        }
        lines
    }
}

fn fill(c: (u8, u8, u8)) -> Fill {
    Fill { paint: Color::new(c.0, c.1, c.2).into(), ..Default::default() }
}

fn text(s: &mut Surface, font: &Font, pt: f32, x: f32, y: f32, color: (u8, u8, u8), t: &str) {
    if t.is_empty() {
        return;
    }
    s.set_fill(Some(fill(color)));
    s.draw_text(Point::from_xy(x, y), font.clone(), pt, t, false, TextDirection::Auto);
    s.set_fill(None);
}

fn frame(s: &mut Surface, r: Rect, gray: u8, width: f32) {
    let mut pb = PathBuilder::new();
    pb.push_rect(r);
    if let Some(path) = pb.finish() {
        s.set_fill(None);
        s.set_stroke(Some(Stroke { paint: Color::new(gray, gray, gray).into(), width, ..Default::default() }));
        s.draw_path(&path);
        s.set_stroke(None);
    }
}

struct Cell<'a> {
    scene: &'a Scene,
    panel: &'a Panel,
    first_of_scene: bool,
}

/// Number of pages the sheet will have.
pub fn page_count(project: &Project, opts: &SheetOptions) -> usize {
    let per = (opts.columns.clamp(1, 6) * opts.rows.clamp(1, 6)) as usize;
    project.panel_count().div_ceil(per).max(1)
}

/// Write the board sheet. `font` is a TrueType/OpenType font (`index` in a collection).
pub fn board_sheet_pdf(project: &Project, opts: &SheetOptions, font_data: &[u8], index: u32) -> Result<Vec<u8>> {
    craft_pdf::write::guard(|| sheet(project, opts, font_data, index))
}

impl From<craft_pdf::write::WriteError> for Error {
    fn from(e: craft_pdf::write::WriteError) -> Self {
        Error::Format(format!("PDF: {e}"))
    }
}

fn sheet(project: &Project, opts: &SheetOptions, font_data: &[u8], index: u32) -> Result<Vec<u8>> {
    let font = craft_pdf::write::font(font_data, index, None)?;
    let measure = Measure { font: skrifa::FontRef::from_index(font_data, index).map_err(|e| Error::Format(format!("sheet font: {e}")))? };
    let (cols, rows) = (opts.columns.clamp(1, 6), opts.rows.clamp(1, 6));
    let (pw, ph) = opts.paper.size();
    let cell_w = (pw - 2.0 * MARGIN - GUTTER * (cols - 1) as f32) / cols as f32;
    let cell_h = (ph - 2.0 * MARGIN - HEADER - GUTTER * (rows - 1) as f32) / rows as f32;
    let aspect = project.settings.height as f32 / project.settings.width.max(1) as f32;
    // The picture takes the cell width, but leaves at least four text lines below it.
    let min_text = LABEL_PT * 1.6 + TEXT_PT * 1.3 * 4.0;
    let img_h = (cell_w * aspect).min(cell_h - min_text).max(cell_h * 0.3);
    let img_w = (img_h / aspect).min(cell_w);
    // Picture resolution: about 200 dpi, never above the canvas.
    let px_w = ((img_w * 200.0 / 72.0) as u32).clamp(16, project.settings.width);
    let px_h = ((px_w as f32 * aspect) as u32).max(9);

    let cells: Vec<Cell> =
        project.scenes.iter().flat_map(|sc| sc.panels.iter().enumerate().map(move |(i, p)| Cell { scene: sc, panel: p, first_of_scene: i == 0 })).collect();
    let per = (cols * rows) as usize;
    let total = page_count(project, opts);
    let fps = project.settings.fps_f64() as f32;
    let metadata = craft_pdf::write::Metadata {
        creator: crate::GENERATOR.into(),
        producer: crate::GENERATOR.into(),
        title: (!project.name.trim().is_empty()).then(|| project.name.clone()),
        ..Default::default()
    };
    let mut writer = craft_pdf::write::Writer::new(Default::default(), metadata);
    let pdf = writer.document_mut();
    let size = Size::from_wh(pw, ph).ok_or_else(|| Error::Format("paper size".into()))?;
    for page_no in 0..total {
        let mut page = pdf.start_page_with(PageSettings::new(size));
        {
            let mut s = page.surface();
            // Header: board name, page count.
            text(&mut s, &font, 13.0, MARGIN, MARGIN + 12.0, INK, &project.name);
            let pg = opts.labels.page_of.replace("{n}", &(page_no + 1).to_string()).replace("{total}", &total.to_string());
            let w = measure.width(&pg, 8.0);
            text(&mut s, &font, 8.0, pw - MARGIN - w, MARGIN + 12.0, MUTED, &pg);
            for (k, cell) in cells.iter().skip(page_no * per).take(per).enumerate() {
                let (c, r) = ((k as u32) % cols, (k as u32) / cols);
                let x = MARGIN + c as f32 * (cell_w + GUTTER);
                let y = MARGIN + HEADER + r as f32 * (cell_h + GUTTER);
                // Picture.
                let img = render_frame(project, cell.panel, 0.0).resized(px_w, px_h);
                s.push_transform(&Transform::from_translate(x, y));
                if let Some(sz) = Size::from_wh(img_w, img_h) {
                    s.draw_image(Image::from_rgba8(img.data().to_vec(), img.width(), img.height()), sz);
                }
                s.pop();
                if let Some(rect) = Rect::from_xywh(x, y, img_w, img_h) {
                    frame(&mut s, rect, 0x30, 0.8);
                }
                // Label line: number, duration, heading on a scene's first panel.
                let mut ty = y + img_h + LABEL_PT * 1.3;
                let shot = if cell.panel.shot.trim().is_empty() { String::new() } else { format!("   {}", cell.panel.shot.trim()) };
                let label = format!("{}{shot}   {:.1} {}", project.panel_label(cell.panel.id), cell.panel.duration as f32 / fps, opts.labels.seconds);
                text(&mut s, &font, LABEL_PT, x, ty, INK, &label);
                if cell.first_of_scene && !cell.scene.heading.is_empty() {
                    let lw = measure.width(&label, LABEL_PT) + 12.0;
                    let heading = measure.wrap(&cell.scene.heading, LABEL_PT - 1.0, (cell_w - lw).max(10.0), 1);
                    text(&mut s, &font, LABEL_PT - 1.0, x + lw, ty, MUTED, heading.first().map(String::as_str).unwrap_or(""));
                }
                // Text fields, as many lines as fit.
                let line_h = TEXT_PT * 1.3;
                let bottom = y + cell_h;
                for (name, body) in
                    [(&opts.labels.dialogue, &cell.panel.dialogue), (&opts.labels.action, &cell.panel.action), (&opts.labels.notes, &cell.panel.notes)]
                {
                    if body.trim().is_empty() {
                        continue;
                    }
                    let left = ((bottom - ty) / line_h).floor() as usize;
                    if left < 1 {
                        break;
                    }
                    let lines = measure.wrap(&format!("{name}: {body}"), TEXT_PT, cell_w, left);
                    for l in lines {
                        ty += line_h;
                        text(&mut s, &font, TEXT_PT, x, ty, INK, &l);
                    }
                }
            }
            s.finish();
        }
        page.finish();
    }
    Ok(writer.finish()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &[u8] = epaint_default_fonts::UBUNTU_LIGHT;

    #[test]
    fn demo_sheet_is_a_pdf_with_text() {
        let p = crate::demo::demo_project();
        let pdf = board_sheet_pdf(&p, &SheetOptions::default(), FONT, 0).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert_eq!(page_count(&p, &SheetOptions::default()), 2, "8 panels at 6 per page");
        let one = SheetOptions { columns: 2, rows: 1, paper: Paper::Letter, ..Default::default() };
        assert_eq!(page_count(&p, &one), 4);
        assert!(board_sheet_pdf(&p, &one, FONT, 0).unwrap().len() > 1000);
    }

    #[test]
    fn hostile_options_and_fonts() {
        let p = crate::demo::demo_project();
        let o = SheetOptions { columns: 0, rows: 99, ..Default::default() };
        assert!(board_sheet_pdf(&p, &o, FONT, 0).is_ok());
        assert!(board_sheet_pdf(&p, &SheetOptions::default(), b"not a font", 0).is_err());
        assert!(board_sheet_pdf(&crate::model::Project::blank("x"), &SheetOptions::default(), FONT, 0).is_ok());
    }

    #[test]
    fn wrapping() {
        let m = Measure { font: skrifa::FontRef::new(FONT).unwrap() };
        let lines = m.wrap("one two three four five six seven eight nine ten", 10.0, 60.0, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
        assert!(lines.iter().all(|l| m.width(l, 10.0) <= 60.0));
        assert_eq!(m.wrap("short", 10.0, 200.0, 3), vec!["short".to_string()]);
    }
}
