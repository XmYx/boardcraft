//! `boardcraft-cli mcp <board.boardcraft>`: an MCP server over one board, on stdio.
//!
//! The protocol (JSON-RPC 2.0, lifecycle, argument checks) is `craft-mcp-server` from craft-libs;
//! this module lists BoardCraft's tools and runs them:
//!
//! - `info`: scenes, panels, layers and settings as JSON.
//! - one tool per board command (`set_duration`, `add_panel`, …), its arguments the command's
//!   fields (`boardcraft_core::command::schemas`).
//! - `import_script_file`, `conform_file`: read a screenplay or an edited timeline from disk.
//! - `export_timeline`, `export_sheet`, `export_shot_list`, `export_png`: write files.
//! - `render_panel`: a panel's picture as a PNG image.
//!
//! Every tool that changes the board saves it in place (atomically) before it answers.

use std::path::{Path, PathBuf};

use boardcraft_core::{Id, Project, command, format, sheet, shotlist, stills, timeline};
use craft_mcp_server::{Effect, Info, Server, Tools, image_result, object, string, text_result, tool};
use serde_json::{Value, json};

/// Longest request line read: screenplays and timelines travel inline in some tools.
const MAX_LINE: usize = 64 << 20;

pub fn info() -> Info {
    Info { name: "boardcraft", title: "BoardCraft", version: env!("CARGO_PKG_VERSION"), max_line: MAX_LINE }
}

/// The board the server works on.
pub struct BoardTools {
    path: PathBuf,
    project: Project,
}

impl BoardTools {
    pub fn open(path: &Path) -> boardcraft_core::Result<BoardTools> {
        Ok(BoardTools { path: path.to_path_buf(), project: format::load(path)? })
    }

    #[cfg(test)]
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Save after a change; the reply says whether it worked.
    fn saved(&self, message: String) -> Value {
        match format::save(&self.project, &self.path) {
            Ok(()) => text_result(format!("{message}\nSaved {}.", self.path.display()), false),
            Err(e) => text_result(format!("{message}\nThe change is in memory but saving {} failed: {e}", self.path.display()), true),
        }
    }

    fn run_command(&mut self, name: &str, args: &Value) -> Value {
        let mut v = args.clone();
        if let Some(o) = v.as_object_mut() {
            o.insert("cmd".into(), name.into());
        }
        let cmd = match serde_json::from_value::<command::Command>(v) {
            Ok(c) => c,
            Err(e) => return text_result(format!("{name}: {e}"), true),
        };
        match command::apply(&mut self.project, &cmd) {
            Ok(out) => {
                let created = out.created.map(|i| format!(" (created id {})", i.0)).unwrap_or_default();
                self.saved(format!("{}{created}", out.message))
            }
            Err(e) => text_result(format!("{name}: {e}"), true),
        }
    }

