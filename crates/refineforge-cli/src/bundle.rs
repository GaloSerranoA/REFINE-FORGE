//! Proof bundle: a directory containing everything a third party
//! needs to re-verify a claim, plus a `manifest.json` listing every
//! file with its SHA-256.
//!
//! Verification model:
//!   * `bundle export` copies all source files, runs the proof, and
//!     records `manifest.json` + `report.json`.
//!   * `bundle verify` re-hashes every file listed in the manifest
//!     and confirms it matches. It does NOT re-run Lean — that is
//!     the verifier's responsibility, and is the whole point of
//!     pinning `lean-toolchain`. The bundle is a sealed input; the
//!     verifier supplies the Lean compiler.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{claim, runner};

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub claim_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub lean_toolchain: String,
    /// Map of `relative-path-in-source-repo` → `sha256 hex`.
    pub files: BTreeMap<String, String>,
    /// SHA-256 of `report.json` itself.
    pub report_sha256: String,
    /// Bundle schema version, for future migrations.
    pub bundle_schema: u32,
}

const BUNDLE_SCHEMA: u32 = 1;

fn sha256_file(p: &Path) -> Result<String> {
    let data =
        std::fs::read(p).with_context(|| format!("hashing {}", p.display()))?;
    let mut h = Sha256::new();
    h.update(&data);
    Ok(hex::encode(h.finalize()))
}

/// Encode a relative path so it survives flat copy into the bundle
/// directory. We replace `/` and `\` with `__` (neither appears in our
/// repo paths). Handles `\` so bundles produced on Windows still
/// flatten correctly. Verify reverses this.
fn encode_rel(rel: &str) -> String {
    rel.replace('\\', "__").replace('/', "__")
}

pub fn export(root: &Path, claim_id: &str, out: Option<PathBuf>) -> Result<()> {
    let (claim_path, c) = claim::load(root, claim_id)?;
    let report = runner::run(root, &c)?;
    let out_dir = out.unwrap_or_else(|| root.join("artifacts").join(claim_id));
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("creating {}", out_dir.display()))?;

    // Walk lean/ for every source file lake needs to build the library.
    // Lake build will import the whole library root, so we cannot ship
    // only the claim's own .lean file.
    let mut sources: Vec<PathBuf> = vec![claim_path.clone()];
    let lean_dir = root.join("lean");
    for entry in walkdir::WalkDir::new(&lean_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let keep = matches!(
            p.extension().and_then(|s| s.to_str()),
            Some("lean") | Some("toml")
        ) || p.file_name().map(|n| n == "lean-toolchain").unwrap_or(false);
        if keep {
            sources.push(p.to_path_buf());
        }
    }
    // Include the refinement-argument doc when present — it's the
    // trust-critical artifact the verifier reads after Lake succeeds.
    let refinement = root
        .join("docs")
        .join("refinement")
        .join(format!("{claim_id}.md"));
    if refinement.exists() {
        sources.push(refinement);
    }
    // Stable order so the manifest is deterministic.
    sources.sort();
    sources.dedup();

    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for src in &sources {
        if !src.exists() {
            return Err(anyhow!("bundle source missing: {}", src.display()));
        }
        let rel = src
            .strip_prefix(root)
            .unwrap_or(src)
            .to_string_lossy()
            .replace('\\', "/");
        let h = sha256_file(src)?;
        let dst = out_dir.join(encode_rel(&rel));
        std::fs::copy(src, &dst).with_context(|| {
            format!("copying {} -> {}", src.display(), dst.display())
        })?;
        files.insert(rel, h);
    }

    let report_json = serde_json::to_vec_pretty(&report)?;
    let report_path = out_dir.join("report.json");
    std::fs::write(&report_path, &report_json)?;
    let mut h = Sha256::new();
    h.update(&report_json);
    let report_sha = hex::encode(h.finalize());

    let manifest = Manifest {
        claim_id: c.claim_id.clone(),
        created_at: chrono::Utc::now(),
        lean_toolchain: c.lean.toolchain.clone(),
        files,
        report_sha256: report_sha,
        bundle_schema: BUNDLE_SCHEMA,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    std::fs::write(out_dir.join("manifest.json"), &manifest_bytes)?;

    // Convenience: also write a human-readable verify-me note.
    let readme = format!(
        "refineforge proof bundle: {claim_id}
============================================

This bundle contains every source file required to reproduce the
Lean verification. Every file is hashed in manifest.json (SHA-256).
Paths in the manifest use the repo-relative form; on disk they are
flattened with `/` replaced by `__` so the bundle stays a single
directory.

To independently verify this bundle:

  1. Install elan: https://github.com/leanprover/elan
  2. Reconstruct the source layout. For each entry KEY in
     manifest.json `files`:
       - read the on-disk file `<KEY with '/' → '__'>`
       - write it to `<KEY>` relative to a fresh working directory
     A short shell loop does this:
       for k in $(jq -r '.files | keys[]' manifest.json); do
         mkdir -p \"$(dirname \"$k\")\"
         cp \"${{k//\\//__}}\" \"$k\"
       done
  3. Run:  cd lean && lake build
  4. Re-hash this bundle with `refine bundle verify <this-dir>`
     and confirm every entry in manifest.json matches.

Pinned Lean toolchain: {toolchain}
Bundle schema: {schema}
",
        claim_id = claim_id,
        toolchain = manifest.lean_toolchain,
        schema = BUNDLE_SCHEMA
    );
    std::fs::write(out_dir.join("VERIFY.txt"), readme)?;

    println!("Bundle exported to {}", out_dir.display());
    println!("  report status: {:?}", report.status);
    println!("  files in manifest: {}", manifest.files.len());
    Ok(())
}

pub fn verify(bundle: &Path) -> Result<()> {
    let manifest_path = bundle.join("manifest.json");
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?,
    )?;

    if manifest.bundle_schema != BUNDLE_SCHEMA {
        return Err(anyhow!(
            "bundle schema mismatch: bundle is v{}, tool understands v{}",
            manifest.bundle_schema,
            BUNDLE_SCHEMA
        ));
    }

    let mut mismatches: Vec<String> = Vec::new();
    for (rel, expected) in &manifest.files {
        let local = bundle.join(encode_rel(rel));
        if !local.exists() {
            mismatches.push(format!("missing in bundle: {rel}"));
            continue;
        }
        let got = sha256_file(&local)?;
        if &got != expected {
            mismatches.push(format!(
                "hash mismatch for {rel}: expected {expected}, got {got}"
            ));
        }
    }

    let report_data = std::fs::read(bundle.join("report.json"))
        .context("reading report.json")?;
    let mut h = Sha256::new();
    h.update(&report_data);
    let got = hex::encode(h.finalize());
    if got != manifest.report_sha256 {
        mismatches.push(format!(
            "report.json hash mismatch: expected {}, got {got}",
            manifest.report_sha256
        ));
    }

    if mismatches.is_empty() {
        println!("Bundle {} verified OK", bundle.display());
        println!(
            "  claim: {}, lean toolchain: {}, files: {}",
            manifest.claim_id,
            manifest.lean_toolchain,
            manifest.files.len()
        );
        Ok(())
    } else {
        for m in &mismatches {
            eprintln!("{m}");
        }
        Err(anyhow!(
            "bundle verification failed ({} issue(s))",
            mismatches.len()
        ))
    }
}
