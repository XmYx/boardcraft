//! UI tests with egui_kittest. Every `run()` also checks that each visible input widget has an
//! accessible name (house rule: accessibility).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::{BoardApp, CanvasTool, View};
use crate::args::{Cli, StartView};

fn harness(view: StartView) -> Harness<'static, BoardApp> {
    let ctx = egui::Context::default();
    let app = BoardApp::with_context(&ctx, Cli { demo: true, view, ..Default::default() });
    Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_ui_state(
        |ui, app: &mut BoardApp| {
            let ctx = ui.ctx().clone();
            app.tick(&ctx);
            app.draw(ui);
        },
        app,
    )
}

#[test]
fn board_view_is_accessible_and_selects_panels() {
    let mut h = harness(StartView::Board);
    h.run();
    h.get_by_label("Panel 2.1").click();
    h.run();
    let app = h.state();
    assert_eq!(app.project.panel_label(app.selected.unwrap()), "2.1");
    assert_eq!(app.playhead as u64, app.project.panel_start(app.selected.unwrap()).unwrap());
}

#[test]
fn panel_view_is_accessible_and_draws() {
    let mut h = harness(StartView::Panel);
    h.run();
    assert_eq!(h.state().view, View::Panel);
    let canvas = h.get_by_label("Panel canvas").rect();
    let before = {
        let app = h.state();
        let p = app.project.panel(app.selected.unwrap()).unwrap();
        p.layers.last().unwrap().pixels.data().iter().filter(|b| **b != 0).count()
    };
    // Drag across the canvas with the brush.
    let a = canvas.center() + egui::vec2(-200.0, 120.0);
    let b = canvas.center() + egui::vec2(200.0, 160.0);
    h.drag_at(a);
    h.run();
    h.hover_at(b);
    h.run();
    h.drop_at(b);
    h.run();
    let app = h.state();
    let p = app.project.panel(app.selected.unwrap()).unwrap();
    let after = p.layers.last().unwrap().pixels.data().iter().filter(|b| **b != 0).count();
    assert!(after > before, "the stroke painted nothing ({before} → {after})");
    assert!(app.dirty);
    assert_eq!(app.undo.len(), 1);
}

#[test]
fn camera_tool_and_play() {
    let mut h = harness(StartView::Panel);
    h.run();
    h.get_by_label("Camera").click();
    h.run();
    assert_eq!(h.state().tool, CanvasTool::Camera);
    h.get_by_label("Play").click();
    h.run_steps(5);
    assert!(h.state().playing);
    assert!(h.state().playhead > 0.0);
}
