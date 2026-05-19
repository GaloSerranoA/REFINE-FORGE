//! Category 4 — Refinement-doc claim about customer / user /
//! regulator / operator intent.

use refineforge_escalation::{Action, Category, Decision, Engine, ProjectContext, SentenceKind};

fn eng() -> Engine {
    Engine::new()
}

fn sentence(claim: &str, body: &str, kind: SentenceKind) -> Action {
    Action::WriteRefinementSentence {
        claim_id: claim.into(),
        sentence: body.into(),
        kind,
    }
}

// ---------- Positive: customer-intent escalates ----------

#[test]
fn customer_expects_revoke_within_1s_escalates() {
    let act = sentence(
        "EXAMPLE-002",
        "Customers expect `revoke` to take effect within 1 second.",
        SentenceKind::CustomerIntent,
    );
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(
        d.primary_category(),
        Some(Category::RefinementDocCustomerIntent)
    );
}

#[test]
fn operator_interpretation_escalates() {
    let act = sentence(
        "EXAMPLE-002",
        "Operators interpret this claim as covering both audit AND replay scenarios.",
        SentenceKind::CustomerIntent,
    );
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

#[test]
fn owasp_definition_claim_escalates() {
    let act = sentence(
        "EXAMPLE-002",
        "This matches the OWASP definition of XSS prevention.",
        SentenceKind::CustomerIntent,
    );
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

#[test]
fn gdpr_interpretation_escalates() {
    let act = sentence(
        "EXAMPLE-002",
        "Per the GDPR right-to-erasure, this counts as erasure.",
        SentenceKind::CustomerIntent,
    );
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Negative: describes the math/code only ----------

#[test]
fn describing_what_lean_theorem_proves_proceeds() {
    let act = sentence(
        "EXAMPLE-002",
        "Theorem `incr_increases` states ∀ c, (incr c).value > c.value when c.value < u64::MAX.",
        SentenceKind::MachineCheckable,
    );
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}

#[test]
fn describing_what_rust_code_does_proceeds() {
    let act = sentence(
        "EXAMPLE-002",
        "The function `Counter::incr` uses `saturating_add(1)` to clamp at `u64::MAX`.",
        SentenceKind::MachineCheckable,
    );
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}

#[test]
fn citing_approved_doc_proceeds() {
    let act = sentence(
        "EXAMPLE-002",
        "Per `docs/methodology.md` §4, the refinement argument is the trust-critical artifact.",
        SentenceKind::RepoCitable {
            source_path: "docs/methodology.md".into(),
        },
    );
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}
