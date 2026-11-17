//! Lasso and rectangle selections on the active layer: move, scale and rotate the selected pixels
//! (by dragging, arrow keys or typed values), apply with Enter, cancel with Esc, and cut, copy,
//! paste or delete them.
//!
//! The project is not touched while a selection floats: the canvas shows a preview texture, and
//! applying it is one edit (one undo step) through `Command::TransformSelection`.

use boardcraft_core::command::Command;
use boardcraft_core::select::{self, Mask, Piece, SCALE_RANGE, Transform};
use boardcraft_core::{Id, Raster};

use super::view::ViewXf;
use crate::app::{BoardApp, CanvasTool};

/// Most lasso points kept from one drag.
const MAX_LASSO_POINTS: usize = 20_000;
/// Hit radius of the transform handles, in screen points.
const HANDLE: f32 = 9.0;
/// Distance of the rotation handle above the selection, in screen points.
const ROTATE_OFFSET: f32 = 26.0;

/// A selection on one layer.
pub struct Selection {
    pub panel: Id,
    pub layer: Id,
    /// The outline in canvas pixels (lasso points or rectangle corners), before the transform.
    pub outline: Vec<[f32; 2]>,
    mask: Mask,
    /// The layer's pixels when the selection was made (shared, not copied).
    base: Raster,
    /// `base` with the selected pixels removed; made on first use.
    hole: Option<Raster>,
    pub piece: Piece,
    pub transform: Transform,
    /// Pasted pixels are not on the layer yet; applying draws them.
    pub pasted: bool,
    serial: u64,
}

impl Selection {
    fn floating(&self) -> bool {
        self.pasted || !self.transform.is_identity()
    }

    /// The pivot after the move.
    fn pivot(&self) -> [f32; 2] {
        let p = self.piece.pivot();
        let t = self.transform.sanitized();
        [p[0] + t.dx, p[1] + t.dy]
    }

    /// The layer as it looks with the transform applied.
    fn result(&mut self) -> Raster {
        let hole = match &self.hole {
            Some(h) => h.clone(),
            None => {
                let mut h = self.base.clone();
                if !self.pasted {
                    select::clear_masked(&mut h, &self.mask);
                }
                self.hole = Some(h.clone());
                h
            }
        };
        let mut out = hole;
        select::composite_transformed(&mut out, &self.piece, &self.transform);
        out
    }
}

/// A pointer gesture of the selection tools.
pub enum Drag {
    Lasso(Vec<[f32; 2]>),
    Rect([f32; 2], [f32; 2]),
    Move([f32; 2]),
    Scale { start: f32, from: Transform },
    Rotate { start: f32, from: Transform },
}

/// Selection state kept by the app.
#[derive(Default)]
pub struct State {
    pub selection: Option<Selection>,
    pub drag: Option<Drag>,
    pub clipboard: Option<Piece>,
    serial: u64,
    preview: Option<(egui::TextureHandle, u64, Transform)>,
}

pub fn is_select_tool(tool: CanvasTool) -> bool {
    matches!(tool, CanvasTool::Lasso | CanvasTool::RectSelect)
}

/// The active layer of the selected panel, if it is visible.
fn target(app: &BoardApp) -> Option<(Id, Id)> {
    let panel = app.selected?;
    let layer = app.active_layer?;
    app.project.panel(panel)?.layer(layer).filter(|l| l.visible && !l.pixels.is_empty())?;
    Some((panel, layer))
}

fn still_valid(app: &BoardApp, sel: &Selection) -> bool {
    app.project.panel(sel.panel).and_then(|p| p.layer(sel.layer)).is_some_and(|l| l.pixels.shares_pixels(&sel.base))
}

/// Per frame: drop a selection whose layer changed under it (undo, another edit), and apply one
/// left behind when the tool, panel or layer changes.
pub fn tick(app: &mut BoardApp) {
    let Some(sel) = &app.tools.sel.selection else { return };
    if !still_valid(app, sel) {
        app.tools.sel.selection = None;
        app.tools.sel.drag = None;
        return;
    }
    let left = app.view != crate::app::View::Panel
        || !is_select_tool(app.tool)
        || app.selected != Some(sel.panel)
        || app.active_layer != Some(sel.layer)
        || app.playing;
    if left {
        apply(app);
    }
}

