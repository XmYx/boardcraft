//! Bucket fill.
//!
//! [`flood_fill`] fills the connected area around a seed pixel whose colour is within a tolerance
//! of the seed's. The area is found on a *sample* raster (the layer itself, or a composite of all
//! visible layers) and painted on the *target* layer. Gap closing treats openings in the line art
//! up to about twice the gap as closed, so a sketchy outline still holds the fill. The fill is an
//! explicit scanline stack (no recursion), so its memory is bounded by the canvas size.

use crate::raster::Raster;

/// Largest gap-closing radius (pixels).
pub const MAX_GAP: u32 = 16;

/// How a fill decides what belongs to the area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FillOptions {
    /// Largest per-channel difference (0..=255, premultiplied) still counted as the seed's colour.
    pub tolerance: u8,
    /// Close openings in the line art up to about twice this many pixels (0 = off).
    pub gap: u32,
}

impl Default for FillOptions {
    fn default() -> Self {
        Self { tolerance: 32, gap: 0 }
    }
}

fn premul(p: [u8; 4]) -> [i32; 4] {
    let a = p[3] as i32;
    [p[0] as i32 * a / 255, p[1] as i32 * a / 255, p[2] as i32 * a / 255, a]
}

/// Grow `set` by `r` pixels (square neighbourhood), in two separable passes.
fn dilate(set: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    if r == 0 {
        return set.to_vec();
    }
    let mut horiz = vec![false; set.len()];
    for y in 0..h {
        let row = &set[y * w..(y + 1) * w];
        // Distance to the nearest set pixel to the left and right, in one sweep each.
        let mut last: Option<usize> = None;
        for x in 0..w {
            if row[x] {
                last = Some(x);
            }
            if last.is_some_and(|l| x - l <= r) {
                horiz[y * w + x] = true;
            }
        }
        let mut next: Option<usize> = None;
        for x in (0..w).rev() {
            if row[x] {
                next = Some(x);
            }
            if next.is_some_and(|n| n - x <= r) {
                horiz[y * w + x] = true;
            }
        }
    }
    let mut out = vec![false; set.len()];
    for x in 0..w {
        let mut last: Option<usize> = None;
        for y in 0..h {
            if horiz[y * w + x] {
                last = Some(y);
            }
            if last.is_some_and(|l| y - l <= r) {
                out[y * w + x] = true;
            }
        }
        let mut next: Option<usize> = None;
        for y in (0..h).rev() {
            if horiz[y * w + x] {
                next = Some(y);
            }
            if next.is_some_and(|n| n - y <= r) {
                out[y * w + x] = true;
            }
        }
    }
    out
}

/// Scanline flood fill over `open` from `seed`; returns the filled set.
fn flood(open: &[bool], w: usize, h: usize, seed: usize) -> Vec<bool> {
    let mut filled = vec![false; open.len()];
    if !open.get(seed).copied().unwrap_or(false) {
        return filled;
    }
    let mut stack = vec![seed];
    while let Some(i) = stack.pop() {
        if filled[i] || !open[i] {
            continue;
        }
        let y = i / w;
        let row = y * w;
        let (mut x0, mut x1) = (i - row, i - row);
        while x0 > 0 && open[row + x0 - 1] && !filled[row + x0 - 1] {
            x0 -= 1;
        }
        while x1 + 1 < w && open[row + x1 + 1] && !filled[row + x1 + 1] {
            x1 += 1;
        }
        for x in x0..=x1 {
            filled[row + x] = true;
        }
        for ny in [y.checked_sub(1), (y + 1 < h).then_some(y + 1)].into_iter().flatten() {
            let nrow = ny * w;
            let mut in_run = false;
            for x in x0..=x1 {
                let j = nrow + x;
                let ok = open[j] && !filled[j];
                if ok && !in_run {
                    stack.push(j);
                }
                in_run = ok;
            }
        }
    }
    filled
}

