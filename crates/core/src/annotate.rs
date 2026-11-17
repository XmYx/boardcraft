//! Vector annotations: editable shapes drawn on top of a panel's raster layers.
//!
//! A panel's `annotations` hold freehand vector strokes, straight arrows, curved motion arrows and
//! camera-move arrows. They stay editable (select, move, recolour, delete) and resolution-free:
//! [`draw`] rasterizes them on top of the layers whenever a panel is composited, so stills, board
//! sheets, thumbnails and timeline exports show them. Coordinates are canvas pixels (top-left
//! origin); a canvas resize shifts them with the drawing ([`crate::model::Project::set_canvas`]).
//!
//! Shapes go out and come back as SVG ([`to_svg`], [`from_svg`]) through `craft-svg` (writer,
//! path data) and `craft-xml`: a BoardCraft SVG keeps each shape's own description on its group,
//! so it reads back exactly; any other SVG's `path`, `line` and `polyline` elements come in as
//! freehand strokes.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::raster::{Brush, MAX_SIDE, Raster, StrokePoint, Tool};

/// Most shapes one panel keeps.
pub const MAX_SHAPES: usize = 5_000;
/// Most points one freehand stroke keeps.
pub const MAX_POINTS: usize = 20_000;
/// Line widths, canvas pixels.
pub const MIN_WIDTH: f32 = 0.5;
pub const MAX_WIDTH: f32 = 100.0;
/// Default annotation colour: a strong red that reads over pencil (straight alpha).
pub const DEFAULT_COLOR: [u8; 4] = [210, 40, 40, 255];
/// Largest SVG file read back (bytes of text).
const MAX_SVG_BYTES: usize = 32 << 20;
/// Segments a curve is cut into for drawing and hit tests.
const CURVE_STEPS: usize = 32;

/// What a shape is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShapeKind {
    /// A freehand vector line through `points`.
    Stroke { points: Vec<[f32; 2]> },
    /// A straight arrow from `from` to `to` (the head).
    Arrow { from: [f32; 2], to: [f32; 2] },
    /// A curved motion arrow: a quadratic curve from `from` through the pull of `via` to `to`.
    MotionArrow { from: [f32; 2], via: [f32; 2], to: [f32; 2] },
    /// A camera-move arrow: a broad hollow arrow, the storyboard sign for a pan or push.
    CameraArrow { from: [f32; 2], to: [f32; 2] },
}

/// One annotation shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    #[serde(flatten)]
    pub kind: ShapeKind,
    /// Straight-alpha RGBA.
    #[serde(default = "default_color")]
    pub color: [u8; 4],
    /// Line width in canvas pixels.
    #[serde(default = "default_width")]
    pub width: f32,
}

fn default_color() -> [u8; 4] {
    DEFAULT_COLOR
}

fn default_width() -> f32 {
    4.0
}

/// A shape as lines and filled triangles in canvas pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    /// Open polylines, stroked at `line_width`.
    pub lines: Vec<Vec<[f32; 2]>>,
    /// Filled triangles (arrow heads).
    pub fills: Vec<[[f32; 2]; 3]>,
    pub line_width: f32,
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn mul(a: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] * k, a[1] * k]
}

fn unit(v: [f32; 2]) -> [f32; 2] {
    let l = v[0].hypot(v[1]);
    if l > 1e-6 { [v[0] / l, v[1] / l] } else { [1.0, 0.0] }
}

fn quad_point(a: [f32; 2], b: [f32; 2], c: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [u * u * a[0] + 2.0 * u * t * b[0] + t * t * c[0], u * u * a[1] + 2.0 * u * t * b[1] + t * t * c[1]]
}

/// Distance from `p` to the segment `a–b`.
fn seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = sub(b, a);
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let q = add(a, mul(ab, t));
    (p[0] - q[0]).hypot(p[1] - q[1])
}

