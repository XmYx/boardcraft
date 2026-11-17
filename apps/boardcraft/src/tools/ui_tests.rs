//! Keyboard and pointer tests of the drawing tools, each with its undo.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};

use crate::app::{BoardApp, CanvasTool};
use crate::args::StartView;
use crate::ui_tests::harness;
use boardcraft_core::Raster;

fn layer(h: &Harness<'static, BoardApp>) -> Raster {
    let app = h.state();
    let p = app.project.panel(app.selected.unwrap()).unwrap();
    p.layer(app.active_layer.unwrap()).unwrap().pixels.clone()
}

fn opaque(r: &Raster) -> usize {
    r.data().as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count()
}

fn key(h: &mut Harness<'static, BoardApp>, k: Key) {
    h.key_press(k);
    h.run();
}

fn key_mod(h: &mut Harness<'static, BoardApp>, m: Modifiers, k: Key) {
    h.key_press_modifiers(m, k);
    h.run();
}

fn click(h: &mut Harness<'static, BoardApp>, pos: egui::Pos2) {
    h.drag_at(pos);
    h.run();
    h.drop_at(pos);
    h.run();
}

fn drag(h: &mut Harness<'static, BoardApp>, path: &[egui::Pos2]) {
    h.drag_at(path[0]);
    h.run();
    for p in &path[1..] {
        h.hover_at(*p);
        h.run();
    }
    h.drop_at(*path.last().unwrap());
    h.run();
}

fn canvas(h: &Harness<'static, BoardApp>) -> egui::Rect {
    h.get_by_label_contains("Panel canvas").rect()
}

/// The first opaque pixel at least `margin` from the right and bottom edges.
fn some_ink(r: &Raster, margin: u32) -> (u32, u32) {
    for y in 0..r.height() - margin {
        for x in 0..r.width() - margin {
            if r.pixel(x, y)[3] == 255 {
                return (x, y);
            }
        }
    }
    panic!("no ink on the layer");
}

#[test]
fn tool_keys_pick_the_tools() {
    let mut h = harness(StartView::Panel);
    h.run();
    key(&mut h, Key::L);
    assert_eq!(h.state().tool, CanvasTool::Lasso);
    key(&mut h, Key::M);
    assert_eq!(h.state().tool, CanvasTool::RectSelect);
    key(&mut h, Key::G);
    assert_eq!(h.state().tool, CanvasTool::Fill);
    // The toolbar shows each as a selectable button with its name.
    let _ = h.get(By::new().label("Lasso"));
    let _ = h.get(By::new().label("Rectangle select"));
    let _ = h.get(By::new().label("Fill"));
}

