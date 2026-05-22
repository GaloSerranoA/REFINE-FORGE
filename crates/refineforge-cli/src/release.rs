use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Passed,
    Failed,
    Skipped,
    Blocked,
}

impl GateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GateStatus::Passed => "passed",
            GateStatus::Failed => "failed",
            GateStatus::Skipped => "skipped",
            GateStatus::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateReport {
    pub name: String,
    pub command: Vec<String>,
    pub status: GateStatus,
    pub required: bool,
    pub duration_ms: u128,
    pub log_path: Option<PathBuf>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum SignatureEvidence {
    Unsigned,
    Skipped {
        reason: String,
    },
    Verified {
        sigbundle: PathBuf,
        signer_identity: Option<String>,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleEvidence {
    pub claim_id: String,
    pub bundle_dir: PathBuf,
    pub manifest_sha256: String,
    pub signature: SignatureEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseReport {
    pub requested_version: String,
    pub generated_at: DateTime<Utc>,
    pub git_commit: Option<String>,
    pub git_branch: Option<String>,
    pub dirty_tree: bool,
    pub host_os: String,
    pub evidence_dir: PathBuf,
    pub gates: Vec<GateReport>,
    pub bundles: Vec<BundleEvidence>,
}

impl ReleaseReport {
    pub fn new(requested_version: String, evidence_dir: PathBuf) -> Self {
        Self {
            requested_version,
            generated_at: Utc::now(),
            git_commit: None,
            git_branch: None,
            dirty_tree: false,
            host_os: std::env::consts::OS.to_string(),
            evidence_dir,
            gates: Vec::new(),
            bundles: Vec::new(),
        }
    }

    #[cfg(test)]
    fn test_fixture(version: &str) -> Self {
        Self {
            requested_version: version.into(),
            generated_at: DateTime::parse_from_rfc3339("2026-05-22T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            git_commit: Some("abc123".into()),
            git_branch: Some("codex/release-infrastructure-track".into()),
            dirty_tree: false,
            host_os: "test-os".into(),
            evidence_dir: PathBuf::from("release/evidence/test"),
            gates: Vec::new(),
            bundles: Vec::new(),
        }
    }

    pub fn required_gates_succeeded(&self) -> bool {
        self.gates
            .iter()
            .filter(|gate| gate.required)
            .all(|gate| matches!(gate.status, GateStatus::Passed | GateStatus::Skipped))
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# refineforge release readiness report\n\n");
        out.push_str(&format!("- Version: `{}`\n", self.requested_version));
        out.push_str(&format!(
            "- Generated at: `{}`\n",
            self.generated_at.to_rfc3339()
        ));
        out.push_str(&format!(
            "- Git commit: `{}`\n",
            self.git_commit.as_deref().unwrap_or("unknown")
        ));
        out.push_str(&format!(
            "- Git branch: `{}`\n",
            self.git_branch.as_deref().unwrap_or("unknown")
        ));
        out.push_str(&format!("- Dirty tree: `{}`\n", self.dirty_tree));
        out.push_str(&format!("- Host OS: `{}`\n\n", self.host_os));

        out.push_str("## Gates\n\n");
        out.push_str("| Gate | Status | Required | Message |\n");
        out.push_str("|---|---|---:|---|\n");
        for gate in &self.gates {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                gate.name,
                gate.status.as_str(),
                if gate.required { "yes" } else { "no" },
                gate.message.as_deref().unwrap_or("")
            ));
        }

        out.push_str("\n## Bundles\n\n");
        out.push_str("| Claim | Manifest SHA-256 | Signature |\n");
        out.push_str("|---|---|---|\n");
        for bundle in &self.bundles {
            out.push_str(&format!(
                "| {} | `{}` | {} |\n",
                bundle.claim_id,
                bundle.manifest_sha256,
                signature_label(&bundle.signature)
            ));
        }
        out
    }
}

fn signature_label(sig: &SignatureEvidence) -> &'static str {
    match sig {
        SignatureEvidence::Unsigned => "unsigned",
        SignatureEvidence::Skipped { .. } => "skipped",
        SignatureEvidence::Verified { .. } => "verified",
        SignatureEvidence::Failed { .. } => "failed",
    }
}

pub fn gate_log_name(name: &str) -> String {
    let re = Regex::new("[^A-Za-z0-9]+").unwrap();
    let trimmed = name.trim().to_ascii_lowercase();
    let collapsed = re.replace_all(&trimmed, "-");
    collapsed.trim_matches('-').to_string() + ".log"
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut h = Sha256::new();
    h.update(data);
    Ok(hex::encode(h.finalize()))
}

pub fn write_evidence(
    report: &ReleaseReport,
    sbom: &serde_json::Value,
    provenance: &serde_json::Value,
) -> Result<()> {
    std::fs::create_dir_all(&report.evidence_dir)
        .with_context(|| format!("creating {}", report.evidence_dir.display()))?;

    let report_json = serde_json::to_vec_pretty(report)?;
    std::fs::write(report.evidence_dir.join("release-report.json"), report_json)?;
    std::fs::write(
        report.evidence_dir.join("release-report.md"),
        report.to_markdown(),
    )?;
    std::fs::write(
        report.evidence_dir.join("sbom.cyclonedx.json"),
        serde_json::to_vec_pretty(sbom)?,
    )?;
    std::fs::write(
        report.evidence_dir.join("provenance.intoto.json"),
        serde_json::to_vec_pretty(provenance)?,
    )?;
    Ok(())
}

pub fn sbom_from_cargo_metadata(
    metadata: &serde_json::Value,
    version: &str,
) -> Result<serde_json::Value> {
    let packages = metadata["packages"]
        .as_array()
        .context("cargo metadata missing packages array")?;

    let components: Vec<serde_json::Value> = packages
        .iter()
        .map(|pkg| {
            let name = pkg["name"].as_str().unwrap_or("unknown");
            let version = pkg["version"].as_str().unwrap_or("0.0.0");
            let license = pkg["license"].as_str().unwrap_or("NOASSERTION");
            serde_json::json!({
                "type": "library",
                "name": name,
                "version": version,
                "licenses": [{"license": {"id": license}}],
                "purl": format!("pkg:cargo/{name}@{version}")
            })
        })
        .collect();

    Ok(serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": "refineforge",
                "version": version
            }
        },
        "components": components
    }))
}

