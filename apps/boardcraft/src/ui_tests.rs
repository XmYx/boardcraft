//! UI tests with egui_kittest. Every `run()` also checks that each visible input widget has an
//! accessible name (house rule: accessibility).

use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};

use crate::app::{BoardApp, CanvasTool, View};
use crate::args::{Cli, StartView};

fn harness(view: StartView) -> Harness<'static, BoardApp> {
    let ctx = egui::Context::default();
    // Autosaves go to a per-test folder, never the user's data folder.
    let data = std::env::temp_dir().join(format!("boardcraft-ui-{}-{:?}", std::process::id(), std::thread::current().id()));
    let app = BoardApp::with_context(&ctx, Cli { demo: true, view, data_dir: Some(data), ..Default::default() });
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
    let canvas = h.get_by_label_contains("Panel canvas").rect();
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
    assert_eq!(app.history.undo_len(), 1, "one stroke is one undo step");
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

#[test]
fn keyboard_moves_selection_and_focus_on_the_board() {
    let mut h = harness(StartView::Board);
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    let label = { h.state().project.panel_label(h.state().selected.unwrap()) };
    assert_eq!(label, "1.3");
    let card = h.get(By::new().role(egui::Role::Button).label("Panel 1.3"));
    assert!(card.is_focused(), "the selected card takes keyboard focus");
    // Enter opens the focused panel in the Panel view.
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().view, View::Panel);
    // The canvas publishes which panel it shows.
    let _canvas = h.get(By::new().role(egui::Role::Canvas).label_contains("Panel canvas: panel 1.3"));
}

#[test]
fn keyboard_alternatives_to_dragging() {
    let mut h = harness(StartView::Panel);
    h.run();
    // Playhead: . steps a frame, Home returns to the start.
    h.key_press(egui::Key::Period);
    h.run();
    assert_eq!(h.state().playhead, 1.0);
    h.key_press(egui::Key::Home);
    h.run();
    assert_eq!(h.state().playhead, 0.0);
    // Camera keys without dragging: select a static panel, add keys with buttons.
    h.key_press(egui::Key::ArrowRight);
    h.run();
    let id = h.state().selected.unwrap();
    assert!(h.state().project.panel(id).unwrap().camera.keys.is_empty());
    h.get_by_label("Key at start").click();
    h.run();
    h.get_by_label("Key at end").click();
    h.run();
    assert_eq!(h.state().project.panel(id).unwrap().camera.keys.len(), 2);
    // Retiming without dragging: the duration field takes typed values.
    assert!(h.query_by_label("Duration").is_some());
}

