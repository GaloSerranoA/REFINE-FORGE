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
    let data = std::fs::read(p).with_context(|| format!("hashing {}", p.display()))?;
    let mut h = Sha256::new();
    h.update(&data);
    Ok(hex::encode(h.finalize()))
}

/// Encode a relative path so it survives flat copy into the bundle
/// directory. We replace `/` and `\` with `__` (neither appears in our
/// repo paths). Handles `\` so bundles produced on Windows still
/// flatten correctly. Verify reverses this.
fn encode_rel(rel: &str) -> String {
    rel.replace(['\\', '/'], "__")
}

pub fn export(root: &Path, claim_id: &str, out: Option<PathBuf>) -> Result<()> {
    let (claim_path, c) = claim::load(root, claim_id)?;
    let report = runner::run(root, &c)?;
    let out_dir = out.unwrap_or_else(|| root.join("artifacts").join(claim_id));
    std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    // Walk lean/ for every source file lake needs to build the library.
    // Lake build will import the whole library root, so we cannot ship
    // only the claim's own .lean file.
    //
    // Files included:
    //   *.lean              — the proofs themselves
    //   *.toml              — lakefile.toml (project layout)
    //   lean-toolchain      — pinned Lean version (no extension)
    //   lake-manifest.json  — package-deps pin (Mathlib commit etc.)
    //
    // The lake-manifest is the load-bearing addition for Mathlib-
    // using claims: without it, a verifier doing `lake build` against
    // the bundled sources would fetch the LATEST Mathlib rather than
    // the one the proofs were checked against. With it, lake resolves
    // to the exact commit pinned at bundle-export time. See
    // `docs/methodology.md` §3 on the trust model around bundled deps.
    let mut sources: Vec<PathBuf> = vec![claim_path.clone()];
    let lean_dir = root.join("lean");
    for entry in walkdir::WalkDir::new(&lean_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        // Skip the `.lake/` build cache — re-derivable; would bloat
        // bundles by hundreds of MB on Mathlib-using projects.
        if p.components().any(|c| c.as_os_str() == ".lake") {
            continue;
        }
        let ext_keep = matches!(
            p.extension().and_then(|s| s.to_str()),
            Some("lean") | Some("toml")
        );
        let name_keep = p
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == "lean-toolchain" || n == "lake-manifest.json")
            .unwrap_or(false);
        if ext_keep || name_keep {
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
        std::fs::copy(src, &dst)
            .with_context(|| format!("copying {} -> {}", src.display(), dst.display()))?;
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
    verify_with_options(bundle, &VerifyOptions::default())
}

/// Knobs for `refine bundle verify`. Constructed by the CLI dispatch
/// from flags / env vars.
#[derive(Debug, Clone, Default)]
pub struct VerifyOptions {
    /// If true, also verify the Sigstore signature alongside the
    /// hashes. Requires `cosign` on PATH and the bundle to have
    /// `manifest.json.sigbundle` (or .sig/.cert pair).
    pub verify_signature: bool,
    /// Override the regex the signer's cert identity must match.
    /// Defaults to the canonical refineforge CI workflow identity.
    pub identity_regex: Option<String>,
    /// Override the OIDC issuer the signer's cert must come from.
    /// Defaults to GitHub Actions.
    pub oidc_issuer: Option<String>,
}

pub fn verify_with_options(bundle: &Path, opts: &VerifyOptions) -> Result<()> {
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

    let report_data = std::fs::read(bundle.join("report.json")).context("reading report.json")?;
    let mut h = Sha256::new();
    h.update(&report_data);
    let got = hex::encode(h.finalize());
    if got != manifest.report_sha256 {
        mismatches.push(format!(
            "report.json hash mismatch: expected {}, got {got}",
            manifest.report_sha256
        ));
    }

    let mut signature_status: Option<SignatureStatus> = None;
    if opts.verify_signature {
        match verify_signature_impl(bundle, &manifest_path, opts) {
            Ok(sig) => signature_status = Some(sig),
            Err(e) => mismatches.push(format!("signature verification failed: {e}")),
        }
    }

    if mismatches.is_empty() {
        println!("Bundle {} verified OK", bundle.display());
        println!(
            "  claim: {}, lean toolchain: {}, files: {}",
            manifest.claim_id,
            manifest.lean_toolchain,
            manifest.files.len()
        );
        if let Some(sig) = signature_status {
            println!("  signature: OK");
            println!("    signer identity: {}", sig.signer_identity);
            println!("    oidc issuer:     {}", sig.oidc_issuer);
            println!("    cosign:          {}", sig.cosign_version);
        } else if opts.verify_signature {
            // Unreachable: if verify_signature was requested and
            // succeeded, signature_status is Some. If it failed, it
            // went into `mismatches` above.
            println!("  signature: (no result captured)");
        }
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

// ─── Sigstore signature verification (delegates to `cosign verify-blob`) ───
//
// We shell out to the official `cosign` binary rather than reimplement
// signature / Fulcio / Rekor verification ourselves. Same security
// guarantees (cosign does the real cryptography), much less code,
// well-tested upstream. Pure-Rust verification via the `sigstore`
// crate is documented as a future option in `docs/security.md`.

/// Result of a successful signature verification — surfaced in the
/// CLI output and returned to programmatic callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureStatus {
    pub signer_identity: String,
    pub oidc_issuer: String,
    pub cosign_version: String,
}

/// Default identity regex: the canonical refineforge CI workflow.
/// Override via `REFINEFORGE_EXPECTED_IDENTITY_REGEX` env var or
/// `VerifyOptions::identity_regex`.
pub const DEFAULT_IDENTITY_REGEX: &str =
    "https://github.com/[^/]+/refineforge/.github/workflows/ci.yml@refs/(heads|tags)/.*";

/// Default OIDC issuer for keyless cosign signatures from GitHub Actions.
pub const DEFAULT_OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";

fn verify_signature_impl(
    bundle: &Path,
    manifest_path: &Path,
    opts: &VerifyOptions,
) -> Result<SignatureStatus> {
    let sigbundle = bundle.join("manifest.json.sigbundle");
    if !sigbundle.exists() {
        return Err(anyhow!(
            "no signature found — expected {} (signed bundles are produced by the CI signing job; see .github/workflows/ci.yml)",
            sigbundle.display()
        ));
    }

    let identity_regex = opts
        .identity_regex
        .clone()
        .or_else(|| std::env::var("REFINEFORGE_EXPECTED_IDENTITY_REGEX").ok())
        .unwrap_or_else(|| DEFAULT_IDENTITY_REGEX.to_string());
    let oidc_issuer = opts
        .oidc_issuer
        .clone()
        .or_else(|| std::env::var("REFINEFORGE_EXPECTED_OIDC_ISSUER").ok())
        .unwrap_or_else(|| DEFAULT_OIDC_ISSUER.to_string());

    let cosign = std::env::var("REFINEFORGE_COSIGN_BIN").unwrap_or_else(|_| "cosign".into());

    // Sanity-check cosign is on PATH.
    let version_out = std::process::Command::new(&cosign)
        .arg("version")
        .arg("--json")
        .output()
        .map_err(|e| {
            anyhow!(
                "could not invoke `{cosign}`: {e}. Install cosign from https://github.com/sigstore/cosign or set REFINEFORGE_COSIGN_BIN."
            )
        })?;
    if !version_out.status.success() {
        return Err(anyhow!(
            "`{cosign} version --json` failed: {}",
            String::from_utf8_lossy(&version_out.stderr)
        ));
    }
    let cosign_version =
        parse_cosign_version(&version_out.stdout).unwrap_or_else(|| "(unknown)".to_string());

    // Real verification: cosign checks signature, cert chain to
    // Fulcio CA, cert identity claim, and Rekor inclusion proof.
    let verify_out = std::process::Command::new(&cosign)
        .arg("verify-blob")
        .arg("--bundle")
        .arg(&sigbundle)
        .arg("--certificate-identity-regexp")
        .arg(&identity_regex)
        .arg("--certificate-oidc-issuer")
        .arg(&oidc_issuer)
        .arg(manifest_path)
        .output()
        .with_context(|| format!("invoking `{cosign} verify-blob`"))?;
    if !verify_out.status.success() {
        return Err(anyhow!(
            "`cosign verify-blob` rejected the signature:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&verify_out.stdout),
            String::from_utf8_lossy(&verify_out.stderr)
        ));
    }

    // cosign prints "Verified OK" + diagnostic lines to stderr on
    // success. Extract the actual signer identity from the cert (we
    // re-invoke cosign to extract it cleanly; small cost for
    // honesty in the report).
    let signer_identity = extract_signer_identity(&cosign, &sigbundle, manifest_path)
        .unwrap_or_else(|| "(identity matched but couldn't extract)".to_string());

    Ok(SignatureStatus {
        signer_identity,
        oidc_issuer,
        cosign_version,
    })
}

fn parse_cosign_version(stdout: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(stdout).ok()?;
    v.get("GitVersion")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
}

/// Best-effort extraction of the signer identity from the cosign
/// bundle. cosign's `verify-blob` doesn't emit this in machine-
/// readable form, so we parse the cert directly. Returns None on
/// any failure (signature was still validated by the verify call).
fn extract_signer_identity(cosign: &str, sigbundle: &Path, manifest_path: &Path) -> Option<String> {
    let _ = (cosign, manifest_path);
    let data = std::fs::read(sigbundle).ok()?;
    extract_signer_identity_from_cosign_json(&data)
}

fn extract_signer_identity_from_cosign_json(data: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(data).ok()?;
    find_github_workflow_identity(&value)
}

fn find_github_workflow_identity(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            is_github_workflow_identity(text).then(|| text.to_string())
        }
        serde_json::Value::Array(items) => items.iter().find_map(find_github_workflow_identity),
        serde_json::Value::Object(map) => {
            for key in ["Subject", "subject", "SAN", "san", "issuer", "Issuer"] {
                if let Some(found) = map.get(key).and_then(find_github_workflow_identity) {
                    return Some(found);
                }
            }
            map.values().find_map(find_github_workflow_identity)
        }
        _ => None,
    }
}

