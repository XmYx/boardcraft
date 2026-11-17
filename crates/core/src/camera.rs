//! Panel camera: pan, zoom and rotation keyframes.
//!
//! The camera frame is a window onto the panel's canvas with the canvas's aspect ratio. A key gives
//! the frame's centre (normalized: 0..=1 across the canvas), its zoom (1 = the whole canvas,
//! 2 = half the width) and its rotation (degrees the frame is turned clockwise on the canvas; the
//! picture then appears turned the other way). Between keys the camera eases (smoothstep): position
//! and rotation interpolate linearly in the eased time, zoom geometrically. Before the first key and
//! after the last it holds. A camera without keys shows the whole canvas. A panel may have any
//! number of keys (up to [`MAX_KEYS`]).
//!
//! A turned frame needs more zoom to stay on the canvas: [`CameraKey::fitted`] raises the zoom to
//! the least that keeps all four corners on the canvas and moves the centre inward, so what the
//! camera sees is always drawing, never the void around it. Keys store what the user typed; the
//! fitting happens when a view is rendered ([`Camera::quad`]).

use serde::{Deserialize, Serialize};

/// Most zoom a key may have.
pub const MAX_ZOOM: f32 = 16.0;
/// Most rotation a key may have, either way (two full turns, so a spin can be keyed).
pub const MAX_ROTATION: f32 = 720.0;
/// Most keys one panel's camera keeps (a guard against absurd files).
pub const MAX_KEYS: usize = 10_000;

/// Normalized `[x0, y0, x1, y1]` rectangle of the canvas.
pub type ViewRect = [f32; 4];

/// The camera frame's corners in normalized canvas coordinates, in the order top-left, top-right,
/// bottom-right, bottom-left of the picture the camera sees.
pub type ViewQuad = [[f32; 2]; 4];

/// One camera keyframe, at a frame offset from the start of its panel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraKey {
    pub frame: u32,
    pub center: [f32; 2],
    pub zoom: f32,
    /// Degrees the frame is turned clockwise on the canvas (schema 2; absent = 0).
    #[serde(default)]
    pub rotation: f32,
}

impl Default for CameraKey {
    fn default() -> Self {
        Self { frame: 0, center: [0.5, 0.5], zoom: 1.0, rotation: 0.0 }
    }
}

fn sane_aspect(aspect: f32) -> f32 {
    if aspect.is_finite() && aspect > 0.0 { aspect.clamp(1.0 / 64.0, 64.0) } else { 1.0 }
}

impl CameraKey {
    /// Clamp hostile values: zoom to `1..=MAX_ZOOM`, rotation to `±MAX_ROTATION`, centre so the
    /// unturned frame stays on the canvas.
    pub fn sanitized(self) -> Self {
        let zoom = if self.zoom.is_finite() { self.zoom.clamp(1.0, MAX_ZOOM) } else { 1.0 };
        let half = 0.5 / zoom;
        let c = |v: f32| if v.is_finite() { v.clamp(half, 1.0 - half) } else { 0.5 };
        let rotation = if self.rotation.is_finite() { self.rotation.clamp(-MAX_ROTATION, MAX_ROTATION) } else { 0.0 };
        Self { frame: self.frame, center: [c(self.center[0]), c(self.center[1])], zoom, rotation }
    }

    /// The least zoom at which a frame turned by `rotation` degrees fits on a canvas of `aspect`
    /// (width / height). At most [`MAX_ZOOM`].
    pub fn min_zoom(rotation: f32, aspect: f32) -> f32 {
        let a = sane_aspect(aspect);
        let r = if rotation.is_finite() { rotation.to_radians() } else { 0.0 };
        let (s, c) = (r.sin().abs(), r.cos().abs());
        (c + a.max(1.0 / a) * s).clamp(1.0, MAX_ZOOM)
    }

