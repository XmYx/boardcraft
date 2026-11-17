//! Project tasks: `cargo xtask <command>`.
//!
//! - `ci`: everything a pull request must pass: `fmt --check`, clippy with `-D warnings`, the
//!   tests, the layering check and the asset check.
//! - `layers`: `boardcraft-core` must not depend on any UI crate (egui, eframe, winit, rfd).
//! - `assets`: every file under `assets/` and `docs/images/` is listed in `ATTRIBUTION.md`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const UI_CRATES: &[&str] = &["egui", "eframe", "egui-winit", "winit", "rfd", "egui-wgpu", "egui_glow"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

fn cargo(args: &[&str]) -> Result<(), String> {
    println!("$ cargo {}", args.join(" "));
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())).args(args).current_dir(root()).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("cargo {} failed", args.join(" "))) }
}

fn layers() -> Result<(), String> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["tree", "-p", "boardcraft-core", "-e", "normal", "--prefix", "none", "--format", "{p}"])
        .current_dir(root())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let bad: Vec<&str> = text.lines().filter(|l| UI_CRATES.iter().any(|c| l.split_whitespace().next() == Some(c))).collect();
    if bad.is_empty() {
        println!("layers: boardcraft-core has no UI dependency");
        Ok(())
    } else {
        Err(format!("boardcraft-core depends on UI crates: {}", bad.join(", ")))
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn assets() -> Result<(), String> {
    let root = root();
    let index = std::fs::read_to_string(root.join("ATTRIBUTION.md")).map_err(|e| format!("ATTRIBUTION.md: {e}"))?;
    let mut files = Vec::new();
    for d in ["assets", "docs/images", "docs/brand"] {
        walk(&root.join(d), &mut files);
    }
    let missing: Vec<String> = files
        .iter()
        .filter_map(|f| f.strip_prefix(&root).ok())
        .map(|f| f.to_string_lossy().replace('\\', "/"))
        .filter(|f| !f.ends_with("README.md") && !f.ends_with("LICENSE.txt"))
        .filter(|f| !index.contains(&format!("`{f}`")))
        .collect();
    if missing.is_empty() {
        println!("assets: {} files, all attributed", files.len());
        Ok(())
    } else {
        Err(format!("not listed in ATTRIBUTION.md:\n  {}", missing.join("\n  ")))
    }
}

fn ci() -> Result<(), String> {
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])?;
    cargo(&["test", "--workspace"])?;
    layers()?;
    assets()
}

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    let result = match cmd.as_str() {
        "ci" => ci(),
        "layers" => layers(),
        "assets" => assets(),
        _ => Err("usage: cargo xtask <ci|layers|assets>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}
