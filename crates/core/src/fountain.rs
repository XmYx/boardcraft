//! Fountain screenplay import: the hand-off from ScriptCraft.
//!
//! Implemented from the public Fountain syntax description (fountain.io). Only what a storyboard
//! needs is extracted: the title, scene headings (with `#12#` scene numbers), action paragraphs,
//! dialogue blocks (character, parenthetical, lines) and transitions. Notes (`[[ ]]`), boneyard
//! (`/* */`), sections (`#`), synopses (`=`), page breaks (`===`) and emphasis markers are
//! dropped. A note of the form `[[scene-id: <id>]]` inside a scene is kept as that scene's stable
//! id, so a later re-import can match boards to script scenes (see `docs/interop.md`).
//!
//! [`project_from_script`] turns a parsed [`Script`] into a board: one scene per heading, one
//! panel per action paragraph or dialogue block, with a duration estimated from the text.

use crate::model::{Project, Settings};

/// Longest screenplay accepted (bytes); bigger input is truncated at a line boundary.
pub const MAX_SCRIPT_BYTES: usize = 16 << 20;

/// One storyboard-relevant element of a scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Beat {
    Action(String),
    Dialogue { character: String, parenthetical: Option<String>, text: String },
    Transition(String),
}

/// A scene: heading, optional number and stable id, and its beats.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptScene {
    pub heading: String,
    pub number: Option<String>,
    pub scene_id: Option<String>,
    pub beats: Vec<Beat>,
}

/// A parsed screenplay.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    pub title: String,
    pub scenes: Vec<ScriptScene>,
}

const HEADING_PREFIXES: &[&str] = &["INT./EXT", "INT/EXT", "I/E", "INT", "EXT", "EST"];

fn is_heading(line: &str) -> bool {
    let up = line.trim_start().to_uppercase();
    HEADING_PREFIXES.iter().any(|p| {
        up.strip_prefix(p).is_some_and(|rest| {
            let rest = rest.strip_prefix('.').unwrap_or(rest);
            rest.starts_with(' ') || rest.is_empty() && up.len() > p.len()
        })
    })
}

/// Split `HEADING #12#` into the heading and its scene number.
fn split_scene_number(heading: &str) -> (String, Option<String>) {
    let h = heading.trim_end();
    if let Some(stripped) = h.strip_suffix('#')
        && let Some(start) = stripped.rfind('#')
    {
        let num = stripped.get(start + 1..).unwrap_or("").trim();
        let rest = stripped.get(..start).unwrap_or("").trim_end();
        if !num.is_empty() && num.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '.')) {
            return (rest.to_string(), Some(num.to_string()));
        }
    }
    (h.to_string(), None)
}

fn has_letters_all_upper(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic()) && !s.chars().any(|c| c.is_lowercase())
}

/// A character cue: all caps (ignoring an extension like `(V.O.)` and a dual-dialogue `^`), or
/// forced with `@`.
fn character_name(line: &str) -> Option<String> {
    let t = line.trim();
    if let Some(forced) = t.strip_prefix('@') {
        return (!forced.trim().is_empty()).then(|| forced.trim().trim_end_matches('^').trim().to_string());
    }
    let core = t.trim_end_matches('^').trim();
    let name = match core.find('(') {
        Some(i) => core.get(..i).unwrap_or("").trim(),
        None => core,
    };
    (has_letters_all_upper(name) && !t.starts_with('!') && !name.ends_with(':')).then(|| core.to_string())
}

fn is_transition(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with('>') {
        return !t.ends_with('<');
    }
    has_letters_all_upper(t) && t.ends_with("TO:")
}

