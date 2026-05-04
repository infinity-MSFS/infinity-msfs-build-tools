use crate::config::{Instrument, ModuleAlias, PackageSpec, SimulatorPackage};
use crate::package::{self, EmittedPackage};
use infinity_build_core::{
    Artifact, BuildError, BuildResult, Builder, FileKind, GeneratedFile, SimpleArtifact,
    pick_primary, stat_file,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

// Re-imports: keep all rolldown types behind one use group so future
// API drift is a single search/replace away.
use rolldown::{
    Bundler, BundlerOptions, ChunkFilenamesOutputOption, InputItem, IsExternal, OutputFormat,
    Platform, RawMinifyOptions, ResolveOptions, SourceMapType,
};
use rolldown_common::{BundlerTransformOptions, Either, ModuleType, TreeshakeOptions};
use rolldown_utils::indexmap::FxIndexMap;

#[derive(Debug, Clone, Default)]
pub struct BundleOptions {
    /// Output directory for raw bundles, relative to `project_root`.
    /// Defaults to `bundles`. Each instrument gets its own
    /// `<bundles_dir>/<instrument.name>/` subdirectory.
    pub bundles_dir: Option<PathBuf>,
    /// Minify the JS output.
    pub minify: bool,
    /// Emit sourcemaps. None = no sourcemaps (default).
    pub sourcemap: Option<SourceMapKind>,
    /// Skip the simulator-package emission step. Useful for CI smoke
    /// tests that just want to know whether the bundle compiles.
    pub skip_simulator_package: bool,
    /// Extra `process.env.<name> = <value>` substitutions. Values are
    /// JSON-encoded automatically — pass strings as plain strings, no
    /// quoting needed.
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy)]
pub enum SourceMapKind {
    Inline,
    External,
    File,
}

#[derive(Debug, Clone)]
pub struct JsBuildInput {
    pub instrument: Instrument,
    pub package: PackageSpec,
}

#[derive(Debug, Clone)]
pub struct JsArtifact {
    pub instrument_name: String,
    pub bundle_dir: PathBuf,
    pub generated: Vec<GeneratedFile>,
    pub package: Option<EmittedPackage>,
}

impl Artifact for JsArtifact {
    fn files(&self) -> &[GeneratedFile] {
        &self.generated
    }

    fn name(&self) -> &str {
        &self.instrument_name
    }

    fn primary(&self) -> Option<&GeneratedFile> {
        self.generated
            .iter()
            .find(|f| matches!(f.kind, FileKind::Template))
            .or_else(|| pick_primary(&self.generated))
    }
}

impl From<JsArtifact> for SimpleArtifact {
    fn from(value: JsArtifact) -> Self {
        SimpleArtifact::new(value.instrument_name, value.generated)
    }
}

pub struct JsBundler {
    project_root: PathBuf,
    options: BundleOptions,
    prepared_rescript_dirs: Mutex<HashSet<PathBuf>>,
}

impl JsBundler {
    pub fn new(project_root: impl Into<PathBuf>, options: BundleOptions) -> Self {
        Self {
            project_root: project_root.into(),
            options,
            prepared_rescript_dirs: Mutex::new(HashSet::new()),
        }
    }

    /// Async entry point. Use this when you're already inside a tokio
    /// runtime; calling [`Builder::build`] from inside one will panic
    /// (`cannot start a runtime from within a runtime`).
    pub async fn build_async(&self, input: &JsBuildInput) -> BuildResult<JsArtifact> {
        let bundles_dir = self
            .options
            .bundles_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("bundles"));
        let abs_bundle_dir = self
            .project_root
            .join(&bundles_dir)
            .join(&input.instrument.name);
        std::fs::create_dir_all(&abs_bundle_dir).map_err(|e| BuildError::io(&abs_bundle_dir, e))?;

        self.prepare_entry(input)?;
        let entry = input.instrument.resolved_index(&self.project_root)?;
        let bundler_options = self.bundler_options(&input.instrument, &abs_bundle_dir, &entry)?;

