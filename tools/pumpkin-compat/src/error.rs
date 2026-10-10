use std::path::Path;

use thiserror::Error;

/// Failure from the compatibility client, a spawned server, or a scenario.
#[derive(Debug, Error)]
pub enum Error {
    /// Filesystem or process error.
    #[error("{context}: {source}")]
    Io {
        /// What the harness was trying to do.
        context: &'static str,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The scenario did not observe what it was waiting for.
    #[error("{0}")]
    Scenario(String),
    /// Vanilla jar download or launch could not be completed.
    #[error("{0}")]
    Vanilla(String),
}

impl Error {
    pub(crate) const fn io(context: &'static str, source: std::io::Error) -> Self {
        Self::Io { context, source }
    }

    pub(crate) fn scenario(message: impl Into<String>) -> Self {
        Self::Scenario(message.into())
    }

    pub(crate) fn vanilla(message: impl Into<String>) -> Self {
        Self::Vanilla(message.into())
    }

    pub(crate) fn missing_binary(path: &Path) -> Self {
        Self::scenario(format!(
            "Pumpkin binary not found at {}. Build it with `cargo build -p pumpkin` or set PUMPKIN_BIN.",
            path.display()
        ))
    }
}
