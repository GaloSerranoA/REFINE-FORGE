//! The classifier itself. Pure functional — no I/O, no
//! globals, no `unsafe`. Every branch traces to a positive or
//! negative example in `docs/escalation-criteria.md` §3.

use crate::action::{
    Action, ClaimStatus, ExternalCitation, LossKind, SentenceKind, WeakeningKind,
};
use crate::category::Category;
use crate::context::ProjectContext;
use crate::decision::{Decision, EscalationReason, Evidence};
use crate::CRITERIA_VERSION;
use thiserror::Error;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Engine {
    _priv: (),
}

impl Engine {
    pub fn new() -> Self {
        Self { _priv: () }
    }

    /// Classify a proposed [`Action`] against the 9 categories.
    /// Returns [`Decision::Proceed`] if no category fires, or
    /// [`Decision::Escalate`] otherwise.
    ///
    /// Errors only when the `ctx`'s criteria-doc version differs
    /// from this engine's compiled-in [`crate::CRITERIA_VERSION`]
    /// (per criteria-doc §"Criteria version recording").
    pub fn decide(
        &self,
        action: &Action,
        ctx: &ProjectContext,
    ) -> Result<Decision, EngineError> {
        if ctx.criteria_version != CRITERIA_VERSION {
            return Err(EngineError::CriteriaVersionMismatch {
                expected: CRITERIA_VERSION.into(),
                found: ctx.criteria_version.clone(),
            });
        }

        let mut hits: Vec<(Category, Evidence)> = Vec::new();
        if let Some(e) = classify_scope(action, ctx) {
            hits.push((Category::Scope, e));
        }
        if let Some(e) = classify_idealisation(action) {
            hits.push((Category::Idealisation, e));
        }
        if let Some(e) = classify_custom_axiom(action) {
            hits.push((Category::CustomAxiom, e));
        }
        if let Some(e) = classify_customer_intent(action) {
            hits.push((Category::RefinementDocCustomerIntent, e));
        }
        if let Some(e) = classify_status_upgrade(action) {
            hits.push((Category::StatusUpgrade, e));
        }
        if let Some(e) = classify_theorem_weakening(action) {
            hits.push((Category::TheoremWeakening, e));
        }
        if let Some(e) = classify_external_fact(action) {
            hits.push((Category::ExternalFact, e));
        }
        if let Some(e) = classify_trust_base(action, ctx) {
            hits.push((Category::TrustBaseExtension, e));
        }
        if let Some(e) = classify_bit_exact(action) {
            hits.push((Category::BitExactRegression, e));
        }

        if hits.is_empty() {
            return Ok(Decision::Proceed);
        }

        let primary = pick_primary(hits.iter().map(|(c, _)| *c));
        let categories: Vec<Category> = hits.iter().map(|(c, _)| *c).collect();
        let evidence = hits
            .into_iter()
            .find(|(c, _)| *c == primary)
            .map(|(_, e)| e)
            .expect("primary is always in hits");
        let summary = summarise(action, primary, &categories);

        Ok(Decision::Escalate(EscalationReason {
            categories,
            primary,
            summary,
            evidence,
        }))
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EngineError {
    #[error(
        "criteria-doc version mismatch: engine implements v{expected} but ProjectContext was assembled against v{found}; refusing to operate (per criteria-doc §\"Criteria version recording\")"
    )]
    CriteriaVersionMismatch { expected: String, found: String },
}

// =====================================================================
// Multi-category resolution: when N categories fire on one action,
// pick the most specific as `primary` (its decision-packet template
// is the one the operator receives).
// =====================================================================

fn primary_specificity(c: Category) -> u8 {
    // Higher = more specific. Hand-tuned so that overlap cases
    // (e.g. AddLakePackage trips Scope + TrustBase; TrustBase
    // wins because its packet has the pinned-version field).
    match c {
        Category::CustomAxiom => 100,
        Category::TheoremWeakening => 90,
        Category::Idealisation => 80,
        Category::BitExactRegression => 75,
        Category::TrustBaseExtension => 70,
        Category::RefinementDocCustomerIntent => 60,
        Category::ExternalFact => 50,
        Category::StatusUpgrade => 40,
        Category::Scope => 20,
    }
}

