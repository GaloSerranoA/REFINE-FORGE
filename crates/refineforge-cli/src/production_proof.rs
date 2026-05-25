use crate::agent::common::{
    action_intent, capability, seal_runtime, set_production_proof, tool_check, ActionIntent,
    AgentKind, AgentMode, AgentReport, AgentStatus, CommandRecord, ProductionProofStatus,
    ProductionRequirement, TrustLevel,
};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct VerifyOptions {
    pub evidence_dir: PathBuf,
    pub out_dir: PathBuf,
    pub target: String,
    pub emit_json: bool,
}

#[derive(Debug, Deserialize)]
struct EvidenceManifest {
    schema_version: String,
    target: String,
    release: ReleaseEvidence,
    training: TrainingEvidence,
    kernel: KernelEvidence,
    lean: LeanEvidence,
}

#[derive(Debug, Deserialize)]
struct ReleaseEvidence {
    hosted_ci_url: String,
    oidc_issuer: String,
    signed_bundle_path: String,
    sbom_path: String,
    provenance_path: String,
    verifier_container_digest: String,
    nix_lock_path: String,
    nix_check_log_path: String,
    architecture_matrix_path: String,
    approval_path: String,
}

#[derive(Debug, Deserialize)]
struct TrainingEvidence {
    checkpoint_path: String,
    eval_report_path: String,
    regression_report_path: String,
    compute_ledger_path: String,
    promotion_manifest_path: String,
    approval_path: String,
}

#[derive(Debug, Deserialize)]
struct KernelEvidence {
    source_kind: String,
    source_path: String,
    reference_path: String,
    bitexact_report_path: String,
    hardware_matrix_path: String,
    compiler_metadata_path: String,
    performance_baseline_path: String,
    helyx_handoff_path: String,
    approval_path: String,
}

#[derive(Debug, Deserialize)]
struct LeanEvidence {
    claims_report_path: String,
    proof_inventory_path: String,
    refinement_links_path: String,
    bundle_hashes_path: String,
    approval_path: String,
}

#[derive(Debug, Deserialize)]
struct HumanApproval {
    schema_version: String,
    human_operator: String,
    role: String,
    decision: String,
    approved_at: String,
    evidence_summary: String,
}

#[derive(Default)]
struct Validation {
    requirements: Vec<ProductionRequirement>,
    blockers: Vec<String>,
    reviewer_evidence: Vec<String>,
    artifacts: Vec<PathBuf>,
}

impl Validation {
    fn requirement(
        &mut self,
        id: &str,
        description: &str,
        passed: bool,
        evidence: Vec<String>,
        blocker: impl Into<String>,
    ) {
        self.requirements.push(ProductionRequirement::new_owned(
            id,
            description,
            if passed {
                AgentStatus::Passed
            } else {
                AgentStatus::Blocked
            },
            evidence,
        ));
        if !passed {
            self.blockers.push(blocker.into());
        }
    }

    fn add_artifact(&mut self, path: &str) {
        self.artifacts
            .push(PathBuf::from(normalize_manifest_path(path)));
    }
}

pub fn verify(root: &Path, opts: VerifyOptions) -> Result<()> {
    let report = build_report(root, &opts);
    crate::agent::common::write_reports(&opts.out_dir, AgentKind::RunAll.report_stem(), &report)?;
    if opts.emit_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "production proof report: {} ({})",
            opts.out_dir.join("summary.md").display(),
            report.status.as_str()
        );
    }
    Ok(())
}

