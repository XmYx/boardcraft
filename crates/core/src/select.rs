//! Selections on a layer: masks from a polygon (lasso) or a rectangle, and moving, scaling and
//! rotating the selected pixels with bilinear resampling.
//!
//! A selection is a [`Mask`] over the canvas. [`copy_masked`] lifts the selected pixels into a
//! [`Piece`] (cropped to the mask's bounds), [`clear_masked`] removes them from the layer, and
//! [`composite_transformed`] draws a piece back through a [`Transform`] (move, scale, rotate about
//! the piece's centre). Every function bounds its work by the canvas size.

use serde::{Deserialize, Serialize};

use crate::raster::Raster;

/// Most polygon points a lasso may have (a guard against absurd input).
pub const MAX_POLYGON_POINTS: usize = 100_000;

/// Smallest and largest scale factor a transform may use.
pub const SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.02..=16.0;

/// Which canvas pixels are selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask {
    width: u32,
    height: u32,
    bits: Vec<bool>,
}

impl Mask {
    /// Nothing selected.
    pub fn empty(width: u32, height: u32) -> Self {
        let (width, height) = (width.clamp(1, crate::raster::MAX_SIDE), height.clamp(1, crate::raster::MAX_SIDE));
        Self { width, height, bits: vec![false; width as usize * height as usize] }
    }

    /// The whole canvas.
    pub fn all(width: u32, height: u32) -> Self {
        let mut m = Self::empty(width, height);
        m.bits.iter_mut().for_each(|b| *b = true);
        m
    }

    /// Pixels whose centre lies in the rectangle between two corners (any order).
    pub fn rect(width: u32, height: u32, a: [f32; 2], b: [f32; 2]) -> Self {
        Self::polygon(width, height, &[[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]])
    }

    /// Pixels whose centre lies inside the closed polygon `points` (even-odd rule). Non-finite
    /// points are dropped; fewer than three points select nothing.
    pub fn polygon(width: u32, height: u32, points: &[[f32; 2]]) -> Self {
        let mut m = Self::empty(width, height);
        let pts: Vec<[f32; 2]> = points.iter().take(MAX_POLYGON_POINTS).filter(|p| p[0].is_finite() && p[1].is_finite()).copied().collect();
        if pts.len() < 3 {
            return m;
        }
        let (w, h) = (m.width as usize, m.height as usize);
        let mut xs: Vec<f32> = Vec::new();
        for y in 0..h {
            let cy = y as f32 + 0.5;
            xs.clear();
            for i in 0..pts.len() {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                // Half-open rule so a vertex on the scanline counts once.
                if (a[1] <= cy && b[1] > cy) || (b[1] <= cy && a[1] > cy) {
                    let t = (cy - a[1]) / (b[1] - a[1]);
                    xs.push(a[0] + t * (b[0] - a[0]));
                }
            }
            xs.sort_by(|a, b| a.total_cmp(b));
            for pair in xs.as_chunks::<2>().0 {
                // Pixels whose centre x + 0.5 is in [pair[0], pair[1]).
                let x0 = (pair[0] - 0.5).ceil().max(0.0);
                let x1 = (pair[1] - 0.5).ceil().min(w as f32);
                if x0 >= x1 {
                    continue;
                }
                let row = y * w;
                for x in x0 as usize..x1 as usize {
                    if let Some(b) = m.bits.get_mut(row + x) {
                        *b = true;
                    }
                }
            }
        }
        m
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn contains(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height && self.bits.get(y as usize * self.width as usize + x as usize).copied().unwrap_or(false)
    }

    /// The selected area's bounds `[x0, y0, x1, y1]` (exclusive end), or `None` when empty.
    pub fn bounds(&self) -> Option<[u32; 4]> {
        let w = self.width as usize;
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for (i, _) in self.bits.iter().enumerate().filter(|(_, b)| **b) {
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
        (x0 != usize::MAX).then_some([x0 as u32, y0 as u32, x1 as u32, y1 as u32])
    }

    pub fn is_empty(&self) -> bool {
        !self.bits.iter().any(|b| *b)
    }
}

/// Move, scale and rotate about a pivot (the selection's centre).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    /// Offset in canvas pixels.
    pub dx: f32,
    pub dy: f32,
    /// Scale factors (1 = unchanged).
    pub scale_x: f32,
    pub scale_y: f32,
    /// Clockwise rotation in degrees (canvas y points down).
    pub angle: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self { dx: 0.0, dy: 0.0, scale_x: 1.0, scale_y: 1.0, angle: 0.0 }
    }
}

fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

impl Transform {
    /// Values clamped to what BoardCraft can render (hostile input gives the identity's values).
    pub fn sanitized(&self) -> Self {
        let lim = (crate::raster::MAX_SIDE * 4) as f32;
        let scale = |v: f32| finite_or(v, 1.0).clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
        Self {
            dx: finite_or(self.dx, 0.0).clamp(-lim, lim),
            dy: finite_or(self.dy, 0.0).clamp(-lim, lim),
            scale_x: scale(self.scale_x),
            scale_y: scale(self.scale_y),
            angle: finite_or(self.angle, 0.0).rem_euclid(360.0),
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// Where `p` goes, rotating and scaling about `pivot`.
    pub fn apply(&self, pivot: [f32; 2], p: [f32; 2]) -> [f32; 2] {
        let t = self.sanitized();
        let (s, c) = t.angle.to_radians().sin_cos();
        let (x, y) = ((p[0] - pivot[0]) * t.scale_x, (p[1] - pivot[1]) * t.scale_y);
        [pivot[0] + t.dx + x * c - y * s, pivot[1] + t.dy + x * s + y * c]
    }

    /// The point that [`Transform::apply`] sends to `q`.
    pub fn invert(&self, pivot: [f32; 2], q: [f32; 2]) -> [f32; 2] {
        let t = self.sanitized();
        let (s, c) = t.angle.to_radians().sin_cos();
        let (x, y) = (q[0] - pivot[0] - t.dx, q[1] - pivot[1] - t.dy);
        let (rx, ry) = (x * c + y * s, -x * s + y * c);
        [pivot[0] + rx / t.scale_x, pivot[1] + ry / t.scale_y]
    }
}

/// Selected pixels lifted off a layer: `pixels` sit with their top-left at canvas `(x, y)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub pixels: Raster,
    pub x: i64,
    pub y: i64,
}

impl Piece {
    /// The centre the transform rotates and scales about.
    pub fn pivot(&self) -> [f32; 2] {
        [self.x as f32 + self.pixels.width() as f32 / 2.0, self.y as f32 + self.pixels.height() as f32 / 2.0]
    }

    /// The piece's corners after `t` (top-left, top-right, bottom-right, bottom-left).
    pub fn corners(&self, t: &Transform) -> [[f32; 2]; 4] {
        let (x0, y0) = (self.x as f32, self.y as f32);
        let (x1, y1) = (x0 + self.pixels.width() as f32, y0 + self.pixels.height() as f32);
        let p = self.pivot();
        [t.apply(p, [x0, y0]), t.apply(p, [x1, y0]), t.apply(p, [x1, y1]), t.apply(p, [x0, y1])]
    }

