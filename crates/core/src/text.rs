//! A small text rasterizer for burn-ins: glyph outlines from `skrifa` (the font the board sheets
//! use), flattened to lines and filled with the non-zero rule at 4×4 supersampling into an alpha
//! mask, then blended onto RGBA8 pixels.

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};

use crate::error::{Error, Result};

/// Supersamples per pixel side.
const SS: usize = 4;
/// Largest text size in pixels.
pub const MAX_PX: f32 = 400.0;

/// A font ready to measure and draw text.
pub struct TextRenderer<'a> {
    font: FontRef<'a>,
}

/// Collects a glyph outline as line segments in pixel space (y down), offset by `origin`.
struct Flatten {
    origin: (f32, f32),
    start: (f32, f32),
    cur: (f32, f32),
    edges: Vec<[f32; 4]>,
}

impl Flatten {
    fn pt(&self, x: f32, y: f32) -> (f32, f32) {
        (self.origin.0 + x, self.origin.1 - y)
    }

    fn line(&mut self, to: (f32, f32)) {
        if self.edges.len() < 1 << 16 && to != self.cur {
            self.edges.push([self.cur.0, self.cur.1, to.0, to.1]);
        }
        self.cur = to;
    }
}

impl OutlinePen for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        let p = self.pt(x, y);
        self.start = p;
        self.cur = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.line(p);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (p0, c, p1) = (self.cur, self.pt(cx0, cy0), self.pt(x, y));
        const N: usize = 8;
        for i in 1..=N {
            let t = i as f32 / N as f32;
            let u = 1.0 - t;
            self.line((u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p1.0, u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p1.1));
        }
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (p0, c0, c1, p1) = (self.cur, self.pt(cx0, cy0), self.pt(cx1, cy1), self.pt(x, y));
        const N: usize = 12;
        for i in 1..=N {
            let t = i as f32 / N as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.line((a * p0.0 + b * c0.0 + c * c1.0 + d * p1.0, a * p0.1 + b * c0.1 + c * c1.1 + d * p1.1));
        }
    }

    fn close(&mut self) {
        let s = self.start;
        self.line(s);
    }
}

/// An alpha mask (`width × height`, 0..=255).
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<u8>,
}

impl<'a> TextRenderer<'a> {
    /// The face at `index` of a font file (TTF, OTF or collection).
    pub fn new(data: &'a [u8], index: u32) -> Result<Self> {
        let font = FontRef::from_index(data, index).map_err(|e| Error::Format(format!("font: {e}")))?;
        Ok(Self { font })
    }

    /// Advance width of `text` at `px` pixels.
    pub fn width(&self, text: &str, px: f32) -> f32 {
        let px = px.clamp(1.0, MAX_PX);
        let charmap = self.font.charmap();
        let metrics = self.font.glyph_metrics(Size::new(px), LocationRef::default());
        text.chars().map(|c| charmap.map(c).and_then(|g| metrics.advance_width(g)).unwrap_or(px * 0.5)).sum()
    }

    /// Ascent and descent (positive, below the baseline) at `px`.
    pub fn vertical(&self, px: f32) -> (f32, f32) {
        let m = self.font.metrics(Size::new(px.clamp(1.0, MAX_PX)), LocationRef::default());
        (m.ascent, -m.descent)
    }

    /// Greedy word wrap to `width` pixels; at most `max_lines`, the last ending in `…` when cut.
    pub fn wrap(&self, text: &str, px: f32, width: f32, max_lines: usize) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut cut = false;
        'paragraphs: for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split_whitespace() {
                let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if line.is_empty() || self.width(&candidate, px) <= width {
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
            if line.is_empty() {
                continue;
            }
            if lines.len() >= max_lines {
                cut = true;
                break;
            }
            lines.push(line);
        }
        lines.truncate(max_lines);
        for l in &mut lines {
            while l.chars().count() > 1 && self.width(l, px) > width {
                l.pop();
                cut = true;
            }
        }
        if cut && let Some(last) = lines.last_mut() {
            while !last.is_empty() && self.width(&format!("{last}…"), px) > width {
                last.pop();
            }
            last.push('…');
        }
        lines
    }

    /// Rasterize one line of text at `px` into a mask tall enough for its ascent and descent; the
    /// baseline is at the ascent.
    pub fn render(&self, text: &str, px: f32) -> Mask {
        let px = px.clamp(1.0, MAX_PX);
        let (ascent, descent) = self.vertical(px);
        let width = (self.width(text, px).ceil() as usize + 2).min(16_384);
        let height = ((ascent + descent).ceil() as usize + 2).min(2048);
        let charmap = self.font.charmap();
        let metrics = self.font.glyph_metrics(Size::new(px), LocationRef::default());
        let outlines = self.font.outline_glyphs();
        let mut pen = Flatten { origin: (1.0, ascent + 1.0), start: (0.0, 0.0), cur: (0.0, 0.0), edges: Vec::new() };
        for c in text.chars() {
            let gid = charmap.map(c);
            if let Some(g) = gid.and_then(|g| outlines.get(g)) {
                let _ = g.draw(DrawSettings::unhinted(Size::new(px), LocationRef::default()), &mut pen);
                pen.close();
            }
            pen.origin.0 += gid.and_then(|g| metrics.advance_width(g)).unwrap_or(px * 0.5);
            if pen.origin.0 > width as f32 {
                break;
            }
        }
        Mask { width, height, alpha: fill(&pen.edges, width, height) }
    }
}

