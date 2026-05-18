//! Claim registry: load, list, show.
//!
//! A claim is a YAML file under `claims/`. The schema mirrors the
//! README example. We accept loose YAML and only require the fields
//! needed downstream.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub claim_id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub rust_source: Vec<RustSource>,
    pub lean: LeanInfo,
    #[serde(default)]
    pub policy: Policy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSource {
    pub path: String,
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub functions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeanInfo {
    pub toolchain: String,
    pub module: String,
    pub file: String,
    #[serde(default)]
    pub theorems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    #[serde(default = "default_true")]
    pub no_sorry: bool,
    #[serde(default = "default_true")]
    pub no_admit: bool,
    #[serde(default = "default_true")]
    pub no_axioms_beyond_lean_core: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            no_sorry: true,
            no_admit: true,
            no_axioms_beyond_lean_core: true,
        }
    }
}

fn default_true() -> bool { true }

pub fn claims_dir(root: &Path) -> PathBuf { root.join("claims") }

/// Load a claim by id. Errors if not found or YAML is malformed.
pub fn load(root: &Path, claim_id: &str) -> Result<(PathBuf, Claim)> {
    let dir = claims_dir(root);
    if !dir.exists() {
        return Err(anyhow!("claims directory not found: {}", dir.display()));
    }
    for entry in WalkDir::new(&dir).max_depth(2) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("yaml") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path())
            .with_context(|| format!("reading {}", entry.path().display()))?;
        let claim: Claim = serde_yaml::from_str(&text)
            .with_context(|| format!("parsing {}", entry.path().display()))?;
        if claim.claim_id == claim_id {
            return Ok((entry.path().to_path_buf(), claim));
        }
    }
    Err(anyhow!(
        "claim '{}' not found under {}",
        claim_id,
        dir.display()
    ))
}

/// Load all claims in the registry.
pub fn all(root: &Path) -> Result<Vec<(PathBuf, Claim)>> {
    let mut out = Vec::new();
    let dir = claims_dir(root);
    if !dir.exists() {
        return Ok(out);
    }
    for entry in WalkDir::new(&dir).max_depth(2) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("yaml") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path())?;
        let claim: Claim = serde_yaml::from_str(&text)
            .with_context(|| format!("parsing {}", entry.path().display()))?;
        out.push((entry.path().to_path_buf(), claim));
    }
    Ok(out)
}

pub fn list(root: &Path) -> Result<()> {
    let mut claims = all(root)?;
    claims.sort_by(|a, b| a.1.claim_id.cmp(&b.1.claim_id));
    if claims.is_empty() {
        println!("(no claims found under {})", claims_dir(root).display());
        return Ok(());
    }
    for (path, c) in claims {
        println!(
            "{:<22}  [{:<12}]  {}  ({})",
            c.claim_id,
            c.status,
            c.title,
            path.display()
        );
    }
    Ok(())
}

pub fn show(root: &Path, claim_id: &str) -> Result<()> {
    let (_, claim) = load(root, claim_id)?;
    println!("{}", serde_yaml::to_string(&claim)?);
    Ok(())
}
