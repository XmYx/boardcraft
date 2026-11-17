//! Errors. Every fallible operation returns [`Result`]; nothing in this crate panics on bad input.

use std::path::PathBuf;

/// A BoardCraft error the UI (or an agent) can show and act on.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading or writing a file failed.
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The file is not a BoardCraft project, or it is damaged.
    #[error("not a valid BoardCraft project: {0}")]
    Format(String),
    /// The project was written by a newer BoardCraft.
    #[error("this project was saved by a newer BoardCraft (schema {found}); this build reads up to schema {supported}")]
    NewerSchema { found: u32, supported: u32 },
    /// An image could not be decoded or encoded.
    #[error("image: {0}")]
    Image(String),
    /// A feature that is not available in this build.
    #[error("not supported yet: {0}")]
    Unsupported(String),
    /// A sound file could not be decoded or encoded.
    #[error("audio: {0}")]
    Audio(String),
    /// A movie could not be encoded.
    #[error("video: {0}")]
    Video(String),
    /// The user cancelled a long operation.
    #[error("cancelled")]
    Cancelled,
    /// A reference (scene, panel, layer) that does not exist.
    #[error("not found: {0}")]
    NotFound(String),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}

/// `Result` with [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;
