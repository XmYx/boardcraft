//! Raster canvases and the drawing brush.
//!
//! A [`Raster`] is straight (non-premultiplied) RGBA8, row-major, top-left origin. The brush is a
//! round dab stamped along each stroke segment, with an adjustable soft edge; pen pressure can scale
//! its size and opacity. The eraser removes alpha with the same dab.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Largest canvas side BoardCraft creates or loads (pixels).
pub const MAX_SIDE: u32 = 8192;

/// Most dabs one stroke segment may stamp (a guard against absurd input coordinates).
const MAX_DABS_PER_SEGMENT: usize = 20_000;

/// A straight-alpha RGBA8 image.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Raster {
    width: u32,
    height: u32,
    /// Shared until written (copy-on-write), so cloning a project for undo or autosave costs
    /// nothing until a layer is painted.
    data: Arc<Vec<u8>>,
}

/// What a stroke does to the layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tool {
    /// Paint with the brush colour.
    #[default]
    Brush,
    /// Remove paint.
    Eraser,
}

/// Brush settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Brush {
    pub tool: Tool,
    /// Radius in canvas pixels at full pressure.
    pub radius: f32,
    /// Straight-alpha colour; alpha is the brush opacity.
    pub color: [u8; 4],
    /// 1 = hard edge, 0 = fully soft.
    pub hardness: f32,
    /// Pen pressure scales the dab size.
    pub pressure_size: bool,
    /// Pen pressure scales the dab opacity.
    pub pressure_opacity: bool,
}

impl Default for Brush {
    fn default() -> Self {
        Self { tool: Tool::Brush, radius: 3.0, color: [24, 24, 28, 255], hardness: 0.7, pressure_size: true, pressure_opacity: false }
    }
}

/// One sample of a stroke: canvas position and pen pressure (0..=1; a mouse reports 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

impl StrokePoint {
    pub fn new(x: f32, y: f32, pressure: f32) -> Self {
        Self { x, y, pressure }
    }
}

fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

impl Raster {
    /// A transparent canvas. Sides are clamped to `1..=MAX_SIDE`.
    pub fn new(width: u32, height: u32) -> Self {
        Self::filled(width, height, [0, 0, 0, 0])
    }

