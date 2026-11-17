//! BoardCraft document model and file formats.
//!
//! A BoardCraft [`Project`] is a list of [`Scene`]s; each scene holds [`Panel`]s. A panel is one
//! storyboard drawing (a stack of raster [`Layer`]s), its dialogue / action / notes text, a duration
//! in frames, and a [`Camera`] with pan/zoom keyframes. Played in order with their durations, the
//! panels are the animatic.
//!
//! - [`raster`]: straight-alpha RGBA8 canvases and the pressure-aware round brush and eraser.
//! - [`camera`]: camera keyframes and the eased pan/zoom between them.
//! - [`fountain`]: Fountain screenplay import (scene headings, action, dialogue), the hand-off from
//!   ScriptCraft.
//! - [`format`]: the open `.boardcraft` project file (a zip of `project.json` + one PNG per layer).
//! - [`otio`]: animatic timeline export to OpenTimelineIO, which FilmCraft imports, plus rendered
//!   panel stills.
//! - [`ai`]: a stubbed hook for generating panel drawings through the shared Craft AI layer.
//! - [`demo`]: a small procedurally drawn demo board.
//!
//! This crate has no UI dependency; the egui app lives in `apps/boardcraft`.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

pub mod ai;
pub mod camera;
pub mod demo;
pub mod error;
pub mod format;
pub mod fountain;
pub mod model;
pub mod otio;
pub mod raster;

pub use camera::{Camera, CameraKey, ViewRect};
pub use error::{Error, Result};
pub use model::{Id, Layer, Panel, Project, Scene, Settings};
pub use raster::{Brush, Raster, StrokePoint, Tool};

/// Name and version written into files this build creates.
pub const GENERATOR: &str = concat!("BoardCraft ", env!("CARGO_PKG_VERSION"));
