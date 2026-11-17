//! Annotation tools: vector strokes, arrows, motion arrows and camera-move arrows drawn on the
//! canvas, and their editor in the inspector.
//!
//! Drawing is a drag on the canvas with one of the shape tools; the Select tool picks the topmost
//! shape under the pointer and drags it. Every edit goes through a board command, so it is one undo
//! step and scriptable. Nothing needs a drag: the inspector adds each kind of shape at the centre of
//! the canvas, lists the shapes as selectable rows, nudges, bends, recolours and deletes them; Delete
//! removes the selected shape.

use boardcraft_core::command::Command;
use boardcraft_core::{Id, Shape, ShapeKind};

use crate::app::{BoardApp, CanvasTool};

/// A shape being drawn on the canvas (canvas pixels).
#[derive(Clone, Debug, PartialEq)]
pub enum Draft {
    /// Freehand points of a vector stroke.
    Pen(Vec<[f32; 2]>),
    /// A two-point shape from `from` to `to`.
    Line { tool: CanvasTool, from: [f32; 2], to: [f32; 2] },
    /// Dragging the selected shape; `last` is where the pointer was.
    Move { last: [f32; 2] },
}

/// Pixels a nudge button moves a shape.
const NUDGE: f32 = 10.0;

/// The annotation tools, with their labels and shortcuts.
pub fn tools() -> [(CanvasTool, &'static str, &'static str); 5] {
    [
        (CanvasTool::Select, tl!("Select shape"), tl!("Select, move and delete annotations (Shift+S)")),
        (CanvasTool::Pen, tl!("Vector pen"), tl!("Freehand vector strokes that stay editable (P)")),
        (CanvasTool::Arrow, tl!("Arrow"), tl!("Straight arrow: drag from tail to head (A)")),
        (CanvasTool::MotionArrow, tl!("Motion arrow"), tl!("Curved arrow for movement in the shot (Shift+M)")),
        (CanvasTool::CameraArrow, tl!("Camera arrow"), tl!("Broad arrow for a camera move (Shift+A)")),
    ]
}

/// Translated name of a shape's kind.
pub fn kind_label(shape: &Shape) -> &'static str {
    match shape.kind {
        ShapeKind::Stroke { .. } => tl!("Vector stroke"),
        ShapeKind::Arrow { .. } => tl!("Arrow"),
        ShapeKind::MotionArrow { .. } => tl!("Motion arrow"),
        ShapeKind::CameraArrow { .. } => tl!("Camera arrow"),
    }
}

/// A two-point shape of the kind `tool` draws. A motion arrow bows to the left of its direction.
pub fn shape_for(tool: CanvasTool, from: [f32; 2], to: [f32; 2], color: [u8; 4], width: f32) -> Option<Shape> {
    let kind = match tool {
        CanvasTool::Arrow => ShapeKind::Arrow { from, to },
        CanvasTool::CameraArrow => ShapeKind::CameraArrow { from, to },
        CanvasTool::MotionArrow => {
            let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
            let via = [(from[0] + to[0]) / 2.0 + dy * 0.35, (from[1] + to[1]) / 2.0 - dx * 0.35];
            ShapeKind::MotionArrow { from, via, to }
        }
        _ => return None,
    };
    Some(Shape::new(kind, color, width))
}

/// The selected shape's index when it is on `panel`.
pub fn selected_on(app: &BoardApp, panel: Id) -> Option<usize> {
    app.shape_sel.filter(|(p, i)| *p == panel && app.project.panel(*p).is_some_and(|x| *i < x.annotations.len())).map(|(_, i)| i)
}

fn add(app: &mut BoardApp, panel: Id, shape: Shape) {
    let name = kind_label(&shape);
    if app.run(Command::AddShape { panel, shape }).is_some() {
        let n = app.project.panel(panel).map(|p| p.annotations.len()).unwrap_or(0);
        app.shape_sel = n.checked_sub(1).map(|i| (panel, i));
        app.status = tlf!("Added {shape}", shape = name);
    }
}

/// Delete the selected annotation.
pub fn delete_selected(app: &mut BoardApp) {
    let Some((panel, index)) = app.shape_sel else { return };
    if app.run(Command::DeleteShape { panel, index }).is_some() {
        app.shape_sel = None;
        app.status = tl!("Annotation deleted").into();
    }
}