fn build_report(root: &Path, opts: &VerifyOptions) -> AgentReport {
    let mut report = AgentReport::new(AgentKind::RunAll, AgentMode::Check, opts.target.clone());
    report.liveness.command_surface = "refine production-proof verify".to_string();
    report.capabilities.extend([
        capability(
            "production-proof-evidence-contract",
            "available",
            "validates one explicit evidence manifest across Lean, DevOps, training, and kernel roles",
        ),
        capability(
            "human-approval-boundary",
            "available",
            "accepts human-reviewed status only from named human approval files",
        ),
        capability(
            "artifact-hash-receipts",
            "available",
            "hashes all declared evidence artifacts into deterministic runtime receipts",
        ),
        capability(
            "trust-inflation-guard",
            "available",
            "blocks production proof when evidence is local, missing, placeholder, or stub-backed",
        ),
    ]);
    report.tool_checks.extend([
        tool_check(
            "production-proof-manifest",
            true,
            if opts.evidence_dir.join("evidence.json").exists() {
                "available"
            } else {
                "required"
            },
            "self-contained production proof evidence manifest at evidence.json",
        ),
        tool_check(
            "human-approval-files",
            true,
            "not_checked",
            "approval files are validated from the manifest paths",
        ),
        tool_check(
            "artifact-hash",
            true,
            "available",
            "SHA-256 hashing is provided by the local CLI verifier",
        ),
    ]);
    report.artifacts.push(PathBuf::from("evidence.json"));

    let manifest_path = opts.evidence_dir.join("evidence.json");
    let started = Instant::now();
    let manifest_result = load_manifest(&manifest_path);
    report.commands.push(CommandRecord {
        name: "production-proof-manifest".to_string(),
        command: vec![
            "refine".to_string(),
            "production-proof".to_string(),
            "verify".to_string(),
            "--evidence-dir".to_string(),
            opts.evidence_dir.display().to_string(),
        ],
        status: if manifest_result.is_ok() {
            AgentStatus::Passed
        } else {
            AgentStatus::Failed
        },
        duration_ms: started.elapsed().as_millis(),
        exit_code: None,
        stdout_tail: None,
        stderr_tail: manifest_result.as_ref().err().cloned(),
    });

    let mut validation = Validation::default();
    match manifest_result {
        Ok(manifest) => {
            validate_manifest(&manifest, &opts.evidence_dir, &opts.target, &mut validation)
        }
        Err(err) => {
            validation.requirement(
                "production.manifest",
                "Production-proof manifest exists and parses as refineforge-production-proof-evidence-v1",
                false,
                vec![err],
                "production proof manifest evidence.json is missing or invalid",
            );
        }
    }

    validation.artifacts.sort();
    validation.artifacts.dedup();
    report.artifacts.extend(validation.artifacts);
    report.blockers = validation.blockers.clone();

    let passed = validation.blockers.is_empty();
    report.finish(
        if passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        if passed {
            TrustLevel::HumanReviewed
        } else {
            TrustLevel::Blocked
        },
        if passed {
            "Production proof evidence manifest is complete, human-approved, and all declared artifacts were hashed."
        } else {
            "Production proof evidence is blocked. The report names the missing or invalid evidence without upgrading trust."
        },
    );
    set_production_proof(
        &mut report,
        if passed {
            ProductionProofStatus::HumanReviewed
        } else {
            ProductionProofStatus::Blocked
        },
        validation.requirements,
        validation.reviewer_evidence,
        validation.blockers,
    );
    seal_runtime(
        root,
        Some(&opts.evidence_dir),
        &mut report,
        TrustLevel::HumanReviewed,
        production_proof_action_intents(),
    );
    report
}

fn load_manifest(path: &Path) -> std::result::Result<EvidenceManifest, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|err| format!("could not read {}: {err}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|err| format!("could not parse {}: {err}", path.display()))
}

fn validate_manifest(
    manifest: &EvidenceManifest,
    evidence_dir: &Path,
    expected_target: &str,
    validation: &mut Validation,
) {
    validation.requirement(
        "production.manifest",
        "Production-proof manifest exists and targets this verification run",
        manifest.schema_version == "refineforge-production-proof-evidence-v1"
            && manifest.target == expected_target,
        vec![
            format!("schema_version={}", manifest.schema_version),
            format!("manifest.target={}", manifest.target),
            format!("expected.target={expected_target}"),
        ],
        "production proof manifest schema or target does not match this run",
    );

    validate_release(&manifest.release, evidence_dir, validation);
    validate_training(&manifest.training, evidence_dir, validation);
    validate_kernel(&manifest.kernel, evidence_dir, validation);
    validate_lean(&manifest.lean, evidence_dir, validation);
}

