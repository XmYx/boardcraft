//! `boardcraft-cli`: BoardCraft without a window, for scripts and agents.
//!
//! ```text
//! boardcraft-cli new <out.boardcraft> [--from <script>] [--demo]
//! boardcraft-cli info <board.boardcraft>                 # JSON summary
//! boardcraft-cli exec <board.boardcraft> <command-json>... # apply commands, save in place
//! boardcraft-cli import-script <board.boardcraft> <script> [--append]
//! boardcraft-cli export <board.boardcraft> <out.otio|.edl|.xml|.fcpxml>
//! boardcraft-cli conform <board.boardcraft> <edit.otio|.edl|.xml|.fcpxml>
//! boardcraft-cli export-png <board.boardcraft> <dir>
//! boardcraft-cli export-sheet <board.boardcraft> <out.pdf> [--paper a4|letter] [--grid <cols>x<rows>]
//! boardcraft-cli commands                                # list command names
//! ```
//!
//! Commands are the JSON form of `boardcraft_core::command::Command` (docs/cli.md). Exit codes:
//! 0 success, 1 a failed operation, 2 a usage error.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use std::path::Path;
use std::process::ExitCode;

use boardcraft_core::{command, demo, format, fountain, sheet, stills, timeline};
use serde_json::json;

const USAGE: &str = "usage:
  boardcraft-cli new <out.boardcraft> [--from <script.fountain|.fdx|.scenes.json>] [--demo]
  boardcraft-cli info <board.boardcraft>
  boardcraft-cli exec <board.boardcraft> <command-json>...
  boardcraft-cli import-script <board.boardcraft> <script> [--append]
  boardcraft-cli export <board.boardcraft> <out.otio|.edl|.xml|.fcpxml>
  boardcraft-cli conform <board.boardcraft> <edit.otio|.edl|.xml|.fcpxml>
  boardcraft-cli export-png <board.boardcraft> <dir>
  boardcraft-cli export-sheet <board.boardcraft> <out.pdf> [--paper a4|letter] [--grid <cols>x<rows>]
  boardcraft-cli export-shotlist <board.boardcraft> <out.csv>
  boardcraft-cli commands
  boardcraft-cli --version";

const COMMANDS: &[&str] = &[
    "add_scene",
    "add_panel",
    "delete_panel",
    "move_panel",
    "set_duration",
    "set_text",
    "set_scene_heading",
    "set_camera_key",
    "remove_camera_key",
    "clear_camera",
    "move_camera_key",
    "add_shape",
    "delete_shape",
    "move_shape",
    "style_shape",
    "import_shapes_svg",
    "add_layer",
    "delete_layer",
    "set_layer_visible",
    "set_layer_opacity",
    "import_fountain",
    "import_script",
    "conform",
    "duplicate_panel",
    "set_canvas",
    "set_frame_rate",
];

enum Fail {
    Usage(String),
    Op(String),
}

fn op<E: std::fmt::Display>(e: E) -> Fail {
    Fail::Op(e.to_string())
}