/// Non-zero fill of closed edge loops into a `width × height` coverage mask.
fn fill(edges: &[[f32; 4]], width: usize, height: usize) -> Vec<u8> {
    let mut cover = vec![0u16; width * height];
    let mut xs: Vec<(f32, i32)> = Vec::new();
    for sy in 0..height * SS {
        let y = (sy as f32 + 0.5) / SS as f32;
        xs.clear();
        for e in edges {
            let (x0, y0, x1, y1) = (e[0], e[1], e[2], e[3]);
            if (y0 <= y && y < y1) || (y1 <= y && y < y0) {
                let t = (y - y0) / (y1 - y0);
                xs.push((x0 + t * (x1 - x0), if y1 > y0 { 1 } else { -1 }));
            }
        }
        if xs.is_empty() {
            continue;
        }
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let row = (sy / SS) * width;
        let mut wind = 0;
        let mut k = 0;
        for sx in 0..width * SS {
            let x = (sx as f32 + 0.5) / SS as f32;
            while k < xs.len() && xs[k].0 <= x {
                wind += xs[k].1;
                k += 1;
            }
            if wind != 0
                && let Some(c) = cover.get_mut(row + sx / SS)
            {
                *c += 1;
            }
        }
    }
    let full = (SS * SS) as u32;
    cover.into_iter().map(|c| ((c as u32 * 255 + full / 2) / full).min(255) as u8).collect()
}

/// Blend `color` (straight RGBA) through `mask` onto `rgba` (`width × height` RGBA8) at `(x, y)`.
pub fn blend_mask(rgba: &mut [u8], width: usize, height: usize, mask: &Mask, x: i64, y: i64, color: [u8; 4]) {
    for my in 0..mask.height {
        let py = y + my as i64;
        if py < 0 || py >= height as i64 {
            continue;
        }
        for mx in 0..mask.width {
            let px = x + mx as i64;
            if px < 0 || px >= width as i64 {
                continue;
            }
            let a = mask.alpha.get(my * mask.width + mx).copied().unwrap_or(0) as u32 * color[3] as u32 / 255;
            if a == 0 {
                continue;
            }
            let i = (py as usize * width + px as usize) * 4;
            if let Some(p) = rgba.get_mut(i..i + 3) {
                for c in 0..3 {
                    p[c] = ((p[c] as u32 * (255 - a) + color[c] as u32 * a + 127) / 255) as u8;
                }
            }
        }
    }
}

/// Darken (or tint) a rectangle: blend `color` over every pixel in it.
pub fn fill_rect(rgba: &mut [u8], width: usize, height: usize, x0: i64, y0: i64, x1: i64, y1: i64, color: [u8; 4]) {
    let (x0, x1) = (x0.clamp(0, width as i64) as usize, x1.clamp(0, width as i64) as usize);
    let (y0, y1) = (y0.clamp(0, height as i64) as usize, y1.clamp(0, height as i64) as usize);
    let a = color[3] as u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * width + x) * 4;
            if let Some(p) = rgba.get_mut(i..i + 3) {
                for c in 0..3 {
                    p[c] = ((p[c] as u32 * (255 - a) + color[c] as u32 * a + 127) / 255) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_wraps() {
        let t = TextRenderer::new(epaint_default_fonts::UBUNTU_LIGHT, 0).unwrap();
        let m = t.render("Hello 1.2", 24.0);
        assert!(m.width > 60 && m.height >= 24, "{}x{}", m.width, m.height);
        let ink = m.alpha.iter().filter(|a| **a > 128).count();
        assert!(ink > 50, "{ink}");
        // An 'l' is a solid stem: some pixels are fully covered.
        assert!(m.alpha.contains(&255));
        let lines = t.wrap("one two three four five six seven eight nine ten", 20.0, 120.0, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
        assert!(lines.iter().all(|l| t.width(l, 20.0) <= 120.0));
        assert!(TextRenderer::new(b"not a font", 0).is_err());
        // Empty and absurd input.
        assert_eq!(t.render("", 1e9).alpha.iter().filter(|a| **a > 0).count(), 0);
        assert!(t.wrap("", 10.0, 0.0, 0).is_empty());
    }

    #[test]
    fn blend_is_clipped() {
        let mut px = vec![0u8; 4 * 4 * 4];
        let mask = Mask { width: 3, height: 3, alpha: vec![255; 9] };
        blend_mask(&mut px, 4, 4, &mask, -1, 2, [255, 255, 255, 255]);
        assert_eq!(&px[(2 * 4) * 4..(2 * 4) * 4 + 3], &[255, 255, 255]);
        fill_rect(&mut px, 4, 4, -10, -10, 100, 100, [0, 0, 0, 255]);
        assert!(px.chunks(4).all(|p| p[..3] == [0, 0, 0]));
    }
}