fn validate_release(release: &ReleaseEvidence, evidence_dir: &Path, validation: &mut Validation) {
    validation.requirement(
        "devops.hosted_ci_oidc",
        "Hosted CI run uses GitHub Actions OIDC signing evidence",
        release.hosted_ci_url.starts_with("https://github.com/")
            && release.hosted_ci_url.contains("/actions/runs/")
            && release.oidc_issuer == "https://token.actions.githubusercontent.com",
        vec![
            format!("hosted_ci_url={}", release.hosted_ci_url),
            format!("oidc_issuer={}", release.oidc_issuer),
        ],
        "hosted CI/OIDC evidence is missing or not a GitHub Actions run",
    );

    let signed = required_file(
        evidence_dir,
        &release.signed_bundle_path,
        "devops.signed_bundle",
        validation,
    );
    validation.requirement(
        "devops.sigstore_bundle_verified",
        "Sigstore verification output is present",
        signed.exists && json_status_is(&signed.path, "passed"),
        signed.evidence,
        "Sigstore verification output is missing or not passed",
    );

    let sbom = required_file(evidence_dir, &release.sbom_path, "devops.sbom", validation);
    let provenance = required_file(
        evidence_dir,
        &release.provenance_path,
        "devops.provenance",
        validation,
    );
    validation.requirement(
        "devops.sbom_provenance",
        "SBOM and provenance artifacts are present",
        sbom.exists && provenance.exists,
        [sbom.evidence, provenance.evidence].concat(),
        "SBOM or provenance artifact is missing",
    );

    validation.requirement(
        "devops.verifier_container_digest",
        "Verifier container digest is recorded as a sha256 digest",
        valid_digest(&release.verifier_container_digest),
        vec![format!(
            "verifier_container_digest={}",
            release.verifier_container_digest
        )],
        "verifier container digest is missing or malformed",
    );

    let nix_lock = required_file(
        evidence_dir,
        &release.nix_lock_path,
        "devops.nix_lock",
        validation,
    );
    let nix_check = required_file(
        evidence_dir,
        &release.nix_check_log_path,
        "devops.nix_check",
        validation,
    );
    validation.requirement(
        "devops.nix_locked_check",
        "Nix flake lock and nix check log are present",
        nix_lock.exists && nix_check.exists && text_contains(&nix_check.path, "passed"),
        [nix_lock.evidence, nix_check.evidence].concat(),
        "Nix lock/check evidence is missing or does not show a passed check",
    );

    let matrix = required_file(
        evidence_dir,
        &release.architecture_matrix_path,
        "devops.architecture_matrix",
        validation,
    );
    validation.requirement(
        "devops.architecture_matrix",
        "Hosted CI architecture matrix records runner OS and CPU architecture",
        matrix.exists && json_has_nonempty_array(&matrix.path, "runners"),
        matrix.evidence,
        "hosted CI architecture matrix evidence is missing or empty",
    );

    validate_approval(
        evidence_dir,
        &release.approval_path,
        "release",
        "devops.human_release_approval",
        validation,
    );
}

fn validate_training(
    training: &TrainingEvidence,
    evidence_dir: &Path,
    validation: &mut Validation,
) {
    let checkpoint = required_file(
        evidence_dir,
        &training.checkpoint_path,
        "training.checkpoint",
        validation,
    );
    validation.requirement(
        "training.checkpoint",
        "Live training checkpoint artifact is present",
        checkpoint.exists,
        checkpoint.evidence,
        "training checkpoint evidence is missing",
    );

    let eval = required_file(
        evidence_dir,
        &training.eval_report_path,
        "training.eval_report",
        validation,
    );
    validation.requirement(
        "training.eval_report",
        "Evaluation report exists and passed",
        eval.exists && json_status_is(&eval.path, "passed"),
        eval.evidence,
        "training evaluation evidence is missing or not passed",
    );

    let regression = required_file(
        evidence_dir,
        &training.regression_report_path,
        "training.regression_report",
        validation,
    );
    validation.requirement(
        "training.regression_report",
        "Regression comparison exists and passed",
        regression.exists && json_status_is(&regression.path, "passed"),
        regression.evidence,
        "training regression evidence is missing or not passed",
    );

    let ledger = required_file(
        evidence_dir,
        &training.compute_ledger_path,
        "training.compute_ledger",
        validation,
    );
    validation.requirement(
        "training.compute_ledger",
        "Compute ledger is present for the live run",
        ledger.exists,
        ledger.evidence,
        "training compute ledger evidence is missing",
    );

    let promotion = required_file(
        evidence_dir,
        &training.promotion_manifest_path,
        "training.promotion_manifest",
        validation,
    );
    validation.requirement(
        "training.promotion_manifest",
        "Promotion manifest records approval and rollback evidence",
        promotion.exists
            && (json_status_is(&promotion.path, "approved")
                || json_field_is(&promotion.path, "decision", "promote"))
            && json_has_object(&promotion.path, "rollback"),
        promotion.evidence,
        "training promotion or rollback evidence is missing",
    );

    validate_approval(
        evidence_dir,
        &training.approval_path,
        "training",
        "training.human_approval",
        validation,
    );
}