pub fn provenance_from_report(report: &ReleaseReport) -> serde_json::Value {
    let subjects: Vec<serde_json::Value> = report
        .bundles
        .iter()
        .map(|bundle| {
            serde_json::json!({
                "name": format!("{}/manifest.json", bundle.claim_id),
                "digest": {"sha256": bundle.manifest_sha256}
            })
        })
        .collect();

    serde_json::json!({
        "_type": "https://in-toto.io/Statement/v1",
        "subject": subjects,
        "predicateType": "https://slsa.dev/provenance/v1",
        "predicate": {
            "builder": {"id": "refineforge-local-release-ready"},
            "buildType": "https://github.com/galo/refineforge/release-ready",
            "invocation": {
                "parameters": {
                    "requested_version": report.requested_version
                },
                "environment": {
                    "host_os": report.host_os
                }
            },
            "materials": [
                {
                    "uri": "git+HEAD",
                    "digest": {
                        "sha1": report
                            .git_commit
                            .clone()
                            .unwrap_or_else(|| "unknown".into())
                    }
                },
                {"uri": "file:Cargo.lock"},
                {"uri": "file:lean/lean-toolchain"}
            ],
            "metadata": {
                "buildStartedOn": report.generated_at.to_rfc3339(),
                "completeness": {
                    "parameters": true,
                    "environment": false,
                    "materials": true
                }
            }
        }
    })
}

#[derive(Debug, Clone)]
pub struct ReleaseReadyOptions {
    pub version: String,
    pub evidence_dir: PathBuf,
    pub dry_run: bool,
    pub allow_dirty: bool,
    pub skip_docker: bool,
    pub skip_signature: bool,
    pub ci: bool,
}

