//! Hook for generating a panel drawing with the shared Craft AI layer — **stubbed**.
//!
//! The suite's AI layer (`craft-ai-core` in craft-libs, clients and provider adapters in
//! `storytold/craft-ai-integrations`) works by roles: an app declares a [`Role`] with a stable id,
//! one capability and its inputs; the user binds it once to a connection and model; the app then
//! plans, asks consent and starts a job through `craft-ai-client`.
//!
//! BoardCraft declares its role here ([`panel_image_role`]: capability `image.generate`, inputs
//! prompt + reference sketch) and builds the prompt from the panel's scene and text
//! ([`panel_prompt`]). [`generate_panel_image`] is the single place the client call will go; until
//! the client is wired in it returns [`Error::Unsupported`], never a substitute engine.

use craft_ai_core::{CapabilityId, InputRole, Role, RoleId};

use crate::error::{Error, Result};
use crate::model::{Id, Project};
use crate::raster::Raster;

/// Stable role id (agree suite-wide before shipping).
pub const PANEL_IMAGE_ROLE: &str = "boardcraft-panel-image";

/// The role BoardCraft declares to the shared AI layer. User-visible label follows the suite's
/// naming rule ("AI …", never a vendor name).
pub fn panel_image_role() -> Result<Role> {
    let id = RoleId::new(PANEL_IMAGE_ROLE).map_err(|e| Error::Unsupported(format!("role id: {e}")))?;
    let capability = CapabilityId::parse("image.generate").map_err(|e| Error::Unsupported(format!("capability: {e}")))?;
    Ok(Role { id, label: "AI panel sketch".into(), capability, inputs: vec![InputRole::Prompt, InputRole::Reference] })
}

/// What a generation request carries.
#[derive(Clone, Debug)]
pub struct PanelImageRequest {
    pub panel: Id,
    pub prompt: String,
    /// The panel's current drawing, as a composition reference.
    pub reference: Raster,
    pub width: u32,
    pub height: u32,
}

/// Prompt for a panel: storyboard style, scene heading, action and dialogue.
pub fn panel_prompt(project: &Project, panel: Id) -> Option<String> {
    let scene = project.scene_of(panel)?;
    let p = project.panel(panel)?;
    let mut prompt = String::from("Rough black-and-white storyboard sketch, clear staging, 16:9 frame.");
    if !scene.heading.is_empty() {
        prompt.push_str(&format!(" Scene: {}.", scene.heading));
    }
    if !p.action.is_empty() {
        prompt.push_str(&format!(" Action: {}", p.action.replace('\n', " ")));
    }
    if !p.dialogue.is_empty() {
        prompt.push_str(&format!(" Moment: {}", p.dialogue));
    }
    Some(prompt)
}

/// Build the request for a panel.
pub fn panel_request(project: &Project, panel: Id) -> Result<PanelImageRequest> {
    let prompt = panel_prompt(project, panel).ok_or_else(|| Error::NotFound(format!("panel {}", panel.0)))?;
    let p = project.panel(panel).ok_or_else(|| Error::NotFound(format!("panel {}", panel.0)))?;
    Ok(PanelImageRequest { panel, prompt, reference: p.composite(&project.settings), width: project.settings.width, height: project.settings.height })
}

/// Run the request. **Stub:** the shared AI client is not wired in yet.
pub fn generate_panel_image(_request: &PanelImageRequest) -> Result<Raster> {
    Err(Error::Unsupported("AI panel sketch needs the shared Craft AI client (craft-ai-integrations), which BoardCraft does not include yet".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_is_well_formed() {
        let r = panel_image_role().unwrap();
        assert_eq!(r.capability.as_str(), "image.generate");
        assert_eq!(r.id.as_str(), PANEL_IMAGE_ROLE);
    }

    #[test]
    fn prompt_and_stub() {
        let p = crate::demo::demo_project();
        let id = p.scenes[0].panels[0].id;
        let req = panel_request(&p, id).unwrap();
        assert!(req.prompt.contains("Scene:"));
        assert!(matches!(generate_panel_image(&req), Err(Error::Unsupported(_))));
    }
}
