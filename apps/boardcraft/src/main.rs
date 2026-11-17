//! BoardCraft desktop app.
//!
//! Usage: `boardcraft [--demo] [--view board|panel] [--screenshot <file.png>] [file.boardcraft | script.fountain]`;
//! `--help` prints the options, `--version` the version. An unknown option is an error (exit code 2).
//!
//! - `--demo` opens the built-in demo board.
//! - A `.fountain` file starts a new board from the screenplay (the ScriptCraft hand-off).
//! - `--screenshot <file.png>` renders the window once, writes it to the file and quits (used for
//!   `docs/images/`).
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]
#![forbid(unsafe_code)]

#[macro_use]
mod i18n;
mod app;
mod args;
mod audio_out;
mod audio_ui;
mod autosave;
mod board;
mod canvas;
mod external;
mod history;
mod inspector;
mod movie_ui;
mod settings;
mod textures;
mod timeline;
mod tools;
#[cfg(test)]
mod ui_tests;

use args::{Cli, Launch};

/// Reverse-DNS app id: Linux `.desktop` file, hicolor icon name and Wayland app id.
const APP_ID: &str = "ai.storyteller.boardcraft";

fn main() -> eframe::Result {
    let cli = match args::parse(std::env::args().skip(1)) {
        Ok(Launch::Run(cli)) => cli,
        Ok(Launch::Print(text)) => {
            print!("{text}");
            return Ok(());
        }
        Err(e) => {
            eprintln!("boardcraft: {e}\n\n{}", args::USAGE);
            std::process::exit(2);
        }
    };
    let Cli { screenshot, .. } = &cli;
    let size = if screenshot.is_some() { [1600.0, 1000.0] } else { [1440.0, 900.0] };
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/ai.storyteller.boardcraft.png")).unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BoardCraft")
            .with_app_id(APP_ID)
            .with_icon(icon)
            .with_inner_size(size)
            .with_min_inner_size([800.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("BoardCraft", options, Box::new(move |cc| Ok(Box::new(app::BoardApp::new(cc, cli)))))
}