fn in_triangle(p: [f32; 2], t: &[[f32; 2]; 3]) -> bool {
    let e = |a: [f32; 2], b: [f32; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    let (d0, d1, d2) = (e(t[0], t[1]), e(t[1], t[2]), e(t[2], t[0]));
    (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0) || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
}

/// A solid head at `tip` pointing along `dir`, and where the shaft should stop.
fn head(tip: [f32; 2], dir: [f32; 2], width: f32) -> ([[f32; 2]; 3], [f32; 2]) {
    let len = (width * 4.0).max(10.0);
    let n = [-dir[1], dir[0]];
    let base = sub(tip, mul(dir, len));
    ([tip, add(base, mul(n, len * 0.5)), sub(base, mul(n, len * 0.5))], sub(tip, mul(dir, len * 0.7)))
}

fn clamp_coord(v: f32) -> Option<f32> {
    let lim = MAX_SIDE as f32 * 4.0;
    v.is_finite().then(|| v.clamp(-lim, lim))
}

fn clamp_point(p: [f32; 2]) -> Option<[f32; 2]> {
    Some([clamp_coord(p[0])?, clamp_coord(p[1])?])
}

impl Shape {
    pub fn new(kind: ShapeKind, color: [u8; 4], width: f32) -> Self {
        Self { kind, color, width }
    }

    /// The shape's kind as an English noun (for messages; the app translates its own labels).
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            ShapeKind::Stroke { .. } => "stroke",
            ShapeKind::Arrow { .. } => "arrow",
            ShapeKind::MotionArrow { .. } => "motion arrow",
            ShapeKind::CameraArrow { .. } => "camera arrow",
        }
    }

    /// Hostile values made safe: non-finite points dropped, coordinates and widths clamped, point
    /// counts capped. `None` when nothing drawable is left.
    pub fn sanitized(&self) -> Option<Shape> {
        let width = if self.width.is_finite() { self.width.clamp(MIN_WIDTH, MAX_WIDTH) } else { default_width() };
        let kind = match &self.kind {
            ShapeKind::Stroke { points } => {
                let points: Vec<[f32; 2]> = points.iter().filter_map(|p| clamp_point(*p)).take(MAX_POINTS).collect();
                if points.is_empty() {
                    return None;
                }
                ShapeKind::Stroke { points }
            }
            ShapeKind::Arrow { from, to } => ShapeKind::Arrow { from: clamp_point(*from)?, to: clamp_point(*to)? },
            ShapeKind::MotionArrow { from, via, to } => ShapeKind::MotionArrow { from: clamp_point(*from)?, via: clamp_point(*via)?, to: clamp_point(*to)? },
            ShapeKind::CameraArrow { from, to } => ShapeKind::CameraArrow { from: clamp_point(*from)?, to: clamp_point(*to)? },
        };
        Some(Shape { kind, color: self.color, width })
    }

    /// The points that define the shape (stroke points, arrow ends and curve handle).
    pub fn points_mut(&mut self) -> Vec<&mut [f32; 2]> {
        match &mut self.kind {
            ShapeKind::Stroke { points } => points.iter_mut().collect(),
            ShapeKind::Arrow { from, to } | ShapeKind::CameraArrow { from, to } => vec![from, to],
            ShapeKind::MotionArrow { from, via, to } => vec![from, via, to],
        }
    }

    /// Move the whole shape by `dx, dy` canvas pixels.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        for p in self.points_mut() {
            *p = clamp_point([p[0] + dx, p[1] + dy]).unwrap_or(*p);
        }
    }

    /// The shape as lines and filled triangles.
    pub fn geometry(&self) -> Geometry {
        let w = if self.width.is_finite() { self.width.clamp(MIN_WIDTH, MAX_WIDTH) } else { default_width() };
        let mut g = Geometry { line_width: w, ..Default::default() };
        match &self.kind {
            ShapeKind::Stroke { points } => g.lines.push(points.clone()),
            ShapeKind::Arrow { from, to } => {
                let (tri, stop) = head(*to, unit(sub(*to, *from)), w);
                g.lines.push(vec![*from, stop]);
                g.fills.push(tri);
            }
            ShapeKind::MotionArrow { from, via, to } => {
                let mut line: Vec<[f32; 2]> = (0..=CURVE_STEPS).map(|i| quad_point(*from, *via, *to, i as f32 / CURVE_STEPS as f32)).collect();
                // The head points along the curve's end tangent; a degenerate handle falls back to the chord.
                let tangent = if (to[0] - via[0]).hypot(to[1] - via[1]) > 1e-3 { sub(*to, *via) } else { sub(*to, *from) };
                let (tri, stop) = head(*to, unit(tangent), w);
                if let Some(last) = line.last_mut() {
                    *last = stop;
                }
                g.lines.push(line);
                g.fills.push(tri);
            }
            ShapeKind::CameraArrow { from, to } => {
                // A hollow block arrow: shaft half-width `s`, head half-width `hw`, head length `hl`.
                let dir = unit(sub(*to, *from));
                let n = [-dir[1], dir[0]];
                let s = (w * 2.5).max(6.0);
                let (hw, hl) = (s * 2.2, s * 3.0);
                let neck = sub(*to, mul(dir, hl));
                let outline = vec![
                    add(*from, mul(n, s)),
                    add(neck, mul(n, s)),
                    add(neck, mul(n, hw)),
                    *to,
                    sub(neck, mul(n, hw)),
                    sub(neck, mul(n, s)),
                    sub(*from, mul(n, s)),
                    add(*from, mul(n, s)),
                ];
                g.line_width = (w * 0.6).max(MIN_WIDTH);
                g.lines.push(outline);
            }
        }
        g
    }

    /// Bounding box `[x0, y0, x1, y1]` in canvas pixels, including line width.
    pub fn bounds(&self) -> [f32; 4] {
        let g = self.geometry();
        let mut b = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
        for p in g.lines.iter().flatten().chain(g.fills.iter().flatten()) {
            b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
        }
        if !b[0].is_finite() {
            return [0.0; 4];
        }
        let r = g.line_width / 2.0 + 1.0;
        [b[0] - r, b[1] - r, b[2] + r, b[3] + r]
    }

    /// Whether canvas point `(x, y)` is on the shape, within `tolerance` pixels.
    pub fn hit(&self, x: f32, y: f32, tolerance: f32) -> bool {
        let p = [x, y];
        let g = self.geometry();
        let reach = g.line_width / 2.0 + tolerance.max(0.0);
        g.fills.iter().any(|t| in_triangle(p, t))
            || g.lines.iter().any(|l| match l.as_slice() {
                [only] => (only[0] - x).hypot(only[1] - y) <= reach,
                _ => l.windows(2).any(|w| seg_dist(p, w[0], w[1]) <= reach),
            })
    }
}