/// Canvas input for the annotation tools. `to_canvas` maps a screen point to canvas pixels; `k` is
/// canvas pixels per screen point.
pub fn canvas_input(app: &mut BoardApp, resp: &egui::Response, panel: Id, to_canvas: &dyn Fn(egui::Pos2) -> [f32; 2], k: f32) {
    let tool = app.tool;
    let (color, width) = app.shape_style;
    if resp.drag_started()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let p = to_canvas(pos);
        app.draft = match tool {
            CanvasTool::Pen => Some(Draft::Pen(vec![p])),
            CanvasTool::Select => {
                let hit = app.project.panel(panel).and_then(|x| x.annotations.iter().rposition(|s| s.hit(p[0], p[1], 6.0 * k)));
                app.shape_sel = hit.map(|i| (panel, i));
                hit.map(|_| Draft::Move { last: p })
            }
            _ => Some(Draft::Line { tool, from: p, to: p }),
        };
    } else if resp.clicked()
        && tool == CanvasTool::Select
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let p = to_canvas(pos);
        let hit = app.project.panel(panel).and_then(|x| x.annotations.iter().rposition(|s| s.hit(p[0], p[1], 6.0 * k)));
        app.shape_sel = hit.map(|i| (panel, i));
    }
    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let p = to_canvas(pos);
        let mut moved = None;
        match app.draft.as_mut() {
            Some(Draft::Pen(points)) => {
                if points.last().is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) >= 2.0 * k.max(0.5)) && points.len() < boardcraft_core::annotate::MAX_POINTS {
                    points.push(p);
                }
            }
            Some(Draft::Line { to, .. }) => *to = p,
            Some(Draft::Move { last }) => {
                moved = Some((p[0] - last[0], p[1] - last[1]));
                *last = p;
            }
            None => {}
        }
        if let Some((dx, dy)) = moved
            && (dx != 0.0 || dy != 0.0)
            && let Some(index) = selected_on(app, panel)
        {
            app.run(Command::MoveShape { panel, index, dx, dy });
        }
    }
    if resp.drag_stopped() {
        match app.draft.take() {
            Some(Draft::Pen(points)) => add(app, panel, Shape::new(ShapeKind::Stroke { points }, color, width)),
            Some(Draft::Line { tool, from, to }) => {
                // A click without a drag draws nothing.
                if (to[0] - from[0]).hypot(to[1] - from[1]) >= 4.0
                    && let Some(shape) = shape_for(tool, from, to, color, width)
                {
                    add(app, panel, shape);
                }
            }
            _ => {}
        }
    }
}

/// Draw the shape in progress and the selection's outline. `to_screen` maps canvas pixels to the
/// screen; `scale` is screen points per canvas pixel.
pub fn overlay(app: &BoardApp, painter: &egui::Painter, panel: Id, to_screen: &dyn Fn([f32; 2]) -> egui::Pos2, scale: f32) {
    let (color, width) = app.shape_style;
    let preview = match &app.draft {
        Some(Draft::Pen(points)) => Some(Shape::new(ShapeKind::Stroke { points: points.clone() }, color, width)),
        Some(Draft::Line { tool, from, to }) => shape_for(*tool, *from, *to, color, width),
        _ => None,
    };
    if let Some(shape) = preview {
        let g = shape.geometry();
        let c = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
        let stroke = egui::Stroke::new((g.line_width * scale).max(1.0), c);
        for line in &g.lines {
            painter.add(egui::Shape::line(line.iter().map(|p| to_screen(*p)).collect(), stroke));
        }
        for t in &g.fills {
            painter.add(egui::Shape::convex_polygon(t.iter().map(|p| to_screen(*p)).collect(), c, egui::Stroke::NONE));
        }
    }
    if let Some(i) = selected_on(app, panel)
        && let Some(shape) = app.project.panel(panel).and_then(|p| p.annotations.get(i))
    {
        // The bounds, a little outside the shape, through the (possibly rotated) view.
        let b = shape.bounds();
        let pad = 3.0 / scale.max(1e-3);
        let (x0, y0, x1, y1) = (b[0] - pad, b[1] - pad, b[2] + pad, b[3] + pad);
        let corners = [to_screen([x0, y0]), to_screen([x1, y0]), to_screen([x1, y1]), to_screen([x0, y1])];
        // Two-tone dashes read on paper and on dark ink alike.
        painter.add(egui::Shape::closed_line(corners.to_vec(), egui::Stroke::new(3.0, egui::Color32::WHITE)));
        painter.add(egui::Shape::dashed_line(
            &[corners[0], corners[1], corners[2], corners[3], corners[0]],
            egui::Stroke::new(1.5, egui::Color32::from_rgb(20, 90, 200)),
            6.0,
            4.0,
        ));
    }
}

