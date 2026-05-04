use std::{fs, path::PathBuf};

const CURRENT_FILE: &str = "current.txt";

pub fn cache_base() -> Option<PathBuf> {
    if let Ok(base) = std::env::var("INFINITY_MSFS_SDK_CACHE") {
        return Some(PathBuf::from(base));
    }
    default_sdk_cache_dir()
}

pub fn default_sdk_cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA").ok().map(|d| {
            PathBuf::from(d)
                .join("infinity-msfs")
                .join("sdk")
                .join("msfs2024")
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("XDG_CACHE_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".cache"))
            })
            .map(|base| base.join("infinity-msfs").join("sdk").join("msfs2024"))
    }
}

pub fn current_version(base: &std::path::Path) -> Option<String> {
    fs::read_to_string(base.join(CURRENT_FILE))
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn write_current_version(base: &std::path::Path, version: &str) -> std::io::Result<()> {
    fs::create_dir_all(base)?;
    fs::write(base.join(CURRENT_FILE), version)
}

pub fn sdk_path() -> Result<String, String> {
    if let Ok(sdk) = std::env::var("MSFS2024_SDK") {
        return Ok(sdk);
    }
    let base = cache_base().ok_or_else(|| {
        "could not determine SDK cache directory.\n\
         Set INFINITY_MSFS_SDK_CACHE or MSFS2024_SDK."
            .to_string()
    })?;
    let version = current_version(&base).ok_or_else(|| {
        "no MSFS 2024 SDK installed.\n\
         Run `infinity-msfs sdk install` to fetch it from Microsoft."
            .to_string()
    })?;
    let path = base.join(&version);
    if !path.exists() {
        return Err(format!(
            "current SDK version {version} not found at {}",
            path.display()
        ));
    }
    Ok(path.to_string_lossy().into_owned())
}
