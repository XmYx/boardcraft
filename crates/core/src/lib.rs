//! BoardCraft document model and file formats.
//!
//! A BoardCraft [`Project`] is a list of [`Scene`]s; each scene holds [`Panel`]s. A panel is one
//! storyboard drawing (a stack of raster [`Layer`]s), its dialogue / action / notes text, a duration
//! in frames, and a [`Camera`] with pan/zoom keyframes. Played in order with their durations, the
//! panels are the animatic.
//!
//! - [`raster`]: straight-alpha RGBA8 canvases and the pressure-aware round brush and eraser.
//! - [`audio`]: the sound clips under the animatic: decoding (craft-audio), waveforms, mixing, and
//!   clips anchored to panels that follow them when the board is retimed.
//! - [`camera`]: camera keyframes and the eased pan/zoom between them.
//! - [`fountain`]: Fountain screenplay import (scene headings, action, dialogue), the hand-off from
//!   ScriptCraft.
//! - [`script`]: screenplays in Fountain, Final Draft (`.fdx`) and ScriptCraft's scene list (JSON),
//!   and re-import by stable scene id.
//! - [`format`]: the open `.boardcraft` project file (a zip of `project.json` + one PNG per layer).
//! - [`stills`]: rendered panel stills (the camera's first frame of every panel).
//! - [`command`]: every edit as a serializable command (app, CLI and tests share them).
//! - [`timeline`]: the animatic as an editorial timeline (OTIO, CMX 3600 EDL, FCP7 XML, FCPXML) through
//!   craft-timeline, and conform (timing back from an edit).
//! - [`movie`]: the animatic as an H.264/AAC MP4 with burn-ins (panel, timecode, dialogue).
//! - [`text`]: a small glyph rasterizer (skrifa outlines) for the burn-ins.
//! - [`sheet`]: printable board sheets (PDF) through craft-pdf.
//! - [`shotlist`]: the shot list as CSV.
//! - [`ai`]: a stubbed hook for generating panel drawings through the shared Craft AI layer.
//! - [`demo`]: a small procedurally drawn demo board.
//!
//! This crate has no UI dependency; the egui app lives in `apps/boardcraft`.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

pub mod ai;
pub mod audio;
pub mod camera;
pub mod command;
pub mod demo;
pub mod error;
pub mod format;
pub mod fountain;
pub mod model;
pub mod movie;
pub mod raster;
pub mod script;
pub mod sheet;
pub mod shotlist;
pub mod stills;
pub mod text;
pub mod timeline;

pub use camera::{Camera, CameraKey, ViewRect};
pub use error::{Error, Result};
pub use model::{Id, Layer, Panel, Project, Scene, Settings};
pub use raster::{Brush, Raster, StrokePoint, Tool};

/// Name and version written into files this build creates.
pub const GENERATOR: &str = concat!("BoardCraft ", env!("CARGO_PKG_VERSION"));