    fn call_inner(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let b = |k: &str, default: bool| args.get(k).and_then(Value::as_bool).unwrap_or(default);
        match name {
            "info" => Ok(text_result(serde_json::to_string_pretty(&crate::info_json(&self.project)).map_err(|e| e.to_string())?, false)),
            "import_script_file" => {
                let path = s("path");
                let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
                let cmd =
                    command::Command::ImportScript { name: path, text: String::from_utf8_lossy(&bytes).into_owned(), format: None, merge: b("merge", true) };
                let out = command::apply(&mut self.project, &cmd).map_err(|e| e.to_string())?;
                Ok(self.saved(out.message))
            }
            "conform_file" => {
                let path = s("path");
                let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
                let options = timeline::ConformOptions { reorder: b("reorder", false), mark_missing: b("mark_missing", false) };
                let preview = b("preview", false);
                // Layers share their pixels, so the copy is cheap.
                let mut target = self.project.clone();
                let report = timeline::conform_with(&mut target, Path::new(&path), &bytes, &options).map_err(|e| e.to_string())?;
                if !preview {
                    self.project = target;
                }
                let text = format!("{}\n{}", report.summary(), serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
                Ok(if preview { text_result(format!("Preview, nothing changed: {text}"), false) } else { self.saved(text) })
            }
            "export_timeline" => {
                let out = s("path");
                let dir = timeline::export(&self.project, Path::new(&out)).map_err(|e| e.to_string())?;
                Ok(text_result(format!("Wrote {out} and the panel stills in {}.", dir.display()), false))
            }
            "export_sheet" => {
                let out = s("path");
                let opts = sheet_options(args)?;
                let pdf = sheet::board_sheet_pdf(&self.project, &opts, epaint_default_fonts::UBUNTU_LIGHT, 0).map_err(|e| e.to_string())?;
                format::write_atomic(Path::new(&out), &pdf).map_err(|e| e.to_string())?;
                Ok(text_result(format!("Wrote {out} ({} pages).", sheet::page_count(&self.project, &opts)), false))
            }
            "export_shot_list" => {
                let out = s("path");
                format::write_atomic(Path::new(&out), shotlist::csv(&self.project).as_bytes()).map_err(|e| e.to_string())?;
                Ok(text_result(format!("Wrote {out} ({} shots).", self.project.panel_count()), false))
            }
            "export_png" => {
                let dir = s("dir");
                let files = stills::write_stills(&self.project, Path::new(&dir)).map_err(|e| e.to_string())?;
                Ok(text_result(format!("Wrote {} panel images to {dir}.", files.len()), false))
            }
            "render_panel" => {
                let id = args.get("panel").and_then(Value::as_u64).map(Id).ok_or("panel: an id from info")?;
                let panel = self.project.panel(id).ok_or_else(|| format!("no panel {}", id.0))?;
                let frame = args.get("frame").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                let frame = if frame.is_finite() { frame.clamp(0.0, panel.duration.saturating_sub(1) as f32) } else { 0.0 };
                let png = stills::render_frame(&self.project, panel, frame).to_png().map_err(|e| e.to_string())?;
                Ok(image_result(&png, "image/png", format!("Panel {} at frame {frame}", self.project.panel_label(id))))
            }
            other if command::schemas().iter().any(|c| c.name == other) => Ok(self.run_command(other, args)),
            other => Err(format!("unknown tool {other}")),
        }
    }
}

/// Board-sheet options from tool arguments (`paper`, `columns`, `rows`, `fields`, `header`,
/// `note_lines`).
fn sheet_options(args: &Value) -> Result<sheet::SheetOptions, String> {
    let mut o = sheet::SheetOptions::default();
    if let Some(p) = args.get("paper").and_then(Value::as_str) {
        o.paper = if p == "letter" { sheet::Paper::Letter } else { sheet::Paper::A4 };
    }
    if let Some(c) = args.get("columns").and_then(Value::as_u64) {
        o.columns = c.clamp(1, 6) as u32;
    }
    if let Some(r) = args.get("rows").and_then(Value::as_u64) {
        o.rows = r.clamp(1, 6) as u32;
    }
    if let Some(f) = args.get("fields").and_then(Value::as_array) {
        let list: Vec<&str> = f.iter().filter_map(Value::as_str).collect();
        o.fields = sheet::SheetFields::parse(&list.join(","))?;
    }
    if let Some(h) = args.get("header").and_then(Value::as_str) {
        o.header = h.chars().take(sheet::MAX_HEADER).collect();
    }
    o.note_lines = args.get("note_lines").and_then(Value::as_bool).unwrap_or(false);
    Ok(o)
}

impl Tools for BoardTools {
    fn instructions(&self) -> String {
        format!(
            "BoardCraft storyboard tools for the board {}. Call `info` first: it lists scenes and panels with their ids, which the \
             other tools take. Every tool that changes the board saves it. Panel durations are in frames at the board's rate.",
            self.path.display()
        )
    }

