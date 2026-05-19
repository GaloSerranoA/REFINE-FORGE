//! Category 3 — Custom axiom. Per criteria-doc Cat 3: ANY
//! axiom declaration in our source escalates, even ones the AI
//! considers "obviously true."

use refineforge_escalation::{Action, Category, Decision, Engine, ProjectContext};

fn eng() -> Engine {
    Engine::new()
}

#[test]
fn hash_injectivity_axiom_escalates() {
    let act = Action::WriteAxiom {
        module: "Refineforge.Crypto".into(),
        axiom_name: "hash_is_injective".into(),
        statement: "∀ a b, hash a = hash b → a = b".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::CustomAxiom));
}

#[test]
fn libc_correctness_axiom_escalates() {
    let act = Action::WriteAxiom {
        module: "Refineforge.Platform".into(),
        axiom_name: "rust_libc_is_correct".into(),
        statement: "True -- handwaved".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

#[test]
fn obviously_true_axiom_still_escalates() {
    // The doctrine explicitly: "including ones the AI considers 'obviously true.'"
    let act = Action::WriteAxiom {
        module: "Refineforge.Trivial".into(),
        axiom_name: "zero_is_zero".into(),
        statement: "0 = 0".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::CustomAxiom));
}

// Negative: actions that are NOT axiom declarations should not
// trip Cat 3 even if they look axiom-adjacent.
#[test]
fn adding_a_theorem_does_not_trip_axiom() {
    let act = Action::AddTheorem {
        module: "Refineforge.X".into(),
        name: "zero_is_zero".into(),
        statement: "0 = 0".into(),
    };
    // The theorem may trip Scope (depending on claim), but never Cat 3.
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    let cats = d.categories();
    assert!(!cats.contains(&Category::CustomAxiom));
}

#[test]
fn rename_theorem_does_not_trip_axiom() {
    let act = Action::RenameTheorem {
        module: "Refineforge.X".into(),
        from: "old_name".into(),
        to: "new_name".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}