        let mut bundler = Bundler::new(bundler_options)
            .map_err(|e| BuildError::backend_failure("rolldown-init", format_rolldown_error(&e)))?;
        bundler.write().await.map_err(|e| {
            BuildError::backend_failure("rolldown-bundle", format_rolldown_error(&e))
        })?;

        let js_bundle_path = abs_bundle_dir.join("bundle.js");
        let css_bundle_path = abs_bundle_dir.join("bundle.css");
        let css_present = css_bundle_path.exists();

        if matches!(
            input.instrument.simulator_package,
            Some(SimulatorPackage::RescriptReact { .. })
        ) {
            inject_rescript_react_automount(&js_bundle_path)?;
        }

        let mut generated: Vec<GeneratedFile> = Vec::new();
        if let Ok(file) = stat_file(&js_bundle_path, FileKind::Script) {
            generated.push(file);
        }
        if css_present {
            if let Ok(file) = stat_file(&css_bundle_path, FileKind::Style) {
                generated.push(file);
            }
        }

        let package = if let Some(sim_pkg) = &input.instrument.simulator_package {
            if self.options.skip_simulator_package {
                None
            } else {
                let emitted = package::write_package(
                    &self.project_root,
                    &input.package,
                    &input.instrument,
                    sim_pkg,
                    &js_bundle_path,
                    if css_present {
                        Some(&css_bundle_path)
                    } else {
                        None
                    },
                )?;
                push_emitted_files(&emitted, &mut generated);
                Some(emitted)
            }
        } else {
            None
        };