/// Toolbar controls for the annotation tools: colour and width of new shapes.
pub fn toolbar(app: &mut BoardApp, ui: &mut egui::Ui) {
    let (mut color, mut width) = app.shape_style;
    let mut c = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    let l = ui.label(tl!("Annotation colour"));
    if ui.color_edit_button_srgba(&mut c).labelled_by(l.id).changed() {
        color = c.to_srgba_unmultiplied();
    }
    let l = ui.label(tl!("Line width"));
    ui.add(egui::DragValue::new(&mut width).range(boardcraft_core::annotate::MIN_WIDTH..=boardcraft_core::annotate::MAX_WIDTH).speed(0.2).suffix(" px"))
        .labelled_by(l.id);
    app.shape_style = (color, width);
    if app.tool == CanvasTool::Select {
        ui.label(tl!("Click a shape to select it, drag to move it, Delete to remove it."));
    }
}

/// Delete or Backspace removes the selected annotation (nothing focused, nothing being typed).
/// The tool keys are in the keymap catalog (`crate::keys`).
pub fn shortcuts(app: &mut BoardApp, ctx: &egui::Context) {
    use egui::{Key, Modifiers};
    if app.shape_sel.is_none() || app.view != crate::app::View::Panel || ctx.egui_wants_keyboard_input() || ctx.memory(|m| m.focused().is_some()) {
        return;
    }
    let del = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace));
    if del {
        delete_selected(app);
    }
}

/// The inspector's Annotations section for `panel`.
pub fn inspector(app: &mut BoardApp, ui: &mut egui::Ui, panel: Id) {
    ui.strong(tl!("Annotations"));
    let (w, h) = (app.project.settings.width as f32, app.project.settings.height as f32);
    let (color, width) = app.shape_style;
    ui.horizontal_wrapped(|ui| {
        // Adding without a drag: a shape across the middle of the canvas, ready to nudge and bend.
        let mid = [w * 0.35, h * 0.5];
        let end = [w * 0.65, h * 0.5];
        for (label, tool) in
            [(tl!("Add Arrow"), CanvasTool::Arrow), (tl!("Add Motion Arrow"), CanvasTool::MotionArrow), (tl!("Add Camera Arrow"), CanvasTool::CameraArrow)]
        {
            if ui.button(label).clicked()
                && let Some(shape) = shape_for(tool, mid, end, color, width)
            {
                add(app, panel, shape);
            }
        }
    });
    let shapes: Vec<Shape> = app.project.panel(panel).map(|p| p.annotations.clone()).unwrap_or_default();
    if shapes.is_empty() {
        ui.label(tl!("None yet. Draw with the annotation tools in the Panel view, or add one above."));
    }
    let selected = selected_on(app, panel);
    for (i, shape) in shapes.iter().enumerate() {
        let name = tlf!("{shape} {n}", shape = kind_label(shape), n = i + 1);
        if ui.selectable_label(selected == Some(i), name).clicked() {
            app.shape_sel = Some((panel, i));
        }
    }
    if let Some(index) = selected
        && let Some(shape) = shapes.get(index)
    {
        ui.horizontal_wrapped(|ui| {
            let mut c = egui::Color32::from_rgba_unmultiplied(shape.color[0], shape.color[1], shape.color[2], shape.color[3]);
            let l = ui.label(tl!("Colour"));
            let recolor = ui.color_edit_button_srgba(&mut c).labelled_by(l.id).changed();
            let mut sw = shape.width;
            let l = ui.label(tl!("Width"));
            let rewidth = ui
                .add(egui::DragValue::new(&mut sw).range(boardcraft_core::annotate::MIN_WIDTH..=boardcraft_core::annotate::MAX_WIDTH).speed(0.2))
                .labelled_by(l.id)
                .changed();
            if recolor || rewidth {
                app.run(Command::StyleShape { panel, index, color: c.to_srgba_unmultiplied(), width: Some(sw) });
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (label, dx, dy) in
                [(tl!("Nudge Left"), -NUDGE, 0.0), (tl!("Nudge Right"), NUDGE, 0.0), (tl!("Nudge Up"), 0.0, -NUDGE), (tl!("Nudge Down"), 0.0, NUDGE)]
            {
                if ui.button(label).clicked() {
                    app.run(Command::MoveShape { panel, index, dx, dy });
                }
            }
        });
        if let ShapeKind::MotionArrow { from, via, to } = shape.kind {
            // Bend: how far the curve's handle sits off the straight line, in canvas pixels.
            let d = [to[0] - from[0], to[1] - from[1]];
            let len = d[0].hypot(d[1]).max(1e-3);
            let n = [d[1] / len, -d[0] / len];
            let mid = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
            let mut bend = (via[0] - mid[0]) * n[0] + (via[1] - mid[1]) * n[1];
            ui.horizontal(|ui| {
                let l = ui.label(tl!("Bend"));
                if ui.add(egui::DragValue::new(&mut bend).range(-4.0 * len..=4.0 * len).speed(1.0).suffix(" px")).labelled_by(l.id).changed() {
                    let mut s = shape.clone();
                    s.kind = ShapeKind::MotionArrow { from, via: [mid[0] + n[0] * bend, mid[1] + n[1] * bend], to };
                    replace(app, panel, index, s);
                }
                if ui.button(tl!("Flip Bend")).clicked() {
                    let mut s = shape.clone();
                    s.kind = ShapeKind::MotionArrow { from, via: [mid[0] - n[0] * bend, mid[1] - n[1] * bend], to };
                    replace(app, panel, index, s);
                }
            });
        }
        if ui.button(tl!("Delete Annotation")).on_hover_text(tl!("Delete the selected annotation (Delete)")).clicked() {
            delete_selected(app);
        }
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button(tl!("Export Annotations (SVG)…")).clicked() {
            export_svg(app, panel);
        }
        if ui.button(tl!("Import Annotations (SVG)…")).clicked() {
            import_svg(app, panel);
        }
    });
}