fn validate_kernel(kernel: &KernelEvidence, evidence_dir: &Path, validation: &mut Validation) {
    let source = required_file(
        evidence_dir,
        &kernel.source_path,
        "kernel.source",
        validation,
    );
    let real_source_kind = matches!(
        kernel.source_kind.as_str(),
        "cuda" | "rust" | "external" | "ptx"
    );
    let source_extension_ok = if kernel.source_kind == "cuda" {
        source
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "cu" | "cuh"))
    } else {
        true
    };
    validation.requirement(
        "kernel.real_source",
        "Kernel source is a real CUDA/Rust/external source, not a stub fixture",
        source.exists && real_source_kind && source_extension_ok,
        [
            source.evidence,
            vec![format!("source_kind={}", kernel.source_kind)],
        ]
        .concat(),
        if kernel.source_kind == "stub" {
            "kernel source kind is stub"
        } else {
            "kernel source evidence is missing or not a real production source"
        },
    );

    let reference = required_file(
        evidence_dir,
        &kernel.reference_path,
        "kernel.reference",
        validation,
    );
    let bitexact = required_file(
        evidence_dir,
        &kernel.bitexact_report_path,
        "kernel.bitexact_report",
        validation,
    );
    validation.requirement(
        "kernel.bitexact_reference",
        "CPU/golden reference and bit-exact report are present and passed",
        reference.exists && bitexact.exists && json_status_is(&bitexact.path, "passed"),
        [reference.evidence, bitexact.evidence].concat(),
        "kernel reference or bit-exact report evidence is missing",
    );

    let hardware = required_file(
        evidence_dir,
        &kernel.hardware_matrix_path,
        "kernel.hardware_matrix",
        validation,
    );
    validation.requirement(
        "kernel.hardware_matrix",
        "Hardware matrix records real GPU, driver, and CUDA evidence",
        hardware.exists && valid_kernel_hardware_matrix(&hardware.path),
        hardware.evidence,
        "kernel hardware matrix evidence is missing or incomplete",
    );

    let compiler = required_file(
        evidence_dir,
        &kernel.compiler_metadata_path,
        "kernel.compiler_metadata",
        validation,
    );
    validation.requirement(
        "kernel.compiler_metadata",
        "Compiler metadata is present",
        compiler.exists,
        compiler.evidence,
        "kernel compiler metadata evidence is missing",
    );

    let perf = required_file(
        evidence_dir,
        &kernel.performance_baseline_path,
        "kernel.performance_baseline",
        validation,
    );
    validation.requirement(
        "kernel.performance_baseline",
        "Performance baseline exists and passed",
        perf.exists && json_status_is(&perf.path, "passed"),
        perf.evidence,
        "kernel performance baseline evidence is missing or not passed",
    );

    let handoff = required_file(
        evidence_dir,
        &kernel.helyx_handoff_path,
        "kernel.helyx_handoff",
        validation,
    );
    validation.requirement(
        "kernel.helyx_handoff",
        "HELYX handoff artifact is present",
        handoff.exists,
        handoff.evidence,
        "kernel HELYX handoff evidence is missing",
    );

    validate_approval(
        evidence_dir,
        &kernel.approval_path,
        "kernel",
        "kernel.human_approval",
        validation,
    );
}