/// Remove `/* boneyard */`, and `[[notes]]` (recording `[[scene-id: x]]` notes as markers).
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("/*") {
        out.push_str(rest.get(..i).unwrap_or(""));
        match rest.get(i + 2..).and_then(|r| r.find("*/").map(|j| i + 2 + j + 2)) {
            Some(end) => rest = rest.get(end..).unwrap_or(""),
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    let text = out;
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(i) = rest.find("[[") {
        out.push_str(rest.get(..i).unwrap_or(""));
        let after = rest.get(i + 2..).unwrap_or("");
        match after.find("]]") {
            Some(j) => {
                let note = after.get(..j).unwrap_or("").trim();
                if let Some(id) = note.strip_prefix("scene-id:") {
                    // Keep as a marker line the parser picks up.
                    out.push_str("\u{1}scene-id:");
                    out.push_str(id.trim());
                    out.push('\u{1}');
                }
                rest = after.get(j + 2..).unwrap_or("");
            }
            None => rest = "",
        }
    }
    out.push_str(rest);
    out
}

fn clean_inline(s: &str) -> String {
    s.replace("**", "").replace('*', "").replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Take the title page off the front, returning its `Title:` value and the rest.
fn split_title_page(text: &str) -> (String, &str) {
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let looks_like_key = first.split_once(':').is_some_and(|(k, _)| !k.is_empty() && k.len() < 30 && !k.contains(['.', '!']) && !is_heading(first));
    if !looks_like_key {
        return (String::new(), text);
    }
    let trimmed = text.trim_start_matches(['\n', ' ', '\t']);
    let end = trimmed.find("\n\n").unwrap_or(trimmed.len());
    let page = trimmed.get(..end).unwrap_or("");
    let mut title = String::new();
    let mut in_title = false;
    for line in page.lines() {
        let is_cont = line.starts_with(' ') || line.starts_with('\t');
        if !is_cont {
            in_title = false;
            if let Some((k, v)) = line.split_once(':')
                && k.trim().eq_ignore_ascii_case("title")
            {
                in_title = true;
                title = clean_inline(v);
            }
        } else if in_title {
            if !title.is_empty() {
                title.push(' ');
            }
            title.push_str(&clean_inline(line));
        }
    }
    (title, trimmed.get(end..).unwrap_or(""))
}

/// Parse Fountain text. Never fails: anything unrecognised is action.
pub fn parse(text: &str) -> Script {
    let mut text = text.replace("\r\n", "\n").replace('\r', "\n");
    if text.len() > MAX_SCRIPT_BYTES {
        let mut cut = MAX_SCRIPT_BYTES;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
    }
    let (title, body) = split_title_page(&text);
    let body = strip_comments(body);
    let mut script = Script { title, scenes: Vec::new() };

    // Blocks are runs of non-blank lines.
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in body.split('\n') {
        // A line of two spaces is a deliberate blank line inside dialogue; treat it as content.
        if line.trim().is_empty() && line != "  " {
            if !cur.is_empty() {
                blocks.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(line);
        }
    }
    if !cur.is_empty() {
        blocks.push(cur);
    }

    let mut scene: Option<ScriptScene> = None;
    // Text before the first scene heading (`FADE IN:`) belongs to no scene (ScriptCraft
    // docs/interop.md), so it makes no panels.
    let push_beat = |scene: &mut Option<ScriptScene>, beat: Beat| {
        if let Some(s) = scene.as_mut() {
            s.beats.push(beat);
        }
    };

    let mut pending_id: Option<String> = None;
    for block in blocks {
        // A `[[scene-id: x]]` note belongs to the scene whose heading is in or before this block.
        if let Some(id) = pending_id.take()
            && let Some(s) = scene.as_mut()
            && s.scene_id.is_none()
        {
            // The first `scene-id:` note after a heading wins (ScriptCraft docs/interop.md).
            s.scene_id = Some(id);
        }
        let mut lines: Vec<&str> = Vec::new();
        for l in block {
            if let Some(id) = l.trim().strip_prefix("\u{1}scene-id:").and_then(|r| r.strip_suffix('\u{1}')) {
                if pending_id.is_none() {
                    pending_id = Some(id.trim().to_string());
                }
                continue;
            }
            lines.push(l);
        }
        let Some(&first) = lines.first() else { continue };
        let ft = first.trim();
        if ft.starts_with("===") || ft.starts_with('#') || ft.starts_with('=') {
            continue; // page break, section, synopsis
        }
        let forced_heading = ft.starts_with('.') && !ft.starts_with("..");
        if forced_heading || is_heading(ft) {
            if let Some(s) = scene.take() {
                script.scenes.push(s);
            }
            let raw = if forced_heading { ft.get(1..).unwrap_or("") } else { ft };
            let (heading, number) = split_scene_number(raw);
            scene = Some(ScriptScene { heading: clean_inline(&heading).to_uppercase(), number, scene_id: None, beats: Vec::new() });
            let rest: Vec<&str> = lines.get(1..).unwrap_or(&[]).to_vec();
            if !rest.is_empty() {
                push_beat(&mut scene, Beat::Action(rest.iter().map(|l| clean_inline(l)).collect::<Vec<_>>().join("\n")));
            }
            continue;
        }
        if lines.len() == 1 && is_transition(ft) {
            let t = ft.trim_start_matches('>').trim().to_string();
            push_beat(&mut scene, Beat::Transition(t));
            continue;
        }
        if lines.len() > 1
            && let Some(character) = character_name(ft)
        {
            let mut parenthetical = None;
            let mut text = Vec::new();
            for l in lines.get(1..).unwrap_or(&[]) {
                let t = l.trim();
                if t.starts_with('(') && t.ends_with(')') {
                    if parenthetical.is_none() && text.is_empty() {
                        parenthetical = Some(t.trim_start_matches('(').trim_end_matches(')').trim().to_string());
                    } else {
                        text.push(t.to_string());
                    }
                } else {
                    text.push(clean_inline(t));
                }
            }
            push_beat(&mut scene, Beat::Dialogue { character, parenthetical, text: text.join(" ").trim().to_string() });
            continue;
        }
        let action: Vec<String> = lines
            .iter()
            .map(|l| {
                let t = l.trim();
                let t = t.strip_prefix('!').unwrap_or(t);
                let t = t.strip_prefix('~').unwrap_or(t);
                let t = if t.starts_with('>') && t.ends_with('<') { t.trim_start_matches('>').trim_end_matches('<') } else { t };
                clean_inline(t)
            })
            .filter(|l| !l.is_empty())
            .collect();
        if !action.is_empty() {
            push_beat(&mut scene, Beat::Action(action.join("\n")));
        }
    }
    if let Some(mut s) = scene {
        if let Some(id) = pending_id
            && s.scene_id.is_none()
        {
            s.scene_id = Some(id);
        }
        script.scenes.push(s);
    }
    script
}

/// Seconds a beat should hold on screen: about 2.5 spoken words a second for dialogue, a little
/// longer for busy action; 1.5 s minimum, 8 s maximum.
pub fn estimate_seconds(beat: &Beat) -> f32 {
    let words = |s: &str| s.split_whitespace().count() as f32;
    let s = match beat {
        Beat::Dialogue { text, .. } => words(text) / 2.5 + 0.5,
        Beat::Action(t) => 1.5 + words(t) / 6.0,
        Beat::Transition(_) => 1.0,
    };
    s.clamp(1.5, 8.0)
}

/// The panels one script scene boards to: one per action paragraph or dialogue block, with a
/// duration estimated from the text. Transitions become notes on the panel before them. A scene
/// with no beats gets one empty panel. Ids come from `project`.
pub fn scene_panels(project: &mut Project, scene: &ScriptScene) -> Vec<crate::model::Panel> {
    let fps = project.settings.fps_f64() as f32;
    let mut panels: Vec<crate::model::Panel> = Vec::new();
    for beat in &scene.beats {
        if let Beat::Transition(t) = beat
            && let Some(prev) = panels.last_mut()
        {
            if !prev.notes.is_empty() {
                prev.notes.push('\n');
            }
            prev.notes.push_str(t);
            continue;
        }
        let mut panel = project.new_panel();
        panel.duration = (estimate_seconds(beat) * fps).round().max(1.0) as u32;
        match beat {
            Beat::Action(t) => panel.action = t.clone(),
            Beat::Dialogue { character, parenthetical, text } => {
                panel.dialogue = match parenthetical {
                    Some(p) => format!("{character} ({p}): {text}"),
                    None => format!("{character}: {text}"),
                }
            }
            Beat::Transition(t) => panel.notes = t.clone(),
        }
        panels.push(panel);
    }
    if panels.is_empty() {
        panels.push(project.new_panel());
    }
    panels
}

/// How many panels [`scene_panels`] makes for `scene`.
pub fn scene_panel_count(scene: &ScriptScene) -> usize {
    let mut n = 0;
    for b in &scene.beats {
        if !matches!(b, Beat::Transition(_)) || n == 0 {
            n += 1;
        }
    }
    n.max(1)
}

/// Append the script's scenes to `project`. Returns how many scenes and panels were added.
pub fn append_to_project(project: &mut Project, script: &Script) -> (usize, usize) {
    let mut count = 0;
    for s in &script.scenes {
        let panels = scene_panels(project, s);
        count += panels.len();
        let scene_id = project.add_scene(&s.heading);
        if let Some(sc) = project.scene_mut(scene_id) {
            sc.panels = panels;
            if let Some(n) = &s.number {
                sc.number = n.clone();
            }
            sc.source_scene_id = s.scene_id.clone().unwrap_or_default();
        }
    }
    (script.scenes.len(), count)
}

/// A new board from a screenplay.
pub fn project_from_script(script: &Script, settings: Settings) -> Project {
    let name = if script.title.is_empty() { "Untitled board" } else { &script.title };
    let mut p = Project::new(name);
    p.settings = settings;
    append_to_project(&mut p, script);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Title: **The Lighthouse Keeper**\nCredit: Written by\nAuthor: A. Writer\n\nINT. LIGHTHOUSE - NIGHT #1#\n[[scene-id: s-001]]\n\nRain hammers the glass. MARA (60s) winds the great lamp.\n\nMARA\n(to herself)\nOne more night, old friend.\n\n/* cut this?\nBOB\nNo. */\n\nCUT TO:\n\nEXT. CLIFF EDGE - CONTINUOUS\n\n[[a plain note]]\nA ship's lights blink far out at sea.\n\n@McGREGOR\nShe's coming in too fast!\n\n.FLASHBACK\n\n# Act Two\n\n= synopsis line\n\n>THE END<\n";

    #[test]
    fn parses_scenes_dialogue_and_ids() {
        let s = parse(SAMPLE);
        assert_eq!(s.title, "The Lighthouse Keeper");
        assert_eq!(s.scenes.len(), 3);
        let a = &s.scenes[0];
        assert_eq!(a.heading, "INT. LIGHTHOUSE - NIGHT");
        assert_eq!(a.number.as_deref(), Some("1"));
        assert_eq!(a.scene_id.as_deref(), Some("s-001"));
        assert_eq!(a.beats.len(), 3);
        assert!(matches!(&a.beats[0], Beat::Action(t) if t.starts_with("Rain hammers")));
        assert_eq!(
            a.beats[1],
            Beat::Dialogue { character: "MARA".into(), parenthetical: Some("to herself".into()), text: "One more night, old friend.".into() }
        );
        assert_eq!(a.beats[2], Beat::Transition("CUT TO:".into()));
        let b = &s.scenes[1];
        assert_eq!(b.heading, "EXT. CLIFF EDGE - CONTINUOUS");
        assert!(matches!(&b.beats[1], Beat::Dialogue { character, .. } if character == "McGREGOR"));
        assert_eq!(s.scenes[2].heading, "FLASHBACK");
        assert!(matches!(&s.scenes[2].beats[0], Beat::Action(t) if t == "THE END"));
        // Boneyard content is gone.
        assert!(!s.scenes.iter().flat_map(|s| &s.beats).any(|b| matches!(b, Beat::Dialogue { character, .. } if character == "BOB")));
    }

    #[test]
    fn builds_a_board() {
        let p = project_from_script(&parse(SAMPLE), Settings::default());
        assert_eq!(p.name, "The Lighthouse Keeper");
        assert_eq!(p.scenes.len(), 3);
        assert_eq!(p.scenes[0].source_scene_id, "s-001");
        // Action, dialogue; the transition became a note on the dialogue panel.
        assert_eq!(p.scenes[0].panels.len(), 2);
        assert_eq!(p.scenes[0].panels[1].dialogue, "MARA (to herself): One more night, old friend.");
        assert_eq!(p.scenes[0].panels[1].notes, "CUT TO:");
        assert!(p.panels().all(|(_, x)| x.duration >= 36 && x.duration <= 8 * 24));
    }

    /// The shape ScriptCraft writes (its docs/interop.md): a title page, a section, a synopsis and
    /// `FADE IN:` before the first heading, `[[scene-id: <16 hex>]]` as its own paragraph after a
    /// heading, extensions on cues, a shot line. Hand-written for this test.
    #[test]
    fn scriptcraft_handoff_shape() {
        let text = "Title: THE TEST REEL\nAuthor: Someone\n\n# Act One\n\n= A synopsis.\n\nFADE IN:\n\n\
                    EXT. PIER - NIGHT #4A#\n\n= Pier synopsis.\n\n[[scene-id: 0123456789abcdef]]\n\n[[scene-id: fedcba9876543210]]\n\n\
                    Fog rolls in.\n\nANNA (V.O.)\n(quietly)\nStill here.\n\nCLOSE ON THE ROPE\n\nCUT TO:\n\n\
                    INT. SHED - CONTINUOUS\n\nAnna lights a lamp.\n";
        let s = parse(text);
        assert_eq!(s.title, "THE TEST REEL");
        assert_eq!(s.scenes.len(), 2, "FADE IN: before the first heading makes no scene");
        let pier = &s.scenes[0];
        assert_eq!(pier.number.as_deref(), Some("4A"));
        assert_eq!(pier.scene_id.as_deref(), Some("0123456789abcdef"), "the first scene-id note wins");
        assert_eq!(pier.beats.len(), 4);
        assert!(matches!(&pier.beats[1], Beat::Dialogue { character, parenthetical: Some(p), .. } if character == "ANNA (V.O.)" && p == "quietly"));
        assert!(matches!(&pier.beats[2], Beat::Action(t) if t == "CLOSE ON THE ROPE"));
        assert_eq!(s.scenes[1].scene_id, None);
        let board = project_from_script(&s, Settings::default());
        assert_eq!(board.scenes[0].source_scene_id, "0123456789abcdef");
        assert_eq!(board.scenes[0].number, "4A");
        assert_eq!(board.scenes[0].panels.len(), 3, "action, dialogue, shot; CUT TO: becomes a note");
    }

    #[test]
    fn headings_need_a_separator() {
        assert!(is_heading("INT. HOUSE"));
        assert!(is_heading("int house - day"));
        assert!(is_heading("I/E CAR"));
        assert!(!is_heading("INTERIOR DESIGN IS FUN"));
        assert!(!is_heading("EXTRA"));
    }

    #[test]
    fn hostile_input_does_not_panic() {
        for s in ["", "\n\n\n", "/*", "[[", "INT. #", "#", "@", "(x)\n(y)", ".\n.", "ÉÉÉ\nü", "Title:\n\tx", "A #1é#"] {
            let _ = parse(s);
        }
        let mut long = String::new();
        for _ in 0..2000 {
            long.push_str("EXT. FIELD\n\nBOB\nHi\n\n");
        }
        assert_eq!(parse(&long).scenes.len(), 2000);
    }
}