#[test]
fn select_all_move_with_arrows_apply_and_undo() {
    let mut h = harness(StartView::Panel);
    h.run();
    let before = layer(&h);
    let (x, y) = some_ink(&before, 20);
    let steps = h.state().history.undo_len();
    key_mod(&mut h, Modifiers::COMMAND, Key::A);
    assert!(h.state().tools.sel.selection.is_some());
    assert_eq!(h.state().tool, CanvasTool::RectSelect, "Select All switches to a selection tool");
    for _ in 0..3 {
        key(&mut h, Key::ArrowRight);
    }
    key_mod(&mut h, Modifiers::SHIFT, Key::ArrowDown);
    // Floating: the layer is untouched until applied, and the arrows did not change the panel.
    assert!(layer(&h).shares_pixels(&before));
    let _ = h.get(By::new().role(egui::Role::Canvas).label_contains("moved 3, 10"));
    assert_eq!(h.state().tools.sel.selection.as_ref().unwrap().transform.dx, 3.0);
    key(&mut h, Key::Enter);
    assert!(h.state().tools.sel.selection.is_none());
    let after = layer(&h);
    assert_eq!(after.pixel(x + 3, y + 10), before.pixel(x, y));
    assert_eq!(h.state().history.undo_len(), steps + 1, "applying is one undo step");
    key_mod(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(layer(&h).data(), before.data());
}

#[test]
fn typed_values_and_escape() {
    let mut h = harness(StartView::Panel);
    h.run();
    let before = layer(&h);
    key_mod(&mut h, Modifiers::COMMAND, Key::A);
    // The typed transform values are named fields in the toolbar.
    for name in ["X", "Y", "Width", "Height", "Angle"] {
        assert!(h.query_by_label(name).is_some(), "{name}");
    }
    h.state_mut().tools.sel.selection.as_mut().unwrap().transform.angle = 30.0;
    h.run();
    let _ = h.get(By::new().role(egui::Role::Canvas).label_contains("rotated 30°"));
    // Ctrl+Z first resets the transform; Esc then cancels without changing the layer.
    key_mod(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(h.state().tools.sel.selection.as_ref().unwrap().transform.is_identity());
    key(&mut h, Key::Escape);
    assert!(h.state().tools.sel.selection.is_none());
    assert_eq!(layer(&h).data(), before.data());
}

#[test]
fn delete_cut_and_paste_by_keyboard_with_undo() {
    let mut h = harness(StartView::Panel);
    h.run();
    let before = layer(&h);
    assert!(opaque(&before) > 0);
    key_mod(&mut h, Modifiers::COMMAND, Key::A);
    key(&mut h, Key::Delete);
    assert!(layer(&h).is_blank(), "Delete clears the selection");
    key_mod(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(layer(&h).data(), before.data());
    // Cut, then paste back 5 px to the left.
    key_mod(&mut h, Modifiers::COMMAND, Key::A);
    key_mod(&mut h, Modifiers::COMMAND, Key::X);
    assert!(layer(&h).is_blank());
    assert!(h.state().tools.sel.clipboard.is_some());
    key_mod(&mut h, Modifiers::COMMAND, Key::V);
    assert!(h.state().tools.sel.selection.as_ref().is_some_and(|s| s.pasted));
    assert!(layer(&h).is_blank(), "pasted pixels float until applied");
    for _ in 0..5 {
        key(&mut h, Key::ArrowLeft);
    }
    key(&mut h, Key::Enter);
    let (x, y) = some_ink(&before, 0);
    if x >= 5 {
        assert_eq!(layer(&h).pixel(x - 5, y), before.pixel(x, y));
    }
    assert!(opaque(&layer(&h)) > 0);
    // Undo the paste, then the cut.
    key_mod(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(layer(&h).is_blank());
    key_mod(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(layer(&h).data(), before.data());
}

#[test]
fn lasso_drag_selects_and_drag_inside_moves() {
    let mut h = harness(StartView::Panel);
    h.run();
    key(&mut h, Key::L);
    let c = canvas(&h).center();
    let v = egui::vec2;
    drag(&mut h, &[c + v(-100.0, -80.0), c + v(100.0, -80.0), c + v(100.0, 80.0), c + v(-100.0, 80.0), c + v(-100.0, -60.0)]);
    let sel = h.state().tools.sel.selection.as_ref().map(|s| s.outline.len());
    assert!(sel.is_some_and(|n| n >= 4), "{sel:?}");
    drag(&mut h, &[c, c + v(10.0, 0.0), c + v(40.0, 0.0)]);
    let dx = h.state().tools.sel.selection.as_ref().unwrap().transform.dx;
    assert!(dx > 10.0, "moved by dragging inside ({dx})");
    key(&mut h, Key::Escape);
    assert!(h.state().tools.sel.selection.is_none());
}

#[test]
fn fill_click_and_undo() {
    let mut h = harness(StartView::Panel);
    h.run();
    key(&mut h, Key::G);
    let before = layer(&h);
    let steps = h.state().history.undo_len();
    // Fill on this layer only: the empty area around the drawing.
    h.state_mut().tools.fill_sample = super::FillSample::Layer;
    h.run();
    assert_eq!(before.pixel(4, 4)[3], 0);
    let at = h.state().tools.last_view.unwrap().to_screen([4.5, 4.5]);
    click(&mut h, at);
    let after = layer(&h);
    assert!(opaque(&after) > opaque(&before), "{} → {}", opaque(&before), opaque(&after));
    assert_eq!(h.state().history.undo_len(), steps + 1);
    key_mod(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(layer(&h).data(), before.data());
}

#[test]
fn view_rotation_keys_and_drawing_through_the_rotation() {
    let mut h = harness(StartView::Panel);
    h.run();
    key_mod(&mut h, Modifiers::SHIFT, Key::CloseBracket);
    assert_eq!(h.state().tools.view_angle, 15.0);
    key_mod(&mut h, Modifiers::SHIFT, Key::Num0);
    assert_eq!(h.state().tools.view_angle, 0.0);
    for _ in 0..6 {
        key_mod(&mut h, Modifiers::SHIFT, Key::CloseBracket);
    }
    assert_eq!(h.state().tools.view_angle, 90.0);
    let _ = h.get(By::new().role(egui::Role::Canvas).label_contains("view rotated 90°"));
    // A horizontal drag on screen draws a vertical line on the canvas.
    let before = layer(&h);
    let c = canvas(&h).center();
    drag(&mut h, &[c + egui::vec2(-120.0, 0.0), c + egui::vec2(0.0, 0.0), c + egui::vec2(120.0, 0.0)]);
    let after = layer(&h);
    let (mut x0, mut x1, mut y0, mut y1) = (u32::MAX, 0, u32::MAX, 0);
    for y in 0..after.height() {
        for x in 0..after.width() {
            if after.pixel(x, y) != before.pixel(x, y) {
                (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
            }
        }
    }
    assert!(x1 >= x0 && y1 >= y0, "the stroke painted nothing");
    assert!(y1 - y0 > 4 * (x1 - x0), "vertical on the canvas: {}×{}", x1 - x0, y1 - y0);
    // Undo the stroke; the rotation is a view setting, not an edit.
    key_mod(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(layer(&h).data(), before.data());
    assert_eq!(h.state().tools.view_angle, 90.0);
}

#[test]
fn pen_settings_are_reachable_and_the_stabilized_stroke_ends_at_the_pen() {
    let mut h = harness(StartView::Panel);
    h.run();
    h.get_by_label("Pen Settings").click();
    h.run();
    let _ = h.get(By::new().role(egui::Role::Slider).label("Stabilizer"));
    let _ = h.get(By::new().role(egui::Role::Slider).label("Pressure curve"));
    let _ = h.get(By::new().role(egui::Role::Slider).label("Minimum size (%)"));
    let _ = h.get(By::new().role(egui::Role::Image).label_contains("Pressure curve preview"));
    key(&mut h, Key::Escape);
    h.state_mut().tools.pen.stabilizer = 10;
    h.run();
    let c = canvas(&h).center();
    let end = c + egui::vec2(150.0, 40.0);
    drag(&mut h, &[c, c + egui::vec2(50.0, 40.0), c + egui::vec2(100.0, -40.0), end]);
    let (_, _, last) = h.state().last_point.expect("a stroke was drawn");
    let want = h.state().tools.last_view.unwrap().to_canvas(end);
    assert!((last.x - want[0]).abs() < 1.0 && (last.y - want[1]).abs() < 1.0, "{last:?} vs {want:?}");
}