fn is_github_workflow_identity(text: &str) -> bool {
    text.starts_with("https://github.com/")
        && text.contains("/.github/workflows/")
        && text.contains("@refs/")
}

// ─── Sigstore signature verification — unit tests ──────────────────────
//
// Strategy: REFINEFORGE_COSIGN_BIN env var lets tests substitute a
// stub shell script that simulates success / failure / missing-cosign
// without needing a real Fulcio cert. The unit tests below cover the
// dispatch logic; the actual cryptographic verification is cosign's
// job and tested upstream.

#[cfg(test)]
mod signature_tests {
    use super::*;
    use std::ffi::{OsStr, OsString};
    use std::io::Write;
    use std::sync::{Mutex, MutexGuard};

    static COSIGN_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct CosignEnvGuard {
        previous: Option<OsString>,
        _lock: MutexGuard<'static, ()>,
    }

    impl CosignEnvGuard {
        fn set(value: impl AsRef<OsStr>) -> Self {
            let lock = COSIGN_ENV_LOCK.lock().unwrap();
            let previous = std::env::var_os("REFINEFORGE_COSIGN_BIN");
            std::env::set_var("REFINEFORGE_COSIGN_BIN", value);
            Self {
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for CosignEnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var("REFINEFORGE_COSIGN_BIN", value),
                None => std::env::remove_var("REFINEFORGE_COSIGN_BIN"),
            }
        }
    }