fn info(path: &Path) -> Result<String, Fail> {
    let p = format::load(path).map_err(op)?;
    let scenes: Vec<_> = p
        .scenes
        .iter()
        .map(|s| {
            json!({
                "id": s.id.0, "number": s.number, "heading": s.heading, "source_scene_id": s.source_scene_id,
                "panels": s.panels.iter().map(|x| json!({
                    "id": x.id.0, "label": p.panel_label(x.id), "duration": x.duration,
                    "dialogue": x.dialogue, "action": x.action, "notes": x.notes,
                    "layers": x.layers.iter().map(|l| json!({"id": l.id.0, "name": l.name, "visible": l.visible, "opacity": l.opacity})).collect::<Vec<_>>(),
                    "camera": x.camera.keys,
                    "annotations": x.annotations,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let v = json!({"name": p.name, "settings": p.settings, "total_frames": p.total_frames(), "scenes": scenes});
    serde_json::to_string_pretty(&v).map_err(op)
}

fn run(args: &[String]) -> Result<String, Fail> {
    let arg = |i: usize| args.get(i).map(String::as_str).ok_or_else(|| Fail::Usage(USAGE.into()));
    match arg(0)? {
        "--version" | "-V" => Ok(boardcraft_core::GENERATOR.to_string()),
        "--help" | "-h" => Ok(USAGE.to_string()),
        "commands" => Ok(COMMANDS.join("\n")),
        "new" => {
            let out = Path::new(arg(1)?);
            let mut project = boardcraft_core::Project::blank("Untitled board");
            let mut i = 2;
            while let Some(a) = args.get(i) {
                match a.as_str() {
                    "--demo" => project = demo::demo_project(),
                    "--from" => {
                        let path = arg(i + 1)?;
                        let bytes = std::fs::read(path).map_err(|e| Fail::Op(format!("{path}: {e}")))?;
                        let script = boardcraft_core::script::parse_script(path, &bytes).map_err(op)?;
                        project = fountain::project_from_script(&script, Default::default());
                        i += 1;
                    }
                    other => return Err(Fail::Usage(format!("unknown option {other}\n{USAGE}"))),
                }
                i += 1;
            }
            format::save(&project, out).map_err(op)?;
            Ok(format!("wrote {} ({} scenes, {} panels)", out.display(), project.scenes.len(), project.panel_count()))
        }
        "info" => info(Path::new(arg(1)?)),
        "exec" => {
            let path = Path::new(arg(1)?);
            if args.len() < 3 {
                return Err(Fail::Usage(USAGE.into()));
            }
            let mut project = format::load(path).map_err(op)?;
            let mut outcomes = Vec::new();
            for (n, text) in args.iter().skip(2).enumerate() {
                let cmd = command::parse(text).map_err(|e| Fail::Op(format!("command {}: {e}", n + 1)))?;
                let out = command::apply(&mut project, &cmd).map_err(|e| Fail::Op(format!("command {}: {e}", n + 1)))?;
                outcomes.push(json!({"created": out.created.map(|i| i.0), "message": out.message}));
            }
            format::save(&project, path).map_err(op)?;
            serde_json::to_string_pretty(&outcomes).map_err(op)
        }
        "export" | "export-otio" => {
            let project = format::load(Path::new(arg(1)?)).map_err(op)?;
            let stills = timeline::export(&project, Path::new(arg(2)?)).map_err(op)?;
            Ok(format!("wrote {} and stills in {}", arg(2)?, stills.display()))
        }
        "import-script" => {
            let board = Path::new(arg(1)?);
            let path = arg(2)?;
            let merge = !args.iter().skip(3).any(|a| a == "--append");
            let bytes = std::fs::read(path).map_err(|e| Fail::Op(format!("{path}: {e}")))?;
            let mut project = format::load(board).map_err(op)?;
            let cmd = command::Command::ImportScript { name: path.to_string(), text: String::from_utf8_lossy(&bytes).into_owned(), format: None, merge };
            let out = command::apply(&mut project, &cmd).map_err(op)?;
            format::save(&project, board).map_err(op)?;
            Ok(out.message)
        }
        "conform" => {
            let board = Path::new(arg(1)?);
            let path = arg(2)?;
            let bytes = std::fs::read(path).map_err(|e| Fail::Op(format!("{path}: {e}")))?;
            let mut project = format::load(board).map_err(op)?;
            let r = timeline::conform(&mut project, Path::new(path), &bytes).map_err(op)?;
            format::save(&project, board).map_err(op)?;
            serde_json::to_string_pretty(&r).map_err(op)
        }
        "export-png" => {
            let project = format::load(Path::new(arg(1)?)).map_err(op)?;
            let files = stills::write_stills(&project, Path::new(arg(2)?)).map_err(op)?;
            Ok(format!("wrote {} panel images", files.len()))
        }
        "export-shotlist" => {
            let project = format::load(Path::new(arg(1)?)).map_err(op)?;
            let out = Path::new(arg(2)?);
            format::write_atomic(out, boardcraft_core::shotlist::csv(&project).as_bytes()).map_err(op)?;
            Ok(format!("wrote {} ({} shots)", out.display(), project.panel_count()))
        }
        "export-sheet" => {
            let project = format::load(Path::new(arg(1)?)).map_err(op)?;
            let out = Path::new(arg(2)?);
            let mut opts = sheet::SheetOptions::default();
            let mut i = 3;
            while let Some(a) = args.get(i) {
                match (a.as_str(), args.get(i + 1).map(String::as_str)) {
                    ("--paper", Some("a4")) => opts.paper = sheet::Paper::A4,
                    ("--paper", Some("letter")) => opts.paper = sheet::Paper::Letter,
                    ("--grid", Some(g)) => {
                        let (c, r) = g.split_once('x').ok_or_else(|| Fail::Usage(format!("--grid takes <cols>x<rows>, not {g}")))?;
                        opts.columns = c.parse().map_err(|_| Fail::Usage(format!("bad grid {g}")))?;
                        opts.rows = r.parse().map_err(|_| Fail::Usage(format!("bad grid {g}")))?;
                    }
                    _ => return Err(Fail::Usage(format!("unknown option {a}\n{USAGE}"))),
                }
                i += 2;
            }
            let pdf = sheet::board_sheet_pdf(&project, &opts, epaint_default_fonts::UBUNTU_LIGHT, 0).map_err(op)?;
            format::write_atomic(out, &pdf).map_err(op)?;
            Ok(format!("wrote {} ({} pages)", out.display(), sheet::page_count(&project, &opts)))
        }
        other => Err(Fail::Usage(format!("unknown command {other}\n{USAGE}"))),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(Fail::Usage(e)) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
        Err(Fail::Op(e)) => {
            eprintln!("boardcraft-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn end_to_end() {
        let dir = std::env::temp_dir().join(format!("boardcraft-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("s.fountain");
        std::fs::write(&script, "INT. A\n\nRain.\n\nBOB\nHi.\n").unwrap();
        let board = dir.join("b.boardcraft");
        let (b, sc) = (board.to_str().unwrap(), script.to_str().unwrap());
        assert!(run(&s(&["new", b, "--from", sc])).is_ok());
        let info: serde_json::Value = serde_json::from_str(&info(&board).ok().unwrap()).unwrap();
        let id = info["scenes"][0]["panels"][0]["id"].as_u64().unwrap();
        let cmd = format!(r#"{{"cmd":"set_duration","panel":{id},"frames":30}}"#);
        assert!(run(&s(&["exec", b, &cmd])).is_ok());
        assert_eq!(format::load(&board).unwrap().panels().next().unwrap().1.duration, 30);
        assert!(matches!(run(&s(&["exec", b, r#"{"cmd":"nope"}"#])), Err(Fail::Op(_))));
        for ext in ["otio", "edl", "xml", "fcpxml"] {
            let out = dir.join(format!("a.{ext}"));
            assert!(run(&s(&["export", b, out.to_str().unwrap()])).is_ok(), "{ext}");
            assert!(run(&s(&["conform", b, out.to_str().unwrap()])).is_ok(), "{ext}");
        }
        std::fs::write(&script, "INT. A\n\nRain, harder.\n\nBOB\nHi.\n").unwrap();
        assert!(run(&s(&["import-script", b, sc])).is_ok());
        let p = format::load(&board).unwrap();
        assert_eq!(p.scenes.len(), 1, "merged by heading");
        assert_eq!(p.scenes[0].panels[0].action, "Rain, harder.");
        let csv = dir.join("shots.csv");
        assert!(run(&s(&["export-shotlist", b, csv.to_str().unwrap()])).is_ok());
        assert!(std::fs::read_to_string(&csv).unwrap().starts_with("scene,"));
        let pdf = dir.join("sheet.pdf");
        assert!(run(&s(&["export-sheet", b, pdf.to_str().unwrap(), "--paper", "letter", "--grid", "2x2"])).is_ok());
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF-"));
        assert!(matches!(run(&s(&["export-sheet", b, pdf.to_str().unwrap(), "--grid", "x"])), Err(Fail::Usage(_))));
        assert!(matches!(run(&s(&["bogus"])), Err(Fail::Usage(_))));
        assert!(matches!(run(&s(&[])), Err(Fail::Usage(_))));
        std::fs::remove_dir_all(&dir).ok();
    }
}
