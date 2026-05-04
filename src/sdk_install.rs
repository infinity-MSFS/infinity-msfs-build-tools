use crate::cli::SdkInstallArgs;
use crate::sdk::{cache_base, current_version, write_current_version};
use anyhow::{Context, Result, bail};
use cab::Cabinet;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use msi::{Expr, Package, Row, Select};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use zip::ZipArchive;

const MSFS2024_SDK_URL: &str = "https://sdk.flightsimulator.com/msfs2024/files/";
const MANIFEST_FILE: &str = "sdk.json";
const CORE_INSTALLER_KEY: &str = "SDK Installer (Core)";

#[cfg(target_os = "windows")]
const SDK_EXTRACT_FROM: &str = ".\\MSFS 2024 SDK\\";
#[cfg(not(target_os = "windows"))]
const SDK_EXTRACT_FROM: &str = "./MSFS 2024 SDK/";

const KEEP_PREFIXES: &[&str] = &[
    "WASM/include/",
    "WASM/wasi-sysroot/",
    "WASM/lib/",
    "SimConnect SDK/include/",
    "SimConnect SDK/lib/",
];

#[derive(Debug, Deserialize)]
struct DownloadOption {
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GameVersion {
    downloads_menu: HashMap<String, DownloadOption>,
    release_notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SdkManifest {
    game_versions: Vec<GameVersion>,
}

pub fn run_install(args: SdkInstallArgs) -> Result<()> {
    let base = cache_base().context(
        "could not determine SDK cache directory.\n\
         Set INFINITY_MSFS_SDK_CACHE to an explicit path and retry.",
    )?;

    println!(
        "{} Fetching MSFS 2024 SDK manifest…",
        style("→").cyan().bold()
    );
    let manifest_text = http_text(&format!("{MSFS2024_SDK_URL}{MANIFEST_FILE}"))?;
    let manifest: SdkManifest =
        serde_json::from_str(&manifest_text).context("invalid SDK manifest JSON")?;

    let release = manifest
        .game_versions
        .first()
        .context("manifest contains no game versions")?;
    let version = release
        .release_notes
        .first()
        .context("manifest release_notes is empty")?
        .clone();

    let dest = base.join(&version);
    if dest.exists() && !args.force {
        println!(
            "{} SDK {} already installed at {}",
            style("✓").green().bold(),
            style(&version).bold(),
            style(dest.display()).dim(),
        );
        write_current_version(&base, &version)?;
        return Ok(());
    }

    let download_path = release
        .downloads_menu
        .get(CORE_INSTALLER_KEY)
        .and_then(|o| o.value.as_ref())
        .context("manifest is missing 'SDK Installer (Core)' entry")?;
    let download_url = format!("{MSFS2024_SDK_URL}{download_path}");

    println!(
        "{} Downloading SDK {} from Microsoft",
        style("→").cyan().bold(),
        style(&version).bold(),
    );
    println!("  URL : {}", style(&download_url).dim());
    println!("  Dest: {}", style(dest.display()).dim());

    let bytes = http_download_with_progress(&download_url)?;

    if dest.exists() {
        fs::remove_dir_all(&dest)
            .with_context(|| format!("failed to remove existing {}", dest.display()))?;
    }
    fs::create_dir_all(&dest)?;

    println!("{} Extracting installer…", style("→").cyan().bold());
    extract_msi(&bytes, download_path.ends_with(".zip"), &dest)?;

    write_current_version(&base, &version)?;

    println!(
        "{} SDK {} installed at {}",
        style("✓").green().bold(),
        style(&version).bold(),
        style(dest.display()).dim(),
    );
    Ok(())
}

pub fn run_path() -> Result<()> {
    match crate::sdk::sdk_path() {
        Ok(p) => {
            println!("{p}");
            Ok(())
        }
        Err(e) => bail!(e),
    }
}

pub fn run_remove() -> Result<()> {
    let base = cache_base().context("no cache directory configured")?;
    if !base.exists() {
        println!("{} nothing to remove", style("✓").green().bold());
        return Ok(());
    }
    fs::remove_dir_all(&base).with_context(|| format!("failed to remove {}", base.display()))?;
    println!(
        "{} removed {}",
        style("✓").green().bold(),
        style(base.display()).dim(),
    );
    Ok(())
}

pub fn ensure_sdk() -> Result<()> {
    if std::env::var("MSFS2024_SDK").is_ok() {
        return Ok(());
    }
    if let Some(base) = cache_base() {
        if let Some(v) = current_version(&base) {
            if base.join(v).exists() {
                return Ok(());
            }
        }
    }

    let term = console::Term::stderr();
    if !term.is_term() {
        bail!(
            "MSFS 2024 SDK is not installed.\n\
             Run `infinity-msfs sdk install` to download it,\n\
             or set MSFS2024_SDK to an existing SDK installation."
        );
    }

    eprintln!(
        "{} MSFS 2024 SDK is not installed.",
        style("!").yellow().bold()
    );
    eprint!("  Download and install it now from sdk.flightsimulator.com? [Y/n] ");
    use std::io::Write as _;
    std::io::stderr().flush().ok();

    let answer = term.read_line().unwrap_or_default();
    let proceed = matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    );
    if !proceed {
        bail!(
            "aborted. Run `infinity-msfs sdk install` to install the SDK,\n\
             or set MSFS2024_SDK to an existing installation."
        );
    }

    run_install(SdkInstallArgs { force: false })
}

fn http_text(url: &str) -> Result<String> {
    ureq::get(url)
        .call()
        .with_context(|| format!("HTTP request failed: {url}"))?
        .into_string()
        .context("failed to read response body")
}

fn http_download_with_progress(url: &str) -> Result<Vec<u8>> {
    let response = ureq::get(url)
        .call()
        .with_context(|| format!("HTTP request failed: {url}"))?;

    let total: Option<u64> = response
        .header("Content-Length")
        .and_then(|v| v.parse().ok());

    let bar = match total {
        Some(t) => {
            let pb = ProgressBar::new(t);
            pb.set_style(
                ProgressStyle::with_template(
                    "  {spinner:.cyan} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})",
                )
                .expect("template")
                .progress_chars("=> "),
            );
            pb.enable_steady_tick(Duration::from_millis(120));
            pb
        }
        None => {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::with_template(
                    "  {spinner:.cyan} [{elapsed_precise}] {bytes} downloaded",
                )
                .expect("template"),
            );
            pb.enable_steady_tick(Duration::from_millis(120));
            pb
        }
    };

