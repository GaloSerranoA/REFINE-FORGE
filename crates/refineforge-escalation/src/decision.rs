//! The structured output of [`crate::Engine::decide`].

use crate::action::{ExternalCitation, LossKind, WeakeningKind};
use crate::category::Category;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome")]
pub enum Decision {
    /// Action passes all 9 categories; the driver may execute it.
    Proceed,
    /// Action trips at least one category; the driver must
    /// generate a decision packet and wait for the operator.
    Escalate(EscalationReason),
}

impl Decision {
    pub fn is_proceed(&self) -> bool {
        matches!(self, Self::Proceed)
    }

    pub fn is_escalate(&self) -> bool {
        matches!(self, Self::Escalate(_))
    }

    pub fn categories(&self) -> Vec<Category> {
        match self {
            Self::Proceed => Vec::new(),
            Self::Escalate(r) => r.categories.clone(),
        }
    }

    pub fn primary_category(&self) -> Option<Category> {
        match self {
            Self::Proceed => None,
            Self::Escalate(r) => Some(r.primary),
        }
    }
}

/// Why the engine escalated. `categories` lists every category
/// tripped (multi-category overlap per criteria-doc §"Multiple
/// categories simultaneously"); `primary` is the most-restrictive
/// one (whose required decision-packet fields the operator must
/// receive). `summary` is a one-sentence human-readable reason;
/// `evidence` is the structured per-category data the packet
/// renderer will format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscalationReason {
    pub categories: Vec<Category>,
    pub primary: Category,
    pub summary: String,
    pub evidence: Evidence,
}

/// Per-category structured evidence. Populated by the engine;
/// consumed by the (Phase 2) packet renderer.
///
/// Each variant carries the minimum fields the corresponding
/// "Decision packet contents" list in criteria-doc §3 requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "category")]
pub enum Evidence {
    Scope {
        what_added: String,
        smallest_in_scope_alternative: Option<String>,
    },
    Idealisation {
        rust_type: String,
        lean_type: String,
        lost_properties: Vec<LossKind>,
    },
    CustomAxiom {
        module: String,
        axiom_name: String,
        statement: String,
    },
    CustomerIntent {
        claim_id: String,
        sentence: String,
    },
    StatusUpgrade {
        claim_id: String,
        from: String,
        to: String,
    },
    TheoremWeakening {
        module: String,
        theorem: String,
        statement_before: String,
        statement_after: String,
        weakening: WeakeningKind,
    },
    ExternalFact {
        assertion: String,
        citation: ExternalCitation,
    },
    TrustBaseExtension {
        what: String,
        from: Option<String>,
        to: String,
    },
    BitExactRegression {
        kernel_id: String,
        change_summary: String,
    },
    /// Catch-all for the [`crate::Action::Unknown`] action: per
    /// plan §6, treat as Scope by default and surface the
    /// description.
    UnknownActionShape {
        description: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proceed_is_proceed() {
        assert!(Decision::Proceed.is_proceed());
        assert!(!Decision::Proceed.is_escalate());
        assert!(Decision::Proceed.categories().is_empty());
        assert!(Decision::Proceed.primary_category().is_none());
    }

    #[test]
    fn escalate_carries_primary_and_categories() {
        let r = EscalationReason {
            categories: vec![Category::Scope, Category::TrustBaseExtension],
            primary: Category::TrustBaseExtension,
            summary: "first Mathlib import in this project".into(),
            evidence: Evidence::Scope {
                what_added: "import Mathlib.Tactic.Linarith".into(),
                smallest_in_scope_alternative: None,
            },
        };
        let d = Decision::Escalate(r);
        assert!(d.is_escalate());
        assert!(!d.is_proceed());
        assert_eq!(
            d.categories(),
            vec![Category::Scope, Category::TrustBaseExtension]
        );
        assert_eq!(d.primary_category(), Some(Category::TrustBaseExtension));
    }

    #[test]
    fn decision_round_trips_via_json() {
        let r = EscalationReason {
            categories: vec![Category::Idealisation],
            primary: Category::Idealisation,
            summary: "u64 → Nat loses overflow".into(),
            evidence: Evidence::Idealisation {
                rust_type: "u64".into(),
                lean_type: "Nat".into(),
                lost_properties: vec![LossKind::UnsignedOverflow],
            },
        };
        let d = Decision::Escalate(r.clone());
        let j = serde_json::to_string(&d).expect("ser");
        let back: Decision = serde_json::from_str(&j).expect("de");
        assert_eq!(back, d);
    }
}
