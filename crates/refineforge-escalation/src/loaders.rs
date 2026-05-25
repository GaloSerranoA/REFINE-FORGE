//! File-system loaders for [`ProjectContext`] + [`ClaimSummary`].
//!
//! Phase 3.5: replaces the Phase-1 honest deferral. The loaders
//! parse:
//! - **claim YAMLs** under `<root>/claims/*.yaml` into
//!   [`ClaimSummary`] (only the fields the engine queries).
//! - **`lean/lake-manifest.json`** into the set of Lake packages
//!   already in the project's trust footprint (Cat 8 check).
//! - **`Cargo.lock`** into a conservative bundle-chain set —
//!   every crate the lockfile pins is treated as in-chain for
//!   v0.3 escalation purposes, on the theory that a typo'd
//!   "not in bundle" hint shouldn't bypass Cat 8.
//!
//! Failures are graceful: a missing file logs nothing and
//! returns an empty default. The caller (the autonomous driver)
//! merges these into a `ProjectContext` and runs the engine.

use crate::action::ClaimStatus;
use crate::context::{ClaimSummary, ProjectContext};
use crate::CRITERIA_VERSION;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoaderError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("yaml parse error in {path}: {source}")]
    Yaml {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("json parse error in {path}: {source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("claim '{claim_id}' not found under {dir}")]
    ClaimNotFound { claim_id: String, dir: String },
}

// =====================================================================
// Claim YAML → ClaimSummary
// =====================================================================

/// Minimal projection of a refineforge claim YAML. Only carries
/// the fields the engine queries; the full schema lives in
/// `refineforge-cli/src/claim.rs`.
#[derive(Debug, Deserialize)]
struct LoaderClaim {
    claim_id: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    rust_source: Vec<LoaderRustSource>,
    #[serde(default)]
    lean: LoaderLean,
    #[serde(default)]
    review: LoaderReview,
}

#[derive(Debug, Default, Deserialize)]
struct LoaderRustSource {
    #[serde(default)]
    types: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct LoaderLean {
    #[serde(default)]
    theorems: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct LoaderReview {
    #[serde(default)]
    human_operator: Option<String>,
}

/// Find and load `<root>/claims/**/<*.yaml>` whose
/// `claim_id` matches `claim_id`.
pub fn load_claim_summary(root: &Path, claim_id: &str) -> Result<ClaimSummary, LoaderError> {
    let dir = root.join("claims");
    if !dir.exists() {
        return Err(LoaderError::ClaimNotFound {
            claim_id: claim_id.into(),
            dir: dir.display().to_string(),
        });
    }
    let entries = walk_yaml(&dir);
    for entry in entries {
        let text = std::fs::read_to_string(&entry).map_err(|e| LoaderError::Io {
            path: entry.display().to_string(),
            source: e,
        })?;
        let parsed: LoaderClaim = serde_yaml::from_str(&text).map_err(|e| LoaderError::Yaml {
            path: entry.display().to_string(),
            source: e,
        })?;
        if parsed.claim_id == claim_id {
            return Ok(claim_summary_from(parsed));
        }
    }
    Err(LoaderError::ClaimNotFound {
        claim_id: claim_id.into(),
        dir: dir.display().to_string(),
    })
}

fn claim_summary_from(c: LoaderClaim) -> ClaimSummary {
    let scope_model_only = matches!(
        c.scope.trim().to_lowercase().as_str(),
        "model-only" | "model_only"
    );
    let status = parse_status(&c.status);
    let mut lean_theorems = HashSet::new();
    for t in c.lean.theorems {
        lean_theorems.insert(t);
    }
    let mut rust_source_types = HashSet::new();
    for src in c.rust_source {
        for t in src.types {
            rust_source_types.insert(t);
        }
    }
    ClaimSummary {
        id: c.claim_id,
        status,
        scope_model_only,
        lean_theorems,
        rust_source_types,
        review_human_operator: c.review.human_operator,
    }
}

/// Map a free-text `status:` field from the YAML into the
/// engine's [`ClaimStatus`] enum. Unrecognised values default
/// to `Drafted` (conservative: the engine treats unknown
/// status transitions as escalatable per Cat 5).
fn parse_status(s: &str) -> ClaimStatus {
    match s.trim().to_lowercase().as_str() {
        "unformalized" | "unformalised" => ClaimStatus::Unformalized,
        "drafted" | "draft" => ClaimStatus::Drafted,
        "broken" | "build_failed" | "policy_violation" => ClaimStatus::Broken,
        "proven" | "verified" | "builds" => ClaimStatus::ProvenModelOnly,
        "proven_model_and_refined" | "proven_refined" | "refined" => {
            ClaimStatus::ProvenModelAndRefined
        }
        _ => ClaimStatus::Drafted,
    }
}

fn walk_yaml(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "yaml").unwrap_or(false) {
                out.push(path);
            } else if path.is_dir() {
                out.extend(walk_yaml(&path));
            }
        }
    }
    out
}

// =====================================================================
// lake-manifest.json → (lake_packages, mathlib_imports)
// =====================================================================