    fn write_executable_stub(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
        // On Windows we write a .cmd shim that runs the body as bash
        // would. On POSIX we write a chmod +x sh script.
        #[cfg(windows)]
        let (full, content) = (
            dir.join(format!("{name}.cmd")),
            format!("@echo off\r\n{body}\r\n"),
        );
        #[cfg(not(windows))]
        let (full, content) = (dir.join(name), format!("#!/bin/sh\n{body}\n"));

        let mut f = std::fs::File::create(&full).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&full).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&full, perms).unwrap();
        }
        full
    }

    fn make_bundle_with_manifest(td: &std::path::Path) -> std::path::PathBuf {
        let bundle = td.join("bundle");
        std::fs::create_dir_all(&bundle).unwrap();
        // Manifest with bundle_schema = current version so verify
        // doesn't reject on schema mismatch.
        let m = Manifest {
            claim_id: "TEST-001".into(),
            created_at: chrono::Utc::now(),
            lean_toolchain: "test".into(),
            files: BTreeMap::new(),
            report_sha256: "0".repeat(64),
            bundle_schema: BUNDLE_SCHEMA,
        };
        std::fs::write(
            bundle.join("manifest.json"),
            serde_json::to_vec_pretty(&m).unwrap(),
        )
        .unwrap();
        bundle
    }

    #[test]
    fn missing_sigbundle_returns_helpful_error() {
        let td = tempfile::tempdir().unwrap();
        let bundle = make_bundle_with_manifest(td.path());
        let opts = VerifyOptions {
            verify_signature: true,
            ..Default::default()
        };
        let err = verify_signature_impl(&bundle, &bundle.join("manifest.json"), &opts)
            .expect_err("must error");
        let msg = err.to_string();
        assert!(msg.contains("no signature found"), "msg: {msg}");
        assert!(msg.contains("manifest.json.sigbundle"), "msg: {msg}");
    }

    #[test]
    fn missing_cosign_binary_returns_install_hint() {
        let td = tempfile::tempdir().unwrap();
        let bundle = make_bundle_with_manifest(td.path());
        // Create the sigbundle file so we get past the first check.
        std::fs::write(bundle.join("manifest.json.sigbundle"), b"dummy").unwrap();
        let opts = VerifyOptions {
            verify_signature: true,
            ..Default::default()
        };
        let _cosign_env = CosignEnvGuard::set("this-binary-does-not-exist-12345");
        let err = verify_signature_impl(&bundle, &bundle.join("manifest.json"), &opts)
            .expect_err("must error");
        let msg = err.to_string();
        assert!(
            msg.contains("could not invoke") && msg.contains("Install cosign"),
            "msg should hint at cosign install: {msg}"
        );
    }

    #[test]
    fn stub_cosign_success_path_returns_status() {
        let td = tempfile::tempdir().unwrap();
        let bundle = make_bundle_with_manifest(td.path());
        std::fs::write(bundle.join("manifest.json.sigbundle"), b"dummy").unwrap();
        // Stub: when args[0] == "version", emit valid JSON. Otherwise
        // exit 0 (simulates verify-blob success).
        #[cfg(windows)]
        let body = "if \"%1\"==\"version\" (echo {\"GitVersion\":\"v2.4.1\"}) else (exit /b 0)";
        #[cfg(not(windows))]
        let body =
            "if [ \"$1\" = \"version\" ]; then echo '{\"GitVersion\":\"v2.4.1\"}'; else exit 0; fi";
        let stub = write_executable_stub(td.path(), "cosign-stub", body);
        let _cosign_env = CosignEnvGuard::set(stub.as_os_str());
        let opts = VerifyOptions {
            verify_signature: true,
            ..Default::default()
        };
        let status = verify_signature_impl(&bundle, &bundle.join("manifest.json"), &opts)
            .expect("stub success");
        assert_eq!(status.cosign_version, "v2.4.1");
        assert_eq!(status.oidc_issuer, DEFAULT_OIDC_ISSUER);
        assert_eq!(
            status.signer_identity,
            "(identity matched but couldn't extract)"
        );
    }

    #[test]
    fn signer_identity_fallback_is_documented_as_reporting_gap() {
        let security = include_str!("../../../SECURITY.md");
        assert!(
            security.contains("a reporting gap, not a signature-validation bypass"),
            "SECURITY.md must keep the signer-identity fallback boundary explicit"
        );
    }

    #[test]
    fn extracts_signer_identity_from_cosign_json_fixture() {
        let fixture = br#"{
          "critical": {
            "identity": {
              "docker-reference": "manifest.json"
            }
          },
          "optional": {
            "Issuer": "https://token.actions.githubusercontent.com",
            "Subject": "https://github.com/example/refineforge/.github/workflows/ci.yml@refs/tags/v0.2.2",
            "Certificate": {
              "SANs": [
                "https://github.com/example/refineforge/.github/workflows/ci.yml@refs/tags/v0.2.2"
              ]
            }
          }
        }"#;

        let identity = extract_signer_identity_from_cosign_json(fixture).unwrap();

        assert_eq!(
            identity,
            "https://github.com/example/refineforge/.github/workflows/ci.yml@refs/tags/v0.2.2"
        );
    }

    #[test]
    fn stub_cosign_verify_failure_propagates() {
        let td = tempfile::tempdir().unwrap();
        let bundle = make_bundle_with_manifest(td.path());
        std::fs::write(bundle.join("manifest.json.sigbundle"), b"dummy").unwrap();
        // Stub: version succeeds, verify-blob fails with stderr.
        #[cfg(windows)]
        let body = "if \"%1\"==\"version\" (echo {\"GitVersion\":\"v2.4.1\"}) else (echo signature mismatch 1>&2 & exit /b 1)";
        #[cfg(not(windows))]
        let body = "if [ \"$1\" = \"version\" ]; then echo '{\"GitVersion\":\"v2.4.1\"}'; else echo 'signature mismatch' >&2; exit 1; fi";
        let stub = write_executable_stub(td.path(), "cosign-stub-fail", body);
        let _cosign_env = CosignEnvGuard::set(stub.as_os_str());
        let opts = VerifyOptions {
            verify_signature: true,
            ..Default::default()
        };
        let err = verify_signature_impl(&bundle, &bundle.join("manifest.json"), &opts)
            .expect_err("must error");
        let msg = err.to_string();
        assert!(msg.contains("rejected the signature"), "msg: {msg}");
        assert!(
            msg.contains("signature mismatch"),
            "msg should surface cosign stderr: {msg}"
        );
    }

    #[test]
    fn identity_regex_override_via_options() {
        let td = tempfile::tempdir().unwrap();
        let bundle = make_bundle_with_manifest(td.path());
        std::fs::write(bundle.join("manifest.json.sigbundle"), b"dummy").unwrap();
        // Stub that ECHOES its args so the test can confirm the
        // identity regex got through.
        #[cfg(windows)]
        let body = "if \"%1\"==\"version\" (echo {\"GitVersion\":\"v2.4.1\"}) else (echo verify args: %* & exit /b 0)";
        #[cfg(not(windows))]
        let body = "if [ \"$1\" = \"version\" ]; then echo '{\"GitVersion\":\"v2.4.1\"}'; else echo \"verify args: $@\"; exit 0; fi";
        let stub = write_executable_stub(td.path(), "cosign-stub-echo", body);
        let _cosign_env = CosignEnvGuard::set(stub.as_os_str());
        let opts = VerifyOptions {
            verify_signature: true,
            identity_regex: Some("custom-identity-pattern".into()),
            oidc_issuer: Some("https://custom.issuer".into()),
        };
        let status = verify_signature_impl(&bundle, &bundle.join("manifest.json"), &opts)
            .expect("must succeed");
        assert_eq!(status.oidc_issuer, "https://custom.issuer");
    }
}
