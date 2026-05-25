use super::common::{
    action_intent, capability, existing_artifact, repo_tool_check, seal_runtime,
    set_production_proof, tool_check, valid_sha256, validate_existing_file,
    validate_human_approval, validate_json_file, ActionIntent, AgentMode, AgentReport, AgentStatus,
    CommandRecord, EvidenceValidation, ProductionProofStatus, ProductionRequirement, TrustLevel,
};
use crate::release::{self, ReleaseReadyOptions};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub fn build(
    root: &Path,
    mode: AgentMode,
    target: &str,
    out_dir: &Path,
    allow_expensive: bool,
) -> AgentReport {
    let mut report = AgentReport::new(super::common::AgentKind::Devops, mode, target);
    report.capabilities.extend([
        capability(
            "release-readiness",
            "available",
            "runs the release readiness command and records release evidence artifacts",
        ),
        capability(
            "ci-artifact-contract",
            "available",
            "tracks CI, verifier container, SBOM/provenance, and docs truth-audit surfaces",
        ),
        capability(
            "signed-bundle-flow",
            "tool_gated",
            "cosign/Sigstore gates are only claimed when --allow-expensive runs them successfully",
        ),
        capability(
            "verifier-container",
            "tool_gated",
            "Docker verifier image is only claimed when --allow-expensive runs it successfully",
        ),
    ]);
    report.tool_checks.extend([
        repo_tool_check(root, "refine", true),
        tool_check(
            "docker",
            allow_expensive,
            if allow_expensive {
                "required"
            } else {
                "skipped"
            },
            "verifier container gate is controlled by --allow-expensive",
        ),
        tool_check(
            "cosign",
            allow_expensive,
            if allow_expensive {
                "required"
            } else {
                "skipped"
            },
            "signed bundle gate is controlled by --allow-expensive",
        ),
    ]);
    existing_artifact(
        root,
        "docs/release/release-readiness-inventory.md",
        &mut report,
    );
    existing_artifact(root, "docs/release/ci-audit-report.md", &mut report);
    existing_artifact(root, "docs/release/devops-production-proof.md", &mut report);
    existing_artifact(root, ".github/workflows/ci.yml", &mut report);
    existing_artifact(root, "containers/Dockerfile.verifier", &mut report);

    if !mode.runs_checks() {
        report.warnings.push(
            "inspect mode does not run Docker, Nix, cosign, or hosted CI evidence".to_string(),
        );
        report.finish(
            AgentStatus::Passed,
            TrustLevel::ReleaseReadyLocal,
            "DevOps agent inspected release-readiness docs and CI surfaces. Live hosted signing is not claimed.",
        );
        apply_devops_production_proof(&mut report, root, false, allow_expensive, target, None);
        seal_runtime(
            root,
            Some(out_dir),
            &mut report,
            TrustLevel::ReleaseReadyLocal,
            devops_action_intents(mode, allow_expensive),
        );
        return report;
    }

    let version = release_version_for_target(target);
    let evidence_dir = out_dir.join("release");
    let opts = ReleaseReadyOptions {
        version: version.clone(),
        evidence_dir: evidence_dir.clone(),
        dry_run: false,
        allow_dirty: true,
        skip_docker: !allow_expensive,
        skip_signature: !allow_expensive,
        ci: false,
    };
    let started = Instant::now();
    let result = release::ready(root, opts);
    let mut command = vec![
        "refine".to_string(),
        "release".to_string(),
        "ready".to_string(),
        "--allow-dirty".to_string(),
    ];
    if !allow_expensive {
        command.extend(["--skip-docker".to_string(), "--skip-signature".to_string()]);
    }
    command.extend(["--version".to_string(), version.clone()]);
    report.commands.push(CommandRecord::internal_owned(
        "release-ready-local",
        command,
        started,
        &result,
    ));
    report
        .artifacts
        .push(relative_or_display(root, &evidence_dir));
    if allow_expensive {
        report.warnings.push(
            "Docker/Sigstore gates were requested locally; hosted OIDC CI evidence is still required before release-ready-ci.".to_string(),
        );
    } else {
        report.warnings.push(
            "Docker verifier and Sigstore signature gates were skipped by the local agent check; rerun with --allow-expensive or hosted CI before release-ready-ci.".to_string(),
        );
    }

    if result.is_ok() {
        report.finish(
            AgentStatus::Passed,
            release_ready_success_trust_level(allow_expensive),
            release_ready_success_summary(allow_expensive),
        );
    } else {
        report.finish(
            AgentStatus::Failed,
            TrustLevel::Blocked,
            "Local release readiness failed. See release evidence and command record.",
        );
    }
    apply_devops_production_proof(
        &mut report,
        root,
        result.is_ok(),
        allow_expensive,
        target,
        Some(&evidence_dir),
    );
    let production_status = report.production_proof.status;
    if report.status == AgentStatus::Passed
        && production_status == ProductionProofStatus::HumanReviewed
    {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::HumanReviewed,
            "DevOps local release readiness, hosted CI artifacts, OIDC signing, verifier container digest, SBOM/provenance, Nix check, architecture matrix, and named human release approval evidence all passed.",
        );
    } else if report.status == AgentStatus::Passed
        && devops_production_only_missing_human_approval(&report)
    {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::ReleaseReadyLocal,
            "DevOps local readiness and hosted release evidence passed. Production proof remains blocked until a named human release approval file is provided.",
        );
    }
    let trust_ceiling = if production_status == ProductionProofStatus::HumanReviewed {
        TrustLevel::HumanReviewed
    } else {
        TrustLevel::ReleaseReadyLocal
    };
    seal_runtime(
        root,
        Some(out_dir),
        &mut report,
        trust_ceiling,
        devops_action_intents(mode, allow_expensive),
    );
    report
}