    fn list(&mut self) -> Vec<Value> {
        let path = |d: &str| string(d);
        let boolean = |d: &str| json!({"type": "boolean", "description": d});
        let mut tools = vec![
            tool(
                "info",
                "Board info",
                "Scenes, panels (ids, labels, durations, text, layers, camera keys) and settings as JSON.",
                object(json!({}), &[]),
                Effect::ReadOnly,
            ),
            tool(
                "render_panel",
                "Render panel",
                "A panel's picture through its camera at `frame` (default 0), as a PNG.",
                object(
                    json!({"panel": {"type": "integer", "description": "Panel id"}, "frame": {"type": "number", "description": "Frame within the panel"}}),
                    &["panel"],
                ),
                Effect::ReadOnly,
            ),
            tool(
                "import_script_file",
                "Import screenplay file",
                "Import a screenplay file (Fountain, Final Draft .fdx, ScriptCraft scene list JSON). merge (default true) updates scenes by id and keeps drawings; false appends.",
                object(json!({"path": path("Screenplay file"), "merge": boolean("Update by scene id (default true)")}), &["path"]),
                Effect::Files,
            ),
            tool(
                "conform_file",
                "Conform to an edit",
                "Take panel timing from an edited timeline file (OTIO, EDL, FCP7 XML, FCPXML). reorder applies the edit's order; mark_missing flags panels the edit leaves out; preview reports without changing the board.",
                object(
                    json!({"path": path("Timeline file"), "reorder": boolean("Apply the edit's order"), "mark_missing": boolean("Flag panels not in the edit"), "preview": boolean("Report only")}),
                    &["path"],
                ),
                Effect::Files,
            ),
            tool(
                "export_timeline",
                "Export animatic timeline",
                "Write the animatic as .otio, .edl, .xml (FCP7) or .fcpxml (format from the extension), with panel stills beside it.",
                object(json!({"path": path("Output file")}), &["path"]),
                Effect::Files,
            ),
            tool(
                "export_sheet",
                "Export board sheet",
                "Write a printable board sheet (PDF).",
                object(
                    json!({
                        "path": path("Output .pdf"),
                        "paper": {"type": "string", "enum": ["a4", "letter"]},
                        "columns": {"type": "integer", "minimum": 1, "maximum": 6},
                        "rows": {"type": "integer", "minimum": 1, "maximum": 6},
                        "fields": {"type": "array", "items": {"type": "string", "enum": sheet::SheetFields::NAMES}, "description": "Fields to print (default all)"},
                        "header": path("Header line after the board name"),
                        "note_lines": boolean("Ruled lines for handwritten notes"),
                    }),
                    &["path"],
                ),
                Effect::Files,
            ),
            tool("export_shot_list", "Export shot list", "Write the shot list as CSV.", object(json!({"path": path("Output .csv")}), &["path"]), Effect::Files),
            tool(
                "export_png",
                "Export panel images",
                "Write every panel's still as PNG into a folder.",
                object(json!({"dir": path("Output folder")}), &["dir"]),
                Effect::Files,
            ),
        ];
        for c in command::schemas() {
            let title = c.name.replace('_', " ");
            tools.push(tool(c.name, &title, &format!("Board command `{}`: {} Saves the board.", c.name, c.description), c.fields, Effect::Files));
        }
        tools
    }

