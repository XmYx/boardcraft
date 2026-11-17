//! The canvas view transform: canvas pixels ↔ screen points through zoom, pan and the view
//! rotation. Rotation only changes how the canvas is shown, never its pixels.

/// Maps between canvas pixels and screen points.
#[derive(Clone, Copy, Debug)]
pub struct ViewXf {
    /// The unrotated, zoomed canvas rectangle on screen; the view rotates about its centre.
    pub rect: egui::Rect,
    /// Clockwise rotation in radians.
    pub angle: f32,
    /// Canvas size in pixels.
    pub size: egui::Vec2,
}

fn rotate(v: egui::Vec2, angle: f32) -> egui::Vec2 {
    let (s, c) = angle.sin_cos();
    egui::vec2(v.x * c - v.y * s, v.x * s + v.y * c)
}

impl ViewXf {
    pub fn new(rect: egui::Rect, degrees: f32, width: u32, height: u32) -> Self {
        let angle = if degrees.is_finite() { degrees.to_radians() } else { 0.0 };
        Self { rect, angle, size: egui::vec2(width.max(1) as f32, height.max(1) as f32) }
    }

    /// Screen position of canvas point `c`.
    pub fn to_screen(self, c: [f32; 2]) -> egui::Pos2 {
        let p = egui::pos2(self.rect.left() + c[0] / self.size.x * self.rect.width(), self.rect.top() + c[1] / self.size.y * self.rect.height());
        self.rect.center() + rotate(p - self.rect.center(), self.angle)
    }

    /// Canvas point under screen position `s`.
    pub fn to_canvas(self, s: egui::Pos2) -> [f32; 2] {
        let p = self.rect.center() + rotate(s - self.rect.center(), -self.angle);
        [(p.x - self.rect.left()) / self.rect.width().max(1e-3) * self.size.x, (p.y - self.rect.top()) / self.rect.height().max(1e-3) * self.size.y]
    }

    /// A screen-space movement as a movement in normalized canvas units (0..1 of each side).
    pub fn delta_normalized(&self, d: egui::Vec2) -> egui::Vec2 {
        let r = rotate(d, -self.angle);
        egui::vec2(r.x / self.rect.width().max(1e-3), r.y / self.rect.height().max(1e-3))
    }

    /// Draw a texture over the canvas, rotated with the view.
    pub fn image(&self, painter: &egui::Painter, tex: egui::TextureId, tint: egui::Color32) {
        let mut mesh = egui::Mesh::with_texture(tex);
        let corners = [[0.0, 0.0], [self.size.x, 0.0], [self.size.x, self.size.y], [0.0, self.size.y]];
        let uvs = [egui::pos2(0.0, 0.0), egui::pos2(1.0, 0.0), egui::pos2(1.0, 1.0), egui::pos2(0.0, 1.0)];
        for (c, uv) in corners.iter().zip(uvs) {
            mesh.vertices.push(egui::epaint::Vertex { pos: self.to_screen(*c), uv, color: tint });
        }
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        painter.add(egui::Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_and_screen_round_trip_through_rotation() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(640.0, 360.0));
        for deg in [0.0, 15.0, 90.0, 200.0, -45.0] {
            let xf = ViewXf::new(rect, deg, 1280, 720);
            for c in [[0.0, 0.0], [640.0, 360.0], [1000.0, 100.0]] {
                let back = xf.to_canvas(xf.to_screen(c));
                assert!((back[0] - c[0]).abs() < 0.01 && (back[1] - c[1]).abs() < 0.01, "{deg}: {c:?} → {back:?}");
            }
        }
        // A quarter turn clockwise: canvas "right" points down on screen.
        let xf = ViewXf::new(rect, 90.0, 1280, 720);
        let a = xf.to_screen([640.0, 360.0]);
        let b = xf.to_screen([740.0, 360.0]);
        assert!((b.x - a.x).abs() < 0.01 && b.y > a.y);
        // Non-finite angles show the canvas unrotated.
        assert_eq!(ViewXf::new(rect, f32::NAN, 1280, 720).angle, 0.0);
    }
}