        Ok(JsArtifact {
            instrument_name: input.instrument.name.clone(),
            bundle_dir: abs_bundle_dir,
            generated,
            package,
        })
    }

    fn prepare_entry(&self, input: &JsBuildInput) -> BuildResult<()> {
        let Some(SimulatorPackage::RescriptReact {
            build_command,
            build_dir,
            ..
        }) = &input.instrument.simulator_package
        else {
            return Ok(());
        };

        let build_dir =
            resolve_rescript_build_dir(&self.project_root, &input.instrument, build_dir.as_ref())?;
        let mut prepared = self.prepared_rescript_dirs.lock().map_err(|_| {
            BuildError::backend_failure(
                "rescript-build",
                format!(
                    "failed to acquire ReScript build lock for {}",
                    build_dir.display()
                ),
            )
        })?;
        if !prepared.insert(build_dir.clone()) {
            return Ok(());
        }
        drop(prepared);

        run_rescript_build_command(
            build_command.as_deref().unwrap_or("bun run build"),
            &build_dir,
        )
    }

    fn bundler_options(
        &self,
        instrument: &Instrument,
        abs_bundle_dir: &Path,
        entry: &Path,
    ) -> BuildResult<BundlerOptions> {
        let mut opts = BundlerOptions::default();

        opts.input = Some(vec![InputItem {
            name: Some("bundle".to_string()),
            import: entry.to_string_lossy().into_owned(),
        }]);
        opts.cwd = Some(self.project_root.clone());
        opts.dir = Some(abs_bundle_dir.to_string_lossy().into_owned());
        opts.platform = Some(Platform::Browser);
        opts.format = Some(OutputFormat::Iife);

        // Force literal `bundle.js` / `bundle.css` filenames (no hash).
        // The "[name]" placeholder becomes our InputItem.name = "bundle".
        opts.entry_filenames = Some(ChunkFilenamesOutputOption::String("[name].js".to_string()));
        opts.css_entry_filenames =
            Some(ChunkFilenamesOutputOption::String("[name].css".to_string()));

        // Externals matching mach. Note IsExternal in 0.1.0 has a
        // `From<Vec<String>>` impl on the deserializer side; the
        // public ctor used to be `from_vec`. If the next compile says
        // otherwise, switch to whatever ctor is exposed.
        // FIXME(rolldown-0.1): confirm IsExternal constructor name.
        opts.external = Some(IsExternal::from(vec![
            "/Images/*".to_string(),
            "/Fonts/*".to_string(),
        ]));

        // Treat .otf/.ttf as assets: rolldown copies them next to the
        // bundle and rewrites imports to relative URLs. Closest match
        // to mach's `loader: { ".otf": "file" }`.
        let mut module_types: rustc_hash::FxHashMap<String, ModuleType> = Default::default();
        module_types.insert(".otf".to_string(), ModuleType::Asset);
        module_types.insert(".ttf".to_string(), ModuleType::Asset);
        // Force .js through the Jsx parse path so rolldown 0.1.0's
        // transformer actually runs on plain JS — without this, the
        // transform.target setting below is silently ignored for .js
        // files (see rolldown pre_process_ecma_ast.rs: only !Js types
        // hit the Transformer). JSX is a superset of JS, so non-JSX
        // sources still parse cleanly.
        module_types.insert(".js".to_string(), ModuleType::Jsx);
        module_types.insert(".mjs".to_string(), ModuleType::Jsx);
        module_types.insert(".cjs".to_string(), ModuleType::Jsx);
        opts.module_types = Some(module_types);

        // Lower modern syntax for Coherent GT (MSFS HTML UI). Coherent
        // chokes on optional chaining / nullish coalescing, so target
        // es2019. JSX classic runtime is still the rolldown default.
        let mut transform = BundlerTransformOptions::default();
        transform.target = Some(Either::Left("es2019".to_string()));
        opts.transform = Some(transform);

        // Disable rolldown's DCE pass. Its oxc compressor runs after
        // the transformer with `CompressOptions::dce()` (no target),
        // and re-introduces ES2020 `?.` / `??` on patterns it folds —
        // undoing the lowering and tripping Coherent GT.
        opts.treeshake = TreeshakeOptions::Boolean(false);

        // Module aliases for nested instruments (mach's `modules`
        // feature). Rolldown's ResolveOptions.alias takes a list of
        // (specifier, replacement-paths) tuples.
        // FIXME(rolldown-0.1): confirm whether alias is `Vec<(String, Vec<String>)>`
        // or `IndexMap<String, Vec<String>>`. Both shapes appear in
        // the wider rolldown ecosystem.
        if !instrument.modules.is_empty() {
            let mut resolve = ResolveOptions::default();
            let alias_entries: Vec<(String, Vec<Option<String>>)> = instrument
                .modules
                .iter()
                .map(|ModuleAlias { resolve, index }| {
                    let abs = self.project_root.join(index);
                    (
                        resolve.clone(),
                        vec![Some(abs.to_string_lossy().into_owned())],
                    )
                })
                .collect();
            resolve.alias = Some(alias_entries);
            opts.resolve = Some(resolve);
        }

        // process.env.* substitutions via `define`. Values are JSON-
        // encoded so quoting is preserved — much cleaner than mach's
        // regex hack. In 0.1.0 `define` is `Option<FxIndexMap<String,
        // String>>`. We use the rustc_hash + indexmap re-export
        // surface that rolldown re-exports.
        if !self.options.env.is_empty() {
            let mut define: FxIndexMap<String, String> = Default::default();
            for (key, value) in &self.options.env {
                let json_value = serde_json::to_string(value).unwrap_or_else(|_| "null".into());
                define.insert(format!("process.env.{key}"), json_value);
            }
            opts.define = Some(define);
        }

        if self.options.minify {
            opts.minify = Some(RawMinifyOptions::Bool(true));
        }

        if let Some(kind) = self.options.sourcemap {
            // rolldown 0.1.0's SourceMapType is {File, Inline, Hidden}.
            // We map "external" (linked) → File (writes .map + adds the
            // sourceMappingURL comment) and "file" → Hidden (writes
            // .map without the comment, for shipping side-by-side
            // without exposing a link).
            opts.sourcemap = Some(match kind {
                SourceMapKind::Inline => SourceMapType::Inline,
                SourceMapKind::External => SourceMapType::File,
                SourceMapKind::File => SourceMapType::Hidden,
            });
        }

        Ok(opts)
    }
}

/// `Builder` impl. Boots a current-thread tokio runtime per call. If
/// you already have a runtime, prefer [`JsBundler::build_async`].
impl Builder for JsBundler {
    type Input = JsBuildInput;
    type Output = JsArtifact;

