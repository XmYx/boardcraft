//! Panel camera: pan and zoom keyframes.
//!
//! The camera frame is a window onto the panel's canvas with the canvas's aspect ratio. A key gives
//! the frame's centre (normalized: 0..=1 across the canvas) and its zoom (1 = the whole canvas,
//! 2 = half the width). Between keys the camera eases (smoothstep); before the first key and after
//! the last it holds. A camera without keys shows the whole canvas.

use serde::{Deserialize, Serialize};

/// Most zoom a key may have.
pub const MAX_ZOOM: f32 = 16.0;

/// Normalized `[x0, y0, x1, y1]` rectangle of the canvas.
pub type ViewRect = [f32; 4];

/// One camera keyframe, at a frame offset from the start of its panel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraKey {
    pub frame: u32,
    pub center: [f32; 2],
    pub zoom: f32,
}

impl Default for CameraKey {
    fn default() -> Self {
        Self { frame: 0, center: [0.5, 0.5], zoom: 1.0 }
    }
}

impl CameraKey {
    /// Clamp hostile values: zoom to `1..=MAX_ZOOM`, centre so the frame stays on the canvas.
    pub fn sanitized(self) -> Self {
        let zoom = if self.zoom.is_finite() { self.zoom.clamp(1.0, MAX_ZOOM) } else { 1.0 };
        let half = 0.5 / zoom;
        let c = |v: f32| if v.is_finite() { v.clamp(half, 1.0 - half) } else { 0.5 };
        Self { frame: self.frame, center: [c(self.center[0]), c(self.center[1])], zoom }
    }

    /// The normalized canvas rectangle this key frames.
    pub fn rect(self) -> ViewRect {
        let k = self.sanitized();
        let half = 0.5 / k.zoom;
        [k.center[0] - half, k.center[1] - half, k.center[0] + half, k.center[1] + half]
    }
}

/// A panel's camera move.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    /// Keys sorted by frame, no two on the same frame.
    #[serde(default)]
    pub keys: Vec<CameraKey>,
}

impl Camera {
    /// The camera at `frame` (fractional frames allowed).
    pub fn at(&self, frame: f32) -> CameraKey {
        let Some(first) = self.keys.first() else { return CameraKey::default() };
        let frame = if frame.is_finite() { frame } else { 0.0 };
        if frame <= first.frame as f32 {
            return first.sanitized();
        }
        for pair in self.keys.windows(2) {
            let (a, b) = (pair[0].sanitized(), pair[1].sanitized());
            if frame <= b.frame as f32 {
                let span = (b.frame as f32 - a.frame as f32).max(1.0);
                let t = ((frame - a.frame as f32) / span).clamp(0.0, 1.0);
                let e = t * t * (3.0 - 2.0 * t);
                let lerp = |x: f32, y: f32| x + (y - x) * e;
                // Zoom interpolates geometrically so a push-in feels even.
                let zoom = (a.zoom.ln() + (b.zoom.ln() - a.zoom.ln()) * e).exp();
                return CameraKey { frame: frame as u32, center: [lerp(a.center[0], b.center[0]), lerp(a.center[1], b.center[1])], zoom }.sanitized();
            }
        }
        self.keys.last().copied().unwrap_or_default().sanitized()
    }

    /// The normalized canvas rectangle seen at `frame`.
    pub fn view(&self, frame: f32) -> ViewRect {
        self.at(frame).rect()
    }

    /// Insert `key`, replacing a key on the same frame.
    pub fn set_key(&mut self, key: CameraKey) {
        let key = key.sanitized();
        match self.keys.binary_search_by_key(&key.frame, |k| k.frame) {
            Ok(i) => {
                if let Some(k) = self.keys.get_mut(i) {
                    *k = key;
                }
            }
            Err(i) => self.keys.insert(i, key),
        }
    }

    /// Remove the key on `frame`; returns whether there was one.
    pub fn remove_key(&mut self, frame: u32) -> bool {
        let before = self.keys.len();
        self.keys.retain(|k| k.frame != frame);
        before != self.keys.len()
    }

    /// Whether the camera moves at all.
    pub fn moves(&self) -> bool {
        self.keys.windows(2).any(|w| w[0].rect() != w[1].rect())
    }

    /// Sort, de-duplicate and clamp keys (after loading a file).
    pub fn normalize(&mut self) {
        self.keys.sort_by_key(|k| k.frame);
        self.keys.dedup_by_key(|k| k.frame);
        for k in &mut self.keys {
            *k = k.sanitized();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_keys_is_full_frame() {
        assert_eq!(Camera::default().view(10.0), [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn push_in_eases_and_holds() {
        let mut c = Camera::default();
        c.set_key(CameraKey { frame: 0, center: [0.5, 0.5], zoom: 1.0 });
        c.set_key(CameraKey { frame: 48, center: [0.7, 0.4], zoom: 2.0 });
        assert!(c.moves());
        let mid = c.at(24.0);
        assert!((mid.zoom - 2f32.sqrt()).abs() < 1e-3, "{mid:?}");
        assert_eq!(c.at(100.0).zoom, 2.0);
        assert_eq!(c.at(-5.0).zoom, 1.0);
        // The frame never leaves the canvas.
        let r = c.view(48.0);
        assert!(r[0] >= 0.0 && r[2] <= 1.0 && r[1] >= 0.0 && r[3] <= 1.0);
    }

    #[test]
    fn set_key_replaces_same_frame_and_sorts() {
        let mut c = Camera::default();
        c.set_key(CameraKey { frame: 10, ..Default::default() });
        c.set_key(CameraKey { frame: 0, ..Default::default() });
        c.set_key(CameraKey { frame: 10, zoom: 3.0, ..Default::default() });
        assert_eq!(c.keys.iter().map(|k| k.frame).collect::<Vec<_>>(), [0, 10]);
        assert_eq!(c.keys[1].zoom, 3.0);
        assert!(c.remove_key(0));
        assert!(!c.remove_key(0));
    }

    #[test]
    fn hostile_keys_are_clamped() {
        let k = CameraKey { frame: 0, center: [f32::NAN, 9.0], zoom: f32::INFINITY }.sanitized();
        assert_eq!(k.zoom, 1.0);
        assert_eq!(k.center, [0.5, 0.5]);
        assert_eq!(CameraKey { zoom: 1000.0, ..Default::default() }.sanitized().zoom, MAX_ZOOM);
    }
}
