use super::common::{
    action_intent, capability, existing_artifact, repo_tool_check, seal_runtime,
    set_production_proof, tool_check, ActionIntent, AgentMode, AgentReport, AgentStatus,
    CommandRecord, ProductionProofStatus, ProductionRequirement, TrustLevel,
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
    seal_runtime(
        root,
        Some(out_dir),
        &mut report,
        TrustLevel::ReleaseReadyLocal,
        devops_action_intents(mode, allow_expensive),
    );
    report
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

    let hosted_ci_evidence = std::env::var("REFINEFORGE_HOSTED_CI_EVIDENCE").ok();
    requirements.push(ProductionRequirement::new_owned(
        "devops.hosted_ci_artifacts",
        "Hosted CI workflow passed and uploaded release evidence artifacts",
        if hosted_ci_evidence.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![hosted_ci_evidence.unwrap_or_else(|| {
            "hosted CI evidence path/URL not provided via REFINEFORGE_HOSTED_CI_EVIDENCE"
                .to_string()
        })],
    ));
    if std::env::var("REFINEFORGE_HOSTED_CI_EVIDENCE").is_err() {
        blockers.push("hosted CI evidence blocks release-ready-ci production proof".to_string());
    }

    let sigstore_evidence = std::env::var("REFINEFORGE_SIGSTORE_EVIDENCE").ok();
    requirements.push(ProductionRequirement::new_owned(
        "devops.sigstore_oidc_signature",
        "Sigstore keyless signing ran from GitHub OIDC and verified signer identity",
        if sigstore_evidence.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![sigstore_evidence.unwrap_or_else(|| {
            if allow_expensive {
                "local expensive gates do not replace hosted OIDC Sigstore evidence".to_string()
            } else {
                "Sigstore evidence not run; --allow-expensive was not requested".to_string()
            }
        })],
    ));
    if std::env::var("REFINEFORGE_SIGSTORE_EVIDENCE").is_err() {
        blockers.push("Sigstore OIDC signing evidence is missing".to_string());
    }

    let container_digest = std::env::var("REFINEFORGE_VERIFIER_CONTAINER_DIGEST").ok();
    requirements.push(ProductionRequirement::new_owned(
        "devops.verifier_container_digest",
        "Verifier container image is built, smoke-tested, and digest-recorded",
        if container_digest.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![container_digest.unwrap_or_else(|| {
            "verifier container digest not provided via REFINEFORGE_VERIFIER_CONTAINER_DIGEST"
                .to_string()
        })],
    ));
    if std::env::var("REFINEFORGE_VERIFIER_CONTAINER_DIGEST").is_err() {
        blockers.push("verifier container digest is missing".to_string());
    }

    let sbom = evidence_dir
        .map(|dir| dir.join("sbom.cyclonedx.json"))
        .filter(|path| path.exists());
    let provenance = evidence_dir
        .map(|dir| dir.join("provenance.intoto.json"))
        .filter(|path| path.exists());
    requirements.push(ProductionRequirement::new_owned(
        "devops.sbom_provenance_uploaded",
        "SBOM and provenance artifacts are generated and uploaded from CI",
        if sbom.is_some()
            && provenance.is_some()
            && std::env::var("REFINEFORGE_HOSTED_CI_EVIDENCE").is_ok()
        {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![
            sbom.map_or_else(
                || "sbom.cyclonedx.json missing from local evidence".to_string(),
                |path| path.display().to_string(),
            ),
            provenance.map_or_else(
                || "provenance.intoto.json missing from local evidence".to_string(),
                |path| path.display().to_string(),
            ),
        ],
    ));

    let flake_lock = root.join("flake.lock");
    requirements.push(ProductionRequirement::new(
        "devops.nix_locked_check",
        "Nix flake is locked and nix flake check passes without updating the lock",
        if flake_lock.exists() {
            AgentStatus::Partial
        } else {
            AgentStatus::Blocked
        },
        &[if flake_lock.exists() {
            "flake.lock exists; nix check evidence still required"
        } else {
            "flake.lock is missing"
        }],
    ));
    if !flake_lock.exists() {
        blockers.push(
            "Nix reproducibility evidence is missing because flake.lock is absent".to_string(),
        );
    }

    requirements.push(ProductionRequirement::new(
        "devops.architecture_matrix",
        "Release evidence records runner OS and CPU architecture for every lane",
        AgentStatus::Blocked,
        &["runner architecture matrix evidence is only authoritative from hosted CI"],
    ));
    blockers.push("hosted architecture matrix evidence is missing".to_string());

    let approval = std::env::var("REFINEFORGE_HUMAN_RELEASE_APPROVAL").ok();
    requirements.push(ProductionRequirement::new_owned(
        "devops.human_release_approval",
        "Named human release approval is present for this version",
        if approval.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![approval
            .unwrap_or_else(|| format!("no human release approval provided for target {target}"))],
    ));
    if std::env::var("REFINEFORGE_HUMAN_RELEASE_APPROVAL").is_err() {
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
        std::env::var("REFINEFORGE_HUMAN_RELEASE_APPROVAL")
            .ok()
            .into_iter()
            .collect(),
        blockers,
    );
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