fn validate_lean(lean: &LeanEvidence, evidence_dir: &Path, validation: &mut Validation) {
    let claims = required_file(
        evidence_dir,
        &lean.claims_report_path,
        "lean.claims_report",
        validation,
    );
    validation.requirement(
        "lean.implementation_linked_claims",
        "Claim report contains implementation-linked claims with Rust symbols and Lean theorems",
        claims.exists && claims_report_is_implementation_linked(&claims.path),
        claims.evidence,
        "Lean claim evidence is missing implementation-linked refinement data",
    );

    let inventory = required_file(
        evidence_dir,
        &lean.proof_inventory_path,
        "lean.proof_inventory",
        validation,
    );
    let links = required_file(
        evidence_dir,
        &lean.refinement_links_path,
        "lean.refinement_links",
        validation,
    );
    validation.requirement(
        "lean.refinement_links",
        "Proof inventory and refinement-link evidence are present",
        inventory.exists && links.exists && json_status_is(&links.path, "passed"),
        [inventory.evidence, links.evidence].concat(),
        "Lean proof inventory or refinement-link evidence is missing",
    );

    let bundles = required_file(
        evidence_dir,
        &lean.bundle_hashes_path,
        "lean.bundle_hashes",
        validation,
    );
    validation.requirement(
        "lean.bundle_hashes",
        "Bundle hashes are present and syntactically valid",
        bundles.exists && bundle_hashes_are_valid(&bundles.path),
        bundles.evidence,
        "Lean bundle hash evidence is missing or invalid",
    );

    validate_approval(
        evidence_dir,
        &lean.approval_path,
        "lean",
        "lean.human_approval",
        validation,
    );
}

fn validate_approval(
    evidence_dir: &Path,
    rel_path: &str,
    expected_role: &str,
    requirement_id: &str,
    validation: &mut Validation,
) {
    let file = required_file(evidence_dir, rel_path, requirement_id, validation);
    let mut passed = false;
    let mut evidence = file.evidence;
    let mut blocker = format!("{expected_role} human approval evidence is missing");

    if file.exists {
        match std::fs::read_to_string(&file.path)
            .ok()
            .and_then(|content| serde_json::from_str::<HumanApproval>(&content).ok())
        {
            Some(approval) => {
                let operator = approval.human_operator.trim();
                let role_ok = approval.role.eq_ignore_ascii_case(expected_role);
                let decision_ok = approval.decision.eq_ignore_ascii_case("approved");
                let schema_ok = approval.schema_version == "refineforge-human-approval-v1";
                let time_ok = !approval.approved_at.trim().is_empty();
                let summary_ok = !approval.evidence_summary.trim().is_empty();
                let human_ok = !operator.is_empty() && !is_automated_operator(operator);
                passed = schema_ok && role_ok && decision_ok && time_ok && summary_ok && human_ok;
                evidence.push(format!("human_operator={operator}"));
                evidence.push(format!("approval_role={}", approval.role));
                if !human_ok {
                    blocker = format!("{expected_role} AI/automated approval rejected: {operator}");
                } else if !role_ok || !decision_ok || !schema_ok {
                    blocker = format!("{expected_role} approval file is malformed or not approved");
                } else if !time_ok || !summary_ok {
                    blocker =
                        format!("{expected_role} approval lacks timestamp or evidence summary");
                }
                if passed {
                    validation
                        .reviewer_evidence
                        .push(format!("{expected_role}: {operator} ({rel_path})"));
                }
            }
            None => {
                blocker = format!("{expected_role} human approval file is not valid JSON");
            }
        }
    }

    validation.requirement(
        requirement_id,
        "Named human approval is present and not AI-generated placeholder evidence",
        passed,
        evidence,
        blocker,
    );
}

struct FileEvidence {
    exists: bool,
    path: PathBuf,
    evidence: Vec<String>,
}

fn required_file(
    evidence_dir: &Path,
    rel_path: &str,
    label: &str,
    validation: &mut Validation,
) -> FileEvidence {
    let normalized = normalize_manifest_path(rel_path);
    match resolve_evidence_path(evidence_dir, &normalized) {
        Ok(path) => {
            validation.add_artifact(&normalized);
            if path.is_file() {
                let hash = hash_file(&path)
                    .map(|hash| format!("{normalized} sha256={hash}"))
                    .unwrap_or_else(|err| format!("{normalized} unreadable: {err}"));
                FileEvidence {
                    exists: true,
                    path,
                    evidence: vec![hash],
                }
            } else {
                FileEvidence {
                    exists: false,
                    path,
                    evidence: vec![format!("{label} missing: {normalized}")],
                }
            }
        }
        Err(err) => FileEvidence {
            exists: false,
            path: evidence_dir.join(&normalized),
            evidence: vec![format!("{label} invalid path {normalized}: {err}")],
        },
    }
}