fn devops_production_only_missing_human_approval(report: &AgentReport) -> bool {
    let requirements = &report.production_proof.requirements;
    !requirements.is_empty()
        && requirements
            .iter()
            .filter(|requirement| requirement.status != AgentStatus::Passed)
            .all(|requirement| requirement.id == "devops.human_release_approval")
        && report
            .production_proof
            .blockers
            .iter()
            .all(|blocker| blocker.contains("human release approval"))
}

fn apply_devops_production_proof(
    report: &mut AgentReport,
    root: &Path,
    local_ready: bool,
    allow_expensive: bool,
    target: &str,
    evidence_dir: Option<&Path>,
) {
    let mut blockers = Vec::new();
    let mut requirements = Vec::new();
    let role_evidence_dir = release_evidence_dir().or_else(|| evidence_dir.map(Path::to_path_buf));

    requirements.push(ProductionRequirement::new_owned(
        "devops.local_release_ready",
        "Local release readiness command passes and writes evidence",
        if local_ready {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![evidence_dir.map_or_else(
            || "release ready command not run in inspect mode".to_string(),
            |dir| format!("release evidence directory: {}", dir.display()),
        )],
    ));
    if !local_ready {
        blockers.push("local release readiness evidence is missing or failed".to_string());
    }

    let hosted_ci = validate_hosted_ci(role_evidence_dir.as_deref());
    requirements.push(ProductionRequirement::new_owned(
        "devops.hosted_ci_artifacts",
        "Hosted CI workflow passed and uploaded release evidence artifacts",
        if hosted_ci.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        hosted_ci.evidence,
    ));
    if !hosted_ci.passed {
        blockers.push("hosted CI evidence blocks release-ready-ci production proof".to_string());
    }

    let sigstore = validate_release_json_evidence(
        role_evidence_dir.as_deref(),
        "REFINEFORGE_SIGSTORE_EVIDENCE",
        "release/cosign-verify.json",
        "cosign-verify.json",
        "Sigstore evidence",
        &["passed"],
    );
    requirements.push(ProductionRequirement::new_owned(
        "devops.sigstore_oidc_signature",
        "Sigstore keyless signing ran from GitHub OIDC and verified signer identity",
        if sigstore.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        if sigstore.evidence.is_empty() {
            vec![if allow_expensive {
                "local expensive gates do not replace hosted OIDC Sigstore evidence".to_string()
            } else {
                "Sigstore evidence not run; --allow-expensive was not requested".to_string()
            }]
        } else {
            sigstore.evidence
        },
    ));
    if !sigstore.passed {
        blockers.push("Sigstore OIDC signing evidence is missing".to_string());
    }

    let container_digest = validate_container_digest(role_evidence_dir.as_deref());
    requirements.push(ProductionRequirement::new_owned(
        "devops.verifier_container_digest",
        "Verifier container image is built, smoke-tested, and digest-recorded",
        if container_digest.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        container_digest.evidence,
    ));
    if !container_digest.passed {
        blockers.push("verifier container digest is missing".to_string());
    }

    let sbom = validate_release_file(
        role_evidence_dir.as_deref(),
        "release/sbom.cyclonedx.json",
        "sbom.cyclonedx.json",
        "SBOM",
    );
    let provenance = validate_release_file(
        role_evidence_dir.as_deref(),
        "release/provenance.intoto.json",
        "provenance.intoto.json",
        "provenance",
    );
    requirements.push(ProductionRequirement::new_owned(
        "devops.sbom_provenance_uploaded",
        "SBOM and provenance artifacts are generated and uploaded from CI",
        if sbom.passed && provenance.passed && hosted_ci.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        [sbom.evidence, provenance.evidence].concat(),
    ));

    let flake_lock = role_evidence_dir
        .as_deref()
        .and_then(|dir| release_file_path(dir, "release/flake.lock", "flake.lock"))
        .unwrap_or_else(|| root.join("flake.lock"));
    let nix_check = role_evidence_dir
        .as_deref()
        .and_then(|dir| release_file_path(dir, "release/nix-check.log", "nix-check.log"));
    let nix_lock = validate_existing_file(Some(&flake_lock), "Nix flake lock");
    let nix_check_passed = nix_check
        .as_deref()
        .is_some_and(|path| text_contains(path, "passed"));
    let nix_check_evidence = nix_check.as_ref().map_or_else(
        || "nix check log is missing".to_string(),
        |p| p.display().to_string(),
    );
    requirements.push(ProductionRequirement::new_owned(
        "devops.nix_locked_check",
        "Nix flake is locked and nix flake check passes without updating the lock",
        if nix_lock.passed && nix_check_passed {
            AgentStatus::Passed
        } else if nix_lock.passed {
            AgentStatus::Partial
        } else {
            AgentStatus::Blocked
        },
        vec![format!(
            "{}; {}",
            nix_lock.evidence.join("; "),
            nix_check_evidence
        )],
    ));
    if !nix_lock.passed || !nix_check_passed {
        blockers
            .push("Nix reproducibility evidence is missing or nix check did not pass".to_string());
    }

    let architecture_matrix = validate_architecture_matrix(role_evidence_dir.as_deref());
    requirements.push(ProductionRequirement::new_owned(
        "devops.architecture_matrix",
        "Release evidence records runner OS and CPU architecture for every lane",
        if architecture_matrix.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        architecture_matrix.evidence,
    ));
    if !architecture_matrix.passed {
        blockers.push("hosted architecture matrix evidence is missing".to_string());
    }

    let approval = validate_release_approval(role_evidence_dir.as_deref());
    requirements.push(ProductionRequirement::new_owned(
        "devops.human_release_approval",
        "Named human release approval is present for this version",
        if approval.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        if approval.evidence.is_empty() {
            vec![format!(
                "no human release approval provided for target {target}"
            )]
        } else {
            approval.evidence
        },
    ));
    if !approval.passed {
        blockers.push("human release approval is missing".to_string());
    }

    set_production_proof(
        report,
        if blockers.is_empty() {
            ProductionProofStatus::HumanReviewed
        } else {
            ProductionProofStatus::Blocked
        },
        requirements,
        approval.reviewer_evidence.into_iter().collect(),
        blockers,
    );
}

fn release_evidence_dir() -> Option<PathBuf> {
    std::env::var_os("REFINEFORGE_RELEASE_EVIDENCE_DIR").map(PathBuf::from)
}

fn validate_hosted_ci(evidence_dir: Option<&Path>) -> EvidenceValidation {
    if let Some(path) = evidence_dir
        .and_then(|dir| release_file_path(dir, "release/hosted-ci.json", "hosted-ci.json"))
    {
        return validate_json_file(Some(&path), "hosted CI evidence", &["passed"]);
    }
    let Some(raw) = std::env::var("REFINEFORGE_HOSTED_CI_EVIDENCE").ok() else {
        return EvidenceValidation {
            passed: false,
            evidence: vec![
                "hosted CI evidence path/URL not provided via REFINEFORGE_HOSTED_CI_EVIDENCE"
                    .to_string(),
            ],
            reviewer_evidence: None,
            artifact: None,
        };
    };
    if raw.starts_with("https://github.com/") && raw.contains("/actions/runs/") {
        return EvidenceValidation {
            passed: true,
            evidence: vec![raw],
            reviewer_evidence: None,
            artifact: None,
        };
    }
    let path = PathBuf::from(&raw);
    if path.exists() {
        validate_json_file(Some(&path), "hosted CI evidence", &["passed"])
    } else {
        EvidenceValidation {
            passed: false,
            evidence: vec![format!(
                "hosted CI evidence is not a GitHub Actions run URL or existing file: {raw}"
            )],
            reviewer_evidence: None,
            artifact: None,
        }
    }
}

fn validate_release_json_evidence(
    evidence_dir: Option<&Path>,
    env_name: &str,
    conventional: &str,
    local: &str,
    label: &str,
    status_values: &[&str],
) -> EvidenceValidation {
    if let Some(path) = evidence_dir.and_then(|dir| release_file_path(dir, conventional, local)) {
        return validate_json_file(Some(&path), label, status_values);
    }
    let Some(raw) = std::env::var(env_name).ok() else {
        return EvidenceValidation {
            passed: false,
            evidence: Vec::new(),
            reviewer_evidence: None,
            artifact: None,
        };
    };
    let path = PathBuf::from(&raw);
    validate_json_file(Some(&path), label, status_values)
}

fn validate_release_file(
    evidence_dir: Option<&Path>,
    conventional: &str,
    local: &str,
    label: &str,
) -> EvidenceValidation {
    let path = evidence_dir.and_then(|dir| release_file_path(dir, conventional, local));
    validate_existing_file(path.as_deref(), label)
}

fn validate_container_digest(evidence_dir: Option<&Path>) -> EvidenceValidation {
    let digest_source = evidence_dir
        .and_then(|dir| {
            release_file_path(
                dir,
                "release/verifier-container-digest.txt",
                "verifier-container-digest.txt",
            )
        })
        .and_then(|path| std::fs::read_to_string(path).ok())
        .or_else(|| std::env::var("REFINEFORGE_VERIFIER_CONTAINER_DIGEST").ok());
    let Some(digest) = digest_source.map(|digest| digest.trim().to_string()) else {
        return EvidenceValidation {
            passed: false,
            evidence: vec![
                "verifier container digest not provided via REFINEFORGE_VERIFIER_CONTAINER_DIGEST or release/verifier-container-digest.txt"
                    .to_string(),
            ],
            reviewer_evidence: None,
            artifact: None,
        };
    };
    let digest_ok = digest.strip_prefix("sha256:").is_some_and(valid_sha256);
    EvidenceValidation {
        passed: digest_ok,
        evidence: vec![format!("verifier_container_digest={digest}")],
        reviewer_evidence: None,
        artifact: None,
    }
}

fn validate_architecture_matrix(evidence_dir: Option<&Path>) -> EvidenceValidation {
    let path = evidence_dir.and_then(|dir| {
        release_file_path(
            dir,
            "release/architecture-matrix.json",
            "architecture-matrix.json",
        )
    });
    let mut validation = validate_json_file(path.as_deref(), "architecture matrix", &[]);
    if !validation.passed {
        return validation;
    }
    let Some(path) = path else {
        return validation;
    };
    let Some(value) = super::common::read_json_value(&path) else {
        validation.passed = false;
        return validation;
    };
    validation.passed = value
        .get("runners")
        .and_then(|runners| runners.as_array())
        .is_some_and(|runners| !runners.is_empty());
    if !validation.passed {
        validation
            .evidence
            .push("architecture matrix runners array is missing or empty".to_string());
    }
    validation
}

fn validate_release_approval(evidence_dir: Option<&Path>) -> EvidenceValidation {
    let path = evidence_dir
        .and_then(|dir| release_file_path(dir, "approvals/release.json", "release-approval.json"))
        .or_else(|| std::env::var_os("REFINEFORGE_HUMAN_RELEASE_APPROVAL").map(PathBuf::from));
    validate_human_approval(path.as_deref(), "release", "release human approval")
}

fn release_file_path(dir: &Path, conventional: &str, local: &str) -> Option<PathBuf> {
    let conventional = dir.join(conventional);
    if conventional.exists() {
        return Some(conventional);
    }
    let local = dir.join(local);
    if local.exists() {
        return Some(local);
    }
    None
}

fn text_contains(path: &Path, needle: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|text| text.contains(needle))
        .unwrap_or(false)
}

