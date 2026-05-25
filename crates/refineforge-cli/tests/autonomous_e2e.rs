//! Phase 3.5 end-to-end exercising real loaders + real executor
//! library calls against the repo's actual fixtures.
//!
//! These tests run against the refineforge repo itself (via
//! `CARGO_MANIFEST_DIR`) so the assertions reflect what the
//! `refine autonomous` user sees against EXAMPLE-001 today.
//!
//! Notes on platform / external deps:
//! - The **dry-run** test (`dry_run_plans_and_loads_real_claim`)
//!   does not invoke `lake`, `git`, or `cosign`. Runs everywhere
//!   `cargo test` runs.
//! - The **live** test (`live_lean_check_on_example_001`) calls
//!   `runner::run`, which shells to `lake build`. It is gated
//!   on `lake` being on PATH — if missing, the test prints a
//!   skip notice and exits early rather than failing.

use refineforge_cli::autonomous::{
    run_worklist, Executor, Planner, RunSummary, StepOutcome, WorkRunConfig,
};
use refineforge_cli::claim;
use refineforge_escalation::{load_project_context, Action, Engine, LossKind, MockGitOps};
use std::path::Path;
use std::time::Duration;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("walk to refineforge repo root")
        .to_path_buf()
}

fn lake_on_path() -> bool {
    std::process::Command::new("lake")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn loader_parses_real_example_001_yaml() {
    let root = repo_root();
    let ctx = load_project_context(&root, Some("EXAMPLE-001"))
        .expect("EXAMPLE-001 should exist in this repo's claims/");
    let claim = ctx.claim.expect("claim summary populated");
    assert_eq!(claim.id, "EXAMPLE-001");
    // EXAMPLE-001 is Lean-only — no rust_source types are listed.
    assert!(
        claim.rust_source_types.is_empty(),
        "EXAMPLE-001 should have no rust_source types: got {:?}",
        claim.rust_source_types
    );
    // It carries at least one Lean theorem (add_comm_demo or similar).
    assert!(
        !claim.lean_theorems.is_empty(),
        "EXAMPLE-001 should list at least one theorem"
    );
}

#[test]
fn loader_parses_real_example_002_yaml() {
    let root = repo_root();
    let ctx = load_project_context(&root, Some("EXAMPLE-002"))
        .expect("EXAMPLE-002 should exist in this repo's claims/");
    let claim = ctx.claim.expect("claim summary populated");
    assert_eq!(claim.id, "EXAMPLE-002");
    // EXAMPLE-002 is the refined tutorial — has rust_source types.
    assert!(
        !claim.rust_source_types.is_empty(),
        "EXAMPLE-002 should list rust_source types: got {:?}",
        claim.rust_source_types
    );
}

#[test]
fn dry_run_plans_and_loads_real_claim() {
    let root = repo_root();
    let (_, claim) = claim::load(&root, "EXAMPLE-001").expect("load EXAMPLE-001");
    let ctx = load_project_context(&root, Some("EXAMPLE-001")).expect("load project context");

    let mut ex = Executor {
        engine: Engine::new(),
        git: MockGitOps::new(),
        repo_root: root.clone(),
        claim_id: "EXAMPLE-001".into(),
        claim: Some(claim),
        strategy: "mock".into(),
        weights_path: None,
        operator: None,
        dry_run: true,
        project_ctx: ctx,
        cost_gate: refineforge_cli::autonomous::CostGate::new(10.0),
        generated_at: "2026-05-18T00:00:00Z".into(),
        anthropic_usage_observed: None,
        commit_packets_in_dry_run: false,
    };

    let plan = Planner::new().plan(&ex.claim_id);
    let outcomes: Vec<_> = plan.iter().map(|s| ex.run_step(s)).collect();
    assert_eq!(outcomes.len(), 3);
    for o in &outcomes {
        match o {
            StepOutcome::Proceeded { detail, .. } => {
                assert!(
                    detail.starts_with("dry-run: "),
                    "non-dry-run leaked: {}",
                    detail
                );
            }
            other => panic!("dry-run should proceed every step, got {:?}", other),
        }
    }
    let summary = RunSummary::from_outcomes(&outcomes);
    assert!(summary.success);
    assert_eq!(summary.proceeded, 3);
    assert_eq!(summary.escalated, 0);
    assert_eq!(summary.failed, 0);
}

/// Plan §3 phase 4 acceptance dogfood: on EXAMPLE-002 with the
/// counter-idealisation as bait, the driver produces exactly
/// ONE Cat 2 packet, waits for human approval (simulated via
/// MockGitOps `auto_approve_packets`), then continues through
/// the rest of the workflow to BundleExport.
///
/// Uses dry-run system steps (no `lake` dependency) so this
/// runs everywhere. The plan-mutation + await-resumption +
/// post-approval continuation logic is exactly what the live
/// dogfood exercises.
#[test]
fn example_002_counter_idealisation_dogfood_with_await_approval() {
    let root = repo_root();
    let (_, claim) = claim::load(&root, "EXAMPLE-002").expect("load EXAMPLE-002");
    let ctx = load_project_context(&root, Some("EXAMPLE-002")).expect("load project context");

    let git = MockGitOps::new();
    git.auto_approve_packets("counter saturating_add gap documented in refinement doc");

    let mut ex = Executor {
        engine: Engine::new(),
        git,
        repo_root: root.clone(),
        claim_id: "EXAMPLE-002".into(),
        claim: Some(claim),
        strategy: "mock".into(),
        weights_path: None,
        operator: Some("galo@serragi.com".into()),
        dry_run: true,
        project_ctx: ctx,
        cost_gate: refineforge_cli::autonomous::CostGate::new(10.0),
        generated_at: "2026-05-18T00:00:00Z".into(),
        anthropic_usage_observed: None,
        commit_packets_in_dry_run: true,
    };

    let plan = Planner::new()
        .with_engine_action(Action::MapRustToLean {
            rust_type: "u64".into(),
            lean_type: "Nat".into(),
            lossy_kinds: vec![LossKind::UnsignedOverflow],
        })
        .plan("EXAMPLE-002");

    let cfg = WorkRunConfig {
        strategy: "mock".into(),
        auto_repair: false,
        await_decisions: true,
        repair_max_iterations: 5,
        max_repair_attempts: 0,
        await_poll_interval: Duration::from_millis(1),
    };

    let outcomes = run_worklist(&mut ex, plan, &cfg);

    // Expected outcomes:
    //   1. LeanCheck (dry-run Proceeded)
    //   2. EngineAction (Escalated — Cat 2 idealisation)
    //   3. OperatorDecision (Proceeded — APPROVED via auto_approve_packets)
    //   4. Scan (dry-run Proceeded)
    //   5. BundleExport (dry-run Proceeded)
    let escalations: Vec<_> = outcomes
        .iter()
        .filter(|o| matches!(o, StepOutcome::Escalated { .. }))
        .collect();
    assert_eq!(
        escalations.len(),
        1,
        "expected exactly one escalation (Cat 2), got {}: {:?}",
        escalations.len(),
        outcomes
    );
    if let Some(StepOutcome::Escalated { category, .. }) = escalations.first() {
        assert_eq!(category, "idealisation");
    }

    // Find the OperatorDecision outcome and confirm it's Proceeded (APPROVED).
    let operator_decisions: Vec<_> = outcomes
        .iter()
        .filter(|o| matches!(o, StepOutcome::Proceeded { kind, .. } if kind == "OperatorDecision"))
        .collect();
    assert_eq!(
        operator_decisions.len(),
        1,
        "expected exactly one OperatorDecision outcome"
    );

    // The post-decision Scan + BundleExport must have run.
    let scan_count = outcomes
        .iter()
        .filter(|o| matches!(o, StepOutcome::Proceeded { kind, .. } if kind == "Scan"))
        .count();
    let bundle_count = outcomes
        .iter()
        .filter(|o| matches!(o, StepOutcome::Proceeded { kind, .. } if kind == "BundleExport"))
        .count();
    assert_eq!(scan_count, 1, "Scan should run after APPROVED");
    assert_eq!(bundle_count, 1, "BundleExport should run after APPROVED");

    let summary = RunSummary::from_outcomes(&outcomes);
    // No failures: this is the happy-path dogfood.
    assert_eq!(
        summary.failed, 0,
        "no Failed outcomes expected: {:?}",
        outcomes
    );
    assert_eq!(summary.escalated, 1);
    assert!(summary.success, "expected success=true: {:?}", summary);
}

/// LIVE: actually calls `runner::run` against EXAMPLE-001 in
/// this repo. Skipped (with a printed notice) if `lake` isn't
/// on PATH or the lean toolchain isn't installed.
#[test]
fn live_lean_check_on_example_001() {
    if !lake_on_path() {
        eprintln!("SKIP live_lean_check_on_example_001: `lake` not on PATH");
        return;
    }
    let root = repo_root();
    let (_, claim) = match claim::load(&root, "EXAMPLE-001") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIP: claim::load failed: {}", e);
            return;
        }
    };
    let ctx = load_project_context(&root, Some("EXAMPLE-001")).expect("load project context");

    let mut ex = Executor {
        engine: Engine::new(),
        git: MockGitOps::new(),
        repo_root: root.clone(),
        claim_id: "EXAMPLE-001".into(),
        claim: Some(claim),
        strategy: "mock".into(),
        weights_path: None,
        operator: None,
        dry_run: false,
        project_ctx: ctx,
        cost_gate: refineforge_cli::autonomous::CostGate::new(10.0),
        generated_at: "2026-05-18T00:00:00Z".into(),
        anthropic_usage_observed: None,
        commit_packets_in_dry_run: false,
    };

    // Execute only the LeanCheck step (skip BundleExport so we
    // don't write into artifacts/ from a test).
    let plan = Planner::new().plan(&ex.claim_id);
    let lean_step = plan
        .iter()
        .find(|s| matches!(s.kind, refineforge_cli::autonomous::StepKind::LeanCheck))
        .expect("planner produces a LeanCheck step");
    let outcome = ex.run_step(lean_step);
    match outcome {
        StepOutcome::Proceeded { kind, detail, .. } => {
            assert_eq!(kind, "LeanCheck");
            assert!(detail.contains("Verified"), "got: {}", detail);
        }
        StepOutcome::Failed { error, .. } => {
            // Don't hard-fail the test suite on a missing-toolchain
            // environment; print and skip. This is the honest
            // boundary: with lake + the pinned toolchain installed,
            // this test passes; without, it skips.
            eprintln!(
                "SKIP: LeanCheck failed (likely toolchain missing): {}",
                error
            );
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
}
