use crate::config::{Instrument, PackageSpec, SimulatorPackage};
use crate::templates::{
    DEFAULT_HTML_TEMPLATE, DEFAULT_JS_HARNESS_TEMPLATE, TemplateContext, load_template, render,
};
use infinity_build_core::{BuildError, BuildResult};
use std::path::{Path, PathBuf};

/// One emitted package. Returned alongside the bundle paths so the
/// orchestrator can present a unified file list to the user.
#[derive(Debug, Clone)]
pub struct EmittedPackage {
    pub html_path: PathBuf,
    pub js_path: PathBuf,
    pub css_path: PathBuf,
    /// Only present for React-type instruments. For BaseInstrument
    /// instruments the harness step is skipped.
    pub harness_path: Option<PathBuf>,
}

impl EmittedPackage {
    pub fn iter_paths(&self) -> impl Iterator<Item = &Path> {
        let mut v: Vec<&Path> = vec![&self.html_path, &self.js_path, &self.css_path];
        if let Some(h) = &self.harness_path {
            v.push(h);
        }
        v.into_iter()
    }
}

/// Render the template parameters mach feeds into both templates, then
/// write the output files. `js_bundle_path` and `css_bundle_path` are
/// the raw rolldown outputs in `bundles/<name>/`; we copy them into
/// place under the new names rather than re-bundling.
pub fn write_package(
    project_root: &Path,
    package: &PackageSpec,
    instrument: &Instrument,
    sim_pkg: &SimulatorPackage,
    js_bundle_path: &Path,
    css_bundle_path: Option<&Path>,
) -> BuildResult<EmittedPackage> {
    let html_ui_path = project_root.join(&package.package_dir).join("html_ui");
    let package_target = html_ui_path
        .join("Pages")
        .join("VCockpit")
        .join("Instruments")
        .join(&package.package_name)
        .join(&instrument.name);

    std::fs::create_dir_all(&package_target).map_err(|e| BuildError::io(&package_target, e))?;

    let file_name = sim_pkg.file_name();
    let css_path = package_target.join(format!("{file_name}.css"));
    let js_path = package_target.join(format!("{file_name}.js"));

    // Copy the bundle outputs into place. CSS is optional — if no
    // styles were emitted we skip the copy and leave a 0-byte file
    // behind so the HTML template's `<link>` doesn't 404. (Mach does
    // this implicitly because esbuild emits an empty bundle.css.)
    copy_or_create_empty(js_bundle_path, &js_path)?;
    match css_bundle_path {
        Some(src) => copy_or_create_empty(src, &css_path)?,
        None => write_empty(&css_path)?,
    }

    // For React: the HTML template's `instrumentPath` points at the
    // harness, which `Include.addScript`s the bundle. For
    // BaseInstrument: it points at the bundle directly.
    let (instrument_path_on_disk, harness_path) = match sim_pkg {
        SimulatorPackage::React { js_template, .. }
        | SimulatorPackage::RescriptReact { js_template, .. } => {
            let harness = package_target.join(format!("{file_name}.index.js"));
            let template_id = react_template_id(sim_pkg, instrument);
            let js_path_url = strip_html_ui_prefix(&js_path, &html_ui_path);
            let harness_text = render(
                &load_template(js_template.as_deref(), DEFAULT_JS_HARNESS_TEMPLATE)?,
                &TemplateContext::new()
                    .var("templateId", template_id.clone())
                    .var(
                        "instrumentName",
                        format!(
                            "{}-{}",
                            package.package_name.to_lowercase(),
                            template_id.to_lowercase()
                        ),
                    )
                    .var("jsPath", js_path_url)
                    .var("isInteractive", sim_pkg.is_interactive().to_string()),
            )?;
            std::fs::write(&harness, harness_text).map_err(|e| BuildError::io(&harness, e))?;
            (harness.clone(), Some(harness))
        }
        SimulatorPackage::BaseInstrument { .. } => (js_path.clone(), None),
    };

    // Render the HTML template.
    let html_path = package_target.join(format!("{file_name}.html"));
    let html_template_path = match sim_pkg {
        SimulatorPackage::React { html_template, .. }
        | SimulatorPackage::RescriptReact { html_template, .. }
        | SimulatorPackage::BaseInstrument { html_template, .. } => html_template.as_deref(),
    };
    let html_text = render(
        &load_template(html_template_path, DEFAULT_HTML_TEMPLATE)?,
        &html_template_context(
            sim_pkg,
            instrument,
            &html_ui_path,
            &css_path,
            &instrument_path_on_disk,
        ),
    )?;
    std::fs::write(&html_path, html_text).map_err(|e| BuildError::io(&html_path, e))?;

    Ok(EmittedPackage {
        html_path,
        js_path,
        css_path,
        harness_path,
    })
}