pub fn ready(root: &Path, opts: ReleaseReadyOptions) -> Result<()> {
    let report = build_ready_report(root, &opts)?;
    let metadata = cargo_metadata_json(root, opts.dry_run)?;
    let sbom = sbom_from_cargo_metadata(&metadata, &opts.version)?;
    let provenance = provenance_from_report(&report);
    write_evidence(&report, &sbom, &provenance)?;

    println!("release evidence: {}", report.evidence_dir.display());
    println!(
        "release report:   {}",
        report.evidence_dir.join("release-report.md").display()
    );

    if report.required_gates_succeeded() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "release readiness failed; see {}",
            report.evidence_dir.display()
        ))
    }
}

pub fn build_ready_report(root: &Path, opts: &ReleaseReadyOptions) -> Result<ReleaseReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut report = ReleaseReport::new(opts.version.clone(), opts.evidence_dir.clone());
    std::fs::create_dir_all(report.evidence_dir.join("logs"))
        .with_context(|| format!("creating {}", report.evidence_dir.join("logs").display()))?;

    fill_git_context(&root, &mut report, opts)?;
    push_semver_gate(&mut report, &opts.version);
    push_tag_available_gate(&root, &mut report, opts)?;
    push_command_gate(
        &root,
        &mut report,
        opts,
        "cargo-metadata-locked",
        &["cargo", "metadata", "--locked", "--format-version", "1"],
        true,
    )?;

    for package in [
        "refineforge-cli",
        "refineforge-derive",
        "example-counter",
        "example-capability",
    ] {
        push_command_gate(
            &root,
            &mut report,
            opts,
            &format!("cargo-test-{package}"),
            &["cargo", "test", "-p", package],
            true,
        )?;
    }

    for (name, args) in [
        (
            "lean-check-all",
            vec![
                "cargo",
                "run",
                "-p",
                "refineforge-cli",
                "--bin",
                "refine",
                "--",
                "lean",
                "check-all",
            ],
        ),
        (
            "scan-check-all",
            vec![
                "cargo",
                "run",
                "-p",
                "refineforge-cli",
                "--bin",
                "refine",
                "--",
                "scan",
                "check-all",
            ],
        ),
        (
            "lint-check-all",
            vec![
                "cargo",
                "run",
                "-p",
                "refineforge-cli",
                "--bin",
                "refine",
                "--",
                "lint",
                "check-all",
            ],
        ),
    ] {
        push_command_gate(&root, &mut report, opts, name, &args, true)?;
    }

    for claim_id in ["EXAMPLE-001", "EXAMPLE-002", "EXAMPLE-003"] {
        push_bundle_gates(&root, &mut report, opts, claim_id)?;
    }

    push_docs_audit_gate(&root, &mut report)?;
    push_docker_gate(&root, &mut report, opts)?;
    push_signature_gate(&mut report, opts);

    Ok(report)
}

