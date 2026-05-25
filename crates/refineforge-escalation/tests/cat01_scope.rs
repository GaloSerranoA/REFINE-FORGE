//! Category 1 — Scope change. Mirrors the positive + negative
//! examples in `docs/escalation-criteria.md` §Category 1.

use refineforge_escalation::{Action, Category, ClaimSummary, Engine, ProjectContext};

fn eng() -> Engine {
    Engine::new()
}

fn ctx_with_claim_named_theorems(theorems: &[&str]) -> ProjectContext {
    let mut ctx = ProjectContext::test_default();
    let mut claim = ClaimSummary::test_default("EXAMPLE-001");
    for t in theorems {
        claim.lean_theorems.insert((*t).into());
    }
    ctx.claim = Some(claim);
    ctx
}

// ---------- Positive: AddLeanModule ----------

#[test]
fn add_new_lean_module_escalates_as_scope() {
    let act = Action::AddLeanModule {
        path: "lean/Refineforge/NewModule.lean".into(),
        imports: vec![],
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::Scope));
}

// ---------- Positive: AddTheorem outside listed scope ----------

#[test]
fn add_theorem_not_in_claim_scope_escalates() {
    let ctx = ctx_with_claim_named_theorems(&["t1", "t2"]);
    let act = Action::AddTheorem {
        module: "Refineforge.Counter".into(),
        name: "t3".into(),
        statement: "∀ c, P c".into(),
    };
    let d = eng().decide(&act, &ctx).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::Scope));
}

// ---------- Negative: AddTheorem already in scope ----------

#[test]
fn add_theorem_already_in_claim_scope_proceeds() {
    let ctx = ctx_with_claim_named_theorems(&["t1", "t2"]);
    let act = Action::AddTheorem {
        module: "Refineforge.Counter".into(),
        name: "t2".into(),
        statement: "∀ c, P c".into(),
    };
    let d = eng().decide(&act, &ctx).unwrap();
    assert!(d.is_proceed(), "expected proceed, got {:?}", d);
}

// ---------- v0.3: Mathlib imports never trip Cat 1 ----------
//
// Under v0.3, the trust footprint is established when a Lake
// package enters lake-manifest.json (Cat 8 trigger via
// AddLakePackage). Per-module `import Mathlib.X` statements
// always proceed because they draw on already-trusted surface.
// See `cat08_trust_base.rs` for the Cat 8 first-use tests.

#[test]
fn first_time_mathlib_import_does_not_trip_scope_in_v0_3() {
    let ctx = ProjectContext::test_default(); // no Mathlib imports yet
    let act = Action::AddLeanImport {
        module: "Refineforge.Counter".into(),
        import_path: "Mathlib.Tactic.Linarith".into(),
        is_mathlib: true,
    };
    let d = eng().decide(&act, &ctx).unwrap();
    assert!(d.is_proceed(), "v0.3: Mathlib imports proceed; got {:?}", d);
}

#[test]
fn already_used_mathlib_import_proceeds() {
    let mut ctx = ProjectContext::test_default();
    ctx.mathlib_imports_existing
        .insert("Mathlib.Tactic.Linarith".into());
    let act = Action::AddLeanImport {
        module: "Refineforge.Counter".into(),
        import_path: "Mathlib.Tactic.Linarith".into(),
        is_mathlib: true,
    };
    let d = eng().decide(&act, &ctx).unwrap();
    assert!(d.is_proceed(), "expected proceed, got {:?}", d);
}

// ---------- Negative: non-Mathlib import ----------

#[test]
fn non_mathlib_import_proceeds() {
    let ctx = ProjectContext::test_default();
    let act = Action::AddLeanImport {
        module: "Refineforge.Counter".into(),
        import_path: "Refineforge.Util".into(),
        is_mathlib: false,
    };
    let d = eng().decide(&act, &ctx).unwrap();
    assert!(d.is_proceed());
}

// ---------- Positive: AddWorkspaceCrate ----------

#[test]
fn add_workspace_crate_escalates() {
    let act = Action::AddWorkspaceCrate {
        name: "refineforge-autonomous".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::Scope));
}

// ---------- Positive: AddTemplate ----------

#[test]
fn add_template_escalates() {
    let act = Action::AddTemplate {
        name: "merkle_chain".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: AddTopLevelDirectory ----------

#[test]
fn add_top_level_directory_escalates() {
    let act = Action::AddTopLevelDirectory {
        name: "fuzz".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Negative: AddCliHelpText ----------

#[test]
fn add_cli_help_text_proceeds() {
    let act = Action::AddCliHelpText {
        command: "refine".into(),
        flag: "--max-iterations".into(),
        description: "max LSP-driven repair iterations (default 3)".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: rename local var ----------

#[test]
fn rename_local_var_proceeds() {
    let act = Action::RenameLocalVar {
        file: "lean/Refineforge/Counter.lean".into(),
        from: "h".into(),
        to: "hCounter".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: reformat ----------

#[test]
fn reformat_proceeds() {
    let act = Action::Reformat {
        paths: vec!["lean/Refineforge/Counter.lean".into()],
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: add test case ----------

#[test]
fn add_test_case_for_existing_theorem_proceeds() {
    let act = Action::AddTestCase {
        module: "Refineforge.Counter".into(),
        theorem: "incr_increases".into(),
        test_name: "incr_increases_at_zero".into(),
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

// ---------- Negative: restructure proof body ----------

#[test]
fn restructure_proof_proceeds() {
    let act = Action::RestructureProof {
        module: "Refineforge.Counter".into(),
        theorem: "incr_increases".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}

// ---------- Catch-all: Unknown action defaults to Scope ----------

#[test]
fn unknown_action_defaults_to_scope() {
    let act = Action::Unknown {
        description: "AI proposed a totally novel kind of step".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::Scope));
}

// ---------- AddTheorem with no claim loaded defaults to escalate ----------

#[test]
fn add_theorem_without_claim_escalates_defensively() {
    let act = Action::AddTheorem {
        module: "Refineforge.Counter".into(),
        name: "any_name".into(),
        statement: "True".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::Scope));
}