    /// This key as rendered on a canvas of `aspect` (width / height): zoom raised so the turned
    /// frame fits ([`CameraKey::min_zoom`]), centre moved so every corner stays on the canvas.
    pub fn fitted(self, aspect: f32) -> Self {
        let k = self.sanitized();
        let a = sane_aspect(aspect);
        let zoom = k.zoom.max(Self::min_zoom(k.rotation, a));
        let r = k.rotation.to_radians();
        let (s, c) = (r.sin().abs(), r.cos().abs());
        // Half extents of the turned frame's bounding box, normalized to the canvas.
        let ex = ((c + s / a) / (2.0 * zoom)).min(0.5);
        let ey = ((c + a * s) / (2.0 * zoom)).min(0.5);
        Self { center: [k.center[0].clamp(ex, 1.0 - ex), k.center[1].clamp(ey, 1.0 - ey)], zoom, ..k }
    }

    /// The normalized canvas rectangle this key frames, ignoring rotation.
    pub fn rect(self) -> ViewRect {
        let k = self.sanitized();
        let half = 0.5 / k.zoom;
        [k.center[0] - half, k.center[1] - half, k.center[0] + half, k.center[1] + half]
    }

    /// The corners of the frame this key sees on a canvas of `aspect` (after [`CameraKey::fitted`]).
    pub fn quad(self, aspect: f32) -> ViewQuad {
        let k = self.fitted(aspect);
        let a = sane_aspect(aspect);
        // Work in units where the canvas is `a` wide and 1 high, so turning keeps right angles.
        let (hw, hh) = (a / (2.0 * k.zoom), 1.0 / (2.0 * k.zoom));
        let r = k.rotation.to_radians();
        let (s, c) = (r.sin(), r.cos());
        let (cx, cy) = (k.center[0] * a, k.center[1]);
        let corner = |dx: f32, dy: f32| [(cx + dx * c - dy * s) / a, cy + dx * s + dy * c];
        [corner(-hw, -hh), corner(hw, -hh), corner(hw, hh), corner(-hw, hh)]
    }