fn devops_action_intents(mode: AgentMode, allow_expensive: bool) -> Vec<ActionIntent> {
    let execution_policy = if allow_expensive {
        "writes_evidence_and_runs_local_expensive_gates"
    } else {
        "writes_evidence_with_expensive_gates_skipped"
    };
    vec![
        action_intent(
            "devops.inspect.release_surfaces",
            "Inspect release docs, CI workflows, verifier container, and audit surfaces",
            "inspect",
            "read_only",
            "refine agent devops --mode inspect",
            &[
                "docs/release/release-readiness-inventory.md",
                ".github/workflows/ci.yml",
                "containers/Dockerfile.verifier",
            ],
        ),
        action_intent(
            "devops.release.ready_local",
            "Run local release readiness and write evidence artifacts",
            "verify",
            execution_policy,
            &format!("refine agent devops --mode {}", mode.as_str()),
            &[
                "release-ready-local",
                "release/evidence",
                "SBOM/provenance outputs",
            ],
        ),
        action_intent(
            "devops.audit.trust_boundary",
            "Keep hosted CI, OIDC signing, and human approval separate from local evidence",
            "audit",
            "evidence_only",
            "refine agent devops --mode check",
            &[
                "tool_checks.docker",
                "tool_checks.cosign",
                "release report warnings",
            ],
        ),
    ]
}

