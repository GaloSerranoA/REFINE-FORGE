//! Category 6 — Theorem deletion or weakening. Edit-with-weakening
//! escalates; edit-without-weakening proceeds; rename/restructure
//! proceeds.

use refineforge_escalation::{Action, Category, Engine, ProjectContext, WeakeningKind};

fn eng() -> Engine {
    Engine::new()
}

fn edit(name: &str, before: &str, after: &str, w: Option<WeakeningKind>) -> Action {
    Action::EditTheorem {
        module: "Refineforge.Counter".into(),
        name: name.into(),
        statement_before: before.into(),
        statement_after: after.into(),
        weakening: w,
    }
}

// ---------- Positive: added hypothesis (∀x → ∀x∈S) ----------

#[test]
fn added_hypothesis_escalates() {
    let d = eng()
        .decide(
            &edit(
                "t1",
                "∀ x, P x",
                "∀ x : Subset, P x",
                Some(WeakeningKind::AddedHypothesis),
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::TheoremWeakening));
}

// ---------- Positive: dropped conjunct ----------

#[test]
fn dropped_conjunct_escalates() {
    let d = eng()
        .decide(
            &edit(
                "t1",
                "∀ x, P x ∧ Q x",
                "∀ x, P x",
                Some(WeakeningKind::DroppedConjunct),
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: > → ≥ (EXAMPLE-002 trade-off) ----------

#[test]
fn strict_to_nonstrict_escalates() {
    let d = eng()
        .decide(
            &edit(
                "incr_increases",
                "(incr c).value > c.value",
                "(incr c).value ≥ c.value",
                Some(WeakeningKind::StrictToNonStrict),
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: general replacement ----------

#[test]
fn general_replacement_escalates() {
    let d = eng()
        .decide(
            &edit(
                "hash_collision_free",
                "Hash a = Hash b → a = b",
                "Hash a = Hash b → a ≈ b",
                Some(WeakeningKind::GeneralReplacement),
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Negative: same statement, different tactics ----------

#[test]
fn restructure_proof_body_proceeds() {
    let act = Action::RestructureProof {
        module: "Refineforge.Counter".into(),
        theorem: "incr_increases".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: rename theorem (refactor) ----------

#[test]
fn rename_theorem_proceeds() {
    let act = Action::RenameTheorem {
        module: "Refineforge.Counter".into(),
        from: "incr_increases".into(),
        to: "incr_strictly_increases".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: edit without weakening (same strength) ----------

#[test]
fn edit_without_weakening_proceeds() {
    // E.g. AI tightened the statement OR reformulated equivalently.
    // weakening: None signals "no information loss".
    let d = eng()
        .decide(
            &edit(
                "incr_increases",
                "∀ c, (incr c).value > c.value",
                "∀ c : Counter, (incr c).value > c.value",
                None,
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: AddTestCase doesn't trip Cat 6 ----------

#[test]
fn add_test_case_for_theorem_proceeds() {
    let act = Action::AddTestCase {
        module: "Refineforge.Counter".into(),
        theorem: "incr_increases".into(),
        test_name: "incr_increases_at_zero".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}