/// Make a loaded panel's shapes safe: hostile values clamped, empty shapes dropped, count capped.
pub fn sanitize(shapes: &mut Vec<Shape>) {
    let clean: Vec<Shape> = shapes.iter().filter_map(Shape::sanitized).take(MAX_SHAPES).collect();
    *shapes = clean;
}

/// Draw `shapes` into `r`, whose top-left is canvas pixel `(ox, oy)` (a region of the canvas, or
/// the whole canvas at `0, 0`).
pub fn draw(r: &mut Raster, shapes: &[Shape], ox: f32, oy: f32) {
    let (rw, rh) = (r.width() as f32, r.height() as f32);
    for shape in shapes {
        // Skip shapes that miss this raster (drawing a small region while painting).
        let b = shape.bounds();
        if b[2] < ox || b[3] < oy || b[0] > ox + rw || b[1] > oy + rh {
            continue;
        }
        let g = shape.geometry();
        let brush = Brush {
            tool: Tool::Brush,
            radius: (g.line_width / 2.0).max(0.5),
            color: shape.color,
            hardness: 0.85,
            pressure_size: false,
            pressure_opacity: false,
            min_size: crate::raster::DEFAULT_MIN_SIZE,
        };
        for line in &g.lines {
            let mut prev: Option<StrokePoint> = None;
            for p in line {
                let q = StrokePoint::new(p[0] - ox, p[1] - oy, 1.0);
                match prev {
                    None => r.dab(q, &brush),
                    Some(a) => r.stroke(a, q, &brush),
                }
                prev = Some(q);
            }
        }
        for t in &g.fills {
            r.fill_triangle(t.map(|p| [p[0] - ox, p[1] - oy]), shape.color);
        }
    }
}