fn cargo_metadata_json(root: &Path, dry_run: bool) -> Result<serde_json::Value> {
    if dry_run {
        return Ok(serde_json::json!({
            "packages": [],
            "workspace_members": []
        }));
    }

    let output = Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--locked", "--format-version", "1"])
        .output()
        .context("running cargo metadata --locked")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "cargo metadata --locked failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn fill_git_context(
    root: &Path,
    report: &mut ReleaseReport,
    opts: &ReleaseReadyOptions,
) -> Result<()> {
    let git_available = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output();
    match git_available {
        Ok(output) if output.status.success() => report.gates.push(GateReport {
            name: "git-worktree".into(),
            command: vec![
                "git".into(),
                "rev-parse".into(),
                "--is-inside-work-tree".into(),
            ],
            status: GateStatus::Passed,
            required: true,
            duration_ms: 0,
            log_path: None,
            message: Some("inside git worktree".into()),
        }),
        Ok(output) => {
            report.gates.push(GateReport {
                name: "git-worktree".into(),
                command: vec![
                    "git".into(),
                    "rev-parse".into(),
                    "--is-inside-work-tree".into(),
                ],
                status: GateStatus::Failed,
                required: true,
                duration_ms: 0,
                log_path: None,
                message: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            });
            return Ok(());
        }
        Err(e) => {
            report.gates.push(GateReport {
                name: "git-worktree".into(),
                command: vec![
                    "git".into(),
                    "rev-parse".into(),
                    "--is-inside-work-tree".into(),
                ],
                status: GateStatus::Blocked,
                required: true,
                duration_ms: 0,
                log_path: None,
                message: Some(e.to_string()),
            });
            return Ok(());
        }
    }

    report.git_commit = command_stdout(root, "git", &["rev-parse", "HEAD"]).ok();
    report.git_branch = command_stdout(root, "git", &["symbolic-ref", "--short", "HEAD"]).ok();

    let status = command_stdout(root, "git", &["status", "--porcelain"]).unwrap_or_default();
    report.dirty_tree = !status.trim().is_empty();
    let (status, message) = if report.dirty_tree && !opts.allow_dirty {
        (GateStatus::Failed, "working tree is dirty".to_string())
    } else if report.dirty_tree {
        (GateStatus::Skipped, "allowed by --allow-dirty".to_string())
    } else {
        (GateStatus::Passed, "working tree is clean".to_string())
    };
    report.gates.push(GateReport {
        name: "git-clean".into(),
        command: vec!["git".into(), "status".into(), "--porcelain".into()],
        status,
        required: true,
        duration_ms: 0,
        log_path: None,
        message: Some(message),
    });

    Ok(())
}

fn command_stdout(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .current_dir(root)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "{} failed: {}",
            program,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn is_cargo_program(program: &str) -> bool {
    Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("cargo") || name.eq_ignore_ascii_case("cargo.exe"))
        .unwrap_or(false)
}

fn release_cargo_target_dir(root: &Path) -> PathBuf {
    root.join("target").join("release-ready")
}

fn push_semver_gate(report: &mut ReleaseReport, version: &str) {
    let re = Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$").unwrap();
    let passed = re.is_match(version);
    report.gates.push(GateReport {
        name: "version-semver".into(),
        command: vec!["semver".into(), version.into()],
        status: if passed {
            GateStatus::Passed
        } else {
            GateStatus::Failed
        },
        required: true,
        duration_ms: 0,
        log_path: None,
        message: Some(if passed {
            "valid semver".into()
        } else {
            "version must be X.Y.Z or X.Y.Z-prerelease".into()
        }),
    });
}

fn push_tag_available_gate(
    root: &Path,
    report: &mut ReleaseReport,
    opts: &ReleaseReadyOptions,
) -> Result<()> {
    let tag = format!("v{}", opts.version);
    if opts.dry_run {
        report.gates.push(GateReport {
            name: "tag-available".into(),
            command: vec!["git".into(), "rev-parse".into(), tag],
            status: GateStatus::Skipped,
            required: true,
            duration_ms: 0,
            log_path: None,
            message: Some("dry-run".into()),
        });
        return Ok(());
    }

    let start = Instant::now();
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "-q", "--verify", &format!("refs/tags/{tag}")])
        .output();
    let status = match output {
        Ok(output) if output.status.success() => GateStatus::Failed,
        Ok(_) => GateStatus::Passed,
        Err(_) => GateStatus::Blocked,
    };
    report.gates.push(GateReport {
        name: "tag-available".into(),
        command: vec!["git".into(), "rev-parse".into(), tag],
        status,
        required: true,
        duration_ms: start.elapsed().as_millis(),
        log_path: None,
        message: Some(match status {
            GateStatus::Passed => "tag is available".into(),
            GateStatus::Failed => "tag already exists".into(),
            GateStatus::Blocked => "could not invoke git".into(),
            GateStatus::Skipped => "dry-run".into(),
        }),
    });
    Ok(())
}