fn pick_primary(cats: impl IntoIterator<Item = Category>) -> Category {
    cats.into_iter()
        .max_by_key(|c| primary_specificity(*c))
        .expect("called with empty iter")
}

fn summarise(action: &Action, primary: Category, all: &[Category]) -> String {
    let suffix = if all.len() > 1 {
        format!(
            " (also trips: {})",
            all.iter()
                .filter(|c| **c != primary)
                .map(|c| c.slug())
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        String::new()
    };
    let head = match (primary, action) {
        (Category::Scope, Action::AddLeanModule { path, .. }) => {
            format!("add new Lean module {}", path)
        }
        (Category::Scope, Action::AddTheorem { module, name, .. }) => {
            format!("add theorem {}.{} not in claim scope", module, name)
        }
        (Category::Scope, Action::AddLeanImport { import_path, .. }) => {
            format!("first-time Mathlib import {}", import_path)
        }
        (Category::Scope, Action::AddWorkspaceCrate { name }) => {
            format!("add workspace crate {}", name)
        }
        (Category::Scope, Action::AddTemplate { name }) => {
            format!("add scaffolding template {}", name)
        }
        (Category::Scope, Action::AddTopLevelDirectory { name }) => {
            format!("add top-level directory {}/", name)
        }
        (Category::Scope, Action::Unknown { description }) => {
            format!("unrecognised action shape: {}", description)
        }
        (Category::Idealisation, Action::MapRustToLean { rust_type, lean_type, .. }) => {
            format!("idealisation {} → {}", rust_type, lean_type)
        }
        (Category::CustomAxiom, Action::WriteAxiom { axiom_name, .. }) => {
            format!("declare custom axiom `{}`", axiom_name)
        }
        (
            Category::RefinementDocCustomerIntent,
            Action::WriteRefinementSentence { claim_id, .. },
        ) => format!("customer-intent claim in {} refinement doc", claim_id),
        (Category::StatusUpgrade, Action::BumpClaimStatus { claim_id, from, to }) => {
            format!(
                "{}: status {} → {}",
                claim_id,
                status_label(*from),
                status_label(*to)
            )
        }
        (Category::StatusUpgrade, Action::SetReviewOperator { claim_id, to, .. }) => {
            format!("{}: set human_operator = {}", claim_id, to)
        }
        (Category::TheoremWeakening, Action::EditTheorem { module, name, .. }) => {
            format!("weaken theorem {}.{}", module, name)
        }
        (Category::ExternalFact, Action::AssertExternalFact { .. }) => {
            "external-fact assertion not grounded in repo".to_string()
        }
        (Category::TrustBaseExtension, Action::BumpLeanToolchain { from, to }) => {
            format!("bump lean-toolchain {} → {}", from, to)
        }
        (Category::TrustBaseExtension, Action::AddLakePackage { name, .. }) => {
            format!("add Lake package {}", name)
        }
        (
            Category::TrustBaseExtension,
            Action::BumpCargoPin { crate_name, from, to, .. },
        ) => format!("bump {} pin {} → {}", crate_name, from, to),
        (Category::TrustBaseExtension, Action::SwitchCrate { from, to, .. }) => {
            format!("switch crate {} → {}", from, to)
        }
        (Category::TrustBaseExtension, Action::BumpCosignVersion { from, to }) => {
            format!("bump cosign {} → {}", from, to)
        }
        (Category::TrustBaseExtension, Action::BumpGitHubActionSha { action, .. }) => {
            format!("bump GH Action SHA for {}", action)
        }
        (Category::TrustBaseExtension, Action::AddVerifierDockerTool { tool }) => {
            format!("add tool `{}` to verifier image", tool)
        }
        (Category::TrustBaseExtension, Action::SwitchAnthropicModel { from, to }) => {
            format!("switch ANTHROPIC_MODEL {} → {}", from, to)
        }
        (Category::BitExactRegression, Action::EditKernelSource { kernel_id, summary }) => {
            format!("edit kernel {} source: {}", kernel_id, summary)
        }
        (
            Category::BitExactRegression,
            Action::ChangeKernelBuildFlags { kernel_id, .. },
        ) => format!("change build flags for kernel {}", kernel_id),
        (
            Category::BitExactRegression,
            Action::BumpKernelCompilerPin { kernel_id, compiler, .. },
        ) => format!("bump {} pin for kernel {}", compiler, kernel_id),
        (
            Category::BitExactRegression,
            Action::LowerBitExactRunCount { kernel_id, from, to },
        ) => format!(
            "lower run_count for kernel {} from {} to {}",
            kernel_id, from, to
        ),
        (Category::BitExactRegression, Action::AddKernelDirectory { kernel_id }) => {
            format!("add kernel directory {}/", kernel_id)
        }
        _ => format!("{}: {}", primary, action_kind(action)),
    };
    format!("{}{}", head, suffix)
}

fn status_label(s: ClaimStatus) -> &'static str {
    match s {
        ClaimStatus::Unformalized => "unformalized",
        ClaimStatus::Drafted => "drafted",
        ClaimStatus::Broken => "broken",
        ClaimStatus::ProvenModelOnly => "proven(model-only)",
        ClaimStatus::ProvenModelAndRefined => "proven(model+refined)",
    }
}

