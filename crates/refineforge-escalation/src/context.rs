//! Read-only view of the project the engine queries during
//! [`crate::Engine::decide`]. The driver populates this once
//! per claim; the engine never mutates it and never performs I/O.
//!
//! File-loaders that build a `ProjectContext` from claim YAMLs,
//! `lean/lake-manifest.json`, and `Cargo.lock` are deferred to
//! Phase 2 of `docs/autonomous-driver-plan.md` — the driver
//! crate will own them. Phase 1 ships the struct + a
//! [`ProjectContext::test_default`] constructor for unit tests.

use crate::action::ClaimStatus;
use crate::CRITERIA_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectContext {
    /// The criteria-doc version this context was assembled
    /// against. Must equal [`crate::CRITERIA_VERSION`] or
    /// [`crate::Engine::decide`] refuses to operate.
    pub criteria_version: String,

    /// The claim the autonomous driver is currently working on,
    /// if any. `None` is allowed (e.g. for cross-cutting actions
    /// like a toolchain bump).
    pub claim: Option<ClaimSummary>,

    /// Mathlib import paths that already appear in
    /// `lean/lake-manifest.json`. Used to detect first-time
    /// Mathlib use (Category 1, per v0.2 resolution of open
    /// question §1 — Mathlib first-use is merged into Scope).
    pub mathlib_imports_existing: HashSet<String>,

    /// Lake packages present in `lake-manifest.json` already.
    /// Adding any other Lake package is a Cat-1 + Cat-8 trip.
    pub lake_packages_existing: HashSet<String>,

    /// Crate names known to participate in the bundle's trust
    /// chain. Bumping a pin for any of these is Cat 8.
    pub bundle_chain_crates: HashSet<String>,

    /// Anthropic model IDs the operator has previously approved.
    /// Switching to anything outside this set is Cat 8.
    pub approved_anthropic_models: HashSet<String>,

    /// Kernel IDs that already have a passing `refine-bitexact`
    /// baseline. Changing anything that affects them is Cat 9.
    pub kernels_with_baseline: HashSet<String>,

    /// Existing top-level workspace crate names.
    pub existing_workspace_crates: HashSet<String>,

    /// Existing template names under `templates/`.
    pub existing_templates: HashSet<String>,

    /// Existing top-level directory names in the repo root.
    pub existing_top_level_dirs: HashSet<String>,

    /// Existing Lean module qualified names (e.g.
    /// `"Refineforge.Counter"`).
    pub existing_lean_modules: HashSet<String>,
}

impl ProjectContext {
    /// Minimal context valid for tests. Carries the engine's
    /// own `CRITERIA_VERSION` and no claim / no
    /// existing-things-known.
    pub fn test_default() -> Self {
        Self {
            criteria_version: CRITERIA_VERSION.into(),
            claim: None,
            mathlib_imports_existing: HashSet::new(),
            lake_packages_existing: HashSet::new(),
            bundle_chain_crates: HashSet::new(),
            approved_anthropic_models: HashSet::new(),
            kernels_with_baseline: HashSet::new(),
            existing_workspace_crates: HashSet::new(),
            existing_templates: HashSet::new(),
            existing_top_level_dirs: HashSet::new(),
            existing_lean_modules: HashSet::new(),
        }
    }

    /// Build a context whose `criteria_version` is wrong on
    /// purpose — for testing the mismatch refusal path.
    #[doc(hidden)]
    pub fn test_with_wrong_criteria_version(v: impl Into<String>) -> Self {
        Self {
            criteria_version: v.into(),
            ..Self::test_default()
        }
    }
}

/// The fields the engine needs from a claim YAML. Populated by
/// the driver's claim-YAML loader (Phase 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimSummary {
    pub id: String,
    pub status: ClaimStatus,
    /// True when the claim's `scope:` is `model-only` (no
    /// `rust_source:` block expected).
    pub scope_model_only: bool,
    /// Theorem names listed in the claim YAML's `lean.theorems`.
    pub lean_theorems: HashSet<String>,
    /// Type names listed in the claim YAML's `rust_source.types`.
    pub rust_source_types: HashSet<String>,
    /// `review.human_operator` value (None when null).
    pub review_human_operator: Option<String>,
}

impl ClaimSummary {
    pub fn test_default(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: ClaimStatus::Drafted,
            scope_model_only: true,
            lean_theorems: HashSet::new(),
            rust_source_types: HashSet::new(),
            review_human_operator: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_uses_engine_criteria_version() {
        let ctx = ProjectContext::test_default();
        assert_eq!(ctx.criteria_version, CRITERIA_VERSION);
    }

    #[test]
    fn test_default_is_otherwise_empty() {
        let ctx = ProjectContext::test_default();
        assert!(ctx.claim.is_none());
        assert!(ctx.mathlib_imports_existing.is_empty());
        assert!(ctx.lake_packages_existing.is_empty());
        assert!(ctx.bundle_chain_crates.is_empty());
        assert!(ctx.approved_anthropic_models.is_empty());
        assert!(ctx.kernels_with_baseline.is_empty());
        assert!(ctx.existing_workspace_crates.is_empty());
        assert!(ctx.existing_templates.is_empty());
        assert!(ctx.existing_top_level_dirs.is_empty());
        assert!(ctx.existing_lean_modules.is_empty());
    }

    #[test]
    fn wrong_criteria_version_helper_uses_arg() {
        let ctx = ProjectContext::test_with_wrong_criteria_version("0.99");
        assert_eq!(ctx.criteria_version, "0.99");
    }

    #[test]
    fn claim_summary_test_default_uses_arg_id() {
        let c = ClaimSummary::test_default("EXAMPLE-001");
        assert_eq!(c.id, "EXAMPLE-001");
        assert_eq!(c.status, ClaimStatus::Drafted);
        assert!(c.scope_model_only);
        assert!(c.lean_theorems.is_empty());
        assert!(c.review_human_operator.is_none());
    }

    #[test]
    fn project_context_round_trips_via_json() {
        let mut ctx = ProjectContext::test_default();
        ctx.mathlib_imports_existing
            .insert("Mathlib.Tactic.Linarith".into());
        ctx.bundle_chain_crates.insert("sha2".into());
        let j = serde_json::to_string(&ctx).expect("ser");
        let back: ProjectContext = serde_json::from_str(&j).expect("de");
        assert_eq!(back, ctx);
    }
}
