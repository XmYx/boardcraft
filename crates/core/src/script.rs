//! Screenplay input in every format BoardCraft reads, and re-import by scene id.
//!
//! - Fountain (`.fountain`): [`crate::fountain`].
//! - ScriptCraft's scene list (`.scenes.json`, format `scriptcraft-scenes` v1): [`parse_scene_list`].
//! - Final Draft (`.fdx`, XML): [`parse_fdx`], from the publicly documented FDX structure
//!   (`FinalDraft` → `Content` → `Paragraph Type="…"` → `Text` runs; scene numbers in the scene
//!   heading's `Number` attribute).
//!
//! All three read into the same [`Script`]. [`reimport`] merges a script into an existing board:
//! scenes are matched by their stable id (`[[scene-id: …]]`, the scene list's `id`), then by an
//! identical heading; matched scenes keep their drawings, cameras and timing and take the script's
//! heading, number and panel text; new scenes are added in script order; board scenes the script no
//! longer has are kept after the others and reported.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::fountain::{self, Beat, Script, ScriptScene};
use crate::model::Project;

/// Largest script file accepted.
pub const MAX_SCRIPT_FILE: usize = 64 << 20;

/// A screenplay file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptFormat {
    Fountain,
    FinalDraft,
    SceneList,
}

impl ScriptFormat {
    /// From the file name, then the content.
    pub fn detect(name: &str, bytes: &[u8]) -> ScriptFormat {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".fdx") {
            return ScriptFormat::FinalDraft;
        }
        if lower.ends_with(".json") {
            return ScriptFormat::SceneList;
        }
        let head = String::from_utf8_lossy(bytes.get(..bytes.len().min(4096)).unwrap_or(&[])).into_owned();
        if head.contains("<FinalDraft") {
            ScriptFormat::FinalDraft
        } else if head.trim_start().starts_with('{') && head.contains("scriptcraft-scenes") {
            ScriptFormat::SceneList
        } else {
            ScriptFormat::Fountain
        }
    }
}

/// Read a screenplay in any supported format.
pub fn parse_script(name: &str, bytes: &[u8]) -> Result<Script> {
    parse_script_as(ScriptFormat::detect(name, bytes), name, bytes)
}

/// Read a screenplay in a known format.
pub fn parse_script_as(format: ScriptFormat, name: &str, bytes: &[u8]) -> Result<Script> {
    if bytes.len() > MAX_SCRIPT_FILE {
        return Err(Error::Format(format!("{name}: screenplay larger than {} MB", MAX_SCRIPT_FILE >> 20)));
    }
    match format {
        ScriptFormat::Fountain => Ok(fountain::parse(&String::from_utf8_lossy(bytes))),
        ScriptFormat::FinalDraft => parse_fdx(bytes),
        ScriptFormat::SceneList => parse_scene_list(bytes),
    }
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

/// ScriptCraft's scene list (`scriptcraft-scenes`, version 1; ScriptCraft `docs/interop.md`).
pub fn parse_scene_list(bytes: &[u8]) -> Result<Script> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| Error::Format(format!("scene list: {e}")))?;
    if v.get("format").and_then(Value::as_str) != Some("scriptcraft-scenes") {
        return Err(Error::Format("not a ScriptCraft scene list (format is not \"scriptcraft-scenes\")".into()));
    }
    let version = v.get("version").and_then(Value::as_u64).unwrap_or(0);
    if version != 1 {
        return Err(Error::Format(format!("scene list version {version}; BoardCraft reads version 1")));
    }
    let mut script = Script { title: s(&v, "title").unwrap_or_default(), scenes: Vec::new() };
    for sc in v.get("scenes").and_then(Value::as_array).into_iter().flatten() {
        let mut scene = ScriptScene { heading: s(sc, "heading").unwrap_or_default(), number: s(sc, "number"), scene_id: s(sc, "id"), beats: Vec::new() };
        for b in sc.get("beats").and_then(Value::as_array).into_iter().flatten() {
            let text = s(b, "text").unwrap_or_default();
            let beat = match b.get("kind").and_then(Value::as_str).unwrap_or("action") {
                "dialogue" | "lyric" => {
                    let character = s(b, "cue").or_else(|| s(b, "character")).unwrap_or_default();
                    let parenthetical = s(b, "parenthetical").map(|p| p.trim_start_matches('(').trim_end_matches(')').trim().to_string());
                    Beat::Dialogue { character, parenthetical, text }
                }
                "transition" => Beat::Transition(text),
                _ => Beat::Action(text),
            };
            scene.beats.push(beat);
        }
        script.scenes.push(scene);
    }
    Ok(script)
}

