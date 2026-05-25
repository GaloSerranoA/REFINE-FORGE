use super::common::{
    action_intent, capability, existing_artifact, repo_tool_check, seal_runtime,
    set_production_proof, valid_sha256, validate_human_approval, validate_json_file, ActionIntent,
    AgentMode, AgentReport, AgentStatus, CommandRecord, EvidenceValidation, ProductionProofStatus,
    ProductionRequirement, TrustLevel,
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
    existing_artifact(
        root,
        "docs/verification/lean-production-proof-checklist.md",
        &mut report,
    );
    existing_artifact(root, "claims", &mut report);
    existing_artifact(root, "lean/Refineforge.lean", &mut report);

    if !mode.runs_checks() {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::ModelOnly,
            "Lean agent inspected proof inventory and claim/Lean surfaces. No implementation correctness is claimed.",
        );
        apply_lean_production_proof(&mut report, root, target, false, false, false);
        seal_runtime(
            root,
            None,
            &mut report,
            TrustLevel::ModelLinked,
            lean_action_intents(mode),
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
    apply_lean_production_proof(
        &mut report,
        root,
        target,
        lean.is_ok(),
        scan.is_ok(),
        lint.is_ok(),
    );
    seal_runtime(
        root,
        None,
        &mut report,
        TrustLevel::ModelLinked,
        lean_action_intents(mode),
    );
    report
}

fn apply_lean_production_proof(
    report: &mut AgentReport,
    root: &Path,
    target: &str,
    lean_ok: bool,
    scan_ok: bool,
    lint_ok: bool,
) {
    let mut requirements = Vec::new();
    let mut blockers = Vec::new();
    let mut reviewer_evidence = Vec::new();
    let evidence_dir = lean_evidence_dir();
    let bundle_hashes = validate_lean_bundle_hashes(evidence_dir.as_deref());
    let role_approval = validate_lean_approval(evidence_dir.as_deref());

    requirements.push(ProductionRequirement::new(
        "lean.no_sorry_gate",
        "Lean theorem gate passes without sorry, admit, or project-local axioms",
        if lean_ok {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        &[if lean_ok {
            "lean-check-all passed"
        } else {
            "lean-check-all not run or failed"
        }],
    ));
    requirements.push(ProductionRequirement::new(
        "lean.rust_scan_symbols",
        "Deterministic Rust scan resolves every cited implementation symbol",
        if scan_ok {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        &[if scan_ok {
            "scan-check-all passed"
        } else {
            "scan-check-all not run or failed"
        }],
    ));

    if !lean_ok {
        blockers
            .push("Lean production proof requires a passing lean-check-all command".to_string());
    }
    if !scan_ok {
        blockers
            .push("Lean production proof requires a passing scan-check-all command".to_string());
    }
    if !lint_ok {
        blockers
            .push("Lean production proof requires a passing lint-check-all command".to_string());
    }

    match selected_claims(root, target) {
        Ok(claims) => {
            let claim_ids: Vec<String> = claims
                .iter()
                .map(|(_, claim)| claim.claim_id.clone())
                .collect();
            let model_only: Vec<String> = claims
                .iter()
                .filter(|(_, claim)| {
                    claim.scope.trim().eq_ignore_ascii_case("model-only")
                        || claim.rust_source.is_empty()
                })
                .map(|(_, claim)| claim.claim_id.clone())
                .collect();
            let missing_refinement_docs: Vec<String> = claims
                .iter()
                .filter(|(_, claim)| {
                    !claim.scope.trim().eq_ignore_ascii_case("model-only")
                        && !claim.rust_source.is_empty()
                        && !refinement_doc(root, &claim.claim_id).exists()
                })
                .map(|(_, claim)| claim.claim_id.clone())
                .collect();
            let missing_review: Vec<String> = claims
                .iter()
                .filter(|(_, claim)| claim.review.human_operator.is_none())
                .map(|(_, claim)| claim.claim_id.clone())
                .collect();

            for (_, claim) in &claims {
                if let Some(operator) = &claim.review.human_operator {
                    reviewer_evidence.push(format!(
                        "{} reviewed by {} on {}",
                        claim.claim_id,
                        operator,
                        claim
                            .review
                            .reviewed_on
                            .as_deref()
                            .unwrap_or("unknown-date")
                    ));
                }
            }

            if !model_only.is_empty() {
                blockers.push(format!(
                    "model-only claims block implementation production proof: {}",
                    model_only.join(", ")
                ));
            }
            if !missing_refinement_docs.is_empty() {
                blockers.push(format!(
                    "missing refinement docs block production proof: {}",
                    missing_refinement_docs.join(", ")
                ));
            }
            if !missing_review.is_empty() && !role_approval.passed {
                blockers.push(format!(
                    "missing human review blocks production proof: {}",
                    missing_review.join(", ")
                ));
            }
            requirements.push(ProductionRequirement::new_owned(
                "lean.claim_scope_model_refined",
                "Every selected implementation claim uses model+refined scope",
                if model_only.is_empty() {
                    AgentStatus::Passed
                } else {
                    AgentStatus::Blocked
                },
                vec![format!("selected claims: {}", claim_ids.join(", "))],
            ));
            requirements.push(ProductionRequirement::new_owned(
                "lean.refinement_docs",
                "Every selected implementation claim has a refinement document",
                if model_only.is_empty() && missing_refinement_docs.is_empty() {
                    AgentStatus::Passed
                } else {
                    AgentStatus::Blocked
                },
                vec![if missing_refinement_docs.is_empty() {
                    "no missing implementation refinement docs detected".to_string()
                } else {
                    format!("missing docs: {}", missing_refinement_docs.join(", "))
                }],
            ));
            requirements.push(ProductionRequirement::new_owned(
                "lean.bundle_hashes",
                "Selected claims have exported verification bundle hashes",
                if bundle_hashes.passed {
                    AgentStatus::Passed
                } else {
                    AgentStatus::Blocked
                },
                if bundle_hashes.evidence.is_empty() {
                    vec!["bundle export evidence is absent".to_string()]
                } else {
                    bundle_hashes.evidence.clone()
                },
            ));
            if !bundle_hashes.passed {
                blockers.push(
                    "Lean production proof requires exported bundle hash evidence for selected claims"
                        .to_string(),
                );
            }
            if let Some(reviewer) = &role_approval.reviewer_evidence {
                reviewer_evidence.push(reviewer.clone());
            }
            requirements.push(ProductionRequirement::new_owned(
                "lean.human_review",
                "Every selected implementation claim has explicit human review",
                if (missing_review.is_empty() && !reviewer_evidence.is_empty())
                    || role_approval.passed
                {
                    AgentStatus::Passed
                } else {
                    AgentStatus::Blocked
                },
                if !role_approval.evidence.is_empty() {
                    role_approval.evidence.clone()
                } else if reviewer_evidence.is_empty() {
                    vec!["review.human_operator is absent for selected claims".to_string()]
                } else {
                    reviewer_evidence.clone()
                },
            ));
        }
        Err(err) => {
            blockers.push(format!(
                "could not inspect selected claims for Lean production proof: {err}"
            ));
            requirements.push(ProductionRequirement::new(
                "lean.claim_scope_model_refined",
                "Every selected implementation claim uses model+refined scope",
                AgentStatus::Blocked,
                &["selected claims could not be loaded"],
            ));
            requirements.push(ProductionRequirement::new(
                "lean.refinement_docs",
                "Every selected implementation claim has a refinement document",
                AgentStatus::Blocked,
                &["selected claims could not be loaded"],
            ));
            requirements.push(ProductionRequirement::new(
                "lean.bundle_hashes",
                "Selected claims have exported verification bundle hashes",
                AgentStatus::Blocked,
                &["selected claims could not be loaded"],
            ));
            requirements.push(ProductionRequirement::new(
                "lean.human_review",
                "Every selected implementation claim has explicit human review",
                AgentStatus::Blocked,
                &["selected claims could not be loaded"],
            ));
        }
    }

    let status = if blockers.is_empty() && !reviewer_evidence.is_empty() {
        ProductionProofStatus::HumanReviewed
    } else if blockers.is_empty() {
        ProductionProofStatus::Ready
    } else {
        ProductionProofStatus::Blocked
    };
    set_production_proof(report, status, requirements, reviewer_evidence, blockers);
}

fn lean_evidence_dir() -> Option<PathBuf> {
    std::env::var_os("REFINEFORGE_LEAN_EVIDENCE_DIR").map(PathBuf::from)
}

fn lean_file_path(dir: &Path, conventional: &str, local: &str) -> Option<PathBuf> {
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

fn validate_lean_bundle_hashes(evidence_dir: Option<&Path>) -> EvidenceValidation {
    let path = evidence_dir
        .and_then(|dir| lean_file_path(dir, "lean/bundle-hashes.json", "bundle-hashes.json"));
    let mut validation = validate_json_file(path.as_deref(), "Lean bundle hashes", &["passed"]);
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
        .get("bundles")
        .and_then(|bundles| bundles.as_array())
        .is_some_and(|bundles| {
            !bundles.is_empty()
                && bundles.iter().all(|bundle| {
                    bundle
                        .get("sha256")
                        .and_then(|hash| hash.as_str())
                        .is_some_and(valid_sha256)
                })
        });
    if !validation.passed {
        validation
            .evidence
            .push("bundle hashes must contain non-empty bundles with sha256".to_string());
    }
    validation
}

fn validate_lean_approval(evidence_dir: Option<&Path>) -> EvidenceValidation {
    let path = evidence_dir
        .and_then(|dir| lean_file_path(dir, "approvals/lean.json", "lean-approval.json"));
    validate_human_approval(path.as_deref(), "lean", "Lean human approval")
}

fn lean_action_intents(mode: AgentMode) -> Vec<ActionIntent> {
    vec![
        action_intent(
            "lean.inspect.claims",
            "Inspect Lean inventory and claim scopes",
            "inspect",
            "read_only",
            "refine agent lean --mode inspect",
            &[
                "docs/verification/proof-inventory.md",
                "claims/*.yaml",
                "lean/Refineforge.lean",
            ],
        ),
        action_intent(
            "lean.verify.gates",
            "Run Lean, scan, and claim lint gates",
            "verify",
            "writes_evidence",
            "refine agent lean --mode check",
            &["lean-check-all", "scan-check-all", "lint-check-all"],
        ),
        action_intent(
            "lean.classify.trust",
            "Classify model-only versus model-linked trust",
            "audit",
            "evidence_only",
            &format!("refine agent lean --mode {}", mode.as_str()),
            &[
                "claim.scope",
                "claim.rust_source",
                "docs/refinement/<claim>.md",
            ],
        ),
    ]
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