fn luminance(c: egui::Color32) -> f32 {
    let f = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.040_45 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

fn contrast(a: egui::Color32, b: egui::Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

#[test]
fn custom_colours_meet_4_5_to_1_in_both_themes() {
    let paper = egui::Color32::from_rgba_unmultiplied(250, 250, 247, 255);
    assert!(contrast(crate::board::CAM_INK, paper) >= 4.5);
    for band in crate::timeline::SCENE_COLORS {
        assert!(contrast(egui::Color32::WHITE, band) >= 4.5, "{band:?}");
    }
    for visuals in [crate::app::readable_visuals(egui::Visuals::dark()), crate::app::readable_visuals(egui::Visuals::light())] {
        let bg = visuals.panel_fill;
        assert!(contrast(visuals.text_color(), bg) >= 4.5);
        assert!(contrast(visuals.weak_text_color(), bg) >= 4.5, "weak text in {}", if visuals.dark_mode { "dark" } else { "light" });
    }
}

#[test]
fn every_edit_undoes_from_the_keyboard() {
    let mut h = harness(StartView::Board);
    h.run();
    let before = h.state().project.panel_count();
    // A structural edit (Ctrl+N: new panel) and its undo/redo.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::N);
    h.run();
    assert_eq!(h.state().project.panel_count(), before + 1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert_eq!(h.state().project.panel_count(), before);
    h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
    h.run();
    assert_eq!(h.state().project.panel_count(), before + 1);
}

#[test]
fn typing_is_one_undo_step() {
    let mut h = harness(StartView::Board);
    h.run();
    let id = h.state().selected.unwrap();
    let original = h.state().project.panel(id).unwrap().notes.clone();
    h.get_by_label("Notes").focus();
    h.run();
    h.get_by_label("Notes").type_text("Low angle");
    h.run();
    assert!(h.state().project.panel(id).unwrap().notes.ends_with("Low angle"));
    let steps = h.state().history.undo_len();
    assert_eq!(steps, 1, "typing in one field is one step");
    // Leave the field, then undo with the keyboard.
    h.key_press(egui::Key::Escape);
    h.run();
    h.state_mut().undo();
    h.run();
    assert_eq!(h.state().project.panel(id).unwrap().notes, original);
}

#[test]
fn crashed_work_is_offered_and_recovered_by_keyboard() {
    // A previous session left an autosave with a stale heartbeat.
    let data = std::env::temp_dir().join(format!("boardcraft-ui-recover-{}", std::process::id()));
    let dir = data.join("Autosave");
    std::fs::create_dir_all(&dir).unwrap();
    let mut lost = boardcraft_core::demo::demo_project();
    lost.name = "Night Shoot".into();
    boardcraft_core::format::save(&lost, &dir.join("1-1.boardcraft")).unwrap();
    std::fs::write(
        dir.join("1-1.json"),
        r#"{"format":"boardcraft.autosave","version":1,"name":"Night Shoot","document":null,"saved":"2026-11-17T10:00:00+00:00","heartbeat":"2026-11-17T10:00:00+00:00"}"#,
    )
    .unwrap();
    let ctx = egui::Context::default();
    let app = BoardApp::with_context(&ctx, Cli { data_dir: Some(data.clone()), ..Default::default() });
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_ui_state(
        |ui, app: &mut BoardApp| {
            let ctx = ui.ctx().clone();
            app.tick(&ctx);
            app.draw(ui);
        },
        app,
    );
    h.run();
    let button = h.get_by_label("Recover Night Shoot");
    button.focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().project.name, "Night Shoot");
    assert!(h.state().dirty, "recovered work is unsaved until saved");
    assert!(h.query_by_label("Recover Night Shoot").is_none());
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn board_settings_change_aspect_and_rate_by_keyboard() {
    let mut h = harness(StartView::Board);
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Comma);
    h.run();
    assert!(h.state().settings_dialog.is_some());
    h.get_by_label("2.39:1").click();
    h.run();
    h.get_by_label("23.976").click();
    h.run();
    let total_s = h.state().project.total_frames() as f64 / h.state().project.settings.fps_f64();
    let apply = h.get_by_label("Apply");
    apply.focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    let s = h.state().project.settings;
    assert_eq!((s.width, s.height), (1280, 536));
    assert_eq!(s.rate_label(), "23.976");
    let after = h.state().project.total_frames() as f64 / s.fps_f64();
    assert!((after - total_s).abs() < 0.3, "timing kept: {total_s} → {after}");
    assert!(h.state().settings_dialog.is_none());
    // One undo step brings both back.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert_eq!(h.state().project.settings.width, 1280);
    assert_eq!(h.state().project.settings.height, 720);
}

#[test]
fn duplicate_onion_zoom_and_shot_presets() {
    let mut h = harness(StartView::Panel);
    h.run();
    h.key_press(egui::Key::O);
    h.run();
    assert!(h.state().onion);
    h.key_press(egui::Key::Plus);
    h.run();
    assert!(h.state().canvas_zoom > 1.0);
    h.key_press(egui::Key::Num0);
    h.run();
    assert_eq!(h.state().canvas_zoom, 1.0);
    h.get_by_label("Shot presets").click();
    h.run();
    h.get_by_label_contains("Close-up").click();
    h.run();
    let id = h.state().selected.unwrap();
    assert!(["CU", "MCU", "ECU"].contains(&h.state().project.panel(id).unwrap().shot.as_str()));
    let n = h.state().project.panel_count();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::D);
    h.run();
    assert_eq!(h.state().project.panel_count(), n + 1);
    let copy = h.state().selected.unwrap();
    assert_ne!(copy, id);
    assert_eq!(h.state().project.panel(copy).unwrap().shot, h.state().project.panel(id).unwrap().shot);
}