    /// Same framing (centre, zoom, rotation), whatever the frame number.
    pub fn same_view(self, other: CameraKey) -> bool {
        let (a, b) = (self.sanitized(), other.sanitized());
        a.center == b.center && a.zoom == b.zoom && a.rotation == b.rotation
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
    /// The camera at `frame` (fractional frames allowed), as keyed (not yet fitted).
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
                return CameraKey {
                    frame: frame as u32,
                    center: [lerp(a.center[0], b.center[0]), lerp(a.center[1], b.center[1])],
                    zoom,
                    rotation: lerp(a.rotation, b.rotation),
                }
                .sanitized();
            }
        }
        self.keys.last().copied().unwrap_or_default().sanitized()
    }

    /// The normalized canvas rectangle seen at `frame`, ignoring rotation.
    pub fn view(&self, frame: f32) -> ViewRect {
        self.at(frame).rect()
    }

    /// The frame seen at `frame` on a canvas of `aspect` (width / height): its four corners.
    pub fn quad(&self, frame: f32, aspect: f32) -> ViewQuad {
        self.at(frame).quad(aspect)
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
            Err(i) => {
                if self.keys.len() < MAX_KEYS {
                    self.keys.insert(i, key);
                }
            }
        }
    }

    /// Remove the key on `frame`; returns whether there was one.
    pub fn remove_key(&mut self, frame: u32) -> bool {
        let before = self.keys.len();
        self.keys.retain(|k| k.frame != frame);
        before != self.keys.len()
    }

    /// Move the key on `from` to frame `to`, replacing a key there. Returns whether a key moved.
    pub fn move_key(&mut self, from: u32, to: u32) -> bool {
        let Some(k) = self.key(from) else { return false };
        if from == to {
            return true;
        }
        self.remove_key(from);
        self.set_key(CameraKey { frame: to, ..k });
        true
    }

    /// The key on `frame`, if there is one.
    pub fn key(&self, frame: u32) -> Option<CameraKey> {
        self.keys.iter().find(|k| k.frame == frame).copied()
    }

    /// Whether the camera moves at all.
    pub fn moves(&self) -> bool {
        self.keys.windows(2).any(|w| !w[0].same_view(w[1]))
    }

    /// Whether any key turns the frame.
    pub fn turns(&self) -> bool {
        self.keys.iter().any(|k| k.sanitized().rotation != 0.0)
    }

    /// Sort, de-duplicate and clamp keys (after loading a file).
    pub fn normalize(&mut self) {
        self.keys.truncate(MAX_KEYS);
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
        c.set_key(CameraKey { frame: 0, center: [0.5, 0.5], zoom: 1.0, rotation: 0.0 });
        c.set_key(CameraKey { frame: 48, center: [0.7, 0.4], zoom: 2.0, rotation: 0.0 });
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
        let k = CameraKey { frame: 0, center: [f32::NAN, 9.0], zoom: f32::INFINITY, rotation: f32::NAN }.sanitized();
        assert_eq!(k.zoom, 1.0);
        assert_eq!(k.center, [0.5, 0.5]);
        assert_eq!(k.rotation, 0.0);
        assert_eq!(CameraKey { zoom: 1000.0, ..Default::default() }.sanitized().zoom, MAX_ZOOM);
        assert_eq!(CameraKey { rotation: 1e9, ..Default::default() }.sanitized().rotation, MAX_ROTATION);
        let q = CameraKey { rotation: 33.0, ..Default::default() }.quad(f32::NAN);
        assert!(q.iter().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn rotation_interpolates_and_reads_old_files() {
        let mut c = Camera::default();
        c.set_key(CameraKey { frame: 0, ..Default::default() });
        c.set_key(CameraKey { frame: 10, rotation: 90.0, zoom: 2.0, ..Default::default() });
        assert!((c.at(5.0).rotation - 45.0).abs() < 1e-3);
        assert!(c.moves() && c.turns());
        // A rotation-only change is a move.
        let mut r = Camera::default();
        r.set_key(CameraKey { frame: 0, ..Default::default() });
        r.set_key(CameraKey { frame: 9, rotation: 10.0, ..Default::default() });
        assert!(r.moves());
        // Schema 1 keys have no rotation.
        let k: CameraKey = serde_json::from_str(r#"{"frame":3,"center":[0.5,0.5],"zoom":1.5}"#).unwrap();
        assert_eq!(k.rotation, 0.0);
    }

    #[test]
    fn turned_frames_stay_on_the_canvas() {
        for aspect in [16.0 / 9.0, 1.0, 0.5, 2.39] {
            for rot in [0.0f32, 12.0, 45.0, 90.0, -130.0, 400.0] {
                for (center, zoom) in [([0.5, 0.5], 1.0), ([0.05, 0.95], 1.3), ([0.9, 0.2], 4.0)] {
                    let q = CameraKey { frame: 0, center, zoom, rotation: rot }.quad(aspect);
                    for p in q {
                        assert!((-1e-4..=1.0001).contains(&p[0]) && (-1e-4..=1.0001).contains(&p[1]), "{aspect} {rot} {center:?} {zoom}: {q:?}");
                    }
                }
            }
        }
        // Unturned, the quad is the rectangle.
        let k = CameraKey { center: [0.6, 0.4], zoom: 2.0, ..Default::default() };
        let r = k.rect();
        let q = k.quad(16.0 / 9.0);
        assert!((q[0][0] - r[0]).abs() < 1e-5 && (q[2][1] - r[3]).abs() < 1e-5, "{q:?} {r:?}");
        // Turning a quarter on a wide canvas needs more zoom.
        assert!(CameraKey::min_zoom(90.0, 16.0 / 9.0) > 1.7);
        assert_eq!(CameraKey::min_zoom(0.0, 16.0 / 9.0), 1.0);
    }

    #[test]
    fn keys_move_between_frames() {
        let mut c = Camera::default();
        c.set_key(CameraKey { frame: 0, ..Default::default() });
        c.set_key(CameraKey { frame: 10, zoom: 2.0, ..Default::default() });
        c.set_key(CameraKey { frame: 20, zoom: 3.0, ..Default::default() });
        assert_eq!(c.keys.len(), 3);
        assert!(c.move_key(10, 15));
        assert_eq!(c.keys.iter().map(|k| k.frame).collect::<Vec<_>>(), [0, 15, 20]);
        assert_eq!(c.key(15).unwrap().zoom, 2.0);
        assert!(!c.move_key(10, 3));
        // Moving onto another key replaces it.
        assert!(c.move_key(15, 20));
        assert_eq!(c.keys.len(), 2);
        assert_eq!(c.key(20).unwrap().zoom, 2.0);
    }
}