fn copy_or_create_empty(src: &Path, dst: &Path) -> BuildResult<()> {
    if src.exists() {
        std::fs::copy(src, dst).map_err(|e| BuildError::io(dst, e))?;
    } else {
        write_empty(dst)?;
    }
    Ok(())
}

fn write_empty(path: &Path) -> BuildResult<()> {
    std::fs::write(path, "").map_err(|e| BuildError::io(path, e))
}

fn react_template_id(sim_pkg: &SimulatorPackage, instrument: &Instrument) -> String {
    match sim_pkg {
        SimulatorPackage::React { template_id, .. }
        | SimulatorPackage::RescriptReact { template_id, .. } => template_id
            .clone()
            .unwrap_or_else(|| instrument.name.clone()),
        SimulatorPackage::BaseInstrument { template_id, .. } => template_id.clone(),
    }
}

fn html_template_context(
    sim_pkg: &SimulatorPackage,
    instrument: &Instrument,
    html_ui_path: &Path,
    css_path: &Path,
    instrument_path: &Path,
) -> TemplateContext {
    let template_id = react_template_id(sim_pkg, instrument);
    let mount_element_id = match sim_pkg {
        SimulatorPackage::React { .. } | SimulatorPackage::RescriptReact { .. } => {
            "MSFS_REACT_MOUNT".to_string()
        }
        SimulatorPackage::BaseInstrument {
            mount_element_id, ..
        } => mount_element_id.clone(),
    };

    TemplateContext::new()
        .var("templateId", template_id)
        .var("mountElementId", mount_element_id)
        .var("cssPath", strip_html_ui_prefix(css_path, html_ui_path))
        .var(
            "instrumentPath",
            strip_html_ui_prefix(instrument_path, html_ui_path),
        )
        .list("imports", sim_pkg.imports().to_vec())
}

/// Convert an absolute on-disk path under `html_ui/...` to the URL
/// the simulator will see (no prefix, forward slashes only).
///
/// Mach does this with `path.replace(htmlUiPath, '').replace(/\\/g, '/')`.
/// We use `strip_prefix` so we only ever match at the start, then
/// normalise the separator.
fn strip_html_ui_prefix(path: &Path, html_ui_path: &Path) -> String {
    let trimmed = path.strip_prefix(html_ui_path).unwrap_or(path);
    let raw = trimmed.to_string_lossy();
    // The simulator wants a leading slash. `strip_prefix` removes the
    // separator too, so prepend it back.
    let with_slash = if raw.starts_with('/') || raw.starts_with('\\') {
        raw.to_string()
    } else {
        format!("/{raw}")
    };
    with_slash.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_prefix_and_normalises_slashes() {
        let html_ui = Path::new("/proj/PackageSources/html_ui");
        let css = Path::new(
            "/proj/PackageSources/html_ui/Pages/VCockpit/Instruments/pkg/PFD/instrument.css",
        );
        let s = strip_html_ui_prefix(css, html_ui);
        assert_eq!(s, "/Pages/VCockpit/Instruments/pkg/PFD/instrument.css");
    }

    #[test]
    fn passthrough_when_not_a_prefix() {
        // If the path isn't actually under html_ui, just normalise
        // separators and prepend a slash if needed.
        let html_ui = Path::new("/different/root");
        let css = Path::new("/proj/PackageSources/html_ui/x.css");
        let s = strip_html_ui_prefix(css, html_ui);
        assert!(s.starts_with('/'));
        assert!(!s.contains('\\'));
    }
}