/// Replace shape `index` (delete and re-add in place keeps one undo step per press).
fn replace(app: &mut BoardApp, panel: Id, index: usize, shape: Shape) {
    if let Some(p) = app.project.panel_mut(panel)
        && let Some(slot) = p.annotations.get_mut(index)
        && let Some(clean) = shape.sanitized()
    {
        *slot = clean;
        p.touch();
        app.edited();
    }
}

fn export_svg(app: &mut BoardApp, panel: Id) {
    let Some(shapes) = app.project.panel(panel).map(|p| p.annotations.clone()) else { return };
    let name = format!("{} {}.svg", app.project.name, app.project.panel_label(panel));
    let Some(mut path) = rfd::FileDialog::new().add_filter("SVG", &["svg"]).set_file_name(name).save_file() else { return };
    if path.extension().is_none() {
        path.set_extension("svg");
    }
    let svg = boardcraft_core::annotate::to_svg(&shapes, app.project.settings.width, app.project.settings.height);
    match boardcraft_core::format::write_atomic(&path, svg.as_bytes()) {
        Ok(()) => app.status = tlf!("Wrote {n} annotations to {file}", n = shapes.len(), file = path.display()),
        Err(e) => app.status = tlf!("Export failed: {error}", error = e),
    }
}

fn import_svg(app: &mut BoardApp, panel: Id) {
    let Some(path) = rfd::FileDialog::new().add_filter("SVG", &["svg", "svgz"]).pick_file() else { return };
    match std::fs::read(&path) {
        Ok(bytes) => {
            // Decoded here so compressed (SVGZ) files work; the command takes SVG text.
            let svg = match boardcraft_core::annotate::from_svg(&bytes) {
                Ok(shapes) => boardcraft_core::annotate::to_svg(&shapes, app.project.settings.width, app.project.settings.height),
                Err(e) => {
                    app.status = tlf!("Could not import {file}: {error}", file = path.display(), error = e);
                    return;
                }
            };
            let cmd = Command::ImportShapesSvg { panel, svg, replace: false };
            if let Some(out) = app.run(cmd) {
                app.status = tlf!("{summary} from {file}", summary = out.message, file = path.display());
            }
        }
        Err(e) => app.status = tlf!("Could not read {file}: {error}", file = path.display(), error = e),
    }
}