fn new_selection(app: &mut BoardApp, mask: Mask, outline: Vec<[f32; 2]>) {
    let Some((panel, layer)) = target(app) else {
        app.status = tl!("Select a visible layer first").into();
        return;
    };
    let Some(base) = app.project.panel(panel).and_then(|p| p.layer(layer)).map(|l| l.pixels.clone()) else { return };
    let Some(piece) = select::copy_masked(&base, &mask) else {
        app.status = tl!("Nothing selected").into();
        return;
    };
    app.tools.sel.serial = app.tools.sel.serial.wrapping_add(1);
    let (w, h) = (piece.pixels.width(), piece.pixels.height());
    app.tools.sel.selection =
        Some(Selection { panel, layer, outline, mask, base, hole: None, piece, transform: Transform::default(), pasted: false, serial: app.tools.sel.serial });
    app.status = tlf!("Selected {w} × {h} px. Drag, use the arrow keys or type values to move it; Enter applies, Esc cancels.", w = w, h = h);
}

/// Select the whole active layer (Ctrl+A).
pub fn select_all(app: &mut BoardApp) {
    apply(app);
    let s = app.project.settings;
    let (w, h) = (s.width as f32, s.height as f32);
    if !is_select_tool(app.tool) {
        app.tool = CanvasTool::RectSelect;
    }
    new_selection(app, Mask::all(s.width, s.height), vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]);
}

/// Apply the selection's transform to the layer (one undo step) and deselect.
pub fn apply(app: &mut BoardApp) {
    app.tools.sel.drag = None;
    let Some(mut sel) = app.tools.sel.selection.take() else { return };
    if !sel.floating() || !still_valid(app, &sel) {
        return;
    }
    if sel.pasted {
        let pixels = sel.result();
        if let Some(p) = app.project.panel_mut(sel.panel) {
            if let Some(l) = p.layer_mut(sel.layer) {
                l.pixels = pixels;
            }
            p.touch();
        }
        app.edited();
    } else {
        let cmd = Command::TransformSelection { panel: sel.panel, layer: sel.layer, polygon: sel.outline.clone(), transform: sel.transform.sanitized() };
        app.run(cmd);
    }
    app.status = tl!("Selection applied").into();
}

/// Drop the selection without changing the layer (Esc).
pub fn cancel(app: &mut BoardApp) {
    app.tools.sel.drag = None;
    if app.tools.sel.selection.take().is_some() {
        app.status = tl!("Selection cancelled").into();
    }
}

/// Reset a moved or rotated selection to where it started.
pub fn reset_transform(app: &mut BoardApp) -> bool {
    match app.tools.sel.selection.as_mut() {
        Some(sel) if !sel.transform.is_identity() => {
            sel.transform = Transform::default();
            app.status = tl!("Transform reset").into();
            true
        }
        _ => false,
    }
}

/// Clear the selected pixels (Delete).
pub fn delete(app: &mut BoardApp) {
    app.tools.sel.drag = None;
    let Some(sel) = app.tools.sel.selection.take() else { return };
    if sel.pasted || !still_valid(app, &sel) {
        app.status = tl!("Selection cancelled").into();
        return;
    }
    if app.run(Command::ClearSelection { panel: sel.panel, layer: sel.layer, polygon: sel.outline.clone() }).is_some() {
        app.status = tl!("Selection deleted").into();
    }
}

/// Copy the selection (as transformed) to BoardCraft's clipboard and the system clipboard.
pub fn copy(app: &mut BoardApp, ctx: &egui::Context) -> bool {
    let Some(sel) = &app.tools.sel.selection else { return false };
    let piece = sel.piece.transformed(&sel.transform);
    let r = &piece.pixels;
    ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied([r.width() as usize, r.height() as usize], r.data()));
    app.tools.sel.clipboard = Some(piece);
    app.status = tl!("Copied the selection").into();
    true
}