    /// The piece rendered through `t` into a new piece covering its transformed bounds (limited to
    /// [`crate::raster::MAX_SIDE`] a side).
    pub fn transformed(&self, t: &Transform) -> Piece {
        if t.is_identity() {
            return self.clone();
        }
        let c = self.corners(t);
        let fx0 = c.iter().map(|p| p[0]).fold(f32::MAX, f32::min).floor();
        let fy0 = c.iter().map(|p| p[1]).fold(f32::MAX, f32::min).floor();
        let fx1 = c.iter().map(|p| p[0]).fold(f32::MIN, f32::max).ceil();
        let fy1 = c.iter().map(|p| p[1]).fold(f32::MIN, f32::max).ceil();
        let max = crate::raster::MAX_SIDE as f32;
        let w = (fx1 - fx0).clamp(1.0, max) as u32;
        let h = (fy1 - fy0).clamp(1.0, max) as u32;
        let mut out = Piece { pixels: Raster::new(w, h), x: fx0 as i64, y: fy0 as i64 };
        render_into(&mut out.pixels, out.x, out.y, self, t);
        out
    }
}

/// Copy the selected pixels of `layer` (everything else transparent), cropped to the selection's
/// bounds. `None` when the selection is empty or a different size than the layer.
pub fn copy_masked(layer: &Raster, mask: &Mask) -> Option<Piece> {
    if layer.width() != mask.width || layer.height() != mask.height {
        return None;
    }
    let [x0, y0, x1, y1] = mask.bounds()?;
    let mut pixels = layer.crop(x0, y0, x1 - x0, y1 - y0);
    let w = pixels.width();
    let data = pixels.data_mut();
    for y in y0..y1 {
        for x in x0..x1 {
            if !mask.contains(x, y) {
                let i = (((y - y0) * w + (x - x0)) * 4) as usize;
                if let Some(px) = data.get_mut(i..i + 4) {
                    px.copy_from_slice(&[0, 0, 0, 0]);
                }
            }
        }
    }
    Some(Piece { pixels, x: x0 as i64, y: y0 as i64 })
}

/// Make the selected pixels of `layer` transparent. Returns whether anything was selected.
pub fn clear_masked(layer: &mut Raster, mask: &Mask) -> bool {
    if layer.width() != mask.width || layer.height() != mask.height || mask.is_empty() {
        return false;
    }
    let data = layer.data_mut();
    for (i, _) in mask.bits.iter().enumerate().filter(|(_, b)| **b) {
        if let Some(px) = data.get_mut(i * 4..i * 4 + 4) {
            px.copy_from_slice(&[0, 0, 0, 0]);
        }
    }
    true
}

/// Draw `piece` through `t` over `dst` (bilinear resampling; straight-alpha "over").
pub fn composite_transformed(dst: &mut Raster, piece: &Piece, t: &Transform) {
    render_into(dst, 0, 0, piece, t);
}

/// Render `piece` through `t` over `dst`, whose top-left sits at canvas `(ox, oy)`.
fn render_into(dst: &mut Raster, ox: i64, oy: i64, piece: &Piece, t: &Transform) {
    if piece.pixels.is_empty() {
        return;
    }
    let t = t.sanitized();
    let c = piece.corners(&t);
    let min_x = c.iter().map(|p| p[0]).fold(f32::MAX, f32::min).floor() - ox as f32 - 1.0;
    let min_y = c.iter().map(|p| p[1]).fold(f32::MAX, f32::min).floor() - oy as f32 - 1.0;
    let max_x = c.iter().map(|p| p[0]).fold(f32::MIN, f32::max).ceil() - ox as f32 + 1.0;
    let max_y = c.iter().map(|p| p[1]).fold(f32::MIN, f32::max).ceil() - oy as f32 + 1.0;
    let x0 = min_x.max(0.0) as u32;
    let y0 = min_y.max(0.0) as u32;
    let x1 = max_x.clamp(0.0, dst.width() as f32) as u32;
    let y1 = max_y.clamp(0.0, dst.height() as f32) as u32;
    let pivot = piece.pivot();
    // An exact copy when only moved by whole pixels: no resampling blur.
    let whole = t.scale_x == 1.0 && t.scale_y == 1.0 && t.angle == 0.0 && t.dx.fract() == 0.0 && t.dy.fract() == 0.0;
    for y in y0..y1 {
        for x in x0..x1 {
            let q = [(x as i64 + ox) as f32 + 0.5, (y as i64 + oy) as f32 + 0.5];
            let src = t.invert(pivot, q);
            let (u, v) = (src[0] - piece.x as f32, src[1] - piece.y as f32);
            let color = if whole {
                let (ux, vy) = (u.floor(), v.floor());
                if ux < 0.0 || vy < 0.0 {
                    continue;
                }
                piece.pixels.pixel(ux as u32, vy as u32)
            } else {
                let s = piece.pixels.sample(u, v);
                if s[3] <= 0.0 {
                    continue;
                }
                let a = s[3];
                let ch = |c: f32| (c / a * 255.0).round().clamp(0.0, 255.0) as u8;
                [ch(s[0]), ch(s[1]), ch(s[2]), (a * 255.0).round().clamp(0.0, 255.0) as u8]
            };
            if color[3] == 0 {
                continue;
            }
            if let Some(i) = dst.index(x as i64, y as i64) {
                dst.paint_px(i, color, 1.0);
            }
        }
    }
}

/// Lift the pixels inside `polygon` from `layer` and draw them back through `t`. Returns whether
/// anything was selected.
pub fn transform_region(layer: &mut Raster, polygon: &[[f32; 2]], t: &Transform) -> bool {
    let mask = Mask::polygon(layer.width(), layer.height(), polygon);
    let Some(piece) = copy_masked(layer, &mask) else { return false };
    clear_masked(layer, &mask);
    composite_transformed(layer, &piece, t);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(r: &Raster) -> usize {
        r.data().as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count()
    }

    #[test]
    fn polygon_mask_selects_inside_only() {
        let m = Mask::polygon(20, 20, &[[2.0, 2.0], [12.0, 2.0], [12.0, 12.0], [2.0, 12.0]]);
        assert!(m.contains(2, 2) && m.contains(11, 11));
        assert!(!m.contains(12, 12) && !m.contains(1, 5) && !m.contains(19, 19));
        assert_eq!(m.bounds(), Some([2, 2, 12, 12]));
        // A triangle covers about half its box.
        let t = Mask::polygon(100, 100, &[[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]]);
        let n = (0..100).flat_map(|y| (0..100).map(move |x| (x, y))).filter(|(x, y)| t.contains(*x, *y)).count();
        assert!((4800..=5200).contains(&n), "{n}");
        // Rectangle corners in any order.
        assert_eq!(Mask::rect(20, 20, [12.0, 12.0], [2.0, 2.0]), m);
    }

    #[test]
    fn hostile_polygons_select_nothing_or_stay_in_bounds() {
        assert!(Mask::polygon(10, 10, &[[1.0, 1.0], [5.0, 5.0]]).is_empty());
        assert!(Mask::polygon(10, 10, &[[f32::NAN, 1.0], [5.0, f32::INFINITY], [1.0, 1.0]]).is_empty());
        let huge = Mask::polygon(10, 10, &[[-1e30, -1e30], [1e30, -1e30], [1e30, 1e30], [-1e30, 1e30]]);
        assert_eq!(huge.bounds(), Some([0, 0, 10, 10]));
        assert_eq!(Mask::polygon(0, 0, &[]).width(), 1);
    }

    #[test]
    fn copy_and_clear_follow_the_mask() {
        let mut layer = Raster::filled(10, 10, [200, 0, 0, 255]);
        let mask = Mask::rect(10, 10, [2.0, 3.0], [6.0, 5.0]);
        let piece = copy_masked(&layer, &mask).unwrap();
        assert_eq!((piece.x, piece.y, piece.pixels.width(), piece.pixels.height()), (2, 3, 4, 2));
        assert_eq!(count(&piece.pixels), 8);
        assert!(clear_masked(&mut layer, &mask));
        assert_eq!(layer.pixel(3, 4)[3], 0);
        assert_eq!(layer.pixel(1, 4)[3], 255);
        assert_eq!(count(&layer), 100 - 8);
        assert!(copy_masked(&layer, &Mask::empty(10, 10)).is_none());
        assert!(copy_masked(&layer, &Mask::all(5, 5)).is_none(), "size mismatch");
    }

    #[test]
    fn whole_pixel_moves_are_exact() {
        let mut layer = Raster::new(20, 20);
        let mut src = Raster::new(20, 20);
        src.data_mut()[(5 * 20 + 5) * 4..(5 * 20 + 5) * 4 + 4].copy_from_slice(&[10, 20, 30, 255]);
        layer.composite_over(&src, 1.0);
        let poly = [[4.0, 4.0], [7.0, 4.0], [7.0, 7.0], [4.0, 7.0]];
        assert!(transform_region(&mut layer, &poly, &Transform { dx: 3.0, dy: -2.0, ..Transform::default() }));
        assert_eq!(layer.pixel(5, 5)[3], 0);
        assert_eq!(layer.pixel(8, 3), [10, 20, 30, 255]);
        assert_eq!(count(&layer), 1);
    }

    #[test]
    fn scale_and_rotate_resample() {
        // A 4×2 bar becomes a 2×4 bar after a quarter turn about its centre.
        let mut layer = Raster::new(20, 20);
        let mask = Mask::rect(20, 20, [8.0, 9.0], [12.0, 11.0]);
        let piece = copy_masked(&Raster::filled(20, 20, [0, 0, 0, 255]), &mask).unwrap();
        composite_transformed(&mut layer, &piece, &Transform { angle: 90.0, ..Transform::default() });
        assert!(layer.pixel(10, 8)[3] > 200 && layer.pixel(10, 11)[3] > 200, "vertical after rotation");
        assert!(layer.pixel(8, 10)[3] < 60, "{:?}", layer.pixel(8, 10));
        // Doubling the scale covers about four times the area.
        let mut big = Raster::new(40, 40);
        let sq = copy_masked(&Raster::filled(40, 40, [0, 0, 0, 255]), &Mask::rect(40, 40, [15.0, 15.0], [25.0, 25.0])).unwrap();
        composite_transformed(&mut big, &sq, &Transform { scale_x: 2.0, scale_y: 2.0, ..Transform::default() });
        let n = big.data().as_chunks::<4>().0.iter().filter(|p| p[3] > 128).count();
        assert!((380..=420).contains(&n), "{n}");
        // transformed() renders the same pixels into its own bounds.
        let moved = sq.transformed(&Transform { angle: 45.0, ..Transform::default() });
        assert!(moved.pixels.width() >= 14 && moved.pixels.width() <= 16, "{}", moved.pixels.width());
    }

    #[test]
    fn transform_round_trips_and_sanitizes() {
        let t = Transform { dx: 5.0, dy: -3.0, scale_x: 2.0, scale_y: 0.5, angle: 30.0 };
        let p = t.apply([10.0, 10.0], [14.0, 7.0]);
        let back = t.invert([10.0, 10.0], p);
        assert!((back[0] - 14.0).abs() < 1e-3 && (back[1] - 7.0).abs() < 1e-3);
        let bad = Transform { dx: f32::NAN, dy: 1e30, scale_x: 0.0, scale_y: f32::INFINITY, angle: -90.0 }.sanitized();
        assert_eq!(bad.dx, 0.0);
        assert_eq!(bad.scale_x, *SCALE_RANGE.start());
        assert_eq!(bad.scale_y, 1.0);
        assert_eq!(bad.angle, 270.0);
        // Hostile transforms render without panicking.
        let mut layer = Raster::filled(16, 16, [0, 0, 0, 255]);
        let poly = [[0.0, 0.0], [16.0, 0.0], [16.0, 16.0], [0.0, 16.0]];
        transform_region(&mut layer, &poly, &Transform { dx: f32::NAN, scale_x: 1e9, scale_y: -1.0, angle: f32::INFINITY, dy: 0.0 });
    }
}