fn push_command_gate(
    root: &Path,
    report: &mut ReleaseReport,
    opts: &ReleaseReadyOptions,
    name: &str,
    args: &[&str],
    required: bool,
) -> Result<()> {
    let log_path = report.evidence_dir.join("logs").join(gate_log_name(name));
    if opts.dry_run {
        write_log(&log_path, "dry-run: command not executed\n")?;
        report.gates.push(GateReport {
            name: name.into(),
            command: args.iter().map(|arg| (*arg).to_string()).collect(),
            status: GateStatus::Skipped,
            required,
            duration_ms: 0,
            log_path: Some(log_path),
            message: Some("dry-run".into()),
        });
        return Ok(());
    }

    let Some((program, cmd_args)) = args.split_first() else {
        report.gates.push(GateReport {
            name: name.into(),
            command: Vec::new(),
            status: GateStatus::Failed,
            required,
            duration_ms: 0,
            log_path: None,
            message: Some("empty command".into()),
        });
        return Ok(());
    };

    let cargo_target_dir = is_cargo_program(program).then(|| release_cargo_target_dir(root));
    let start = Instant::now();
    let mut command = Command::new(program);
    command.current_dir(root).args(cmd_args);
    if let Some(target_dir) = &cargo_target_dir {
        command.env("CARGO_TARGET_DIR", target_dir);
    }
    let output = command.output();
    let duration_ms = start.elapsed().as_millis();
    match output {
        Ok(output) => {
            let mut log = String::new();
            log.push_str("$ ");
            log.push_str(&args.join(" "));
            if let Some(target_dir) = &cargo_target_dir {
                log.push_str("\n[env]\nCARGO_TARGET_DIR=");
                log.push_str(&target_dir.display().to_string());
            }
            log.push_str("\n\n[stdout]\n");
            log.push_str(&String::from_utf8_lossy(&output.stdout));
            log.push_str("\n[stderr]\n");
            log.push_str(&String::from_utf8_lossy(&output.stderr));
            write_log(&log_path, &log)?;
            report.gates.push(GateReport {
                name: name.into(),
                command: args.iter().map(|arg| (*arg).to_string()).collect(),
                status: if output.status.success() {
                    GateStatus::Passed
                } else {
                    GateStatus::Failed
                },
                required,
                duration_ms,
                log_path: Some(log_path),
                message: Some(if output.status.success() {
                    "command exited 0".into()
                } else {
                    format!("command exited {}", output.status)
                }),
            });
        }
        Err(e) => {
            write_log(&log_path, &e.to_string())?;
            report.gates.push(GateReport {
                name: name.into(),
                command: args.iter().map(|arg| (*arg).to_string()).collect(),
                status: GateStatus::Blocked,
                required,
                duration_ms,
                log_path: Some(log_path),
                message: Some(e.to_string()),
            });
        }
    }
    Ok(())
}

fn push_bundle_gates(
    root: &Path,
    report: &mut ReleaseReport,
    opts: &ReleaseReadyOptions,
    claim_id: &str,
) -> Result<()> {
    push_command_gate(
        root,
        report,
        opts,
        &format!("bundle-export-{claim_id}"),
        &[
            "cargo",
            "run",
            "-p",
            "refineforge-cli",
            "--bin",
            "refine",
            "--",
            "bundle",
            "export",
            claim_id,
        ],
        true,
    )?;
    push_command_gate(
        root,
        report,
        opts,
        &format!("bundle-verify-{claim_id}"),
        &[
            "cargo",
            "run",
            "-p",
            "refineforge-cli",
            "--bin",
            "refine",
            "--",
            "bundle",
            "verify",
            &format!("artifacts/{claim_id}"),
        ],
        true,
    )?;

    if !opts.dry_run {
        let manifest = root.join("artifacts").join(claim_id).join("manifest.json");
        if manifest.exists() {
            report.bundles.push(BundleEvidence {
                claim_id: claim_id.into(),
                bundle_dir: PathBuf::from("artifacts").join(claim_id),
                manifest_sha256: sha256_file(&manifest)?,
                signature: SignatureEvidence::Unsigned,
            });
        }
    }
    Ok(())
}