    fn build(&self, input: &Self::Input) -> BuildResult<Self::Output> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                BuildError::backend_failure(
                    "tokio-runtime",
                    format!("could not start runtime: {e}"),
                )
            })?;
        rt.block_on(self.build_async(input))
    }
}

fn push_emitted_files(emitted: &EmittedPackage, into: &mut Vec<GeneratedFile>) {
    for path in emitted.iter_paths() {
        let kind = match path.extension().and_then(|e| e.to_str()) {
            Some("html") => FileKind::Template,
            Some("css") => FileKind::Style,
            Some("js" | "mjs" | "cjs") => FileKind::Script,
            Some("map") => FileKind::SourceMap,
            _ => FileKind::Other,
        };
        if let Ok(file) = stat_file(path, kind) {
            into.push(file);
        }
    }
}

/// Render a rolldown error into a single human-readable string. The
/// rolldown error type's `Display` is often empty for diagnostic
/// containers, so we fall back to `Debug` if Display gives us nothing.
fn format_rolldown_error<E: std::fmt::Debug + std::fmt::Display>(err: &E) -> String {
    let display = format!("{err}");
    if display.trim().is_empty() {
        format!("{err:?}")
    } else {
        display
    }
}

fn resolve_rescript_build_dir(
    project_root: &Path,
    instrument: &Instrument,
    configured: Option<&PathBuf>,
) -> BuildResult<PathBuf> {
    let dir = if let Some(configured) = configured {
        resolve_path(project_root, configured)
    } else {
        discover_rescript_project_dir(project_root, instrument)
    };

    if !dir.is_dir() {
        return Err(BuildError::invalid_path(
            &dir,
            "ReScript build directory does not exist or is not a directory",
        ));
    }

    Ok(dir)
}

/// Inject an auto-invocation of the bundle's `mount` export into the
/// IIFE produced by rolldown for ReScript-React instruments.
///
/// rolldown emits `(function(exports){ ...; exports.mount = mount; return exports; })({});`
/// and discards the returned object, so the user's `mount` is defined
/// but never called. We splice a call inside the closure, just before
/// `return exports;`, so it runs in the IIFE scope when the script
/// loads.
fn inject_rescript_react_automount(bundle_path: &Path) -> BuildResult<()> {
    let source = std::fs::read_to_string(bundle_path).map_err(|e| BuildError::io(bundle_path, e))?;

    let marker = "return exports;";
    let Some(idx) = source.rfind(marker) else {
        return Err(BuildError::backend_failure(
            "rescript-automount",
            format!(
                "expected `return exports;` in IIFE bundle at {}",
                bundle_path.display()
            ),
        ));
    };

    let injected = "if (typeof exports.mount === 'function') { exports.mount(); }\n";
    let mut patched = String::with_capacity(source.len() + injected.len());
    patched.push_str(&source[..idx]);
    patched.push_str(injected);
    patched.push_str(&source[idx..]);

    std::fs::write(bundle_path, patched).map_err(|e| BuildError::io(bundle_path, e))
}

fn discover_rescript_project_dir(project_root: &Path, instrument: &Instrument) -> PathBuf {
    let entry_path = resolve_path(project_root, &instrument.index);
    let mut current = entry_path.parent().unwrap_or(project_root).to_path_buf();

    loop {
        if contains_rescript_marker(&current) {
            return current;
        }

        if current == project_root {
            break;
        }

        let Some(parent) = current.parent() else {
            break;
        };
        if !parent.starts_with(project_root) {
            break;
        }
        current = parent.to_path_buf();
    }

    project_root.to_path_buf()
}

fn contains_rescript_marker(dir: &Path) -> bool {
    ["rescript.json", "bsconfig.json", "package.json"]
        .into_iter()
        .any(|name| dir.join(name).exists())
}

