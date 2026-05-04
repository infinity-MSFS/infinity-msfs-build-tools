use crate::{
    manifest::{PromptSpec, TemplateManifest},
    render::{apply_tokens, apply_tokens_path, render_expr, validate},
};
use anyhow::{Context, Result, bail};
use console::style;
use dialoguer::{Input, Select, theme::ColorfulTheme};
use heck::{ToKebabCase, ToPascalCase, ToSnakeCase};
use include_dir::{Dir, include_dir};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

#[derive(Debug, Default)]
pub struct CreateOptions {
    /// Target directory. If `None`, prompts for one.
    pub path: Option<PathBuf>,
    /// Skip the picker, jump to a known template id.
    pub template: Option<String>,
    /// Skip the project_name prompt.
    pub name: Option<String>,
    /// Reject any prompt that needs interactive input.
    pub no_input: bool,
    /// Overwrite a non-empty target dir.
    pub force: bool,
}

pub fn run(opts: CreateOptions) -> Result<()> {
    let templates = load_all_templates()?;
    if templates.is_empty() {
        bail!("no templates compiled into this binary");
    }

    let chosen = match &opts.template {
        Some(id) => templates
            .iter()
            .find(|t| t.0.template.id == *id)
            .with_context(|| format!("no template with id `{id}`"))?,
        None => {
            if opts.no_input {
                bail!("--template required when --no-input is set");
            }
            pick_template(&templates)?
        }
    };

    let theme = ColorfulTheme::default();

    let target = match &opts.path {
        Some(p) => p.clone(),
        None => {
            if opts.no_input {
                bail!("path argument required when --no-input is set");
            }
            let raw: String = Input::with_theme(&theme)
                .with_prompt("Target directory")
                .default(".".to_string())
                .interact_text()?;
            PathBuf::from(raw)
        }
    };
    let target = if target.is_absolute() {
        target
    } else {
        std::env::current_dir()?.join(&target)
    };

    if target.exists() {
        let empty = match fs::read_dir(&target) {
            Ok(mut it) => it.next().is_none(),
            Err(_) => false,
        };
        if !empty && !opts.force {
            bail!(
                "target directory {} is not empty (pass --force to override)",
                target.display()
            );
        }
    }

    let project_default = derived_project_default(&target, opts.name.as_deref());
    let mut answers: BTreeMap<String, String> = BTreeMap::new();

    // Shared `project_name` prompt — kebab-case, validated.
    let project_name = if let Some(n) = opts.name.as_deref() {
        validate(n, "kebab").with_context(|| format!("invalid --name `{n}`"))?;
        n.to_string()
    } else if opts.no_input {
        bail!("--name required when --no-input is set");
    } else {
        prompt_with_validation(
            &theme,
            "Project name (kebab-case)",
            Some(&project_default),
            "kebab",
        )?
    };
    answers.insert("project_name".to_string(), project_name.clone());

    for spec in &chosen.0.prompts {
        let value = if opts.no_input {
            let raw = spec
                .default
                .as_deref()
                .with_context(|| format!("--no-input but prompt `{}` has no default", spec.key))?;
            let resolved = render_expr(raw, &answers)?;
            validate(&resolved, &spec.validate)?;
            resolved
        } else {
            ask_prompt(&theme, spec, &answers)?
        };
        answers.insert(spec.key.clone(), value);
    }

    let tokens = build_token_map(&answers, &chosen.0.tokens)?;

    println!();
    println!(
        "{} {} {} {}",
        style("Scaffolding").cyan().bold(),
        style(&chosen.0.template.display).bold(),
        style("→").dim(),
        style(target.display()).dim(),
    );

    fs::create_dir_all(&target)
        .with_context(|| format!("failed to create {}", target.display()))?;

    let mut written = 0usize;
    write_dir(chosen.1, &target, &tokens, &mut written)?;

    println!(
        "{} wrote {} file(s) to {}",
        style("✓").green().bold(),
        style(written).bold(),
        style(target.display()).dim(),
    );

    print_next_steps(&chosen.0.template.id, &target);

    Ok(())
}

type LoadedTemplate = (TemplateManifest, &'static Dir<'static>);

fn load_all_templates() -> Result<Vec<LoadedTemplate>> {
    let mut out = Vec::new();
    for dir in TEMPLATES.dirs() {
        let manifest_file = match dir.get_file(format!(
            "{}/template.toml",
            dir.path().to_string_lossy()
        )) {
            Some(f) => f,
            None => continue,
        };
        let raw = manifest_file
            .contents_utf8()
            .context("template.toml not utf-8")?;
        let manifest: TemplateManifest = toml::from_str(raw).with_context(|| {
            format!("invalid template.toml in {}", dir.path().display())
        })?;
        out.push((manifest, dir));
    }
    out.sort_by(|a, b| a.0.template.display.cmp(&b.0.template.display));
    Ok(out)
}