    struct ProgressReader<R: Read> {
        inner: R,
        bar: ProgressBar,
    }
    impl<R: Read> Read for ProgressReader<R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.bar.inc(n as u64);
            Ok(n)
        }
    }

    let mut reader = ProgressReader {
        inner: response.into_reader(),
        bar: bar.clone(),
    };
    let mut out = match total {
        Some(t) => Vec::with_capacity(t as usize),
        None => Vec::new(),
    };
    reader.read_to_end(&mut out).context("download stream")?;
    bar.finish_and_clear();
    Ok(out)
}

fn extract_msi(bytes: &[u8], is_zip: bool, dest: &Path) -> Result<()> {
    let cursor = Cursor::new(bytes.to_vec());
    let mut package = if is_zip {
        open_msi_from_zip(cursor)?
    } else {
        Package::open(cursor)?
    };

    if !package.has_table("File") {
        let tables: Vec<String> = package.tables().map(|t| t.name().to_string()).collect();
        bail!(
            "MSI is missing the 'File' table.\n\
             Tables present: {}\n\
             The installer layout may have changed; please report this with the SDK version.",
            tables.join(", ")
        );
    }

    let files: Vec<Row> = package
        .select_rows(
            Select::table("File")
                .inner_join(
                    Select::table("Component"),
                    Expr::col("Component.Component").eq(Expr::col("File.Component_")),
                )
                .columns(&["File.File", "File.FileName", "Component.Directory_"]),
        )?
        .collect();

    let directories: Vec<Row> = package
        .select_rows(Select::table("Directory").columns(&[
            "Directory",
            "Directory_Parent",
            "DefaultDir",
        ]))?
        .collect();

    let mut file_map: HashMap<String, PathBuf> = HashMap::new();
    for f in &files {
        let id = f["File.File"].as_str().context("file id")?.to_string();
        let name = long_file_name(f["File.FileName"].as_str().context("file name")?)?.to_string();
        let dir = resolve_dir(
            f["Component.Directory_"]
                .as_str()
                .context("directory key")?,
            &directories,
        )?;
        file_map.insert(id, dir.join(name));
    }

    let mut written = 0u64;
    let stream_names: Vec<String> = package.streams().collect();
    for stream_name in stream_names {
        let stream = package.read_stream(&stream_name)?;
        let mut cabinet = match Cabinet::new(stream) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let entries: Vec<String> = cabinet
            .folder_entries()
            .flat_map(|f| f.file_entries())
            .map(|f| f.name().to_string())
            .collect();

        for entry_name in entries {
            let Some(rel) = file_map.get(&entry_name) else {
                continue;
            };
            let rel_str = rel.to_str().context("path not utf-8")?;
            let Some(stripped) = strip_sdk_root(rel_str, SDK_EXTRACT_FROM) else {
                continue;
            };
            if !is_wanted(&stripped) {
                continue;
            }
            let out_path = dest.join(&stripped);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&out_path)
                .with_context(|| format!("create {}", out_path.display()))?;
            let mut data = cabinet.read_file(&entry_name)?;
            io::copy(&mut data, &mut out)?;
            written += 1;
        }
    }

    if written == 0 {
        bail!("no files extracted from installer — SDK layout may have changed");
    }
    Ok(())
}

