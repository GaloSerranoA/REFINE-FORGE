//! Edge cases: criteria-version mismatch, evidence shape,
//! decision sanity.

use refineforge_escalation::{
    Action, Category, Decision, Engine, EngineError, Evidence, LossKind, ProjectContext,
    CRITERIA_VERSION,
};

#[test]
fn engine_refuses_mismatched_criteria_version() {
    let eng = Engine::new();
    let ctx = ProjectContext::test_with_wrong_criteria_version("0.99");
    let act = Action::Reformat {
        paths: vec!["x.rs".into()],
    };
    match eng.decide(&act, &ctx) {
        Err(EngineError::CriteriaVersionMismatch { expected, found }) => {
            assert_eq!(expected, CRITERIA_VERSION);
            assert_eq!(found, "0.99");
        }
        other => panic!("expected CriteriaVersionMismatch, got {:?}", other),
    }
}

#[test]
fn engine_accepts_matched_criteria_version() {
    let eng = Engine::new();
    let ctx = ProjectContext::test_default();
    let act = Action::Reformat {
        paths: vec!["x.rs".into()],
    };
    let d = eng.decide(&act, &ctx).expect("matched version");
    assert!(d.is_proceed());
}

#[test]
fn idealisation_evidence_carries_lost_properties() {
    let eng = Engine::new();
    let ctx = ProjectContext::test_default();
    let act = Action::MapRustToLean {
        rust_type: "u64".into(),
        lean_type: "Nat".into(),
        lossy_kinds: vec![LossKind::UnsignedOverflow],
    };
    let d = eng.decide(&act, &ctx).unwrap();
    if let Decision::Escalate(r) = d {
        if let Evidence::Idealisation { lost_properties, .. } = r.evidence {
            assert_eq!(lost_properties, vec![LossKind::UnsignedOverflow]);
        } else {
            panic!("expected Idealisation evidence, got {:?}", r.evidence);
        }
    } else {
        panic!("expected Escalate");
    }
}

#[test]
fn unknown_action_evidence_is_unknown_shape() {
    let eng = Engine::new();
    let ctx = ProjectContext::test_default();
    let act = Action::Unknown {
        description: "AI invented a step".into(),
    };
    let d = eng.decide(&act, &ctx).unwrap();
    if let Decision::Escalate(r) = d {
        assert_eq!(r.primary, Category::Scope);
        if let Evidence::UnknownActionShape { description } = r.evidence {
            assert_eq!(description, "AI invented a step");
        } else {
            panic!("expected UnknownActionShape evidence, got {:?}", r.evidence);
        }
    } else {
        panic!("expected Escalate");
    }
}

#[test]
fn criteria_version_constant_is_exact_v0_2() {
    // Public constant pinned so a silent bump can't drift away
    // from the criteria-doc revision.
    assert_eq!(CRITERIA_VERSION, "0.2");
}

#[test]
fn decision_proceed_has_no_categories() {
    let d = Decision::Proceed;
    assert!(d.is_proceed());
    assert!(!d.is_escalate());
    assert!(d.categories().is_empty());
    assert!(d.primary_category().is_none());
}