fn pick_template(templates: &[LoadedTemplate]) -> Result<&LoadedTemplate> {
    let labels: Vec<String> = templates
        .iter()
        .map(|t| {
            format!(
                "{}  {}",
                style(&t.0.template.display).bold(),
                style(format!("— {}", t.0.template.description)).dim(),
            )
        })
        .collect();
    let idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Pick a template")
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(&templates[idx])
}

fn ask_prompt(
    theme: &ColorfulTheme,
    spec: &PromptSpec,
    answers: &BTreeMap<String, String>,
) -> Result<String> {
    let default = match &spec.default {
        Some(expr) => Some(render_expr(expr, answers)?),
        None => None,
    };
    prompt_with_validation(theme, &spec.question, default.as_deref(), &spec.validate)
}

fn prompt_with_validation(
    theme: &ColorfulTheme,
    question: &str,
    default: Option<&str>,
    validate_kind: &str,
) -> Result<String> {
    let kind = validate_kind.to_string();
    let mut input = Input::<String>::with_theme(theme)
        .with_prompt(question)
        .validate_with(move |v: &String| -> std::result::Result<(), String> {
            validate(v, &kind).map_err(|e| e.to_string())
        });
    if let Some(d) = default {
        input = input.default(d.to_string());
    }
    Ok(input.interact_text()?)
}

fn build_token_map(
    answers: &BTreeMap<String, String>,
    extras: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let project = answers
        .get("project_name")
        .context("project_name missing")?;
    let mut tokens = BTreeMap::new();
    tokens.insert("ReplaceMe".to_string(), project.to_pascal_case());
    tokens.insert("replace-me".to_string(), project.to_kebab_case());
    tokens.insert("replace_me".to_string(), project.to_snake_case());

    for (k, v) in extras {
        tokens.insert(k.clone(), render_expr(v, answers)?);
    }
    Ok(tokens)
}

fn derived_project_default(target: &Path, cli_name: Option<&str>) -> String {
    if let Some(n) = cli_name {
        return n.to_string();
    }
    target
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_kebab_case())
        .filter(|s| !s.is_empty() && s != ".")
        .unwrap_or_else(|| "my-project".to_string())
}

fn write_dir(
    dir: &Dir<'_>,
    out_root: &Path,
    tokens: &BTreeMap<String, String>,
    written: &mut usize,
) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(sub) => {
                write_dir(sub, out_root, tokens, written)?;
            }
            include_dir::DirEntry::File(file) => {
                // Skip the manifest itself.
                if file
                    .path()
                    .file_name()
                    .map(|n| n == "template.toml")
                    .unwrap_or(false)
                {
                    continue;
                }
                // include_dir paths are rooted at the templates/ dir
                // (e.g. `rust-wasm-gauge/Cargo.toml`); strip the first
                // component (the template id) so the output mirrors the
                // template contents directly.
                let rel = strip_template_prefix(file.path());
                let rendered_rel = apply_tokens_path(&rel.to_string_lossy(), tokens);
                let out_path = out_root.join(rendered_rel);
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create {}", parent.display())
                    })?;
                }
                let bytes = file.contents();
                let final_bytes = match std::str::from_utf8(bytes) {
                    Ok(text) => apply_tokens(text, tokens).into_bytes(),
                    Err(_) => bytes.to_vec(),
                };
                fs::write(&out_path, final_bytes)
                    .with_context(|| format!("failed to write {}", out_path.display()))?;
                *written += 1;
            }
        }
    }
    Ok(())
}

fn strip_template_prefix(p: &Path) -> PathBuf {
    let mut comps = p.components();
    comps.next();
    comps.as_path().to_path_buf()
}

fn print_next_steps(template_id: &str, target: &Path) {
    println!();
    println!("{}", style("Next steps:").cyan().bold());
    println!("  cd {}", target.display());
    if template_id.starts_with("rust-") {
        println!("  infinity-msfs doctor");
        println!("  infinity-msfs build");
    } else {
        println!("  bun install");
        // The `infinity:build` script chains the rescript compile (where
        // applicable) into `infinity-msfs build`, so users only need one
        // command going forward.
        println!("  bun run infinity:build");
    }
    println!();
}
