use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Script,
    Style,
    Template,
    Wasm,
    SourceMap,
    Other,
}

pub trait Artifact {
    fn files(&self) -> &[GeneratedFile];

    fn primary(&self) -> Option<&GeneratedFile> {
        self.files().first()
    }

    fn total_bytes(&self) -> u64 {
        self.files().iter().map(|f| f.size_bytes).sum()
    }

    fn name(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct SimpleArtifact {
    pub name: String,
    pub files: Vec<GeneratedFile>,
}

impl SimpleArtifact {
    pub fn new(name: impl Into<String>, files: Vec<GeneratedFile>) -> Self {
        Self {
            name: name.into(),
            files,
        }
    }
}

impl Artifact for SimpleArtifact {
    fn files(&self) -> &[GeneratedFile] {
        &self.files
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn primary(&self) -> Option<&GeneratedFile> {
        pick_primary(&self.files)
    }
}

/// Pick the most natural "primary" file given a list. The JS bundle
/// wins, then the HTML template, then anything else. Useful as a
/// default for [`Artifact::primary`].
pub fn pick_primary(files: &[GeneratedFile]) -> Option<&GeneratedFile> {
    files
        .iter()
        .find(|f| matches!(f.kind, FileKind::Script))
        .or_else(|| files.iter().find(|f| matches!(f.kind, FileKind::Template)))
        .or_else(|| files.first())
}

/// Stat a file on disk and produce a [`GeneratedFile`]. Backends
/// typically wrap the [`std::io::Error`] in [`crate::BuildError::io`].
pub fn stat_file(path: &Path, kind: FileKind) -> std::io::Result<GeneratedFile> {
    let meta = std::fs::metadata(path)?;
    Ok(GeneratedFile {
        path: path.to_path_buf(),
        size_bytes: meta.len(),
        kind,
    })
}