    /// A canvas filled with one colour. Sides are clamped to `1..=MAX_SIDE`.
    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let width = width.clamp(1, MAX_SIDE);
        let height = height.clamp(1, MAX_SIDE);
        let n = width as usize * height as usize;
        let mut data = Vec::with_capacity(n * 4);
        for _ in 0..n {
            data.extend_from_slice(&rgba);
        }
        Self { width, height, data: Arc::new(data) }
    }

    /// Wrap RGBA8 bytes; `None` when the length does not match or a side is out of range.
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return None;
        }
        let expected = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
        (data.len() == expected).then(|| Self { width, height, data: Arc::new(data) })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// RGBA8 bytes, row-major.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Whether this is the empty default (0 × 0) raster.
    /// Whether two rasters share the same pixel buffer (no copy was made).
    pub fn shares_pixels(&self, other: &Raster) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Whether every pixel is fully transparent.
    pub fn is_blank(&self) -> bool {
        self.data.as_chunks::<4>().0.iter().all(|p| p[3] == 0)
    }

    /// The pixel at `(x, y)`, transparent outside the canvas.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0; 4];
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        match self.data.get(i..i + 4) {
            Some(p) => [p[0], p[1], p[2], p[3]],
            None => [0; 4],
        }
    }

    fn index(&self, x: i64, y: i64) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return None;
        }
        Some((y as usize * self.width as usize + x as usize) * 4)
    }

    /// Composite `color` (straight alpha) with `coverage` (0..=1) over the pixel at `i`.
    fn paint_px(&mut self, i: usize, color: [u8; 4], coverage: f32) {
        let Some(px) = Arc::make_mut(&mut self.data).get_mut(i..i + 4) else { return };
        let a = (color[3] as f32 / 255.0) * coverage.clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let da = px[3] as f32 / 255.0;
        let out_a = a + da * (1.0 - a);
        if out_a <= 0.0 {
            return;
        }
        for c in 0..3 {
            let v = (color[c] as f32 * a + px[c] as f32 * da * (1.0 - a)) / out_a;
            px[c] = v.round().clamp(0.0, 255.0) as u8;
        }
        px[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    fn erase_px(&mut self, i: usize, strength: f32) {
        let Some(px) = Arc::make_mut(&mut self.data).get_mut(i + 3) else { return };
        let keep = 1.0 - strength.clamp(0.0, 1.0);
        *px = (*px as f32 * keep).round().clamp(0.0, 255.0) as u8;
    }

    /// Stamp one round dab of `brush` centred at `p`.
    pub fn dab(&mut self, p: StrokePoint, brush: &Brush) {
        let pressure = finite_or(p.pressure, 1.0).clamp(0.0, 1.0);
        let base = finite_or(brush.radius, 1.0).clamp(0.25, 512.0);
        let radius = if brush.pressure_size { base * (0.25 + 0.75 * pressure) } else { base };
        let opacity = if brush.pressure_opacity { pressure } else { 1.0 };
        let (cx, cy) = (finite_or(p.x, -1e9), finite_or(p.y, -1e9));
        let hard = finite_or(brush.hardness, 1.0).clamp(0.0, 1.0);
        // Soft region: from `inner` to `radius`; at least ~1 px of anti-aliasing.
        let inner = (radius * hard).min(radius - 0.75).max(0.0);
        let x0 = (cx - radius - 1.0).floor().max(0.0);
        let y0 = (cy - radius - 1.0).floor().max(0.0);
        let x1 = (cx + radius + 1.0).ceil().min(self.width as f32);
        let y1 = (cy + radius + 1.0).ceil().min(self.height as f32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for y in y0 as i64..y1 as i64 {
            for x in x0 as i64..x1 as i64 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                if d >= radius {
                    continue;
                }
                let cov = if d <= inner {
                    1.0
                } else {
                    let t = ((d - inner) / (radius - inner).max(1e-3)).clamp(0.0, 1.0);
                    1.0 - t * t * (3.0 - 2.0 * t)
                };
                let Some(i) = self.index(x, y) else { continue };
                match brush.tool {
                    Tool::Brush => self.paint_px(i, brush.color, cov * opacity),
                    Tool::Eraser => self.erase_px(i, cov * opacity * (brush.color[3] as f32 / 255.0).max(0.5)),
                }
            }
        }
    }

    /// Stamp dabs along the segment `from → to` (excluding `from`, which the previous segment or
    /// [`Raster::dab`] already covered), interpolating pressure.
    pub fn stroke(&mut self, from: StrokePoint, to: StrokePoint, brush: &Brush) {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let len = (dx * dx + dy * dy).sqrt();
        if !len.is_finite() {
            return;
        }
        let spacing = (finite_or(brush.radius, 1.0) * 0.2).clamp(0.4, 64.0);
        let steps = ((len / spacing).ceil() as usize).clamp(1, MAX_DABS_PER_SEGMENT);
        for s in 1..=steps {
            let t = s as f32 / steps as f32;
            let p = StrokePoint::new(from.x + dx * t, from.y + dy * t, from.pressure + (to.pressure - from.pressure) * t);
            self.dab(p, brush);
        }
    }

    /// Draw a polyline (convenience for the demo board and tests).
    pub fn polyline(&mut self, points: &[(f32, f32)], brush: &Brush) {
        let mut prev: Option<StrokePoint> = None;
        for &(x, y) in points {
            let p = StrokePoint::new(x, y, 1.0);
            match prev {
                None => self.dab(p, brush),
                Some(q) => self.stroke(q, p, brush),
            }
            prev = Some(p);
        }
    }

    /// Bilinear sample at continuous pixel coordinates (pixel centres at `+0.5`), premultiplied
    /// RGBA in 0..=1. Outside the canvas is transparent.
    pub fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        let fx = x - 0.5;
        let fy = y - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let mut acc = [0.0f32; 4];
        for (ox, oy, w) in [(0.0, 0.0, (1.0 - tx) * (1.0 - ty)), (1.0, 0.0, tx * (1.0 - ty)), (0.0, 1.0, (1.0 - tx) * ty), (1.0, 1.0, tx * ty)] {
            if w <= 0.0 {
                continue;
            }
            let (px, py) = ((x0 + ox) as i64, (y0 + oy) as i64);
            let Some(i) = self.index(px, py) else { continue };
            let Some(p) = self.data.get(i..i + 4) else { continue };
            let a = p[3] as f32 / 255.0;
            acc[0] += p[0] as f32 / 255.0 * a * w;
            acc[1] += p[1] as f32 / 255.0 * a * w;
            acc[2] += p[2] as f32 / 255.0 * a * w;
            acc[3] += a * w;
        }
        acc
    }

    /// Render the normalized rectangle `rect = [x0, y0, x1, y1]` (0..=1 of this canvas) into a new
    /// `width × height` raster (bilinear; area-averaged when shrinking a lot).
    pub fn render_view(&self, rect: [f32; 4], width: u32, height: u32) -> Raster {
        let mut out = Raster::new(width, height);
        let (w, h) = (out.width, out.height);
        let sx = (rect[2] - rect[0]) * self.width as f32 / w as f32;
        let sy = (rect[3] - rect[1]) * self.height as f32 / h as f32;
        // Supersample when shrinking so thin pencil lines survive in thumbnails.
        let ss = (sx.max(sy).ceil() as usize).clamp(1, 4);
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 4];
                for j in 0..ss {
                    for i in 0..ss {
                        let u = rect[0] * self.width as f32 + (x as f32 + (i as f32 + 0.5) / ss as f32) * sx;
                        let v = rect[1] * self.height as f32 + (y as f32 + (j as f32 + 0.5) / ss as f32) * sy;
                        let s = self.sample(u, v);
                        for c in 0..4 {
                            acc[c] += s[c];
                        }
                    }
                }
                let n = (ss * ss) as f32;
                let a = acc[3] / n;
                let i = (y as usize * w as usize + x as usize) * 4;
                if let Some(px) = Arc::make_mut(&mut out.data).get_mut(i..i + 4) {
                    if a > 0.0 {
                        for c in 0..3 {
                            px[c] = (acc[c] / n / a * 255.0).round().clamp(0.0, 255.0) as u8;
                        }
                    }
                    px[3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    }

    /// A copy scaled to `width × height`.
    pub fn resized(&self, width: u32, height: u32) -> Raster {
        self.render_view([0.0, 0.0, 1.0, 1.0], width, height)
    }

    /// A copy of the rectangle `x, y, w, h` (clamped to the canvas).
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Raster {
        let x = x.min(self.width.saturating_sub(1));
        let y = y.min(self.height.saturating_sub(1));
        let w = w.clamp(1, self.width - x);
        let h = h.clamp(1, self.height - y);
        let mut out = Raster::new(w, h);
        let data = Arc::make_mut(&mut out.data);
        for row in 0..h {
            let s0 = ((y + row) as usize * self.width as usize + x as usize) * 4;
            let d0 = row as usize * w as usize * 4;
            let n = w as usize * 4;
            if let (Some(src), Some(dst)) = (self.data.get(s0..s0 + n), data.get_mut(d0..d0 + n)) {
                dst.copy_from_slice(src);
            }
        }
        out
    }

    /// The same pixels on a `width × height` canvas, centred: cropped where smaller, transparent
    /// where larger. No resampling.
    pub fn recanvas(&self, width: u32, height: u32) -> Raster {
        let mut out = Raster::new(width, height);
        if self.is_empty() {
            return out;
        }
        let ox = (out.width as i64 - self.width as i64) / 2;
        let oy = (out.height as i64 - self.height as i64) / 2;
        let row = self.width as usize * 4;
        let data = Arc::make_mut(&mut out.data);
        for y in 0..self.height as i64 {
            let ty = y + oy;
            if ty < 0 || ty >= out.height as i64 {
                continue;
            }
            let x0 = ox.max(0);
            let x1 = (ox + self.width as i64).min(out.width as i64);
            if x0 >= x1 {
                continue;
            }
            let src_x = (x0 - ox) as usize;
            let n = (x1 - x0) as usize * 4;
            let s0 = y as usize * row + src_x * 4;
            let d0 = (ty as usize * out.width as usize + x0 as usize) * 4;
            if let (Some(src), Some(dst)) = (self.data.get(s0..s0 + n), data.get_mut(d0..d0 + n)) {
                dst.copy_from_slice(src);
            }
        }
        out
    }

    /// Composite `top` over this raster with `opacity`. `top` is resampled when sizes differ.
    pub fn composite_over(&mut self, top: &Raster, opacity: f32) {
        let opacity = finite_or(opacity, 1.0).clamp(0.0, 1.0);
        if opacity <= 0.0 || top.is_empty() {
            return;
        }
        let scaled;
        let top = if top.width == self.width && top.height == self.height {
            top
        } else {
            scaled = top.resized(self.width, self.height);
            &scaled
        };
        for (i, p) in top.data.as_chunks::<4>().0.iter().enumerate() {
            if p[3] == 0 {
                continue;
            }
            self.paint_px(i * 4, *p, opacity);
        }
    }

    /// Draw `src` scaled to fit inside this canvas, centred (image import).
    pub fn place_fit(&mut self, src: &Raster) {
        if src.is_empty() {
            return;
        }
        let k = (self.width as f32 / src.width as f32).min(self.height as f32 / src.height as f32);
        let w = ((src.width as f32 * k).round() as u32).clamp(1, self.width);
        let h = ((src.height as f32 * k).round() as u32).clamp(1, self.height);
        let scaled = src.resized(w, h);
        let ox = (self.width - w) / 2;
        let oy = (self.height - h) / 2;
        for y in 0..h {
            for x in 0..w {
                let p = scaled.pixel(x, y);
                if let Some(i) = self.index((x + ox) as i64, (y + oy) as i64) {
                    self.paint_px(i, p, 1.0);
                }
            }
        }
    }

    /// Encode as PNG (through `craft-image-codecs`).
    pub fn to_png(&self) -> crate::Result<Vec<u8>> {
        use craft_image_codecs::{ChannelLayout, EncodeOptions, Format, Image, SampleType};
        let img = Image::from_raw(self.width, self.height, ChannelLayout::Rgba, SampleType::U8, self.data.to_vec())
            .map_err(|e| crate::Error::Image(e.to_string()))?;
        craft_image_codecs::encode(&img, Format::Png, &EncodeOptions::default()).map_err(|e| crate::Error::Image(e.to_string()))
    }

    /// Decode any flat image `craft-image-codecs` reads (PNG, JPEG, WebP, TIFF, …). Images larger
    /// than [`MAX_SIDE`] on a side are refused.
    pub fn decode(bytes: &[u8]) -> crate::Result<Raster> {
        let (w, h) = craft_image_codecs::probe_size(bytes).map_err(|e| crate::Error::Image(e.to_string()))?;
        if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
            return Err(crate::Error::Image(format!("image is {w}×{h}; BoardCraft takes images up to {MAX_SIDE}×{MAX_SIDE}")));
        }
        let img = craft_image_codecs::decode(bytes).map_err(|e| crate::Error::Image(e.to_string()))?;
        let (w, h) = (img.width(), img.height());
        Raster::from_rgba(w, h, img.to_rgba8()).ok_or_else(|| crate::Error::Image(format!("unsupported image size {w}×{h}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dab_paints_centre_and_leaves_far_pixels() {
        let mut r = Raster::new(32, 32);
        r.dab(StrokePoint::new(16.0, 16.0, 1.0), &Brush { radius: 4.0, hardness: 1.0, ..Brush::default() });
        assert_eq!(r.pixel(16, 16)[3], 255);
        assert_eq!(r.pixel(2, 2)[3], 0);
    }

    #[test]
    fn pressure_scales_size() {
        let brush = Brush { radius: 8.0, hardness: 1.0, ..Brush::default() };
        let mut light = Raster::new(64, 64);
        light.dab(StrokePoint::new(32.0, 32.0, 0.1), &brush);
        let mut heavy = Raster::new(64, 64);
        heavy.dab(StrokePoint::new(32.0, 32.0, 1.0), &brush);
        let count = |r: &Raster| r.data().as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
        assert!(count(&heavy) > 3 * count(&light));
    }

    #[test]
    fn stroke_connects_points_and_eraser_removes() {
        let mut r = Raster::new(100, 20);
        let brush = Brush { radius: 2.0, ..Brush::default() };
        r.polyline(&[(5.0, 10.0), (95.0, 10.0)], &brush);
        for x in (6..94).step_by(4) {
            assert!(r.pixel(x, 10)[3] > 200, "gap at x={x}");
        }
        let eraser = Brush { tool: Tool::Eraser, radius: 6.0, hardness: 1.0, ..Brush::default() };
        r.polyline(&[(40.0, 10.0), (60.0, 10.0)], &eraser);
        assert_eq!(r.pixel(50, 10)[3], 0);
        assert!(r.pixel(20, 10)[3] > 200);
    }

    #[test]
    fn hostile_coordinates_do_not_panic() {
        let mut r = Raster::new(16, 16);
        let b = Brush::default();
        r.stroke(StrokePoint::new(f32::NAN, 0.0, 1.0), StrokePoint::new(f32::INFINITY, -1e30, f32::NAN), &b);
        r.stroke(StrokePoint::new(-1e9, -1e9, 1.0), StrokePoint::new(1e9, 1e9, 1.0), &b);
        r.dab(StrokePoint::new(8.0, 8.0, 1.0), &Brush { radius: f32::NAN, hardness: f32::NAN, ..b });
        let _ = r.render_view([f32::NAN, 0.0, 1.0, 1.0], 4, 4);
    }

    #[test]
    fn png_round_trip_and_bad_bytes() {
        let mut r = Raster::new(20, 10);
        r.polyline(&[(2.0, 2.0), (18.0, 8.0)], &Brush::default());
        let back = Raster::decode(&r.to_png().unwrap()).unwrap();
        assert_eq!(back, r);
        assert!(Raster::decode(b"not an image").is_err());
        let png = r.to_png().unwrap();
        assert!(Raster::decode(&png[..png.len() / 2]).is_err());
    }

    #[test]
    fn composite_and_view() {
        let mut paper = Raster::filled(10, 10, [255, 255, 255, 255]);
        let ink = Raster::filled(10, 10, [0, 0, 0, 255]);
        paper.composite_over(&ink, 0.5);
        let p = paper.pixel(5, 5);
        assert!((120..=135).contains(&p[0]) && p[3] == 255, "{p:?}");
        let half = paper.render_view([0.0, 0.0, 0.5, 0.5], 10, 10);
        assert_eq!(half.width(), 10);
        assert_eq!(half.pixel(9, 9)[3], 255);
    }

    #[test]
    fn recanvas_crops_and_extends_without_stretching() {
        let mut r = Raster::new(10, 10);
        r.dab(StrokePoint::new(5.0, 5.0, 1.0), &Brush { radius: 1.0, hardness: 1.0, ..Brush::default() });
        let wide = r.recanvas(20, 10);
        assert_eq!((wide.width(), wide.height()), (20, 10));
        assert_eq!(wide.pixel(10, 5), r.pixel(5, 5));
        assert_eq!(wide.pixel(1, 5)[3], 0);
        let small = r.recanvas(4, 4);
        assert_eq!(small.pixel(2, 2), r.pixel(5, 5));
        assert!(Raster::default().recanvas(3, 3).is_blank());
    }

    #[test]
    fn clones_share_pixels_until_painted() {
        let a = Raster::new(32, 32);
        let mut b = a.clone();
        assert!(a.shares_pixels(&b));
        b.dab(StrokePoint::new(16.0, 16.0, 1.0), &Brush::default());
        assert!(!a.shares_pixels(&b));
        assert!(a.is_blank() && !b.is_blank());
    }

    #[test]
    fn from_rgba_rejects_wrong_sizes() {
        assert!(Raster::from_rgba(2, 2, vec![0; 15]).is_none());
        assert!(Raster::from_rgba(0, 2, vec![]).is_none());
        assert!(Raster::from_rgba(MAX_SIDE + 1, 1, vec![0; (MAX_SIDE as usize + 1) * 4]).is_none());
    }
}