/// Fill the area around `(x, y)` on `target` with `color`, finding the area on `sample` (same size
/// as `target`). Returns how many pixels were filled; 0 when the seed is outside the canvas or the
/// sizes differ.
pub fn flood_fill(target: &mut Raster, sample: &Raster, x: u32, y: u32, color: [u8; 4], opts: FillOptions) -> usize {
    let (w, h) = (target.width() as usize, target.height() as usize);
    if sample.width() as usize != w || sample.height() as usize != h || x as usize >= w || y as usize >= h || target.is_empty() {
        return 0;
    }
    let seed_color = premul(sample.pixel(x, y));
    let tol = opts.tolerance as i32;
    let open: Vec<bool> = sample
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let q = premul(*p);
            (0..4).all(|c| (q[c] - seed_color[c]).abs() <= tol)
        })
        .collect();
    let seed = y as usize * w + x as usize;
    let gap = opts.gap.min(MAX_GAP) as usize;
    let filled = if gap > 0 {
        let walls: Vec<bool> = open.iter().map(|o| !o).collect();
        let near_wall = dilate(&walls, w, h, gap);
        let narrowed: Vec<bool> = open.iter().zip(&near_wall).map(|(o, n)| *o && !n).collect();
        let core = flood(&narrowed, w, h, seed);
        if core.iter().any(|f| *f) {
            // Grow back into the band along the lines, but not past it.
            let grown = dilate(&core, w, h, gap);
            grown.iter().zip(&open).zip(&near_wall).zip(&core).map(|(((g, o), n), c)| *c || (*g && *o && *n)).collect()
        } else {
            // The seed sits in a gap-sized nook: fill it without gap closing.
            flood(&open, w, h, seed)
        }
    } else {
        flood(&open, w, h, seed)
    };
    // One pixel more under the edges, so anti-aliased line edges leave no halo.
    let edge = dilate(&filled, w, h, 1);
    let mut n = 0;
    for i in 0..filled.len() {
        if filled[i] {
            target.paint_px(i * 4, color, 1.0);
            n += 1;
        } else if edge[i] {
            target.paint_under_px(i * 4, color);
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{Brush, StrokePoint};

    const RED: [u8; 4] = [220, 30, 30, 255];

    fn square_outline(size: u32, gap: f32) -> Raster {
        let mut r = Raster::new(size, size);
        let b = Brush { radius: 1.5, hardness: 1.0, ..Brush::default() };
        let (a, z) = (10.0, size as f32 - 10.0);
        r.polyline(&[(a, a), (z, a), (z, z), (a, z)], &b);
        // Left side, with an opening of `gap` pixels in the middle.
        let mid = size as f32 / 2.0;
        if gap > 0.0 {
            r.polyline(&[(a, z), (a, mid + gap / 2.0 + 1.5)], &b);
            r.polyline(&[(a, mid - gap / 2.0 - 1.5), (a, a)], &b);
        } else {
            r.polyline(&[(a, z), (a, a)], &b);
        }
        r
    }

    #[test]
    fn fills_inside_a_closed_outline_only() {
        let mut layer = square_outline(60, 0.0);
        let sample = layer.clone();
        let n = flood_fill(&mut layer, &sample, 30, 30, RED, FillOptions::default());
        assert!(n > 1200 && n < 1700, "{n}");
        assert_eq!(layer.pixel(30, 30), RED);
        assert_eq!(layer.pixel(2, 2)[3], 0, "outside untouched");
        // The line stays on top of the fill along its edge.
        assert!(layer.pixel(10, 30)[0] < 100, "{:?}", layer.pixel(10, 30));
    }

    #[test]
    fn gap_closing_holds_a_leaky_outline() {
        let leaky = square_outline(60, 4.0);
        let mut open = leaky.clone();
        let n_open = flood_fill(&mut open, &leaky, 30, 30, RED, FillOptions::default());
        assert!(n_open > 2500, "without gap closing the fill leaks out ({n_open})");
        let mut closed = leaky.clone();
        let n = flood_fill(&mut closed, &leaky, 30, 30, RED, FillOptions { gap: 3, ..FillOptions::default() });
        assert!(n > 1100 && n < 1700, "{n}");
        assert_eq!(closed.pixel(2, 2)[3], 0, "no leak");
        assert_eq!(closed.pixel(30, 30), RED);
    }

    #[test]
    fn samples_another_raster_and_respects_tolerance() {
        // Lines on one layer, fill on an empty one (sampling "all layers").
        let lines = square_outline(60, 0.0);
        let mut colour = Raster::new(60, 60);
        let n = flood_fill(&mut colour, &lines, 30, 30, RED, FillOptions::default());
        assert!(n > 1200);
        assert_eq!(lines.pixel(30, 30)[3], 0, "the sample is not painted");
        // Tolerance: a soft gradient stops a strict fill and passes a loose one.
        let mut grad = Raster::new(50, 1);
        for x in 0..50u32 {
            let v = (x * 5) as u8;
            grad.data_mut()[(x * 4) as usize..(x * 4 + 4) as usize].copy_from_slice(&[v, v, v, 255]);
        }
        let g = grad.clone();
        assert!(flood_fill(&mut grad.clone(), &g, 0, 0, RED, FillOptions { tolerance: 0, gap: 0 }) == 1);
        assert_eq!(flood_fill(&mut grad, &g, 0, 0, RED, FillOptions { tolerance: 255, gap: 0 }), 50);
    }

    #[test]
    fn hostile_seeds_and_sizes_fill_nothing() {
        let mut r = Raster::new(10, 10);
        let s = r.clone();
        assert_eq!(flood_fill(&mut r, &s, 10, 0, RED, FillOptions::default()), 0);
        assert_eq!(flood_fill(&mut r, &Raster::new(5, 5), 1, 1, RED, FillOptions::default()), 0);
        assert_eq!(flood_fill(&mut r, &s, 1, 1, RED, FillOptions { gap: u32::MAX, tolerance: 0 }), 100);
    }

    #[test]
    fn large_canvas_fills_without_recursion() {
        // A maze-like comb would blow a recursive fill's stack; this one is iterative.
        let mut r = Raster::new(2000, 2000);
        let b = Brush { radius: 1.0, hardness: 1.0, ..Brush::default() };
        for x in (20..1980).step_by(40) {
            r.stroke(StrokePoint::new(x as f32, 0.0, 1.0), StrokePoint::new(x as f32, 1990.0, 1.0), &b);
        }
        let s = r.clone();
        let n = flood_fill(&mut r, &s, 1, 1999, RED, FillOptions::default());
        assert!(n > 3_000_000, "{n}");
    }
}
