use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Stats {
    #[serde(default)]
    sizes: HashMap<String, u64>,

    /// Frozen snapshot from disk so the UI can render +/- deltas
    /// even after `record` mutates `sizes` mid-build.
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
