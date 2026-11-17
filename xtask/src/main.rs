//! Project tasks: `cargo xtask <command>`.
//!
//! - `ci`: everything a pull request must pass: `fmt --check`, clippy with `-D warnings`, the
//!   tests, the layering check and the asset check.
//! - `layers`: `boardcraft-core` must not depend on any UI crate (egui, eframe, winit, rfd).
//! - `assets`: every file under `assets/` and `docs/images/` is listed in `ATTRIBUTION.md`.
//! - `licenses`: every dependency's licence expression is satisfiable from the house-rules
//!   allowlist (`cargo metadata`).
//! - `progress [--check]`: the RFC-0017 progress log, `ROADMAP.md`'s table joined with the
//!   fragments in `docs/progress.d/`; `--check` validates the fragments (part of `ci`).
//! - `version [set <x.y.z>]`: print or set `[workspace.package] version`.
//! - `strings [<craft-i18n checkout>]`: write every interface source text (`tl!`/`tlf!`) to
//!   `docs/i18n/boardcraft-strings.tsv`, marking the ones the shared catalog already has.
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

/// SPDX ids allowed by craft-contrib `standards/house-rules-rust.md` (Licence allowlist).
const ALLOWED: &[&str] = &[
    "ISC",
    "MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "Apache-2.0",
    "Unicode-3.0",
    "OFL-1.1",
    "Ubuntu-font-1.0",
    "Bitstream-Vera",
    "CC0-1.0",
    "CC-BY-4.0",
    "CC-BY-SA-4.0",
    "Zlib",
    "BSL-1.0",
    "0BSD",
    "Unlicense",
    "IJG",
    "NCSA",
    "CDLA-Permissive-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "LicenseRef-Public-Domain",
    // Third-party dependencies used unmodified only (house rules).
    "MPL-2.0",
];

/// Evaluate an SPDX expression (`A OR B`, `A AND (B OR C)`, `A/B`, `X WITH Y`) against the allowlist.
fn license_ok(expr: &str) -> bool {
    fn tokens(e: &str) -> Vec<String> {
        e.replace('(', " ( ").replace(')', " ) ").replace('/', " OR ").split_whitespace().map(str::to_string).collect()
    }
    fn or(t: &[String], i: &mut usize) -> bool {
        let mut v = and(t, i);
        while t.get(*i).map(String::as_str) == Some("OR") {
            *i += 1;
            v |= and(t, i);
        }
        v
    }
    fn and(t: &[String], i: &mut usize) -> bool {
        let mut v = atom(t, i);
        while t.get(*i).map(String::as_str) == Some("AND") {
            *i += 1;
            v &= atom(t, i);
        }
        v
    }
    fn atom(t: &[String], i: &mut usize) -> bool {
        match t.get(*i).map(String::as_str) {
            Some("(") => {
                *i += 1;
                let v = or(t, i);
                *i += 1; // ")"
                v
            }
            Some(id) => {
                let mut id = id.trim_end_matches('+').to_string();
                *i += 1;
                if t.get(*i).map(String::as_str) == Some("WITH") {
                    id = format!("{id} WITH {}", t.get(*i + 1).cloned().unwrap_or_default());
                    *i += 2;
                }
                ALLOWED.contains(&id.as_str())
            }
            None => false,
        }
    }
    let t = tokens(expr);
    let mut i = 0;
    !t.is_empty() && or(&t, &mut i) && i >= t.len()
}

fn licenses() -> Result<(), String> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(root())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let mut bad = Vec::new();
    let mut n = 0;
    for p in meta["packages"].as_array().into_iter().flatten() {
        n += 1;
        let name = p["name"].as_str().unwrap_or("?");
        // Workspace members inherit MIT OR Apache-2.0.
        match p["license"].as_str() {
            Some(l) if license_ok(l) => {}
            Some(l) => bad.push(format!("{name} {}: {l}", p["version"].as_str().unwrap_or(""))),
            None => bad.push(format!("{name}: no licence field")),
        }
    }
    if bad.is_empty() {
        println!("licenses: {n} packages, all on the allowlist");
        Ok(())
    } else {
        Err(format!("licences outside the allowlist:\n  {}", bad.join("\n  ")))
    }
}

fn fragments() -> Result<Vec<(PathBuf, String)>, String> {
    let dir = root().join("docs/progress.d");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else { return Ok(out) };
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name == "README.md" {
            continue;
        }
        let (issue, slug) = name.strip_suffix(".md").and_then(|n| n.split_once('-')).ok_or(format!("{name}: expected <issue>-<branch-slug>.md"))?;
        if issue.is_empty()
            || !issue.chars().all(|c| c.is_ascii_digit())
            || slug.is_empty()
            || !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(format!("{name}: expected <issue number>-<lowercase-branch-slug>.md"));
        }
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{name}: {e}"))?;
        let rows: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        if rows.len() != 1 || !rows[0].starts_with("| ") || !rows[0].ends_with(" |") || rows[0].matches(" | ").count() != 1 {
            return Err(format!("{name}: a fragment is one progress-log row: `| YYYY-MM-DD | what landed |`"));
        }
        let date = rows[0].trim_start_matches("| ").split(' ').next().unwrap_or("");
        if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
            return Err(format!("{name}: the row must start with a YYYY-MM-DD date, found {date:?}"));
        }
        out.push((p, rows[0].to_string()));
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(out)
}