fn css(c: [u8; 4]) -> String {
    craft_svg::color::Color::new(c[0], c[1], c[2]).to_css()
}

/// The shapes as an SVG document the size of the canvas (`width × height` pixels). Every shape is a
/// group carrying its BoardCraft description (`data-boardcraft-shape`), so [`from_svg`] reads it
/// back exactly; other programs see ordinary paths.
pub fn to_svg(shapes: &[Shape], width: u32, height: u32) -> String {
    use craft_svg::write::{SvgWriter, num, path_data};
    use kurbo::BezPath;
    let pt = |p: [f32; 2]| kurbo::Point::new(p[0] as f64, p[1] as f64);
    let mut w = SvgWriter::new();
    w.root()
        .attr("width", width)
        .attr("height", height)
        .attr("viewBox", format_args!("0 0 {width} {height}"))
        .attr("fill", "none")
        .attr("stroke-linecap", "round")
        .attr("stroke-linejoin", "round")
        .open();
    for shape in shapes {
        let Some(shape) = shape.sanitized() else { continue };
        let json = serde_json::to_string(&shape).unwrap_or_default();
        let g = shape.geometry();
        let color = css(shape.color);
        let opacity = shape.color[3] as f64 / 255.0;
        w.line(1);
        w.tag("g").attr("data-boardcraft-shape", &json).attr("opacity", num(opacity, 3)).open();
        let mut body = BezPath::new();
        match &shape.kind {
            // The motion arrow's body stays a true curve in the file.
            ShapeKind::MotionArrow { from, via, .. } => {
                let stop = g.lines.first().and_then(|l| l.last()).copied().unwrap_or(*from);
                body.move_to(pt(*from));
                body.quad_to(pt(*via), pt(stop));
            }
            _ => {
                for line in &g.lines {
                    for (i, p) in line.iter().enumerate() {
                        if i == 0 { body.move_to(pt(*p)) } else { body.line_to(pt(*p)) }
                    }
                    if line.len() == 1 {
                        body.line_to(pt(line[0]));
                    }
                }
            }
        }
        w.line(2);
        w.tag("path").attr("d", path_data(&body)).attr("stroke", &color).attr("stroke-width", num(g.line_width as f64, 2)).empty();
        for t in &g.fills {
            let mut tri = BezPath::new();
            tri.move_to(pt(t[0]));
            tri.line_to(pt(t[1]));
            tri.line_to(pt(t[2]));
            tri.close_path();
            w.line(2);
            w.tag("path").attr("d", path_data(&tri)).attr("fill", &color).empty();
        }
        w.line(1);
        w.close();
    }
    w.line(0);
    w.close();
    w.finish()
}

fn color_attr(el: &craft_xml::El) -> Option<[u8; 4]> {
    let v = el.attr("stroke")?;
    let (c, a) = craft_svg::color::parse_color_alpha(v).ok()?;
    Some([c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8])
}

fn numbers(s: &str) -> Vec<f32> {
    s.split(|c: char| c == ',' || c.is_whitespace()).filter_map(|t| t.parse::<f32>().ok()).collect()
}