fn action_kind(a: &Action) -> &'static str {
    match a {
        Action::AddLeanModule { .. } => "AddLeanModule",
        Action::AddTheorem { .. } => "AddTheorem",
        Action::RenameTheorem { .. } => "RenameTheorem",
        Action::RestructureProof { .. } => "RestructureProof",
        Action::AddTestCase { .. } => "AddTestCase",
        Action::EditTheorem { .. } => "EditTheorem",
        Action::AddLeanImport { .. } => "AddLeanImport",
        Action::WriteAxiom { .. } => "WriteAxiom",
        Action::MapRustToLean { .. } => "MapRustToLean",
        Action::WriteRefinementSentence { .. } => "WriteRefinementSentence",
        Action::BumpClaimStatus { .. } => "BumpClaimStatus",
        Action::SetReviewOperator { .. } => "SetReviewOperator",
        Action::AssertExternalFact { .. } => "AssertExternalFact",
        Action::BumpLeanToolchain { .. } => "BumpLeanToolchain",
        Action::AddLakePackage { .. } => "AddLakePackage",
        Action::BumpCargoPin { .. } => "BumpCargoPin",
        Action::SwitchCrate { .. } => "SwitchCrate",
        Action::BumpCosignVersion { .. } => "BumpCosignVersion",
        Action::BumpGitHubActionSha { .. } => "BumpGitHubActionSha",
        Action::AddVerifierDockerTool { .. } => "AddVerifierDockerTool",
        Action::SwitchAnthropicModel { .. } => "SwitchAnthropicModel",
        Action::AddWorkspaceCrate { .. } => "AddWorkspaceCrate",
        Action::AddTemplate { .. } => "AddTemplate",
        Action::AddTopLevelDirectory { .. } => "AddTopLevelDirectory",
        Action::EditKernelSource { .. } => "EditKernelSource",
        Action::ChangeKernelBuildFlags { .. } => "ChangeKernelBuildFlags",
        Action::BumpKernelCompilerPin { .. } => "BumpKernelCompilerPin",
        Action::LowerBitExactRunCount { .. } => "LowerBitExactRunCount",
        Action::AddKernelDirectory { .. } => "AddKernelDirectory",
        Action::Reformat { .. } => "Reformat",
        Action::RenameLocalVar { .. } => "RenameLocalVar",
        Action::AddCliHelpText { .. } => "AddCliHelpText",
        Action::Unknown { .. } => "Unknown",
    }
}

// =====================================================================
// Per-category classifiers. Each returns Some(Evidence) when the
// category fires; None means "doesn't apply." The criteria-doc
// example traceability for each branch is named in a one-line
// comment — keep these in sync with v0.2.
// =====================================================================

