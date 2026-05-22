use super::common::{
    capability, existing_artifact, repo_tool_check, AgentMode, AgentReport, AgentStatus,
    CommandRecord, TrustLevel,
};
use crate::{claim, lint, runner, scan};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub fn build(root: &Path, mode: AgentMode, target: &str) -> AgentReport {
    let mut report = AgentReport::new(super::common::AgentKind::Lean, mode, target);
    report.capabilities.extend([
        capability(
            "proof-inventory",
            "available",
            "inspects Lean theorem inventory and claim linkage surfaces",
        ),
        capability(
            "verification-gates",
            "available",
            "runs Lean, scanner, and claim-linter gates in check/repair/execute modes",
        ),
        capability(
            "truth-bounded-claims",
            "available",
            "keeps CRS/model-only scopes separate from implementation correctness claims",
        ),
        capability(
            "repair-boundary",
            "evidence_only",
            "repair mode runs the same verification gates and reports blockers for operator-directed fixes",
        ),
    ]);
    report.tool_checks.extend([
        repo_tool_check(root, "refine", true),
        repo_tool_check(root, "lake", false),
    ]);
    existing_artifact(root, "docs/verification/proof-inventory.md", &mut report);
    existing_artifact(root, "claims", &mut report);
    existing_artifact(root, "lean/Refineforge.lean", &mut report);

    if !mode.runs_checks() {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::ModelOnly,
            "Lean agent inspected proof inventory and claim/Lean surfaces. No implementation correctness is claimed.",
        );
        return report;
    }

    let lean_started = Instant::now();
    let lean = runner::check_all(root);
    report.commands.push(CommandRecord::internal(
        "lean-check-all",
        &["refine", "lean", "check-all"],
        lean_started,
        &lean,
    ));

    let scan_started = Instant::now();
    let scan = scan::scan_all(root);
    report.commands.push(CommandRecord::internal(
        "scan-check-all",
        &["refine", "scan", "check-all"],
        scan_started,
        &scan,
    ));

    let lint_started = Instant::now();
    let lint = lint::lint_all(root);
    report.commands.push(CommandRecord::internal(
        "lint-check-all",
        &["refine", "lint", "check-all"],
        lint_started,
        &lint,
    ));

    if lean.is_ok() && scan.is_ok() && lint.is_ok() {
        match claim_trust_floor(root, target) {
            Ok(assessment) => {
                report.warnings.extend(assessment.warnings);
                report.finish(
                    AgentStatus::Passed,
                    assessment.trust_level,
                    assessment.summary,
                );
            }
            Err(err) => {
                report.blockers.push(format!(
                    "could not derive Lean trust floor from claims: {err}"
                ));
                report.finish(
                    AgentStatus::Blocked,
                    TrustLevel::Blocked,
                    "Lean, scan, and claim-lint gates passed, but trust classification could not be derived from claim scopes.",
                );
            }
        }
    } else {
        report.finish(
            AgentStatus::Failed,
            TrustLevel::Blocked,
            "One or more Lean verification gates failed. See command records for the failing gate.",
        );
    }
    report
}

struct ClaimTrustAssessment {
    trust_level: TrustLevel,
    warnings: Vec<String>,
    summary: &'static str,
}

fn claim_trust_floor(root: &Path, target: &str) -> Result<ClaimTrustAssessment> {
    let claims = selected_claims(root, target)?;
    if claims.is_empty() {
        bail!("no claims selected for target {target:?}");
    }

    let mut model_only = Vec::new();
    let mut missing_refinement_docs = Vec::new();
    for (_, claim) in &claims {
        let scope = claim.scope.trim().to_ascii_lowercase();
        if scope == "model-only" || claim.rust_source.is_empty() {
            model_only.push(claim.claim_id.clone());
        } else if !refinement_doc(root, &claim.claim_id).exists() {
            missing_refinement_docs.push(claim.claim_id.clone());
        }
    }

    let mut warnings = Vec::new();
    if !model_only.is_empty() {
        warnings.push(format!(
            "Lean gates passed, but trust remains model-only because selected claim(s) have model-only scope or no Rust source: {}",
            model_only.join(", ")
        ));
    }
    if !missing_refinement_docs.is_empty() {
        warnings.push(format!(
            "Lean gates passed, but model-linked trust is blocked by missing refinement docs for: {}",
            missing_refinement_docs.join(", ")
        ));
    }

    if model_only.is_empty() && missing_refinement_docs.is_empty() {
        Ok(ClaimTrustAssessment {
            trust_level: TrustLevel::ModelLinked,
            warnings,
            summary: "Lean, scan, and claim-lint gates passed. Trust is model-linked because selected claims include Rust sources and refinement docs.",
        })
    } else {
        Ok(ClaimTrustAssessment {
            trust_level: TrustLevel::ModelOnly,
            warnings,
            summary: "Lean, scan, and claim-lint gates passed. Trust is model-only because selected claim scopes/refinement evidence do not all establish model-Rust links.",
        })
    }
}

fn selected_claims(root: &Path, target: &str) -> Result<Vec<(PathBuf, claim::Claim)>> {
    let mut claims = claim::all(root)?;
    claims.sort_by(|a, b| a.1.claim_id.cmp(&b.1.claim_id));
    if target.eq_ignore_ascii_case("helyx") || target.eq_ignore_ascii_case("all") {
        return Ok(claims);
    }

    let selected: Vec<_> = claims
        .into_iter()
        .filter(|(_, claim)| claim.claim_id.eq_ignore_ascii_case(target))
        .collect();
    if selected.is_empty() {
        bail!("target claim {target:?} not found");
    }
    Ok(selected)
}

fn refinement_doc(root: &Path, claim_id: &str) -> PathBuf {
    root.join("docs")
        .join("refinement")
        .join(format!("{claim_id}.md"))
}