#[derive(Debug, Deserialize)]
struct LakeManifest {
    #[serde(default)]
    packages: Vec<LakeManifestPackage>,
}

#[derive(Debug, Deserialize)]
struct LakeManifestPackage {
    name: String,
}

/// Read `<root>/lean/lake-manifest.json` and return the set of
/// Lake-package names already in the manifest. Missing file
/// returns an empty set. Used by the engine to detect first-time
/// Mathlib (or any registry-package) use per criteria v0.3.
pub fn load_lake_manifest_packages(root: &Path) -> Result<HashSet<String>, LoaderError> {
    let path = root.join("lean").join("lake-manifest.json");
    if !path.exists() {
        return Ok(HashSet::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| LoaderError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let parsed: LakeManifest = serde_json::from_str(&text).map_err(|e| LoaderError::Json {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(parsed.packages.into_iter().map(|p| p.name).collect())
}

// =====================================================================
// Cargo.lock → conservative bundle-chain set
// =====================================================================

/// Read `<root>/Cargo.lock` and return every pinned crate name.
///
/// **Conservative choice**: every crate the lockfile pins is
/// treated as "in bundle chain" for engine purposes. A
/// per-bundle audit of which crates actually ship in produced
/// bundles is more accurate but substantially more work; the
/// conservative default over-escalates rather than
/// under-escalates, which matches the criteria-doc doctrine
/// (§2 "Conservative by default").
pub fn load_cargo_lock_bundle_chain(root: &Path) -> Result<HashSet<String>, LoaderError> {
    let path = root.join("Cargo.lock");
    if !path.exists() {
        return Ok(HashSet::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| LoaderError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    // Use toml-parse via serde — but we don't have the `toml`
    // crate dep. Hand-parse: Cargo.lock has a stable enough
    // shape that we can extract `name = "..."` lines under
    // `[[package]]` headers without a full TOML parser.
    let mut names = HashSet::new();
    let mut in_package = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line == "[[package]]" {
            in_package = true;
            continue;
        }
        if line.starts_with('[') {
            in_package = false;
            continue;
        }
        if in_package {
            if let Some(rest) = line.strip_prefix("name = \"") {
                if let Some(end) = rest.find('"') {
                    names.insert(rest[..end].to_string());
                }
            }
        }
    }
    Ok(names)
}

// =====================================================================
// Top-level: build a ProjectContext from disk
// =====================================================================

/// Load a populated [`ProjectContext`] from `root`. Any
/// sub-load that fails gracefully (missing file) contributes
/// an empty default; a real read/parse error propagates.
///
/// The `claim_id`, if provided, additionally loads the named
/// claim's summary into `ctx.claim`.
pub fn load_project_context(
    root: &Path,
    claim_id: Option<&str>,
) -> Result<ProjectContext, LoaderError> {
    let mut ctx = ProjectContext::test_default();
    ctx.criteria_version = CRITERIA_VERSION.into();
    ctx.lake_packages_existing = load_lake_manifest_packages(root)?;
    ctx.bundle_chain_crates = load_cargo_lock_bundle_chain(root)?;
    if let Some(id) = claim_id {
        let summary = load_claim_summary(root, id)?;
        ctx.claim = Some(summary);
    }
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(p: &std::path::Path, content: &str) {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }

    #[test]
    fn load_claim_summary_finds_id_by_yaml_field() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("claims/example.yaml"),
            "claim_id: EXAMPLE-001\nscope: tutorial\nstatus: verified\nlean:\n  toolchain: x\n  module: M\n  file: m.lean\n  theorems: [t1, t2]\n",
        );
        let s = load_claim_summary(root, "EXAMPLE-001").unwrap();
        assert_eq!(s.id, "EXAMPLE-001");
        assert_eq!(s.status, ClaimStatus::ProvenModelOnly);
        assert!(s.lean_theorems.contains("t1"));
        assert!(s.lean_theorems.contains("t2"));
    }

    #[test]
    fn load_claim_summary_model_only_scope_flag() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("claims/foo.yaml"),
            "claim_id: X-001\nscope: model-only\nstatus: drafted\nlean:\n  toolchain: x\n  module: M\n  file: m.lean\n  theorems: []\n",
        );
        let s = load_claim_summary(root, "X-001").unwrap();
        assert!(s.scope_model_only);
    }

    #[test]
    fn load_claim_summary_refined_scope_not_model_only() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("claims/foo.yaml"),
            "claim_id: X-001\nscope: model+refined\nstatus: drafted\nlean:\n  toolchain: x\n  module: M\n  file: m.lean\n  theorems: []\n",
        );
        let s = load_claim_summary(root, "X-001").unwrap();
        assert!(!s.scope_model_only);
    }

    #[test]
    fn load_claim_summary_collects_rust_source_types() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("claims/foo.yaml"),
            "claim_id: X-001\nlean:\n  toolchain: x\n  module: M\n  file: m.lean\n  theorems: []\nrust_source:\n  - path: a.rs\n    types: [Foo, Bar]\n  - path: b.rs\n    types: [Baz]\n",
        );
        let s = load_claim_summary(root, "X-001").unwrap();
        assert_eq!(s.rust_source_types.len(), 3);
        assert!(s.rust_source_types.contains("Foo"));
        assert!(s.rust_source_types.contains("Bar"));
        assert!(s.rust_source_types.contains("Baz"));
    }

    #[test]
    fn load_claim_summary_missing_returns_not_found() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("claims")).unwrap();
        let err = load_claim_summary(root, "ZZZ-999").unwrap_err();
        assert!(matches!(err, LoaderError::ClaimNotFound { .. }));
    }

    #[test]
    fn load_claim_summary_missing_dir_returns_not_found() {
        let tmp = tempdir().unwrap();
        let err = load_claim_summary(tmp.path(), "X-001").unwrap_err();
        assert!(matches!(err, LoaderError::ClaimNotFound { .. }));
    }

    #[test]
    fn parse_status_known_values_map_correctly() {
        assert_eq!(parse_status("verified"), ClaimStatus::ProvenModelOnly);
        assert_eq!(parse_status("drafted"), ClaimStatus::Drafted);
        assert_eq!(parse_status("broken"), ClaimStatus::Broken);
        assert_eq!(parse_status("unformalized"), ClaimStatus::Unformalized);
        assert_eq!(parse_status("refined"), ClaimStatus::ProvenModelAndRefined);
        assert_eq!(parse_status("UNKNOWN_THING"), ClaimStatus::Drafted);
    }

    #[test]
    fn load_lake_manifest_returns_package_set() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("lean/lake-manifest.json"),
            r#"{"version": 7, "packagesDir": ".lake/packages", "packages": [
                {"name": "mathlib", "type": "git"},
                {"name": "std", "type": "git"}
            ]}"#,
        );
        let pkgs = load_lake_manifest_packages(root).unwrap();
        assert_eq!(pkgs.len(), 2);
        assert!(pkgs.contains("mathlib"));
        assert!(pkgs.contains("std"));
    }

    #[test]
    fn load_lake_manifest_missing_returns_empty_set() {
        let tmp = tempdir().unwrap();
        let pkgs = load_lake_manifest_packages(tmp.path()).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn load_cargo_lock_collects_pinned_crate_names() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("Cargo.lock"),
            r#"version = 3

[[package]]
name = "serde"
version = "1.0"

[[package]]
name = "sha2"
version = "0.10"

[other-section]
name = "ignored"
"#,
        );
        let pins = load_cargo_lock_bundle_chain(root).unwrap();
        assert_eq!(pins.len(), 2);
        assert!(pins.contains("serde"));
        assert!(pins.contains("sha2"));
        assert!(!pins.contains("ignored"));
    }

    #[test]
    fn load_cargo_lock_missing_returns_empty_set() {
        let tmp = tempdir().unwrap();
        let pins = load_cargo_lock_bundle_chain(tmp.path()).unwrap();
        assert!(pins.is_empty());
    }

    #[test]
    fn load_project_context_populates_from_disk() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("claims/example.yaml"),
            "claim_id: EXAMPLE-001\nscope: tutorial\nstatus: verified\nlean:\n  toolchain: x\n  module: M\n  file: m.lean\n  theorems: [t1]\n",
        );
        write(
            &root.join("lean/lake-manifest.json"),
            r#"{"packages": [{"name": "mathlib", "type": "git"}]}"#,
        );
        write(
            &root.join("Cargo.lock"),
            "[[package]]\nname = \"serde\"\nversion = \"1.0\"\n",
        );
        let ctx = load_project_context(root, Some("EXAMPLE-001")).unwrap();
        assert_eq!(ctx.criteria_version, CRITERIA_VERSION);
        assert_eq!(ctx.claim.as_ref().unwrap().id, "EXAMPLE-001");
        assert!(ctx.lake_packages_existing.contains("mathlib"));
        assert!(ctx.bundle_chain_crates.contains("serde"));
    }

    #[test]
    fn load_project_context_without_claim_id_works() {
        let tmp = tempdir().unwrap();
        let ctx = load_project_context(tmp.path(), None).unwrap();
        assert!(ctx.claim.is_none());
        assert_eq!(ctx.criteria_version, CRITERIA_VERSION);
    }

    /// Integration: load the real EXAMPLE-001 claim from the
    /// refineforge repo's own claims/ directory. This test is
    /// the "no-mocks" verification that the loader works
    /// against the actual schema the project ships with.
    #[test]
    fn integration_loads_real_example_001_claim_from_repo() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("walk to repo root");
        let summary = match load_claim_summary(repo_root, "EXAMPLE-001") {
            Ok(s) => s,
            Err(LoaderError::ClaimNotFound { .. }) => {
                eprintln!(
                    "skipping integration test: EXAMPLE-001 not found at {}",
                    repo_root.display()
                );
                return;
            }
            Err(e) => panic!("unexpected error: {:?}", e),
        };
        assert_eq!(summary.id, "EXAMPLE-001");
    }
}
