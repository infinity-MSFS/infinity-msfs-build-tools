use anyhow::{Context, Result, anyhow, bail};
use cargo_metadata::{Metadata, MetadataCommand, Package};
use std::path::Path;

pub fn load_metadata(root: &Path) -> Result<Metadata> {
    let mut cmd = MetadataCommand::new();
    cmd.current_dir(root);

    cmd.exec()
        .with_context(|| format!("failed to read cargo metadata in {}", root.display()))
}

pub fn resolve_package<'a>(
    metadata: &'a Metadata,
    requested_package: Option<&str>,
) -> Result<&'a Package> {
    if let Some(name) = requested_package {
        return metadata
            .packages
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| anyhow!("package '{name}' not found in workspace"));
    }

    if let Some(root_id) = &metadata.resolve.as_ref().and_then(|r| r.root.as_ref()) {
        if let Some(pkg) = metadata.packages.iter().find(|p| &p.id == *root_id) {
            return Ok(pkg);
        }
    }

    let workspace_members: Vec<&Package> = metadata
        .packages
        .iter()
        .filter(|p| metadata.workspace_members.contains(&p.id))
        .collect();

    if workspace_members.len() == 1 {
        return Ok(workspace_members[0]);
    }

    bail!(
        "could not determine target package automatically; this appears to be a workspace root. Supply -p <package> or set [build].package in infinity-msfs.toml"
    );
}

pub fn resolve_bin_name(pkg: &Package, configured_bin: Option<&str>) -> String {
    if let Some(bin) = configured_bin {
        return bin.to_string();
    }

    pkg.name.replace('-', "_")
}