/// Final Draft `.fdx`.
pub fn parse_fdx(bytes: &[u8]) -> Result<Script> {
    let root = craft_xml::parse(bytes).map_err(|e| Error::Format(format!("FDX: {e}")))?;
    if root.local() != "FinalDraft" {
        return Err(Error::Format("not a Final Draft document (no FinalDraft root)".into()));
    }
    let mut script = Script::default();
    // Title page: the first non-empty paragraph of TitlePage, if any.
    if let Some(tp) = root.child("TitlePage").and_then(|t| t.child("Content")) {
        script.title = tp.children("Paragraph").map(|p| p.deep_text().trim().to_string()).find(|t| !t.is_empty()).unwrap_or_default();
    }
    let content = root.child("Content").ok_or_else(|| Error::Format("FDX: no Content".into()))?;
    let mut scene: Option<ScriptScene> = None;
    let mut pending_character: Option<String> = None;
    let mut pending_paren: Option<String> = None;
    for p in content.children("Paragraph") {
        let text: String = p.children("Text").map(|t| t.deep_text()).collect::<String>().trim().to_string();
        let kind = p.attr("Type").unwrap_or("Action");
        if kind == "Scene Heading" {
            if let Some(s) = scene.take() {
                script.scenes.push(s);
            }
            let number = p.attr("Number").map(str::to_string).filter(|n| !n.is_empty());
            scene = Some(ScriptScene { heading: text.to_uppercase(), number, scene_id: None, beats: Vec::new() });
            pending_character = None;
            continue;
        }
        let Some(sc) = scene.as_mut() else { continue }; // before the first heading: no scene
        match kind {
            "Character" => {
                pending_character = Some(text);
                pending_paren = None;
            }
            "Parenthetical" => pending_paren = Some(text.trim_start_matches('(').trim_end_matches(')').trim().to_string()),
            "Dialogue" => {
                let character = pending_character.clone().unwrap_or_default();
                // Continued dialogue after a parenthetical joins the same speech.
                if let Some(Beat::Dialogue { character: c, text: t, .. }) = sc.beats.last_mut()
                    && *c == character
                    && pending_paren.is_none()
                    && pending_character.is_none()
                {
                    t.push(' ');
                    t.push_str(&text);
                } else {
                    sc.beats.push(Beat::Dialogue { character, parenthetical: pending_paren.take(), text });
                }
                pending_character = None;
            }
            "Transition" => sc.beats.push(Beat::Transition(text)),
            _ if text.is_empty() => {}
            _ => sc.beats.push(Beat::Action(text)), // Action, Shot, General, …
        }
    }
    if let Some(s) = scene {
        script.scenes.push(s);
    }
    Ok(script)
}

/// What a re-import changed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ReimportReport {
    /// Scenes matched by stable id.
    pub matched_by_id: usize,
    /// Scenes matched by an identical heading (no id on one side).
    pub matched_by_heading: usize,
    /// Scenes the script added.
    pub added_scenes: usize,
    /// Panels added to matched scenes (the script has more beats now).
    pub added_panels: usize,
    /// Panels whose dialogue or action changed.
    pub updated_panels: usize,
    /// Panels kept although their scene has fewer beats now (they keep their drawings).
    pub extra_panels: usize,
    /// Board scenes the script no longer has (kept, moved after the others).
    pub orphaned: Vec<String>,
}

impl ReimportReport {
    pub fn summary(&self) -> String {
        format!(
            "{} scenes matched by id, {} by heading, {} added; {} panels updated, {} added, {} extra kept; {} scenes no longer in the script",
            self.matched_by_id,
            self.matched_by_heading,
            self.added_scenes,
            self.updated_panels,
            self.added_panels,
            self.extra_panels,
            self.orphaned.len()
        )
    }
}