fn resolve_evidence_path(
    evidence_dir: &Path,
    rel_path: &str,
) -> std::result::Result<PathBuf, String> {
    let path = Path::new(rel_path);
    if path.is_absolute() {
        return Err(
            "absolute paths are not accepted; evidence packs must be self-contained".to_string(),
        );
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("path traversal or non-normal path component is not accepted".to_string());
    }
    Ok(evidence_dir.join(path))
}

fn normalize_manifest_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn hash_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn read_json(path: &Path) -> Option<Value> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn json_status_is(path: &Path, expected: &str) -> bool {
    json_field_is(path, "status", expected)
}

fn json_field_is(path: &Path, field: &str, expected: &str) -> bool {
    read_json(path)
        .and_then(|value| value.get(field).and_then(Value::as_str).map(str::to_string))
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn json_has_object(path: &Path, field: &str) -> bool {
    read_json(path)
        .and_then(|value| value.get(field).cloned())
        .is_some_and(|value| value.is_object())
}

fn json_has_nonempty_array(path: &Path, field: &str) -> bool {
    read_json(path)
        .and_then(|value| value.get(field).cloned())
        .and_then(|value| value.as_array().cloned())
        .is_some_and(|items| !items.is_empty())
}

fn text_contains(path: &Path, needle: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|content| {
            content
                .to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

fn valid_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn valid_kernel_hardware_matrix(path: &Path) -> bool {
    let Some(value) = read_json(path) else {
        return false;
    };
    if value
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| !status.eq_ignore_ascii_case("passed"))
    {
        return false;
    }
    let Some(gpus) = value.get("gpus").and_then(Value::as_array) else {
        return false;
    };
    gpus.iter().any(|gpu| {
        ["name", "driver", "cuda"].iter().all(|field| {
            gpu.get(*field)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        })
    })
}

fn claims_report_is_implementation_linked(path: &Path) -> bool {
    let Some(value) = read_json(path) else {
        return false;
    };
    let Some(claims) = value.get("claims").and_then(Value::as_array) else {
        return false;
    };
    !claims.is_empty()
        && claims.iter().all(|claim| {
            claim
                .get("scope")
                .and_then(Value::as_str)
                .is_some_and(|scope| {
                    matches!(scope, "implementation-linked" | "implementation-refinement")
                })
                && claim
                    .get("refinement_doc")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
                && claim
                    .get("rust_symbols")
                    .and_then(Value::as_array)
                    .is_some_and(|items| !items.is_empty())
                && claim
                    .get("lean_theorems")
                    .and_then(Value::as_array)
                    .is_some_and(|items| !items.is_empty())
        })
}

fn bundle_hashes_are_valid(path: &Path) -> bool {
    let Some(value) = read_json(path) else {
        return false;
    };
    let Some(bundles) = value.get("bundles").and_then(Value::as_array) else {
        return false;
    };
    !bundles.is_empty()
        && bundles.iter().all(|bundle| {
            bundle
                .get("sha256")
                .and_then(Value::as_str)
                .is_some_and(|hash| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
        })
}

fn is_automated_operator(operator: &str) -> bool {
    let lower = operator.to_ascii_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    let blocked = [
        "ai",
        "automated",
        "automation",
        "bot",
        "chatgpt",
        "claude",
        "codex",
        "gemini",
        "gpt",
        "llm",
        "none",
        "null",
        "placeholder",
        "tbd",
        "todo",
    ];
    tokens.iter().any(|token| blocked.contains(token))
}

fn production_proof_action_intents() -> Vec<ActionIntent> {
    vec![
        action_intent(
            "production_proof.verify.manifest",
            "Validate the self-contained evidence manifest",
            "verify",
            "evidence_only",
            "refine production-proof verify --evidence-dir <dir>",
            &["evidence.json"],
        ),
        action_intent(
            "production_proof.hash.artifacts",
            "Hash every declared evidence artifact into runtime receipts",
            "audit",
            "evidence_only",
            "refine production-proof verify",
            &["runtime.evidence_receipts"],
        ),
        action_intent(
            "production_proof.validate.human_review",
            "Reject AI, placeholder, or missing approval records",
            "policy",
            "evidence_only",
            "refine production-proof verify",
            &["approvals/*.json", "production_proof.reviewer_evidence"],
        ),
    ]
}