fn classify_scope(action: &Action, ctx: &ProjectContext) -> Option<Evidence> {
    match action {
        // "AI wants to introduce a new Lean module that isn't imported from the library root."
        Action::AddLeanModule { path, .. } => Some(Evidence::Scope {
            what_added: format!("new Lean module: {}", path),
            smallest_in_scope_alternative: None,
        }),

        // "Claim's lean.theorems lists [t1, t2] and the AI wants to add t3."
        Action::AddTheorem { name, .. } => {
            let in_scope = ctx
                .claim
                .as_ref()
                .map(|c| c.lean_theorems.contains(name))
                .unwrap_or(false);
            if in_scope {
                None
            } else {
                Some(Evidence::Scope {
                    what_added: format!("new theorem `{}` not in claim's lean.theorems", name),
                    smallest_in_scope_alternative: Some(
                        "rephrase as a lemma named in claim scope".into(),
                    ),
                })
            }
        }

        // "First time the AI uses a Mathlib import in this project."
        // v0.2: Mathlib-first-use merged into Scope (resolution of open Q1).
        Action::AddLeanImport {
            import_path,
            is_mathlib,
            ..
        } => {
            if *is_mathlib && !ctx.mathlib_imports_existing.contains(import_path) {
                Some(Evidence::Scope {
                    what_added: format!("first-time Mathlib import: {}", import_path),
                    smallest_in_scope_alternative: Some(
                        "inline the needed lemma instead of importing".into(),
                    ),
                })
            } else {
                None
            }
        }

        // "AI wants to add a new workspace crate ..."
        Action::AddWorkspaceCrate { name } => Some(Evidence::Scope {
            what_added: format!("new workspace crate: {}", name),
            smallest_in_scope_alternative: None,
        }),

        // "... a new template under templates/."
        Action::AddTemplate { name } => Some(Evidence::Scope {
            what_added: format!("new scaffolding template: {}", name),
            smallest_in_scope_alternative: None,
        }),

        // "... a new top-level directory ..."
        Action::AddTopLevelDirectory { name } => Some(Evidence::Scope {
            what_added: format!("new top-level directory: {}/", name),
            smallest_in_scope_alternative: None,
        }),

        // AddLakePackage also trips Cat 8; surface it as Scope too
        // because it expands the project's external dep surface.
        Action::AddLakePackage { name, .. } => Some(Evidence::Scope {
            what_added: format!("new Lake package: {}", name),
            smallest_in_scope_alternative: Some(
                "inline the lemma from the package instead".into(),
            ),
        }),

        // AddKernelDirectory expands the kernels/ surface.
        Action::AddKernelDirectory { kernel_id } => Some(Evidence::Scope {
            what_added: format!("new kernel directory: kernels/{}/", kernel_id),
            smallest_in_scope_alternative: None,
        }),

        // Per autonomous-driver-plan.md §6: "treat unknown action
        // shapes as Category-1 (Scope) by default; never silently
        // auto-proceed."
        Action::Unknown { description } => Some(Evidence::UnknownActionShape {
            description: description.clone(),
        }),

        // Everything else is not a scope action.
        _ => None,
    }
}

fn classify_idealisation(action: &Action) -> Option<Evidence> {
    if let Action::MapRustToLean {
        rust_type,
        lean_type,
        lossy_kinds,
    } = action
    {
        if !lossy_kinds.is_empty() {
            return Some(Evidence::Idealisation {
                rust_type: rust_type.clone(),
                lean_type: lean_type.clone(),
                lost_properties: lossy_kinds.clone(),
            });
        }
    }
    None
}

fn classify_custom_axiom(action: &Action) -> Option<Evidence> {
    // "ANY axiom statement, including ones the AI considers 'obviously true.'"
    if let Action::WriteAxiom {
        module,
        axiom_name,
        statement,
    } = action
    {
        return Some(Evidence::CustomAxiom {
            module: module.clone(),
            axiom_name: axiom_name.clone(),
            statement: statement.clone(),
        });
    }
    None
}

fn classify_customer_intent(action: &Action) -> Option<Evidence> {
    if let Action::WriteRefinementSentence {
        claim_id,
        sentence,
        kind: SentenceKind::CustomerIntent,
    } = action
    {
        return Some(Evidence::CustomerIntent {
            claim_id: claim_id.clone(),
            sentence: sentence.clone(),
        });
    }
    None
}

