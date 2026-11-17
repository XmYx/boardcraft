//! Pen input shaping: the pressure curve and the stroke stabilizer.
//!
//! Both are pure functions of the input samples, so the app, the CLI and the tests agree on what a
//! pen does. The pressure curve is a gamma curve (`out = in^gamma`): below 1 a light touch already
//! gives a thick line (soft), above 1 the pen needs more force (firm). The stabilizer is a moving
//! average of the last input points (a "lazy" brush): higher strength smooths more and trails the
//! pen further; [`Stabilizer::finish`] lets the stroke catch up with the last point.

use std::collections::VecDeque;

use crate::raster::StrokePoint;

/// Softest and firmest pressure curve.
pub const GAMMA_RANGE: std::ops::RangeInclusive<f32> = 0.25..=4.0;

/// Strongest stabilizer setting.
pub const MAX_STABILIZER: u8 = 10;

/// Map raw pen pressure (0..=1) through the gamma curve. Hostile values give sane output: a
/// non-finite pressure is full pressure, a non-finite gamma is linear.
pub fn apply_curve(pressure: f32, gamma: f32) -> f32 {
    let p = if pressure.is_finite() { pressure.clamp(0.0, 1.0) } else { 1.0 };
    let g = if gamma.is_finite() { gamma.clamp(*GAMMA_RANGE.start(), *GAMMA_RANGE.end()) } else { 1.0 };
    p.powf(g).clamp(0.0, 1.0)
}

/// How many recent points the stabilizer averages at `strength` (0 = no smoothing).
fn window(strength: u8) -> usize {
    1 + 2 * strength.min(MAX_STABILIZER) as usize
}

/// Smooths a stroke as it is drawn: each output is the average of the last few input points.
#[derive(Clone, Debug)]
pub struct Stabilizer {
    size: usize,
    recent: VecDeque<StrokePoint>,
}

impl Stabilizer {
    pub fn new(strength: u8) -> Self {
        let size = window(strength);
        Self { size, recent: VecDeque::with_capacity(size) }
    }

    /// Feed one input point; returns the smoothed point to draw to. Non-finite points are
    /// ignored (the last smoothed point is repeated, or `None` before any valid point).
    pub fn push(&mut self, p: StrokePoint) -> Option<StrokePoint> {
        if p.x.is_finite() && p.y.is_finite() {
            let pressure = if p.pressure.is_finite() { p.pressure.clamp(0.0, 1.0) } else { 1.0 };
            if self.recent.len() == self.size {
                self.recent.pop_front();
            }
            self.recent.push_back(StrokePoint::new(p.x, p.y, pressure));
        }
        self.average()
    }

    fn average(&self) -> Option<StrokePoint> {
        let n = self.recent.len();
        if n == 0 {
            return None;
        }
        let (mut x, mut y, mut pr) = (0.0f64, 0.0f64, 0.0f64);
        for q in &self.recent {
            x += q.x as f64;
            y += q.y as f64;
            pr += q.pressure as f64;
        }
        let k = n as f64;
        Some(StrokePoint::new((x / k) as f32, (y / k) as f32, (pr / k) as f32))
    }

    /// The points that bring the smoothed stroke to the last input point when the pen lifts.
    pub fn finish(&mut self) -> Vec<StrokePoint> {
        let Some(last) = self.recent.back().copied() else { return Vec::new() };
        let mut out = Vec::with_capacity(self.size);
        while self.recent.len() > 1 {
            self.recent.pop_front();
            if let Some(p) = self.average() {
                out.push(p);
            }
        }
        if out.last() != Some(&last) {
            out.push(last);
        }
        out
    }
}

/// Stabilize a whole stroke: the smoothed points, ending on the last input point.
pub fn stabilize(points: &[StrokePoint], strength: u8) -> Vec<StrokePoint> {
    let mut s = Stabilizer::new(strength);
    let mut out: Vec<StrokePoint> = points.iter().filter_map(|p| s.push(*p)).collect();
    if strength > 0 {
        out.extend(s.finish());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_is_linear_at_one_and_bends_either_way() {
        assert_eq!(apply_curve(0.5, 1.0), 0.5);
        assert!(apply_curve(0.5, 0.5) > 0.6, "soft curve lifts light pressure");
        assert!(apply_curve(0.5, 2.0) < 0.3, "firm curve needs more force");
        assert_eq!(apply_curve(1.0, 3.0), 1.0);
        assert_eq!(apply_curve(0.0, 0.3), 0.0);
    }

    #[test]
    fn curve_survives_hostile_input() {
        assert_eq!(apply_curve(f32::NAN, 2.0), 1.0);
        assert_eq!(apply_curve(0.5, f32::INFINITY), 0.5);
        assert_eq!(apply_curve(7.0, 1.0), 1.0);
        assert_eq!(apply_curve(-3.0, 1.0), 0.0);
        assert!((0.0..=1.0).contains(&apply_curve(0.5, 1e9)));
    }

    #[test]
    fn strength_zero_passes_points_through() {
        let pts: Vec<StrokePoint> = (0..10).map(|i| StrokePoint::new(i as f32, (i * i) as f32, 0.5)).collect();
        assert_eq!(stabilize(&pts, 0), pts);
    }

    #[test]
    fn smoothing_reduces_jitter_and_ends_on_the_last_point() {
        // A horizontal line with ±3 px of zigzag noise.
        let pts: Vec<StrokePoint> = (0..60).map(|i| StrokePoint::new(i as f32 * 2.0, if i % 2 == 0 { 3.0 } else { -3.0 }, 1.0)).collect();
        let jitter = |ps: &[StrokePoint]| ps.windows(2).map(|w| (w[1].y - w[0].y).abs()).sum::<f32>() / ps.len() as f32;
        let smooth = stabilize(&pts, 6);
        assert!(jitter(&smooth) < jitter(&pts) / 4.0, "{} vs {}", jitter(&smooth), jitter(&pts));
        assert_eq!(smooth.last(), pts.last());
        // More strength smooths more.
        assert!(jitter(&stabilize(&pts, 10)) <= jitter(&stabilize(&pts, 2)));
    }

    #[test]
    fn stabilizer_ignores_non_finite_points_and_clamps_strength() {
        let mut s = Stabilizer::new(200);
        assert_eq!(s.push(StrokePoint::new(f32::NAN, 0.0, 1.0)), None);
        let a = s.push(StrokePoint::new(10.0, 10.0, f32::NAN));
        assert_eq!(a, Some(StrokePoint::new(10.0, 10.0, 1.0)));
        assert_eq!(s.push(StrokePoint::new(f32::INFINITY, 1.0, 1.0)), a);
        assert_eq!(window(200), window(MAX_STABILIZER));
        assert!(Stabilizer::new(3).finish().is_empty());
    }
}