pub fn cut(app: &mut BoardApp, ctx: &egui::Context) {
    if copy(app, ctx) {
        delete(app);
        app.status = tl!("Cut the selection").into();
    }
}

/// Paste BoardCraft's clipboard as a floating selection on the active layer.
pub fn paste(app: &mut BoardApp) {
    let Some(piece) = app.tools.sel.clipboard.clone() else {
        app.status = tl!("Nothing to paste").into();
        return;
    };
    paste_piece(app, piece);
}

/// Paste an image from the system clipboard, centred on the canvas.
pub fn paste_image(app: &mut BoardApp, img: &egui::ColorImage) {
    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    let Some(pixels) = Raster::from_rgba(img.size[0] as u32, img.size[1] as u32, rgba) else {
        app.status = tl!("The clipboard image is too large").into();
        return;
    };
    let s = app.project.settings;
    let x = (s.width as i64 - pixels.width() as i64) / 2;
    let y = (s.height as i64 - pixels.height() as i64) / 2;
    paste_piece(app, Piece { pixels, x, y });
}

fn paste_piece(app: &mut BoardApp, piece: Piece) {
    apply(app);
    let Some((panel, layer)) = target(app) else {
        app.status = tl!("Select a visible layer first").into();
        return;
    };
    let Some(base) = app.project.panel(panel).and_then(|p| p.layer(layer)).map(|l| l.pixels.clone()) else { return };
    if !is_select_tool(app.tool) {
        app.tool = CanvasTool::RectSelect;
    }
    let (x0, y0) = (piece.x as f32, piece.y as f32);
    let (x1, y1) = (x0 + piece.pixels.width() as f32, y0 + piece.pixels.height() as f32);
    app.tools.sel.serial = app.tools.sel.serial.wrapping_add(1);
    app.tools.sel.selection = Some(Selection {
        panel,
        layer,
        outline: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
        mask: Mask::empty(1, 1),
        base,
        hole: None,
        piece,
        transform: Transform::default(),
        pasted: true,
        serial: app.tools.sel.serial,
    });
    app.status = tl!("Pasted. Drag, use the arrow keys or type values to place it; Enter applies, Esc cancels.").into();
}

/// Move the selection by whole pixels (arrow keys).
pub fn nudge(app: &mut BoardApp, dx: f32, dy: f32) {
    if let Some(sel) = app.tools.sel.selection.as_mut() {
        sel.transform.dx += dx;
        sel.transform.dy += dy;
        sel.transform = sel.transform.sanitized();
    }
}

/// The texture showing the panel with the floating selection, while one floats on it.
pub fn preview(app: &mut BoardApp, ctx: &egui::Context, panel: Id) -> Option<egui::TextureId> {
    let valid = app.tools.sel.selection.as_ref().is_some_and(|s| s.panel == panel && s.floating() && still_valid(app, s));
    if !valid {
        return None;
    }
    let sel = app.tools.sel.selection.as_mut()?;
    if let Some((tex, serial, t)) = &app.tools.sel.preview
        && *serial == sel.serial
        && *t == sel.transform
    {
        return Some(tex.id());
    }
    let pixels = sel.result();
    let (layer, serial, transform) = (sel.layer, sel.serial, sel.transform);
    let mut p = app.project.panel(panel)?.clone();
    if let Some(l) = p.layer_mut(layer) {
        l.pixels = pixels;
    }
    let img = p.composite(&app.project.settings);
    let img = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.data());
    match &mut app.tools.sel.preview {
        Some((tex, s, t)) => {
            tex.set(img, egui::TextureOptions::LINEAR);
            *s = serial;
            *t = transform;
        }
        None => app.tools.sel.preview = Some((ctx.load_texture("selection-preview", img, egui::TextureOptions::LINEAR), serial, transform)),
    }
    app.tools.sel.preview.as_ref().map(|(t, _, _)| t.id())
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn angle_deg(from: [f32; 2], to: [f32; 2]) -> f32 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
}

