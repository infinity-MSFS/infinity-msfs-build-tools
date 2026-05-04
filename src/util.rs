use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn find_project_root() -> Result<PathBuf> {
    let mut cur = std::env::current_dir()?;

    loop {
        let cargo = cur.join("Cargo.toml");
        let infinity = cur.join("infinity-msfs.toml");

        if cargo.exists() || infinity.exists() {
            return Ok(cur);
        }

        if !cur.pop() {
            bail!(
                "could not find project root (no Cargo.toml or infinity-msfs.toml found in parents)"
            );
        }
    }
}

pub fn config_path(root: &Path) -> PathBuf {
    root.join("infinity-msfs.toml")
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    Ok(())
}

pub fn copy_file(from: &Path, to: &Path) -> Result<u64> {
    ensure_parent_dir(to)?;

    fs::copy(from, to).with_context(|| {
        format!(
            "failed to copy file from {} to {}",
            from.display(),
            to.display()
        )
    })
}
