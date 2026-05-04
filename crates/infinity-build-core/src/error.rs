use std::path::PathBuf;
use thiserror::Error;

pub type BuildResult<T> = Result<T, BuildError>;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("invalid path {path}: {reason}")]
    InvalidPath { path: PathBuf, reason: String },

    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("{stage} failed: {message}")]
    BackendFailure {
        stage: &'static str,
        message: String,
    },

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Backend(#[from] anyhow::Error),
}

impl BuildError {
    pub fn invalid_path(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::InvalidPath {
            path: path.into(),
            reason: reason.into(),
        }
    }

    pub fn invalid_config(msg: impl Into<String>) -> Self {
        Self::InvalidConfig(msg.into())
    }

    pub fn backend_failure(stage: &'static str, message: impl Into<String>) -> Self {
        Self::BackendFailure {
            stage,
            message: message.into(),
        }
    }

    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