/// Merge `script` into `project` (see the module docs). Never deletes drawings.
pub fn reimport(project: &mut Project, script: &Script) -> ReimportReport {
    let mut report = ReimportReport::default();
    let mut old = std::mem::take(&mut project.scenes);
    let mut used: HashSet<usize> = HashSet::new();
    let mut ordered = Vec::new();
    for s in &script.scenes {
        let by_id = s
            .scene_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .and_then(|id| old.iter().enumerate().position(|(i, o)| !used.contains(&i) && o.source_scene_id == id));
        let by_heading = if by_id.is_none() {
            old.iter().enumerate().position(|(i, o)| {
                !used.contains(&i)
                    && !o.heading.is_empty()
                    && o.heading.eq_ignore_ascii_case(&s.heading)
                    && (o.source_scene_id.is_empty() || s.scene_id.is_none())
            })
        } else {
            None
        };
        let fresh = fountain::scene_panels(project, s);
        match by_id.or(by_heading) {
            Some(i) => {
                used.insert(i);
                if by_id.is_some() {
                    report.matched_by_id += 1;
                } else {
                    report.matched_by_heading += 1;
                }
                let Some(scene) = old.get_mut(i) else { continue };
                scene.heading.clone_from(&s.heading);
                if let Some(n) = &s.number {
                    scene.number.clone_from(n);
                }
                if let Some(id) = &s.scene_id {
                    scene.source_scene_id.clone_from(id);
                }
                let had = scene.panels.len();
                for (k, f) in fresh.into_iter().enumerate() {
                    match scene.panels.get_mut(k) {
                        Some(p) => {
                            if p.dialogue != f.dialogue || p.action != f.action {
                                report.updated_panels += 1;
                                p.dialogue = f.dialogue;
                                p.action = f.action;
                            }
                            if p.notes.is_empty() {
                                p.notes = f.notes;
                            }
                        }
                        None => {
                            report.added_panels += 1;
                            scene.panels.push(f);
                        }
                    }
                }
                report.extra_panels += had.saturating_sub(fountain::scene_panel_count(s));
                ordered.push(std::mem::replace(scene, placeholder()));
            }
            None => {
                report.added_scenes += 1;
                let id = project.alloc();
                ordered.push(crate::model::Scene {
                    id,
                    number: s.number.clone().unwrap_or_default(),
                    heading: s.heading.clone(),
                    source_scene_id: s.scene_id.clone().unwrap_or_default(),
                    panels: fresh,
                });
            }
        }
    }
    for (i, scene) in old.into_iter().enumerate() {
        if !used.contains(&i) {
            report.orphaned.push(if scene.heading.is_empty() { format!("scene {}", scene.number) } else { scene.heading.clone() });
            ordered.push(scene);
        }
    }
    // Number scenes that have no number of their own by position.
    for (i, s) in ordered.iter_mut().enumerate() {
        if s.number.is_empty() {
            s.number = (i + 1).to_string();
        }
    }
    project.scenes = ordered;
    report
}