fn classify_status_upgrade(action: &Action) -> Option<Evidence> {
    match action {
        Action::BumpClaimStatus { claim_id, from, to } => {
            use ClaimStatus::*;
            // Per criteria-doc Cat 5 "Do NOT escalate":
            //   Unformalized → Drafted (intent change only).
            // Everything else is conservative: escalate.
            match (from, to) {
                (Unformalized, Drafted) => None,
                _ => Some(Evidence::StatusUpgrade {
                    claim_id: claim_id.clone(),
                    from: status_label(*from).into(),
                    to: status_label(*to).into(),
                }),
            }
        }
        // "AI wants to flip review.human_operator from null to any value."
        Action::SetReviewOperator { claim_id, from, to } => {
            if from.is_none() {
                Some(Evidence::StatusUpgrade {
                    claim_id: claim_id.clone(),
                    from: "human_operator: null".into(),
                    to: format!("human_operator: {}", to),
                })
            } else {
                // Operator-replacement (e.g. handover) is its own
                // process; still escalate defensively.
                Some(Evidence::StatusUpgrade {
                    claim_id: claim_id.clone(),
                    from: format!("human_operator: {}", from.as_deref().unwrap_or("?")),
                    to: format!("human_operator: {}", to),
                })
            }
        }
        _ => None,
    }
}

fn classify_theorem_weakening(action: &Action) -> Option<Evidence> {
    if let Action::EditTheorem {
        module,
        name,
        statement_before,
        statement_after,
        weakening: Some(kind),
    } = action
    {
        return Some(Evidence::TheoremWeakening {
            module: module.clone(),
            theorem: name.clone(),
            statement_before: statement_before.clone(),
            statement_after: statement_after.clone(),
            weakening: *kind,
        });
    }
    None
}

fn classify_external_fact(action: &Action) -> Option<Evidence> {
    if let Action::AssertExternalFact {
        assertion,
        citation,
    } = action
    {
        if !citation.is_repo_grounded() {
            return Some(Evidence::ExternalFact {
                assertion: assertion.clone(),
                citation: citation.clone(),
            });
        }
    }
    None
}

fn classify_trust_base(action: &Action, ctx: &ProjectContext) -> Option<Evidence> {
    match action {
        Action::BumpLeanToolchain { from, to } => Some(Evidence::TrustBaseExtension {
            what: "lean-toolchain pin".into(),
            from: Some(from.clone()),
            to: to.clone(),
        }),
        Action::AddLakePackage {
            name,
            version_or_rev,
        } => Some(Evidence::TrustBaseExtension {
            what: format!("Lake package {}", name),
            from: None,
            to: version_or_rev.clone(),
        }),
        Action::BumpCargoPin {
            crate_name,
            from,
            to,
            in_bundle,
            cited_in_refinement_doc,
        } => {
            // Per criteria-doc Cat 8 "Do NOT escalate":
            //   "Updating a dev-dependency that doesn't appear
            //    in any bundle ... DO escalate if a
            //    refinement-doc cites it."
            // Treat the bundle-chain set as authoritative; the
            // `in_bundle` flag is the action's own hint.
            let in_chain = *in_bundle || ctx.bundle_chain_crates.contains(crate_name);
            if in_chain || *cited_in_refinement_doc {
                Some(Evidence::TrustBaseExtension {
                    what: format!("Cargo pin: {}", crate_name),
                    from: Some(from.clone()),
                    to: to.clone(),
                })
            } else {
                None
            }
        }
        Action::SwitchCrate {
            from,
            to,
            in_bundle,
        } => {
            if *in_bundle {
                Some(Evidence::TrustBaseExtension {
                    what: "Cargo crate switch".into(),
                    from: Some(from.clone()),
                    to: to.clone(),
                })
            } else {
                None
            }
        }
        Action::BumpCosignVersion { from, to } => Some(Evidence::TrustBaseExtension {
            what: "cosign version".into(),
            from: Some(from.clone()),
            to: to.clone(),
        }),
        Action::BumpGitHubActionSha { action, from, to } => {
            Some(Evidence::TrustBaseExtension {
                what: format!("GitHub Action {}", action),
                from: Some(from.clone()),
                to: to.clone(),
            })
        }
        Action::AddVerifierDockerTool { tool } => Some(Evidence::TrustBaseExtension {
            what: "verifier Dockerfile tool".into(),
            from: None,
            to: tool.clone(),
        }),
        Action::SwitchAnthropicModel { from, to } => {
            if ctx.approved_anthropic_models.contains(to) {
                None
            } else {
                Some(Evidence::TrustBaseExtension {
                    what: "ANTHROPIC_MODEL".into(),
                    from: Some(from.clone()),
                    to: to.clone(),
                })
            }
        }
        _ => None,
    }
}

