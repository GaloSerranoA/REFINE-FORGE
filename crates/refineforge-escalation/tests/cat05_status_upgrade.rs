//! Category 5 — Status upgrades. Claim YAML `status:` changes
//! and `review.human_operator` flips.

use refineforge_escalation::{Action, Category, ClaimStatus, Engine, ProjectContext};

fn eng() -> Engine {
    Engine::new()
}

fn bump(from: ClaimStatus, to: ClaimStatus) -> Action {
    Action::BumpClaimStatus {
        claim_id: "EXAMPLE-002".into(),
        from,
        to,
    }
}

// ---------- Positive: drafted → proven(model-only) ----------

#[test]
fn drafted_to_proven_model_only_escalates() {
    let d = eng()
        .decide(
            &bump(ClaimStatus::Drafted, ClaimStatus::ProvenModelOnly),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::StatusUpgrade));
}

// ---------- Positive: proven(model-only) → proven(model+refined) ----------

#[test]
fn proven_model_only_to_refined_escalates() {
    let d = eng()
        .decide(
            &bump(
                ClaimStatus::ProvenModelOnly,
                ClaimStatus::ProvenModelAndRefined,
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::StatusUpgrade));
}

// ---------- Positive: broken → drafted (rescuing) ----------

#[test]
fn broken_to_drafted_escalates() {
    let d = eng()
        .decide(
            &bump(ClaimStatus::Broken, ClaimStatus::Drafted),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: setting human_operator from null ----------

#[test]
fn set_human_operator_from_null_escalates() {
    let act = Action::SetReviewOperator {
        claim_id: "EXAMPLE-002".into(),
        from: None,
        to: "galo@serragi.com".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::StatusUpgrade));
}

// ---------- Positive: handover (replacing operator) ----------

#[test]
fn set_human_operator_handover_escalates() {
    let act = Action::SetReviewOperator {
        claim_id: "EXAMPLE-002".into(),
        from: Some("former@example.com".into()),
        to: "galo@serragi.com".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Negative: unformalized → drafted (intent only) ----------

#[test]
fn unformalized_to_drafted_proceeds() {
    let d = eng()
        .decide(
            &bump(ClaimStatus::Unformalized, ClaimStatus::Drafted),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_proceed());
}