fn inside(p: [f32; 2], poly: &[[f32; 2]]) -> bool {
    let mut c = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
    }
    c
}

/// Screen positions of the corner handles and the rotation handle.
fn handles(sel: &Selection, xf: &ViewXf) -> ([egui::Pos2; 4], egui::Pos2) {
    let c = sel.piece.corners(&sel.transform);
    let s = c.map(|p| xf.to_screen(p));
    let top = s[0].lerp(s[1], 0.5);
    let centre = xf.to_screen(sel.pivot());
    let dir = (top - centre).normalized();
    let dir = if dir.x.is_finite() && dir.y.is_finite() && dir != egui::Vec2::ZERO { dir } else { egui::vec2(0.0, -1.0) };
    (s, top + dir * ROTATE_OFFSET)
}

/// Pointer input of the Lasso and Rectangle select tools.
pub fn canvas_input(app: &mut BoardApp, resp: &egui::Response, xf: &ViewXf) {
    let Some(pos) = resp.interact_pointer_pos() else {
        if resp.drag_stopped() {
            finish_drag(app);
        }
        return;
    };
    let c = xf.to_canvas(pos);
    if resp.drag_started() {
        let mut drag = None;
        if let Some(sel) = &app.tools.sel.selection {
            let (corners, rot) = handles(sel, xf);
            let pivot = sel.pivot();
            if pos.distance(rot) <= HANDLE {
                drag = Some(Drag::Rotate { start: angle_deg(pivot, c), from: sel.transform });
            } else if corners.iter().any(|k| pos.distance(*k) <= HANDLE) {
                drag = Some(Drag::Scale { start: dist(pivot, c).max(1e-3), from: sel.transform });
            } else if inside(c, &sel.piece.corners(&sel.transform)) {
                drag = Some(Drag::Move(c));
            }
        }
        if drag.is_none() {
            apply(app);
            drag = Some(if app.tool == CanvasTool::Lasso { Drag::Lasso(vec![c]) } else { Drag::Rect(c, c) });
        }
        app.tools.sel.drag = drag;
    } else if resp.dragged() {
        let pivot = app.tools.sel.selection.as_ref().map(|s| s.pivot()).unwrap_or(c);
        let mut moved: Option<Transform> = None;
        match app.tools.sel.drag.as_mut() {
            Some(Drag::Lasso(pts)) => {
                if pts.len() < MAX_LASSO_POINTS && pts.last().is_none_or(|l| dist(*l, c) >= 1.0) {
                    pts.push(c);
                }
            }
            Some(Drag::Rect(_, b)) => *b = c,
            Some(Drag::Move(last)) => {
                let t = app.tools.sel.selection.as_ref().map(|s| s.transform).unwrap_or_default();
                moved = Some(Transform { dx: t.dx + c[0] - last[0], dy: t.dy + c[1] - last[1], ..t });
                *last = c;
            }
            Some(Drag::Scale { start, from }) => {
                let k = dist(pivot, c) / *start;
                let s = |v: f32| (v * k).clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
                moved = Some(Transform { scale_x: s(from.scale_x), scale_y: s(from.scale_y), ..*from });
            }
            Some(Drag::Rotate { start, from }) => {
                moved = Some(Transform { angle: from.angle + angle_deg(pivot, c) - *start, ..*from }.sanitized());
            }
            None => {}
        }
        if let (Some(t), Some(sel)) = (moved, app.tools.sel.selection.as_mut()) {
            sel.transform = t.sanitized();
        }
    }
    if resp.drag_stopped() {
        finish_drag(app);
    }
}

fn finish_drag(app: &mut BoardApp) {
    let s = app.project.settings;
    match app.tools.sel.drag.take() {
        Some(Drag::Lasso(pts)) if pts.len() >= 3 => {
            let mask = Mask::polygon(s.width, s.height, &pts);
            new_selection(app, mask, pts);
        }
        Some(Drag::Rect(a, b)) if (a[0] - b[0]).abs() >= 1.0 && (a[1] - b[1]).abs() >= 1.0 => {
            let outline = vec![[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]];
            new_selection(app, Mask::rect(s.width, s.height, a, b), outline);
        }
        _ => {}
    }
}

