//! The 9 escalation categories. Mirror of
//! `docs/escalation-criteria.md` §"The 9 escalation categories".

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    /// Cat 1 — adding/removing/restructuring an entity not in the
    /// claim's stated scope; first-time-in-project structural choices.
    Scope,
    /// Cat 2 — Rust→Lean mapping that loses information.
    Idealisation,
    /// Cat 3 — declaring a new axiom in our Lean source.
    CustomAxiom,
    /// Cat 4 — claim about customer/user/regulator/operator
    /// understanding written into a refinement doc.
    RefinementDocCustomerIntent,
    /// Cat 5 — claim YAML `status:` change; `review.human_operator` flip.
    StatusUpgrade,
    /// Cat 6 — deleting or weakening an originally-stated theorem.
    TheoremWeakening,
    /// Cat 7 — assertion about the real world not verifiable from repo.
    ExternalFact,
    /// Cat 8 — change to the project's trusted code base (pins, deps, models).
    TrustBaseExtension,
    /// Cat 9 — change that could affect a previously-certified
    /// `refine-bitexact` gate (added in v0.2).
    BitExactRegression,
}

impl Category {
    /// Stable slug used in escalation-packet filenames.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Scope => "scope",
            Self::Idealisation => "idealisation",
            Self::CustomAxiom => "custom-axiom",
            Self::RefinementDocCustomerIntent => "customer-intent",
            Self::StatusUpgrade => "status-upgrade",
            Self::TheoremWeakening => "theorem-weakening",
            Self::ExternalFact => "external-fact",
            Self::TrustBaseExtension => "trust-base",
            Self::BitExactRegression => "bit-exact",
        }
    }

    /// 1-indexed category number as written in the criteria doc.
    pub fn number(self) -> u8 {
        match self {
            Self::Scope => 1,
            Self::Idealisation => 2,
            Self::CustomAxiom => 3,
            Self::RefinementDocCustomerIntent => 4,
            Self::StatusUpgrade => 5,
            Self::TheoremWeakening => 6,
            Self::ExternalFact => 7,
            Self::TrustBaseExtension => 8,
            Self::BitExactRegression => 9,
        }
    }

    /// Iterator over every category. Stable order matches
    /// criteria-doc numbering.
    pub fn all() -> impl Iterator<Item = Category> {
        [
            Self::Scope,
            Self::Idealisation,
            Self::CustomAxiom,
            Self::RefinementDocCustomerIntent,
            Self::StatusUpgrade,
            Self::TheoremWeakening,
            Self::ExternalFact,
            Self::TrustBaseExtension,
            Self::BitExactRegression,
        ]
        .into_iter()
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Category {} — {}", self.number(), self.slug())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_kebab_case() {
        for c in Category::all() {
            let s = c.slug();
            assert!(!s.is_empty(), "{:?} has empty slug", c);
            assert!(
                s.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'),
                "{:?} slug {:?} is not kebab-case",
                c,
                s
            );
        }
    }

    #[test]
    fn slugs_are_unique() {
        let mut slugs: Vec<&str> = Category::all().map(|c| c.slug()).collect();
        slugs.sort();
        let before = slugs.len();
        slugs.dedup();
        assert_eq!(before, slugs.len(), "duplicate slug among categories");
    }

    #[test]
    fn numbers_are_1_through_9() {
        let nums: Vec<u8> = Category::all().map(|c| c.number()).collect();
        assert_eq!(nums, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn display_contains_number_and_slug() {
        let s = format!("{}", Category::Idealisation);
        assert!(s.contains('2'), "{:?} missing number", s);
        assert!(s.contains("idealisation"), "{:?} missing slug", s);
    }

    #[test]
    fn category_serdes_via_serde_json() {
        // Round-trip via JSON; useful for packet front-matter.
        for c in Category::all() {
            let j = serde_json::to_string(&c).expect("serialize");
            let back: Category = serde_json::from_str(&j).expect("deserialize");
            assert_eq!(back, c);
        }
    }

    #[test]
    fn all_iter_yields_nine_categories() {
        assert_eq!(Category::all().count(), 9);
    }
}