fn progress(check_only: bool) -> Result<(), String> {
    let frags = fragments()?;
    if check_only {
        println!("progress: {} fragments join", frags.len());
        return Ok(());
    }
    let roadmap = std::fs::read_to_string(root().join("ROADMAP.md")).map_err(|e| e.to_string())?;
    let mut in_log = false;
    let mut header_rows = 0;
    for line in roadmap.lines() {
        if line.starts_with("## ") {
            in_log = line == "## Progress log";
        }
        println!("{line}");
        if in_log && line.starts_with('|') {
            header_rows += 1;
            if header_rows == 2 {
                for (_, row) in &frags {
                    println!("{row}");
                }
            }
        }
    }
    Ok(())
}

fn version(args: &[String]) -> Result<(), String> {
    let path = root().join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut in_pkg = false;
    let mut out = String::new();
    let mut current = None;
    let new = match args.first().map(String::as_str) {
        Some("set") => Some(args.get(1).ok_or("usage: cargo xtask version set <x.y.z>")?.clone()),
        Some(other) => return Err(format!("unknown argument {other}")),
        None => None,
    };
    if let Some(v) = &new
        && !v.split(['.', '-', '+']).take(3).all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        return Err(format!("not a version: {v}"));
    }
    for line in text.lines() {
        if line.starts_with('[') {
            in_pkg = line == "[workspace.package]";
        }
        if in_pkg && line.starts_with("version = ") && current.is_none() {
            current = Some(line.trim_start_matches("version = ").trim_matches('"').to_string());
            if let Some(v) = &new {
                out.push_str(&format!("version = \"{v}\"\n"));
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    let current = current.ok_or("no [workspace.package] version")?;
    match new {
        Some(v) => {
            std::fs::write(&path, out).map_err(|e| e.to_string())?;
            println!("{current} -> {v}");
        }
        None => println!("{current}"),
    }
    Ok(())
}

/// Source texts of `tl!("…")` and `tlf!("…"` in the app.
fn source_strings() -> Vec<String> {
    let mut files = Vec::new();
    walk(&root().join("apps/boardcraft/src"), &mut files);
    let mut out = std::collections::BTreeSet::new();
    for f in files.iter().filter(|f| f.extension().is_some_and(|e| e == "rs") && !f.ends_with("ui_tests.rs")) {
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        // The shortcut catalog (keys.rs) translates its labels and menu names at run time:
        // `e("id", "Label", "Menu", &[…])` lines.
        for line in text.lines().map(str::trim).filter(|l| l.starts_with("e(\"")) {
            let quoted: Vec<&str> = line.split('"').collect();
            for i in [3, 5] {
                if let Some(s) = quoted.get(i) {
                    out.insert(s.to_string());
                }
            }
        }
        for pat in ["tl!(\"", "tlf!(\""] {
            let mut rest = text.as_str();
            while let Some(i) = rest.find(pat) {
                rest = &rest[i + pat.len()..];
                if let Some(end) = rest.find('"') {
                    out.insert(rest[..end].to_string());
                    rest = &rest[end..];
                }
            }
        }
    }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn strings(args: &[String]) -> Result<(), String> {
    let shared: std::collections::HashSet<String> = match args.first() {
        Some(dir) => {
            let common = Path::new(dir).join("locales/fr/common.tsv");
            let text = std::fs::read_to_string(&common).map_err(|e| format!("{}: {e}", common.display()))?;
            text.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split('\t').nth(1)).map(str::to_string).collect()
        }
        None => Default::default(),
    };
    let list = source_strings();
    let mut out = String::from(
        "# BoardCraft interface source texts (English is the key), generated by `cargo xtask strings`.\n# Columns: in-shared-catalog <TAB> source text. `shared` = already in craft-i18n common.tsv.\n",
    );
    for s in &list {
        out.push_str(if shared.contains(s) { "shared\t" } else { "app\t" });
        out.push_str(&s.replace('\t', "\\t"));
        out.push('\n');
    }
    let dir = root().join("docs/i18n");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("boardcraft-strings.tsv"), out).map_err(|e| e.to_string())?;
    println!("strings: {} source texts ({} already shared)", list.len(), list.iter().filter(|s| shared.contains(*s)).count());
    Ok(())
}

fn ci() -> Result<(), String> {
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])?;
    cargo(&["test", "--workspace"])?;
    layers()?;
    assets()?;
    licenses()?;
    progress(true)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or(&[]);
    let result = match args.first().map(String::as_str).unwrap_or("") {
        "ci" => ci(),
        "layers" => layers(),
        "assets" => assets(),
        "licenses" => licenses(),
        "progress" => progress(rest.first().is_some_and(|a| a == "--check")),
        "version" => version(rest),
        "strings" => strings(rest),
        _ => Err("usage: cargo xtask <ci|layers|assets|licenses|progress [--check]|version [set X]|strings [craft-i18n dir]>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::license_ok;

    #[test]
    fn spdx_expressions() {
        assert!(license_ok("MIT OR Apache-2.0"));
        assert!(license_ok("MIT/Apache-2.0"));
        assert!(license_ok("(MIT OR Apache-2.0) AND Unicode-3.0"));
        assert!(license_ok("Apache-2.0 WITH LLVM-exception OR MIT"));
        assert!(!license_ok("GPL-3.0"));
        assert!(!license_ok("MIT AND GPL-2.0"));
        assert!(license_ok("GPL-2.0 OR MIT"));
        assert!(!license_ok(""));
    }
}
