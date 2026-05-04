//! Lightweight per-build stats persistence.
//!
//! Currently we only store the final artefact size for each package so the
//! UI can render a `+/- N KB` delta on the next build. The file lives at
//! `target/.infinity-msfs-stats.json` (relative to the project root) and
//! is best-effort: failures to read or write are surfaced as warnings,
//! never as build failures.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Stats {
    /// Map of cargo package name → most recent artefact size in bytes.
    #[serde(default)]
    sizes: HashMap<String, u64>,

    #[serde(skip)]
    previous: HashMap<String, u64>,
}

impl Stats {
    pub fn load(root: &Path) -> Self {
        let path = stats_path(root);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return Self::default(),
        };
        let mut stats: Self = serde_json::from_str(&text).unwrap_or_default();
        stats.previous = stats.sizes.clone();
        stats
    }

    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        let path = stats_path(root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        fs::write(path, json)
    }

    /// Size recorded on the previous successful build, if any. The
    /// in-memory `sizes` map is mutated by [`record`] during this run, so
    /// we keep a frozen `previous` snapshot for the delta calculation.
    pub fn previous_size(&self, package: &str) -> Option<u64> {
        self.previous.get(package).copied()
    }

    pub fn record(&mut self, package: &str, size_bytes: u64) {
        self.sizes.insert(package.to_string(), size_bytes);
    }
}

fn stats_path(root: &Path) -> PathBuf {
    root.join("target").join(".infinity-msfs-stats.json")
}
