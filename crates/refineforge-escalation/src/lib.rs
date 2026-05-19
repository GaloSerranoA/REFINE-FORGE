//! refineforge-escalation — pure-functional engine for the
//! AI-to-human contract in `docs/escalation-criteria.md`.
//!
//! Given a [`Action`] the autonomous driver wants to take + a
//! [`ProjectContext`] describing the project, [`Engine::decide`]
//! returns [`Decision::Proceed`] or [`Decision::Escalate`] with
//! the matching category (or categories) and a structured reason.
//!
//! Pure functional. No I/O inside `decide`. No `unsafe`, no
//! `tokio`, no network. The caller is responsible for populating
//! the `ProjectContext` from claim YAMLs, Cargo.lock, and
//! lake-manifest.json — file loaders are deferred to Phase 2 of
//! `docs/autonomous-driver-plan.md` (the driver crate will need
//! them; the engine itself doesn't).
//!
//! See `docs/escalation-criteria.md` v0.2 for the categorical
//! contract this engine enforces.

pub mod action;
pub mod category;
pub mod context;
pub mod decision;
pub mod engine;

pub use action::{
    Action, ClaimStatus, ExternalCitation, LossKind, SentenceKind, WeakeningKind,
};
pub use category::Category;
pub use context::{ClaimSummary, ProjectContext};
pub use decision::{Decision, EscalationReason, Evidence};
pub use engine::{Engine, EngineError};

/// The version of `docs/escalation-criteria.md` this engine
/// implements. Bumping this MUST happen in lock-step with a
/// criteria-doc revision; any `ProjectContext` whose
/// `criteria_version` differs is refused by [`Engine::decide`]
/// per the criteria-doc's "Criteria version recording" rule.
pub const CRITERIA_VERSION: &str = "0.3";
