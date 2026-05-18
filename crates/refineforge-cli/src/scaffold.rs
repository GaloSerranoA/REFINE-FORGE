//! `refine new`: scaffold a new claim from a template.
//!
//! A template is a directory under `templates/<name>/` containing:
//!   * `lean.lean.tmpl`   — Lean source with `{{CLAIM_ID}}`, `{{MODULE}}`,
//!                          `{{LEAN_FILE}}`, `{{TITLE}}` placeholders
//!   * `claim.yaml.tmpl`  — claim YAML with the same placeholders
//!
//! The scaffolder writes:
//!   * `lean/<MODULE_PATH>.lean`     (creating intermediate dirs)
//!   * `claims/<slug>.yaml`
//!
//! `MODULE_PATH` is derived from the user-supplied module string by
//! replacing dots with `/`. E.g. `Refineforge.Capability` →
//! `lean/Refineforge/Capability.lean`.
//!
//! The library root file (e.g. `lean/Refineforge.lean`) is updated to
//! `import` the new module. The root name is read from `lakefile.toml`
//! via the `defaultTargets` key — so renaming your Lean library does
//! not require editing this code.

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};

pub fn list_templates(root: &Path) -> Result<()> {
    let dir = root.join("templates");
    if !dir.exists() {
        return Err(anyhow!("templates directory not found: {}", dir.display()));
    }
    let mut names: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let lean_t = entry.path().join("lean.lean.tmpl");
            let yaml_t = entry.path().join("claim.yaml.tmpl");
            if lean_t.exists() && yaml_t.exists() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.sort();
    if names.is_empty() {
        println!("(no templates found)");
    } else {
        println!("Available templates:");
        for n in names {
            println!("  {n}");
        }
    }
    Ok(())
}

pub fn create(
    root: &Path,
    template: &str,
    claim_id: &str,
    module: &str,
    title: Option<&str>,
) -> Result<()> {
    let tdir = root.join("templates").join(template);
    let lean_tmpl_path = tdir.join("lean.lean.tmpl");
    let yaml_tmpl_path = tdir.join("claim.yaml.tmpl");
    if !lean_tmpl_path.exists() || !yaml_tmpl_path.exists() {
        return Err(anyhow!(
            "template '{}' is incomplete or missing under {}",
            template,
            tdir.display()
        ));
    }

    // Validate inputs early so we don't write half a scaffold.
    validate_claim_id(claim_id)?;
    validate_module(module)?;

    // Derive paths.
    let module_path_parts: Vec<&str> = module.split('.').collect();
    let lean_rel: PathBuf = {
        let mut p = PathBuf::from("lean");
        for part in &module_path_parts {
            p.push(part);
        }
        p.set_extension("lean");
        p
    };
    let lean_abs = root.join(&lean_rel);
    let slug = claim_id.to_lowercase().replace('_', "-");
    let yaml_abs = root.join("claims").join(format!("{slug}.yaml"));

    // Refuse to overwrite — operator must delete first.
    if lean_abs.exists() {
        return Err(anyhow!(
            "refusing to overwrite existing Lean file: {}",
            lean_abs.display()
        ));
    }
    if yaml_abs.exists() {
        return Err(anyhow!(
            "refusing to overwrite existing claim YAML: {}",
            yaml_abs.display()
        ));
    }

    let lean_tmpl = std::fs::read_to_string(&lean_tmpl_path)
        .with_context(|| format!("reading {}", lean_tmpl_path.display()))?;
    let yaml_tmpl = std::fs::read_to_string(&yaml_tmpl_path)
        .with_context(|| format!("reading {}", yaml_tmpl_path.display()))?;

    let lean_rel_str = lean_rel.to_string_lossy().replace('\\', "/");
    let title_str = title.unwrap_or("TODO: short, factual claim title");

    let substitute = |s: &str| -> String {
        s.replace("{{CLAIM_ID}}", claim_id)
            .replace("{{MODULE}}", module)
            .replace("{{LEAN_FILE}}", &lean_rel_str)
            .replace("{{TITLE}}", title_str)
    };
    let lean_out = substitute(&lean_tmpl);
    let yaml_out = substitute(&yaml_tmpl);

    if let Some(parent) = lean_abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&lean_abs, lean_out)?;
    std::fs::write(&yaml_abs, yaml_out)?;

    // Make sure the library root imports the new module. We append a
    // line to `lean/HELYX.lean` if one doesn't already exist for it.
    update_root_import(root, module)?;

    println!("Scaffolded {claim_id}:");
    println!("  Lean: {}", lean_abs.display());
    println!("  YAML: {}", yaml_abs.display());
    println!();
    println!("Next: edit both files, then run");
    println!("  refine lean check {claim_id}");
    Ok(())
}

fn validate_claim_id(s: &str) -> Result<()> {
    // Convention: <PROJECT>-<AREA>-<NNN>, e.g. MYPROJ-AUTH-001
    if s.is_empty() {
        return Err(anyhow!("claim_id may not be empty"));
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(anyhow!(
            "claim_id must be ASCII alphanumeric + '-' or '_': got {s:?}"
        ));
    }
    Ok(())
}

fn validate_module(s: &str) -> Result<()> {
    if s.is_empty() {
        return Err(anyhow!("module may not be empty"));
    }
    for part in s.split('.') {
        if part.is_empty() {
            return Err(anyhow!("module path has empty segment: {s:?}"));
        }
        let first = part.chars().next().unwrap();
        if !first.is_ascii_uppercase() {
            return Err(anyhow!(
                "module segment must start with uppercase letter: {part:?}"
            ));
        }
        if !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(anyhow!(
                "module segment must be ASCII alphanumeric + '_': {part:?}"
            ));
        }
    }
    Ok(())
}

/// Read the Lean library root name from `lean/lakefile.toml`'s
/// `defaultTargets` entry. Falls back to `Refineforge` if the lakefile
/// is missing or unparseable — the auto-import is best-effort and
/// silently skipped when in doubt.
fn lean_lib_root_name(root: &Path) -> String {
    let path = root.join("lean").join("lakefile.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return "Refineforge".to_string();
    };
    let re = Regex::new(r#"(?m)^\s*defaultTargets\s*=\s*\[\s*"([^"]+)""#)
        .expect("static regex must compile");
    if let Some(c) = re.captures(&text) {
        return c[1].to_string();
    }
    "Refineforge".to_string()
}

/// Append `import {module}` to the Lean library root (e.g.
/// `lean/Refineforge.lean`) if not already present and if the module
/// path lives inside the library's namespace.
fn update_root_import(root: &Path, module: &str) -> Result<()> {
    let lib_name = lean_lib_root_name(root);
    let path = root.join("lean").join(format!("{lib_name}.lean"));
    if !path.exists() {
        // No library root file; nothing to update.
        return Ok(());
    }
    let prefix = format!("{lib_name}.");
    if !module.starts_with(&prefix) {
        // Don't auto-import modules outside the library's namespace.
        return Ok(());
    }
    let current = std::fs::read_to_string(&path)?;
    let import_line = format!("import {module}");
    if current.lines().any(|l| l.trim() == import_line.trim()) {
        return Ok(());
    }
    let mut updated = current;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&import_line);
    updated.push('\n');
    std::fs::write(&path, updated)?;
    Ok(())
}