fn placeholder() -> crate::model::Scene {
    crate::model::Scene { id: crate::model::Id(0), number: String::new(), heading: String::new(), source_scene_id: String::new(), panels: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Settings;
    use crate::raster::Brush;

    const V1: &str = "INT. A - DAY\n\n[[scene-id: aaaa000000000001]]\n\nBob enters.\n\nBOB\nHello.\n\nEXT. B - NIGHT\n\n[[scene-id: bbbb000000000002]]\n\nRain.\n\nINT. C - DAY\n\nA clock.\n";
    // Scene B renamed and moved first, A gets a new line, C removed, D added (no id).
    const V2: &str = "EXT. B - DAWN\n\n[[scene-id: bbbb000000000002]]\n\nRain stops.\n\nINT. A - DAY\n\n[[scene-id: aaaa000000000001]]\n\nBob enters.\n\nBOB\nHello there.\n\nANNA\nHi.\n\nINT. D - DAY\n\nNew.\n";

    #[test]
    fn reimport_keeps_drawings_and_follows_the_script() {
        let mut p = fountain::project_from_script(&fountain::parse(V1), Settings { width: 64, height: 36, fps: 24, ntsc: false });
        // Draw on scene A's first panel and retime it.
        let a0 = p.scenes[0].panels[0].id;
        p.panel_mut(a0).unwrap().layers[0].pixels.polyline(&[(2.0, 2.0), (60.0, 30.0)], &Brush::default());
        p.panel_mut(a0).unwrap().duration = 99;
        let r = reimport(&mut p, &fountain::parse(V2));
        assert_eq!(r.matched_by_id, 2);
        assert_eq!(r.added_scenes, 1);
        assert_eq!(r.orphaned, vec!["INT. C - DAY".to_string()]);
        let headings: Vec<&str> = p.scenes.iter().map(|s| s.heading.as_str()).collect();
        assert_eq!(headings, ["EXT. B - DAWN", "INT. A - DAY", "INT. D - DAY", "INT. C - DAY"]);
        let a = &p.scenes[1];
        assert_eq!(a.panels[0].id, a0, "the drawn panel is the same panel");
        assert_eq!(a.panels[0].duration, 99);
        assert!(!a.panels[0].layers[0].pixels.is_blank());
        assert_eq!(a.panels[1].dialogue, "BOB: Hello there.");
        assert_eq!(a.panels.len(), 3);
        assert_eq!(r.added_panels, 1);
        assert_eq!(r.updated_panels, 2, "B's action and A's dialogue changed");
        // A second identical import changes nothing.
        let r2 = reimport(&mut p, &fountain::parse(V2));
        assert_eq!((r2.updated_panels, r2.added_panels, r2.added_scenes), (0, 0, 0));
    }

    #[test]
    fn heading_match_without_ids() {
        let mut p = fountain::project_from_script(&fountain::parse("INT. X\n\nOne.\n"), Settings::default());
        let id = p.scenes[0].panels[0].id;
        let r = reimport(&mut p, &fountain::parse("INT. X\n\nOne, changed.\n"));
        assert_eq!(r.matched_by_heading, 1);
        assert_eq!(p.scenes[0].panels[0].id, id);
        assert_eq!(p.scenes[0].panels[0].action, "One, changed.");
    }

    #[test]
    fn scene_list_json() {
        let json = br#"{"format":"scriptcraft-scenes","version":1,"title":"T","scenes":[
            {"index":1,"number":"1","id":"0123456789abcdef","heading":"INT. ROOM - DAY","beats":[
              {"kind":"action","text":"A lamp."},
              {"kind":"dialogue","text":"Hi.","character":"MAYA","cue":"MAYA (V.O.)","parenthetical":"(softly)"},
              {"kind":"shot","text":"CLOSE ON THE LAMP"},
              {"kind":"transition","text":"CUT TO:"}]}]}"#;
        let s = parse_script("x.scenes.json", json).unwrap();
        assert_eq!(s.scenes[0].scene_id.as_deref(), Some("0123456789abcdef"));
        assert_eq!(s.scenes[0].beats[1], Beat::Dialogue { character: "MAYA (V.O.)".into(), parenthetical: Some("softly".into()), text: "Hi.".into() });
        assert_eq!(s.scenes[0].beats.len(), 4);
        assert!(parse_scene_list(br#"{"format":"scriptcraft-scenes","version":2,"scenes":[]}"#).is_err());
        assert!(parse_scene_list(br#"{"format":"other","version":1}"#).is_err());
        assert!(parse_scene_list(b"[").is_err());
    }

    #[test]
    fn final_draft_fdx() {
        let fdx = br#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="5">
  <Content>
    <Paragraph Type="Action"><Text>FADE IN:</Text></Paragraph>
    <Paragraph Number="4" Type="Scene Heading"><SceneProperties Length="1/8" Page="1"/><Text>int. kitchen - night</Text></Paragraph>
    <Paragraph Type="Action"><Text>The kettle </Text><Text Style="Bold">screams</Text><Text>.</Text></Paragraph>
    <Paragraph Type="Character"><Text>LENA</Text></Paragraph>
    <Paragraph Type="Parenthetical"><Text>(tired)</Text></Paragraph>
    <Paragraph Type="Dialogue"><Text>Not again.</Text></Paragraph>
    <Paragraph Type="Shot"><Text>ANGLE ON THE STOVE</Text></Paragraph>
    <Paragraph Type="Transition"><Text>SMASH CUT TO:</Text></Paragraph>
    <Paragraph Type="Scene Heading"><Text>EXT. STREET - NIGHT</Text></Paragraph>
  </Content>
  <TitlePage><Content><Paragraph><Text>KETTLE</Text></Paragraph></Content></TitlePage>
</FinalDraft>"#;
        let s = parse_script("k.fdx", fdx).unwrap();
        assert_eq!(s.title, "KETTLE");
        assert_eq!(s.scenes.len(), 2);
        assert_eq!(s.scenes[0].heading, "INT. KITCHEN - NIGHT");
        assert_eq!(s.scenes[0].number.as_deref(), Some("4"));
        assert_eq!(s.scenes[0].beats[0], Beat::Action("The kettle screams.".into()));
        assert_eq!(s.scenes[0].beats[1], Beat::Dialogue { character: "LENA".into(), parenthetical: Some("tired".into()), text: "Not again.".into() });
        assert_eq!(s.scenes[0].beats[3], Beat::Transition("SMASH CUT TO:".into()));
        assert!(parse_fdx(b"<html/>").is_err());
        assert!(parse_fdx(b"<FinalDraft><Con").is_err());
    }

    #[test]
    fn detection() {
        assert_eq!(ScriptFormat::detect("a.txt", b"<?xml?><FinalDraft>"), ScriptFormat::FinalDraft);
        assert_eq!(ScriptFormat::detect("a", b"{\"format\":\"scriptcraft-scenes\"}"), ScriptFormat::SceneList);
        assert_eq!(ScriptFormat::detect("a.fountain", b"INT. X"), ScriptFormat::Fountain);
    }
}
