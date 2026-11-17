//! Command-line arguments.

use std::path::PathBuf;

pub const USAGE: &str = "Usage: boardcraft [OPTIONS] [FILE]

FILE                     a .boardcraft project, or a .fountain screenplay to board

Options:
  --demo                 open the built-in demo board
  --view board|panel     start in the board grid (default) or the panel editor
  --screenshot <PNG>     render the window once to PNG and quit
  -h, --help             print this help
  -V, --version          print the version
";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartView {
    #[default]
    Board,
    Panel,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cli {
    pub demo: bool,
    pub view: StartView,
    pub screenshot: Option<PathBuf>,
    pub file: Option<PathBuf>,
}

#[derive(Debug, PartialEq)]
pub enum Launch {
    Run(Cli),
    Print(String),
}

/// `BoardCraft <version>`, plus the commit and date `packaging/env.sh` exports for release builds.
pub fn version() -> String {
    let mut v = boardcraft_core::GENERATOR.to_string();
    if let Some(sha) = option_env!("BOARDCRAFT_BUILD_SHA").filter(|s| !s.is_empty()) {
        v.push_str(&format!(" ({})", sha.get(..12).unwrap_or(sha)));
    }
    if let Some(date) = option_env!("BOARDCRAFT_BUILD_DATE").filter(|s| !s.is_empty()) {
        v.push_str(&format!(" {date}"));
    }
    v.push('\n');
    v
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Launch, String> {
    let mut cli = Cli::default();
    let mut it = args.into_iter();
    let mut only_files = false;
    while let Some(a) = it.next() {
        if only_files || !a.starts_with('-') || a == "-" {
            if cli.file.is_some() {
                return Err("only one file can be opened".into());
            }
            cli.file = Some(PathBuf::from(a));
            continue;
        }
        match a.as_str() {
            "--" => only_files = true,
            "--demo" => cli.demo = true,
            "--view" => {
                cli.view = match it.next().as_deref() {
                    Some("board") => StartView::Board,
                    Some("panel") => StartView::Panel,
                    other => return Err(format!("--view takes board or panel, not {:?}", other.unwrap_or(""))),
                }
            }
            "--screenshot" => cli.screenshot = Some(PathBuf::from(it.next().ok_or("--screenshot needs a file name")?)),
            "-h" | "--help" => return Ok(Launch::Print(USAGE.to_string())),
            "-V" | "--version" => return Ok(Launch::Print(version())),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(Launch::Run(cli))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Launch, String> {
        parse(s.split_whitespace().map(String::from))
    }

    #[test]
    fn options() {
        let Ok(Launch::Run(c)) = p("--demo --view panel --screenshot a.png") else { panic!() };
        assert!(c.demo);
        assert_eq!(c.view, StartView::Panel);
        assert_eq!(c.screenshot, Some(PathBuf::from("a.png")));
        assert!(matches!(p("--help"), Ok(Launch::Print(_))));
        assert!(p("--bogus").is_err());
        assert!(p("--view sideways").is_err());
        assert!(p("a b").is_err());
        let Ok(Launch::Run(c)) = p("-- --odd-name.boardcraft") else { panic!() };
        assert_eq!(c.file, Some(PathBuf::from("--odd-name.boardcraft")));
    }
}