fn long_file_name(s: &str) -> Result<&str> {
    s.split('|').last().context("empty file name")
}

fn open_msi_from_zip(cursor: Cursor<Vec<u8>>) -> Result<Package<Cursor<Vec<u8>>>> {
    let mut zip = ZipArchive::new(cursor)?;

    let msi_paths: Vec<String> = zip
        .file_names()
        .filter(|f| f.to_ascii_lowercase().ends_with(".msi"))
        .map(String::from)
        .collect();
    if msi_paths.is_empty() {
        bail!("zip archive has no .msi member");
    }

    let cab_paths: Vec<String> = zip
        .file_names()
        .filter(|f| f.to_ascii_lowercase().ends_with(".cab"))
        .map(String::from)
        .collect();

    let mut tried: Vec<String> = Vec::new();
    for msi_path in &msi_paths {
        let mut buf = Vec::new();
        zip.by_name(msi_path)?.read_to_end(&mut buf)?;
        let mut pkg = match Package::open(Cursor::new(buf)) {
            Ok(p) => p,
            Err(e) => {
                tried.push(format!("{msi_path}: open failed ({e})"));
                continue;
            }
        };

        if !pkg.has_table("File") {
            let mut report = format!("{msi_path}: no File table");

            let real_tables: Vec<String> = pkg
                .tables()
                .map(|t| t.name().to_string())
                .filter(|n| {
                    !n.is_empty()
                        && n.chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
                })
                .collect();
            report.push_str(&format!("\n    real tables: {}", real_tables.join(", ")));

            let stream_names: Vec<String> = pkg.streams().collect();
            let mut top_streams: Vec<(String, u64)> = Vec::new();
            for name in stream_names {
                let size = pkg
                    .read_stream(&name)
                    .ok()
                    .and_then(|mut s| {
                        let mut buf = Vec::new();
                        s.read_to_end(&mut buf).ok().map(|_| buf.len() as u64)
                    })
                    .unwrap_or(0);
                top_streams.push((name, size));
            }
            top_streams.sort_by(|a, b| b.1.cmp(&a.1));
            report.push_str("\n    top streams (name → size):");
            for (n, sz) in top_streams.iter().take(10) {
                report.push_str(&format!("\n      {n}  {} MB", sz / 1024 / 1024));
            }

            if pkg.has_table("Binary") {
                let bins: Vec<Row> = pkg
                    .select_rows(Select::table("Binary").columns(&["Name"]))?
                    .collect();
                let names: Vec<String> = bins
                    .iter()
                    .filter_map(|r| r["Name"].as_str().map(String::from))
                    .collect();
                report.push_str(&format!("\n    Binary entries: {}", names.join(", ")));
            }

            tried.push(report);
            continue;
        }

        for cab_path in &cab_paths {
            let cab_name = Path::new(cab_path)
                .file_name()
                .context("cab path has no file name")?
                .to_str()
                .context("cab name not utf-8")?
                .to_string();
            let mut cab_buf = Vec::new();
            zip.by_name(cab_path)?.read_to_end(&mut cab_buf)?;
            let mut stream = pkg.write_stream(&cab_name)?;
            stream.write_all(&cab_buf)?;
        }

        return Ok(pkg);
    }

    bail!(
        "no usable MSI found in zip. Tried:\n  - {}",
        tried.join("\n  - ")
    );
}

fn is_wanted(stripped: &str) -> bool {
    KEEP_PREFIXES.iter().any(|p| stripped.starts_with(p))
}

fn strip_sdk_root(rel: &str, prefix: &str) -> Option<String> {
    let norm = rel.replace('\\', "/");
    let prefix_norm = prefix.replace('\\', "/");
    let prefix_norm = prefix_norm.trim_start_matches("./");
    norm.split_once(&*prefix_norm)
        .map(|(_, rest)| rest.trim_start_matches('/').to_string())
}

fn resolve_dir(key: &str, all: &[Row]) -> Result<PathBuf> {
    let row = all
        .iter()
        .find(|r| r["Directory"].as_str() == Some(key))
        .with_context(|| format!("directory {key} not in Directory table"))?;
    let parent_path = if let Some(parent) = row["Directory_Parent"].as_str() {
        resolve_dir(parent, all)?
    } else {
        PathBuf::new()
    };
    let name = long_file_name(row["DefaultDir"].as_str().context("DefaultDir missing")?)?;
    Ok(parent_path.join(name))
}