fn release_ready_success_trust_level(_allow_expensive: bool) -> TrustLevel {
    TrustLevel::ReleaseReadyLocal
}

fn release_ready_success_summary(allow_expensive: bool) -> &'static str {
    if allow_expensive {
        "Local release readiness passed with live expensive gates requested. CI/OIDC readiness is still not claimed."
    } else {
        "Local release readiness passed with explicit Docker/signature skips recorded. CI/OIDC readiness is not claimed."
    }
}

fn release_version_for_target(target: &str) -> String {
    if target.chars().all(|c| c.is_ascii_digit() || c == '.') && target.contains('.') {
        target.to_string()
    } else {
        "0.0.0-agent".to_string()
    }
}

fn relative_or_display(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::{
        release_ready_success_summary, release_ready_success_trust_level,
        release_version_for_target,
    };
    use crate::agent::common::TrustLevel;

    #[test]
    fn helyx_target_uses_non_release_probe_version() {
        assert_eq!(release_version_for_target("helyx"), "0.0.0-agent");
    }

    #[test]
    fn explicit_semver_target_is_preserved() {
        assert_eq!(release_version_for_target("1.2.3"), "1.2.3");
    }

    #[test]
    fn release_ready_success_is_capped_below_ci_readiness() {
        assert_eq!(
            release_ready_success_trust_level(false),
            TrustLevel::ReleaseReadyLocal
        );
        assert_eq!(
            release_ready_success_trust_level(true),
            TrustLevel::ReleaseReadyLocal
        );
        assert!(release_ready_success_summary(false).contains("CI/OIDC readiness is not claimed"));
        assert!(
            release_ready_success_summary(true).contains("CI/OIDC readiness is still not claimed")
        );
    }
}