/// Draw the selection outline, its handles and a lasso or rectangle being dragged.
pub fn draw_overlay(app: &BoardApp, painter: &egui::Painter, xf: &ViewXf, panel: Id) {
    let light = egui::Stroke::new(3.0, egui::Color32::WHITE);
    let dark = egui::Stroke::new(1.5, egui::Color32::from_rgb(20, 20, 30));
    let dashed = |pts: Vec<egui::Pos2>, closed: bool| {
        let mut pts = pts;
        if closed && let Some(f) = pts.first().copied() {
            pts.push(f);
        }
        painter.add(egui::Shape::line(pts.clone(), light));
        painter.extend(egui::Shape::dashed_line(&pts, dark, 6.0, 4.0));
    };
    if let Some(sel) = app.tools.sel.selection.as_ref().filter(|s| s.panel == panel) {
        let pivot = sel.piece.pivot();
        let outline: Vec<egui::Pos2> = sel.outline.iter().map(|p| xf.to_screen(sel.transform.apply(pivot, *p))).collect();
        dashed(outline, true);
        let (corners, rot) = handles(sel, xf);
        let accent = egui::Color32::from_rgb(40, 110, 230);
        for k in corners {
            painter.rect_filled(egui::Rect::from_center_size(k, egui::vec2(9.0, 9.0)), 1.0, egui::Color32::WHITE);
            painter.rect_stroke(egui::Rect::from_center_size(k, egui::vec2(9.0, 9.0)), 1.0, egui::Stroke::new(1.5, accent), egui::StrokeKind::Middle);
        }
        let top = corners[0].lerp(corners[1], 0.5);
        painter.line_segment([top, rot], egui::Stroke::new(1.5, accent));
        painter.circle(rot, 5.5, egui::Color32::WHITE, egui::Stroke::new(1.5, accent));
    }
    match &app.tools.sel.drag {
        Some(Drag::Lasso(pts)) => dashed(pts.iter().map(|p| xf.to_screen(*p)).collect(), false),
        Some(Drag::Rect(a, b)) => dashed(vec![xf.to_screen(*a), xf.to_screen([b[0], a[1]]), xf.to_screen(*b), xf.to_screen([a[0], b[1]])], true),
        _ => {}
    }
}

/// What the canvas node says about the selection.
pub fn describe(app: &BoardApp) -> Option<String> {
    let sel = app.tools.sel.selection.as_ref()?;
    let t = sel.transform;
    Some(tlf!(
        "selection {w} × {h} px, moved {dx}, {dy}, scaled {sx}% × {sy}%, rotated {angle}°",
        w = sel.piece.pixels.width(),
        h = sel.piece.pixels.height(),
        dx = t.dx.round(),
        dy = t.dy.round(),
        sx = (t.scale_x * 100.0).round(),
        sy = (t.scale_y * 100.0).round(),
        angle = t.angle.round()
    ))
}

enum Action {
    Apply,
    Cancel,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
}