fn resolve_path(project_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

fn run_rescript_build_command(command: &str, cwd: &Path) -> BuildResult<()> {
    let output = shell_command(command)
        .current_dir(cwd)
        .output()
        .map_err(|e| {
            BuildError::backend_failure(
                "rescript-build",
                format!("failed to start `{command}` in {}: {e}", cwd.display()),
            )
        })?;

    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
    let detail = if !stderr.trim().is_empty() {
        stderr.trim().to_string()
    } else if !stdout.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        "no output captured".to_string()
    };

    Err(BuildError::backend_failure(
        "rescript-build",
        format!(
            "`{command}` failed in {} with status {}:\n{}",
            cwd.display(),
            output.status,
            detail
        ),
    ))
}

#[cfg(windows)]
fn shell_command(script: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(script);
    cmd
}

#[cfg(not(windows))]
fn shell_command(script: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(script);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimulatorPackageKind;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("infinity-build-js-{prefix}-{unique}"));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn discovers_nearest_rescript_project_dir() {
        let temp = TempDir::new("discover");
        let ui_dir = temp.path.join("ui");
        std::fs::create_dir_all(ui_dir.join("src")).unwrap();
        std::fs::write(ui_dir.join("rescript.json"), "{}").unwrap();

        let instrument = Instrument {
            name: "PFD".into(),
            index: PathBuf::from("ui/src/Main.res.mjs"),
            simulator_package: Some(SimulatorPackage::RescriptReact {
                file_name: None,
                template_id: None,
                is_interactive: true,
                imports: Vec::new(),
                html_template: None,
                js_template: None,
                build_command: None,
                build_dir: None,
            }),
            modules: Vec::new(),
        };

        let resolved = resolve_rescript_build_dir(&temp.path, &instrument, None).unwrap();
        assert_eq!(resolved, ui_dir);
        assert_eq!(
            instrument.simulator_package.unwrap().kind(),
            SimulatorPackageKind::RescriptReact
        );
    }

    #[test]
    fn rescript_react_runs_build_before_bundling() {
        let temp = TempDir::new("bundle");
        std::fs::create_dir_all(temp.path.join("src")).unwrap();
        std::fs::write(temp.path.join("package.json"), "{}").unwrap();
        let build_command = create_entry_build_script(&temp.path, "src/Main.res.mjs");

        let instrument = Instrument {
            name: "PFD".into(),
            index: PathBuf::from("src/Main.res.mjs"),
            simulator_package: Some(SimulatorPackage::RescriptReact {
                file_name: None,
                template_id: Some("PFD".into()),
                is_interactive: true,
                imports: Vec::new(),
                html_template: None,
                js_template: None,
                build_command: Some(build_command),
                build_dir: None,
            }),
            modules: Vec::new(),
        };
        let input = JsBuildInput {
            instrument,
            package: PackageSpec {
                package_name: "pkg".into(),
                package_dir: PathBuf::from("PackageSources"),
            },
        };

        let bundler = JsBundler::new(
            temp.path.clone(),
            BundleOptions {
                skip_simulator_package: true,
                ..BundleOptions::default()
            },
        );

        let artifact = bundler.build(&input).unwrap();
        assert!(temp.path.join("src/Main.res.mjs").exists());
        assert!(artifact.bundle_dir.join("bundle.js").exists());
        assert!(!artifact.files().is_empty());
    }

    #[cfg(windows)]
    fn create_entry_build_script(root: &Path, path: &str) -> String {
        let script_path = root.join("build-entry.ps1");
        let path = path.replace('/', "\\");
        std::fs::write(
            &script_path,
            format!(
                "$null = New-Item -ItemType Directory -Force -Path 'src'\n$null = New-Item -ItemType File -Force -Path '{path}'\n"
            ),
        )
        .unwrap();
        format!(
            "powershell -NoProfile -ExecutionPolicy Bypass -File {}",
            script_path.display()
        )
    }

    #[cfg(not(windows))]
    fn create_entry_build_script(root: &Path, path: &str) -> String {
        let script_path = root.join("build-entry.sh");
        std::fs::write(&script_path, format!("mkdir -p src\n: > {path}\n")).unwrap();
        format!("sh {}", script_path.display())
    }
}
