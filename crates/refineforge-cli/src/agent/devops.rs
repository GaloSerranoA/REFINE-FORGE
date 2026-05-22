use super::common::{
    capability, existing_artifact, repo_tool_check, tool_check, AgentMode, AgentReport,
    AgentStatus, CommandRecord, TrustLevel,
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
            TrustLevel::ReleaseReadyLocal,
            if allow_expensive {
                "Local release readiness passed with live expensive gates requested. CI/OIDC readiness is still not claimed."
            } else {
                "Local release readiness passed with explicit Docker/signature skips recorded. CI/OIDC readiness is not claimed."
            },
        );
    } else {
        report.finish(
            AgentStatus::Failed,
            TrustLevel::Blocked,
            "Local release readiness failed. See release evidence and command record.",
        );
    }
    report
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
    use super::release_version_for_target;

    #[test]
    fn helyx_target_uses_non_release_probe_version() {
        assert_eq!(release_version_for_target("helyx"), "0.0.0-agent");
    }

    #[test]
    fn explicit_semver_target_is_preserved() {
        assert_eq!(release_version_for_target("1.2.3"), "1.2.3");
    }
}