pub fn audit_docs_in_dir(root: &Path) -> Result<Vec<String>> {
    let mut issues = Vec::new();
    for rel in [
        "README.md",
        "SECURITY.md",
        "docs/security.md",
        "docs/reproducible-build.md",
        "ARCHITECTURE.md",
        "ROLES.md",
        "STRUCTURE.md",
        ".github/CODEOWNERS",
    ] {
        let path = root.join(rel);
        if !path.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        if text.contains("Three Sections")
            || text.contains("three sections")
            || text.contains("Three-section")
            || text.contains("three-section")
        {
            issues.push(format!("{rel}: stale Three Sections language"));
        }
        if text.contains("Planned, not yet present:")
            && (text.contains("containers/")
                || text.contains("release/")
                || text.contains("docs/security.md")
                || text.contains("docs/reproducible-build.md"))
        {
            issues.push(format!(
                "{rel}: planned-path language references paths that are present"
            ));
        }
        if text.contains("Sigstore keyless signature over manifest.json | ✅")
            && !text.contains("CI-pending")
            && !text.contains("first real GitHub OIDC")
        {
            issues.push(format!(
                "{rel}: signed-bundle claim lacks first-run truth boundary"
            ));
        }
        for (line_idx, line) in text.lines().enumerate() {
            if line.contains("Nix flake")
                && line.contains("✅ shipped")
                && !line.contains("first-build")
                && !line.contains("first build")
            {
                issues.push(format!(
                    "{rel}:{}: Nix shipped claim lacks first-build boundary",
                    line_idx + 1
                ));
            }
        }
    }
    Ok(issues)
}

fn push_docs_audit_gate(root: &Path, report: &mut ReleaseReport) -> Result<()> {
    let start = Instant::now();
    let log_path = report
        .evidence_dir
        .join("logs")
        .join("docs-truth-audit.log");
    let issues = audit_docs_in_dir(root)?;
    let (status, message, log) = if issues.is_empty() {
        (
            GateStatus::Passed,
            "docs truth audit passed".to_string(),
            "docs truth audit passed\n".to_string(),
        )
    } else {
        (
            GateStatus::Failed,
            format!("{} docs truth issue(s)", issues.len()),
            issues.join("\n") + "\n",
        )
    };
    write_log(&log_path, &log)?;
    report.gates.push(GateReport {
        name: "docs-truth-audit".into(),
        command: vec!["refine".into(), "release".into(), "ready".into()],
        status,
        required: true,
        duration_ms: start.elapsed().as_millis(),
        log_path: Some(log_path),
        message: Some(message),
    });
    Ok(())
}

fn push_docker_gate(
    root: &Path,
    report: &mut ReleaseReport,
    opts: &ReleaseReadyOptions,
) -> Result<()> {
    if opts.skip_docker {
        report.gates.push(GateReport {
            name: "docker-verifier-smoke".into(),
            command: vec!["docker".into(), "build".into()],
            status: GateStatus::Skipped,
            required: true,
            duration_ms: 0,
            log_path: None,
            message: Some("skipped by --skip-docker".into()),
        });
        return Ok(());
    }
    push_command_gate(
        root,
        report,
        opts,
        "docker-build-verifier",
        &[
            "docker",
            "build",
            "-t",
            "refineforge-verifier:release-ready",
            "-f",
            "containers/Dockerfile.verifier",
            ".",
        ],
        true,
    )
}

fn push_signature_gate(report: &mut ReleaseReport, opts: &ReleaseReadyOptions) {
    if opts.skip_signature {
        report.gates.push(GateReport {
            name: "signature-verification".into(),
            command: vec!["cosign".into(), "verify-blob".into()],
            status: GateStatus::Skipped,
            required: true,
            duration_ms: 0,
            log_path: None,
            message: Some("skipped by --skip-signature".into()),
        });
    }
}