    fn call(&mut self, name: &str, args: &Value) -> Value {
        self.call_inner(name, args).unwrap_or_else(|e| text_result(e, true))
    }
}

/// Serve the board on stdin and stdout until the client closes.
pub fn serve(path: &Path) -> Result<(), String> {
    let tools = BoardTools::open(path).map_err(|e| e.to_string())?;
    let mut server = Server::new(info(), tools);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    server.serve(stdin.lock(), stdout.lock()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(server: &mut Server<BoardTools>, id: u64, name: &str, args: Value) -> Value {
        let line = json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}}).to_string();
        let reply: Value = serde_json::from_str(&server.handle_line(&line).unwrap()).unwrap();
        reply
    }

    #[test]
    fn tools_drive_and_save_the_board() {
        let dir = std::env::temp_dir().join(format!("boardcraft-mcp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let board = dir.join("b.boardcraft");
        format::save(&boardcraft_core::demo::demo_project(), &board).unwrap();
        let mut server = Server::new(info(), BoardTools::open(&board).unwrap());

        let init: Value = serde_json::from_str(
            &server.handle_line(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}).to_string()).unwrap(),
        )
        .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "boardcraft");
        let list: Value = serde_json::from_str(&server.handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#).unwrap()).unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        for n in ["info", "set_duration", "conform", "conform_file", "export_sheet", "export_timeline", "export_shot_list", "export_png", "import_script_file"]
        {
            assert!(names.contains(&n), "{n} in {names:?}");
        }

        // info, then a command by id; the board on disk changes.
        let info = call(&mut server, 3, "info", json!({}));
        let text = info["result"]["content"][0]["text"].as_str().unwrap();
        let v: Value = serde_json::from_str(text).unwrap();
        let id = v["scenes"][0]["panels"][0]["id"].as_u64().unwrap();
        let r = call(&mut server, 4, "set_duration", json!({"panel": id, "frames": 7}));
        assert!(r["result"].get("isError").is_none(), "{r}");
        assert_eq!(format::load(&board).unwrap().panel(Id(id)).unwrap().duration, 7);

        // Schema checks run before the tool; command errors are tool errors.
        let bad = call(&mut server, 5, "set_duration", json!({"panel": id}));
        assert!(bad["error"]["message"].as_str().unwrap().contains("frames"), "{bad}");
        let missing = call(&mut server, 6, "delete_panel", json!({"panel": 999_999}));
        assert_eq!(missing["result"]["isError"], true, "{missing}");

        // Exports and a conform from a file, with a preview first.
        let otio = dir.join("cut.otio");
        let r = call(&mut server, 7, "export_timeline", json!({"path": otio.to_str().unwrap()}));
        assert!(r["result"].get("isError").is_none(), "{r}");
        let r = call(&mut server, 8, "conform_file", json!({"path": otio.to_str().unwrap(), "reorder": true, "mark_missing": true, "preview": true}));
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().starts_with("Preview"), "{r}");
        let r = call(&mut server, 9, "conform_file", json!({"path": otio.to_str().unwrap(), "reorder": true}));
        assert!(r["result"].get("isError").is_none(), "{r}");
        let pdf = dir.join("s.pdf");
        let r = call(
            &mut server,
            10,
            "export_sheet",
            json!({"path": pdf.to_str().unwrap(), "paper": "letter", "columns": 2, "rows": 2, "fields": ["dialogue"], "header": "Draft 2", "note_lines": true}),
        );
        assert!(r["result"].get("isError").is_none(), "{r}");
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF-"));
        let csv = dir.join("s.csv");
        assert!(call(&mut server, 11, "export_shot_list", json!({"path": csv.to_str().unwrap()}))["result"].get("isError").is_none());
        let pngs = dir.join("png");
        assert!(call(&mut server, 12, "export_png", json!({"dir": pngs.to_str().unwrap()}))["result"].get("isError").is_none());
        let img = call(&mut server, 13, "render_panel", json!({"panel": id}));
        assert_eq!(img["result"]["content"][0]["type"], "image");

        // A screenplay file updates the board; the server's board and the file agree.
        let script = dir.join("s.fountain");
        std::fs::write(&script, "INT. NEW PLACE\n\nWind.\n").unwrap();
        let r = call(&mut server, 14, "import_script_file", json!({"path": script.to_str().unwrap(), "merge": false}));
        assert!(r["result"].get("isError").is_none(), "{r}");
        let on_disk = format::load(&board).unwrap();
        assert_eq!(on_disk.scenes.len(), server.tools().project().scenes.len());
        assert!(on_disk.scenes.iter().any(|s| s.heading == "INT. NEW PLACE"));
        let r = call(&mut server, 15, "import_script_file", json!({"path": dir.join("nope.fountain").to_str().unwrap()}));
        assert_eq!(r["result"]["isError"], true);
        std::fs::remove_dir_all(&dir).ok();
    }
}
