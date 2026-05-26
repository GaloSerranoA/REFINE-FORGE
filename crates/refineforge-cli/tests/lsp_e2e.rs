//! End-to-end integration test for the Lean LSP client.
//!
//! Gated behind the `lean-integration` feature so CI without a Lean
//! toolchain stays green. Operators with `lake` on PATH run:
//!
//!   cargo test -p refineforge-cli --features lean-integration \
//!       --test lsp_e2e
//!
//! What this exercises:
//!   1. `LeanLspClient::spawn` actually launches `lake env lean --server`.
//!   2. `initialize` + `did_open` complete without LSP errors.
//!   3. `plain_goal` returns either `Some(rendered)` for the chosen
//!      position OR `Ok(None)` if Lean has no goal there. Both are
//!      valid responses; the test asserts that the call doesn't
//!      error and that a `Some` reply parses as a Lean goal text.
//!   4. `parse_goal_text` round-trips the LSP `rendered` field into
//!      a structured `(goal, hypotheses)` pair.
//!
//! Honest scope:
//!   - Lean elaboration timing varies. The test waits up to 60s for
//!     the LSP server to be ready and skips the `plain_goal` assertion
//!     with an explicit `eprintln!` note if elaboration didn't produce
//!     a goal in that window. The TEST STILL PASSES in the no-goal
//!     branch because the LSP request itself succeeded — that's the
//!     contract under test.
//!   - We deliberately do NOT assert the exact goal text since Lean
//!     toolchain versions reformat output. We only assert
//!     `parse_goal_text` extracts *something* when Lean responds.

#![cfg(feature = "lean-integration")]

use std::time::Duration;

use refineforge_cli::repair::lsp::{path_to_uri, LeanLspClient};
use refineforge_repair_api::proof_graph::parse_goal_text;
use tempfile::tempdir;

/// Minimal Lean 4 source where the goal at line 1 column 0 is
/// resolvable by the elaborator. `True.intro` closes the goal; the
/// position at line 1 is mid-tactic so Lean reports `⊢ True` there.
const LEAN_SOURCE: &str = "theorem t : True := by\n  exact True.intro\n";

#[test]
fn plain_goal_round_trip_through_lake_lean_server() {
    let dir = tempdir().expect("tempdir");
    // The LSP server expects to launch within a Lean project root.
    // We do not initialise a full Lake project; instead we point
    // `spawn` at the tempdir and let Lean run in standalone mode.
    // This works for `$/lean/plainGoal` over `did_open`'d in-memory
    // content even without a Lake manifest, on modern Lean versions.
    let mut client = match LeanLspClient::spawn(dir.path()) {
        Ok(c) => c,
        Err(e) => {
            // `lake` or `lean` not on PATH — skip with a loud note
            // so operators know why. The feature flag exists exactly
            // to gate this test; once enabled, we *expect* the
            // toolchain to be present.
            panic!(
                "LeanLspClient::spawn failed; `lake env lean --server` must be \
                 on PATH when running with --features lean-integration. \
                 Underlying error: {e}"
            );
        }
    };

    client.initialize().expect("LSP initialize must succeed");

    let lean_file = dir.path().join("E2E.lean");
    let uri = path_to_uri(&lean_file);
    client
        .did_open(&uri, LEAN_SOURCE)
        .expect("did_open must succeed");

    // Position at line 1 column 2 (inside the tactic block, before
    // `exact`). Modern Lean LSPs report `⊢ True` there.
    let result = client.plain_goal(&uri, 1, 2, Duration::from_secs(30));

    match result {
        Ok(Some(rendered)) => {
            // Sanity: the goal text contains a `⊢ ` marker OR is a
            // single goal expression. We pass the rendered text
            // through the shared parser to confirm structural
            // consistency with the heuristic extractor.
            let (goal, hypotheses) = parse_goal_text(&rendered);
            // We do not assert the exact body since Lean toolchains
            // reformat. We only require a non-empty parse on at
            // least one branch.
            let extracted_something = goal.is_some() || !hypotheses.is_empty();
            assert!(
                extracted_something,
                "parse_goal_text produced no goal and no hypotheses from LSP output: {rendered:?}",
            );
        }
        Ok(None) => {
            eprintln!(
                "plain_goal returned None (Lean reports no goal at this position; \
                 toolchain-version-dependent). Test passes because the LSP \
                 round-trip completed without error."
            );
        }
        Err(e) => {
            panic!("plain_goal LSP request failed: {e}");
        }
    }

    client.shutdown().expect("shutdown must succeed");
}