fn write_log(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn gate(name: &str, status: GateStatus) -> GateReport {
        GateReport {
            name: name.to_string(),
            command: vec!["refine".into(), "dummy".into()],
            status,
            required: true,
            duration_ms: 7,
            log_path: Some(PathBuf::from(format!("logs/{name}.log"))),
            message: Some(format!("{name} message")),
        }
    }

    #[test]
    fn release_report_is_success_only_when_required_gates_pass_or_skip() {
        let mut report = ReleaseReport::test_fixture("0.2.2");
        report
            .gates
            .push(gate("lean-check-all", GateStatus::Passed));
        report.gates.push(gate("signature", GateStatus::Skipped));
        assert!(report.required_gates_succeeded());

        report.gates.push(gate("bundle-verify", GateStatus::Failed));
        assert!(!report.required_gates_succeeded());
    }

    #[test]
    fn blocked_required_gate_fails_release_report() {
        let mut report = ReleaseReport::test_fixture("0.2.2");
        report.gates.push(gate("docker-smoke", GateStatus::Blocked));
        assert!(!report.required_gates_succeeded());
    }

    #[test]
    fn markdown_report_contains_gate_table_and_bundle_hashes() {
        let mut report = ReleaseReport::test_fixture("0.2.2");
        report
            .gates
            .push(gate("scan-check-all", GateStatus::Passed));
        report.bundles.push(BundleEvidence {
            claim_id: "EXAMPLE-003".into(),
            bundle_dir: PathBuf::from("release/evidence/run/bundles/EXAMPLE-003"),
            manifest_sha256: "a".repeat(64),
            signature: SignatureEvidence::Unsigned,
        });

        let md = report.to_markdown();
        assert!(md.contains("# refineforge release readiness report"));
        assert!(md.contains("| scan-check-all | passed | yes |"));
        assert!(md.contains("EXAMPLE-003"));
        assert!(md.contains(&"a".repeat(64)));
    }

    #[test]
    fn gate_log_file_names_are_stable_and_windows_safe() {
        assert_eq!(
            gate_log_name("bundle verify: EXAMPLE-003"),
            "bundle-verify-example-003.log"
        );
    }

    #[test]
    fn writes_json_markdown_sbom_and_provenance_files() {
        let td = tempfile::tempdir().unwrap();
        let mut report = ReleaseReport::test_fixture("0.2.2");
        report.evidence_dir = td.path().join("evidence");
        report
            .gates
            .push(gate("lean-check-all", GateStatus::Passed));

        let sbom = serde_json::json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.5",
            "components": []
        });
        let provenance = serde_json::json!({
            "_type": "https://in-toto.io/Statement/v1",
            "subject": []
        });

        write_evidence(&report, &sbom, &provenance).unwrap();

        assert!(report.evidence_dir.join("release-report.json").exists());
        assert!(report.evidence_dir.join("release-report.md").exists());
        assert!(report.evidence_dir.join("sbom.cyclonedx.json").exists());
        assert!(report.evidence_dir.join("provenance.intoto.json").exists());
    }

    #[test]
    fn sbom_from_cargo_metadata_includes_workspace_and_dependency_components() {
        let metadata = serde_json::json!({
            "packages": [
                {
                    "id": "path+file:///repo#refineforge-cli@0.2.2",
                    "name": "refineforge-cli",
                    "version": "0.2.2",
                    "license": "Apache-2.0 OR MIT",
                    "dependencies": [{"name": "serde", "req": "^1"}]
                },
                {
                    "id": "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0",
                    "name": "serde",
                    "version": "1.0.0",
                    "license": "MIT OR Apache-2.0",
                    "dependencies": []
                }
            ],
            "workspace_members": ["path+file:///repo#refineforge-cli@0.2.2"]
        });

        let sbom = sbom_from_cargo_metadata(&metadata, "0.2.2").unwrap();
        assert_eq!(sbom["bomFormat"], "CycloneDX");
        assert_eq!(sbom["metadata"]["component"]["name"], "refineforge");
        let components = sbom["components"].as_array().unwrap();
        assert!(components.iter().any(|c| c["name"] == "refineforge-cli"));
        assert!(components.iter().any(|c| c["name"] == "serde"));
    }

    #[test]
    fn provenance_records_bundle_subjects_and_materials() {
        let mut report = ReleaseReport::test_fixture("0.2.2");
        report.git_commit = Some("abc123".into());
        report.bundles.push(BundleEvidence {
            claim_id: "EXAMPLE-003".into(),
            bundle_dir: PathBuf::from("bundles/EXAMPLE-003"),
            manifest_sha256: "b".repeat(64),
            signature: SignatureEvidence::Unsigned,
        });

        let provenance = provenance_from_report(&report);
        assert_eq!(provenance["_type"], "https://in-toto.io/Statement/v1");
        assert_eq!(
            provenance["subject"][0]["name"],
            "EXAMPLE-003/manifest.json"
        );
        assert_eq!(provenance["subject"][0]["digest"]["sha256"], "b".repeat(64));
        assert_eq!(provenance["predicate"]["materials"][0]["uri"], "git+HEAD");
    }

    #[test]
    fn dry_run_ready_marks_expensive_gates_skipped() {
        let td = tempfile::tempdir().unwrap();
        let opts = ReleaseReadyOptions {
            version: "0.2.2".into(),
            evidence_dir: td.path().join("evidence"),
            dry_run: true,
            allow_dirty: true,
            skip_docker: true,
            skip_signature: true,
            ci: false,
        };

        let report = build_ready_report(Path::new("."), &opts).unwrap();
        assert!(report
            .gates
            .iter()
            .any(|g| g.name == "cargo-test-refineforge-cli"));
        assert!(report
            .gates
            .iter()
            .filter(|g| g.name.starts_with("cargo-test-"))
            .all(|g| g.status == GateStatus::Skipped));
    }

    #[test]
    fn invalid_semver_is_a_failed_required_gate() {
        let td = tempfile::tempdir().unwrap();
        let opts = ReleaseReadyOptions {
            version: "not-a-version".into(),
            evidence_dir: td.path().join("evidence"),
            dry_run: true,
            allow_dirty: true,
            skip_docker: true,
            skip_signature: true,
            ci: false,
        };

        let report = build_ready_report(Path::new("."), &opts).unwrap();
        let semver = report
            .gates
            .iter()
            .find(|g| g.name == "version-semver")
            .unwrap();
        assert_eq!(semver.status, GateStatus::Failed);
        assert!(!report.required_gates_succeeded());
    }

    #[test]
    fn docs_audit_rejects_three_section_language() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join("README.md"),
            "Architecture has Three Sections",
        )
        .unwrap();
        let issues = audit_docs_in_dir(td.path()).unwrap();
        assert!(issues.iter().any(|issue| issue.contains("Three Sections")));
    }

    #[test]
    fn docs_audit_accepts_ci_pending_signature_language() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join("SECURITY.md"),
            "Sigstore verification is shipped locally; first real GitHub OIDC signed-bundle run is CI-pending.",
        )
        .unwrap();
        let issues = audit_docs_in_dir(td.path()).unwrap();
        assert!(issues.is_empty(), "issues: {issues:?}");
    }

    #[test]
    fn docs_audit_allows_unrelated_shipped_claims_near_nix_pending() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join("SECURITY.md"),
            "SHA-256 manifest over bundle contents | ✅ shipped\nReproducible builds (Nix flake) | not yet\n",
        )
        .unwrap();
        let issues = audit_docs_in_dir(td.path()).unwrap();
        assert!(issues.is_empty(), "issues: {issues:?}");
    }

    #[test]
    fn cargo_gate_commands_use_isolated_target_dir() {
        assert!(is_cargo_program("cargo"));
        assert!(is_cargo_program("cargo.exe"));
        assert!(!is_cargo_program("git"));
        assert_eq!(
            release_cargo_target_dir(Path::new("repo")),
            PathBuf::from("repo").join("target").join("release-ready")
        );
    }
}