/// Shapes from an SVG document (plain or gzip-compressed). A BoardCraft SVG ([`to_svg`]) reads back
/// exactly; in any other SVG each `path`, `line` and `polyline` becomes a freehand stroke (curves
/// cut into short lines, transforms ignored).
pub fn from_svg(bytes: &[u8]) -> Result<Vec<Shape>> {
    let text = craft_svg::svgz::text(bytes, MAX_SVG_BYTES).map_err(|e| Error::Format(format!("SVG: {e}")))?;
    if craft_svg::guard::deeper_than(&text, 256) {
        return Err(Error::Format("SVG: elements are nested too deeply".into()));
    }
    let root = craft_xml::Options::local().parse(text.as_bytes()).map_err(|e| Error::Format(format!("SVG: {e}")))?;
    if root.local() != "svg" {
        return Err(Error::Format("not an SVG document".into()));
    }
    let mut out = Vec::new();
    let own: Vec<&craft_xml::El> = root.descendants().filter(|e| e.has_attr("data-boardcraft-shape")).collect();
    if !own.is_empty() {
        for el in own {
            if let Some(s) = el.attr("data-boardcraft-shape").and_then(|j| serde_json::from_str::<Shape>(j).ok()).and_then(|s| s.sanitized()) {
                out.push(s);
            }
            if out.len() >= MAX_SHAPES {
                break;
            }
        }
        return Ok(out);
    }
    for el in root.descendants() {
        if out.len() >= MAX_SHAPES {
            break;
        }
        let color = color_attr(el).unwrap_or(DEFAULT_COLOR);
        let width = el.attr("stroke-width").and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok()).unwrap_or(2.0);
        let mut lines: Vec<Vec<[f32; 2]>> = Vec::new();
        match el.local() {
            "line" => {
                let n = |a: &str| el.attr(a).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0);
                lines.push(vec![[n("x1"), n("y1")], [n("x2"), n("y2")]]);
            }
            "polyline" | "polygon" => {
                let v = numbers(el.attr("points").unwrap_or(""));
                let mut pts: Vec<[f32; 2]> = v.as_chunks::<2>().0.to_vec();
                if el.local() == "polygon"
                    && let Some(first) = pts.first().copied()
                {
                    pts.push(first);
                }
                lines.push(pts);
            }
            "path" => {
                let Ok(subpaths) = craft_svg::path_data::parse_path(el.attr("d").unwrap_or("")) else { continue };
                for sp in subpaths {
                    let mut pts = vec![[sp.start.x as f32, sp.start.y as f32]];
                    let mut cur = sp.start;
                    for seg in &sp.segs {
                        match seg {
                            craft_svg::path_data::Segment::Line(p) => {
                                pts.push([p.x as f32, p.y as f32]);
                                cur = *p;
                            }
                            craft_svg::path_data::Segment::Cubic(a, b, c) => {
                                let cubic = kurbo::CubicBez::new((cur.x, cur.y), (a.x, a.y), (b.x, b.y), (c.x, c.y));
                                for i in 1..=16 {
                                    let p = kurbo::ParamCurve::eval(&cubic, i as f64 / 16.0);
                                    pts.push([p.x as f32, p.y as f32]);
                                }
                                cur = *c;
                            }
                            craft_svg::path_data::Segment::Arc { end, .. } => {
                                pts.push([end.x as f32, end.y as f32]);
                                cur = *end;
                            }
                        }
                        if pts.len() >= MAX_POINTS {
                            break;
                        }
                    }
                    if sp.closed {
                        pts.push([sp.start.x as f32, sp.start.y as f32]);
                    }
                    lines.push(pts);
                }
            }
            _ => continue,
        }
        for points in lines {
            if let Some(s) = Shape::new(ShapeKind::Stroke { points }, color, width).sanitized() {
                out.push(s);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_kinds() -> Vec<Shape> {
        vec![
            Shape::new(ShapeKind::Stroke { points: vec![[4.0, 4.0], [20.0, 10.0], [40.0, 4.0]] }, [0, 0, 255, 255], 3.0),
            Shape::new(ShapeKind::Arrow { from: [10.0, 50.0], to: [60.0, 50.0] }, DEFAULT_COLOR, 3.0),
            Shape::new(ShapeKind::MotionArrow { from: [10.0, 90.0], via: [40.0, 60.0], to: [70.0, 90.0] }, [0, 160, 0, 200], 2.0),
            Shape::new(ShapeKind::CameraArrow { from: [10.0, 120.0], to: [80.0, 120.0] }, [20, 20, 20, 255], 3.0),
        ]
    }

    #[test]
    fn json_is_tagged_and_defaults() {
        let s = &all_kinds()[1];
        let json = serde_json::to_string(s).unwrap();
        assert!(json.contains(r#""type":"arrow""#), "{json}");
        let back: Shape = serde_json::from_str(r#"{"type":"camera_arrow","from":[0,0],"to":[5,5]}"#).unwrap();
        assert_eq!(back.color, DEFAULT_COLOR);
        assert_eq!(back.width, 4.0);
        assert!(serde_json::from_str::<Shape>(r#"{"type":"teapot"}"#).is_err());
    }

    #[test]
    fn shapes_draw_hit_and_move() {
        let shapes = all_kinds();
        let mut r = Raster::new(100, 140);
        draw(&mut r, &shapes, 0.0, 0.0);
        // The arrow's head is filled, its shaft painted.
        assert!(r.pixel(57, 50)[3] > 200);
        assert!(r.pixel(25, 50)[3] > 200);
        assert_eq!(r.pixel(95, 5)[3], 0);
        // A region drawn with an offset matches the same part of the whole.
        let mut part = Raster::new(30, 20);
        draw(&mut part, &shapes, 40.0, 40.0);
        assert_eq!(part, r.crop(40, 40, 30, 20));
        let mut arrow = shapes[1].clone();
        assert!(arrow.hit(30.0, 51.0, 2.0));
        assert!(!arrow.hit(30.0, 70.0, 2.0));
        arrow.translate(5.0, 20.0);
        assert!(arrow.hit(35.0, 71.0, 2.0));
        let b = shapes[3].bounds();
        assert!(b[0] < 10.0 && b[2] > 80.0, "{b:?}");
    }

    #[test]
    fn hostile_shapes_are_cleaned() {
        let mut shapes = vec![
            Shape::new(ShapeKind::Stroke { points: vec![[f32::NAN, 1.0]] }, DEFAULT_COLOR, 2.0),
            Shape::new(ShapeKind::Arrow { from: [f32::INFINITY, 0.0], to: [1.0, 1.0] }, DEFAULT_COLOR, 2.0),
            Shape::new(ShapeKind::Stroke { points: vec![[1e30, -1e30], [3.0, 3.0]] }, DEFAULT_COLOR, f32::NAN),
        ];
        sanitize(&mut shapes);
        assert_eq!(shapes.len(), 1);
        assert!(shapes[0].width.is_finite());
        let mut r = Raster::new(16, 16);
        draw(&mut r, &shapes, 0.0, 0.0);
        let mut moved = shapes[0].clone();
        moved.translate(f32::NAN, 1.0);
        assert_eq!(moved, shapes[0]);
    }

    #[test]
    fn svg_round_trip_and_foreign_svg() {
        let shapes = all_kinds();
        let svg = to_svg(&shapes, 100, 140);
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains(" Q"), "the motion arrow stays a curve: {svg}");
        assert_eq!(from_svg(svg.as_bytes()).unwrap(), shapes);
        let foreign = br##"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L10 10 C 20 20 30 20 40 10" stroke="#00ff00" stroke-width="3"/><line x1="1" y1="2" x2="3" y2="4"/><polyline points="0,0 5,5 9,0"/></svg>"##;
        let got = from_svg(foreign).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].color, [0, 255, 0, 255]);
        assert!(matches!(&got[0].kind, ShapeKind::Stroke { points } if points.len() == 18));
        assert!(from_svg(b"<html/>").is_err());
        assert!(from_svg(b"\x00\x01garbage").is_err());
    }

    #[test]
    fn svg_reader_survives_garbage() {
        let svg = to_svg(&all_kinds(), 100, 140).into_bytes();
        for cut in (0..svg.len()).step_by(37) {
            let _ = std::panic::catch_unwind(|| from_svg(&svg[..cut])).expect("truncation panicked");
        }
        let mut seed = 0x1234_5678_u32;
        for _ in 0..200 {
            let mut m = svg.clone();
            for _ in 0..4 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let i = seed as usize % m.len();
                m[i] ^= (seed >> 8) as u8 | 1;
            }
            let _ = std::panic::catch_unwind(|| from_svg(&m)).expect("bit flip panicked");
        }
    }
}