/// Toolbar of the selection tools: typed transform values and the selection commands.
pub fn toolbar(app: &mut BoardApp, ui: &mut egui::Ui) {
    let mut action = None;
    match app.tools.sel.selection.as_mut() {
        Some(sel) => {
            let t = &mut sel.transform;
            let l = ui.label(tl!("X"));
            ui.add(egui::DragValue::new(&mut t.dx).speed(1.0).suffix(" px")).labelled_by(l.id);
            let l = ui.label(tl!("Y"));
            ui.add(egui::DragValue::new(&mut t.dy).speed(1.0).suffix(" px")).labelled_by(l.id);
            let mut w = t.scale_x * 100.0;
            let l = ui.label(tl!("Width"));
            if ui.add(egui::DragValue::new(&mut w).speed(1.0).range(2.0..=1600.0).suffix(" %")).labelled_by(l.id).changed() {
                t.scale_x = w / 100.0;
            }
            let mut h = t.scale_y * 100.0;
            let l = ui.label(tl!("Height"));
            if ui.add(egui::DragValue::new(&mut h).speed(1.0).range(2.0..=1600.0).suffix(" %")).labelled_by(l.id).changed() {
                t.scale_y = h / 100.0;
            }
            let l = ui.label(tl!("Angle"));
            ui.add(egui::DragValue::new(&mut t.angle).speed(1.0).range(-360.0..=360.0).suffix(" °")).labelled_by(l.id);
            *t = t.sanitized();
            if ui.button(tl!("Apply")).on_hover_text(tl!("Apply the transform (Enter)")).clicked() {
                action = Some(Action::Apply);
            }
            if ui.button(tl!("Cancel")).on_hover_text(tl!("Drop the selection (Esc)")).clicked() {
                action = Some(Action::Cancel);
            }
            if ui.button(tl!("Cut")).clicked() {
                action = Some(Action::Cut);
            }
            if ui.button(tl!("Copy")).clicked() {
                action = Some(Action::Copy);
            }
            if ui.button(tl!("Delete")).clicked() {
                action = Some(Action::Delete);
            }
        }
        None => {
            ui.label(tl!("Drag on the canvas to select."));
            if ui.button(tl!("Select All")).on_hover_text(tl!("Select the whole layer (Ctrl+A)")).clicked() {
                action = Some(Action::SelectAll);
            }
        }
    }
    if ui.add_enabled(app.tools.sel.clipboard.is_some(), egui::Button::new(tl!("Paste"))).clicked() {
        action = Some(Action::Paste);
    }
    let ctx = ui.ctx().clone();
    run(app, &ctx, action);
}

fn run(app: &mut BoardApp, ctx: &egui::Context, action: Option<Action>) {
    match action {
        Some(Action::Apply) => apply(app),
        Some(Action::Cancel) => cancel(app),
        Some(Action::Cut) => cut(app, ctx),
        Some(Action::Copy) => {
            copy(app, ctx);
        }
        Some(Action::Paste) => paste(app),
        Some(Action::Delete) => delete(app),
        Some(Action::SelectAll) => select_all(app),
        None => {}
    }
}

/// The selection entries of the Edit menu.
pub fn edit_menu(app: &mut BoardApp, ui: &mut egui::Ui) {
    let has = app.tools.sel.selection.is_some();
    let mut action = None;
    if ui.add(egui::Button::new(tl!("Select All")).shortcut_text("Ctrl+A")).clicked() {
        action = Some(Action::SelectAll);
    }
    if ui.add_enabled(has, egui::Button::new(tl!("Apply Transform")).shortcut_text("Enter")).clicked() {
        action = Some(Action::Apply);
    }
    if ui.add_enabled(has, egui::Button::new(tl!("Cancel Selection")).shortcut_text("Esc")).clicked() {
        action = Some(Action::Cancel);
    }
    if ui.add_enabled(has, egui::Button::new(tl!("Cut Selection")).shortcut_text("Ctrl+X")).clicked() {
        action = Some(Action::Cut);
    }
    if ui.add_enabled(has, egui::Button::new(tl!("Copy Selection")).shortcut_text("Ctrl+C")).clicked() {
        action = Some(Action::Copy);
    }
    if ui.add_enabled(app.tools.sel.clipboard.is_some(), egui::Button::new(tl!("Paste Selection")).shortcut_text("Ctrl+V")).clicked() {
        action = Some(Action::Paste);
    }
    if ui.add_enabled(has, egui::Button::new(tl!("Delete Selection")).shortcut_text("Delete")).clicked() {
        action = Some(Action::Delete);
    }
    if action.is_some() {
        ui.close();
        // The Edit menu works from the Board view too: selections live on the Panel view's canvas.
        app.view = crate::app::View::Panel;
    }
    let ctx = ui.ctx().clone();
    run(app, &ctx, action);
}