fn classify_bit_exact(action: &Action) -> Option<Evidence> {
    match action {
        Action::EditKernelSource {
            kernel_id,
            summary,
        } => Some(Evidence::BitExactRegression {
            kernel_id: kernel_id.clone(),
            change_summary: format!("edit source: {}", summary),
        }),
        Action::ChangeKernelBuildFlags {
            kernel_id,
            from,
            to,
        } => Some(Evidence::BitExactRegression {
            kernel_id: kernel_id.clone(),
            change_summary: format!("build flags: {} → {}", from, to),
        }),
        Action::BumpKernelCompilerPin {
            kernel_id,
            compiler,
            from,
            to,
        } => Some(Evidence::BitExactRegression {
            kernel_id: kernel_id.clone(),
            change_summary: format!("bump {} pin: {} → {}", compiler, from, to),
        }),
        Action::LowerBitExactRunCount {
            kernel_id,
            from,
            to,
        } => {
            // "AI proposes lowering run_count below the baseline
            // the gate previously passed at (would mask divergence)."
            // Strictly less means escalate; raising or equal is fine.
            if to < from {
                Some(Evidence::BitExactRegression {
                    kernel_id: kernel_id.clone(),
                    change_summary: format!("run_count {} → {} (lowered)", from, to),
                })
            } else {
                None
            }
        }
        Action::AddKernelDirectory { kernel_id } => Some(Evidence::BitExactRegression {
            kernel_id: kernel_id.clone(),
            change_summary: "new kernel directory adds an un-baselined target".into(),
        }),
        _ => None,
    }
}

// Suppress unused-warning for LossKind without importing it via re-export.
#[allow(dead_code)]
fn _unused_loss_kind_anchor(_k: LossKind, _w: WeakeningKind, _c: ExternalCitation) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_new_is_default() {
        assert_eq!(Engine::new(), Engine::default());
    }

    #[test]
    fn criteria_version_mismatch_refuses() {
        let eng = Engine::new();
        let ctx = ProjectContext::test_with_wrong_criteria_version("0.99");
        let act = Action::Reformat {
            paths: vec!["x.rs".into()],
        };
        let res = eng.decide(&act, &ctx);
        assert!(matches!(res, Err(EngineError::CriteriaVersionMismatch { .. })));
    }

    #[test]
    fn criteria_version_match_runs() {
        let eng = Engine::new();
        let ctx = ProjectContext::test_default();
        let act = Action::Reformat {
            paths: vec!["x.rs".into()],
        };
        let res = eng.decide(&act, &ctx);
        assert!(matches!(res, Ok(Decision::Proceed)));
    }

    #[test]
    fn pick_primary_prefers_specific_over_scope() {
        let p = pick_primary([Category::Scope, Category::TrustBaseExtension]);
        assert_eq!(p, Category::TrustBaseExtension);
    }

    #[test]
    fn pick_primary_prefers_axiom_over_everything() {
        let p = pick_primary([
            Category::Scope,
            Category::TrustBaseExtension,
            Category::CustomAxiom,
            Category::Idealisation,
        ]);
        assert_eq!(p, Category::CustomAxiom);
    }

    #[test]
    fn summarise_lists_secondary_categories() {
        let s = summarise(
            &Action::AddLakePackage {
                name: "mathlib".into(),
                version_or_rev: "v4.29.0".into(),
            },
            Category::TrustBaseExtension,
            &[Category::Scope, Category::TrustBaseExtension],
        );
        assert!(s.contains("Lake package mathlib"), "{}", s);
        assert!(s.contains("scope"), "missing secondary in: {}", s);
    }
}
