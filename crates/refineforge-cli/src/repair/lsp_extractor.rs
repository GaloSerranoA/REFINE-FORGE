//! LSP-driven `ProofGraphExtractor` with heuristic fallback.
//!
//! Calls Lean's `$/lean/plainGoal` LSP extension to obtain the
//! elaborator-printed goal state at the diagnostic position, then
//! parses it with the shared
//! [`refineforge_repair_api::proof_graph::parse_goal_text`] function.
//! On LSP failure (timeout, missing goal, error response) or when
//! Lean returns nothing useful, falls back to
//! [`LeanTextHeuristicExtractor`] so the caller still gets a usable
//! [`ProofState`] for prompt rendering.
//!
//! Why not implement `ProofGraphExtractor`: the trait takes `&self`,
//! but the LSP client requires `&mut` to send requests. Rather than
//! force an `Arc<Mutex<LeanLspClient>>` on every caller, we expose
//! this as a free function. Strategies that want LSP-driven
//! extraction call [`extract_with_lsp`] directly.
//!
//! Honest scope:
//!   - The LSP request + parse + fallback logic is real, compiled,
//!     and unit-tested against canned JSON responses.
//!   - End-to-end exercise against `lake env lean --server` is NOT
//!     in CI; same boundary as the existing `lsp.rs` integration
//!     surface. Documented in `docs/llm-repair-design.md` §9.
//!   - `lemma_neighborhood` stays empty — needs a Mathlib index, out
//!     of scope here.

use std::time::Duration;

use anyhow::Result;
use refineforge_repair_api::proof_graph::{
    self, LeanTextHeuristicExtractor, LemmaNeighborhood, ProofGraphExtractor, ProofState,
};
use refineforge_repair_api::Diagnostic;

use super::lsp::LeanLspClient;

/// Extract a [`ProofState`] using Lean's `$/lean/plainGoal` LSP
/// extension. Falls back to the text-heuristic extractor on any LSP
/// failure or empty response — the returned `ProofState` is always
/// at least as rich as the heuristic-only path.
pub fn extract_with_lsp(
    client: &mut LeanLspClient,
    uri: &str,
    file_content: &str,
    diagnostic: &Diagnostic,
    plain_goal_timeout: Duration,
) -> Result<ProofState> {
    // 1. Heuristic baseline. We always run it so tactic_history (which
    //    LSP doesn't provide) is populated, and so we have a clean
    //    fallback if the LSP call fails.
    let mut state = LeanTextHeuristicExtractor::default().extract(file_content, diagnostic)?;

    // 2. Ask the elaborator for its pretty-printed goal state. If it
    //    returns something, replace the heuristic's goal + hypotheses
    //    with the richer LSP version — Lean elaborator state includes
    //    hypotheses introduced by `intro`/`rintro` that the heuristic
    //    can't see from text alone.
    let line = diagnostic.range.start.line;
    let character = diagnostic.range.start.character;
    let lsp_result = client.plain_goal(uri, line, character, plain_goal_timeout);
    if let Ok(Some(rendered)) = lsp_result {
        let (goal, hypotheses) = proof_graph::parse_goal_text(&rendered);
        if goal.is_some() {
            state.current_goal = goal;
        }
        if !hypotheses.is_empty() {
            state.hypotheses = hypotheses;
        }
    }
    // On Err / Ok(None), state already holds the heuristic result.

    // 3. lemma_neighborhood stays empty (no Mathlib index here).
    state.lemma_neighborhood = LemmaNeighborhood::default();
    Ok(state)
}

#[cfg(test)]
mod tests {
    use refineforge_repair_api::proof_graph::parse_goal_text;

    /// Lean's `$/lean/plainGoal` rendered field is the same shape as
    /// the goal block in a diagnostic message: `name : type` lines
    /// followed by `⊢ goal`. Verify the shared parser handles it.
    #[test]
    fn parse_goal_text_handles_plain_goal_rendered_field() {
        // Shape Lean 4 LSP returns in the `rendered` field of
        // `$/lean/plainGoal` (verified against Lean 4.x output format).
        let rendered = "case foo\nn : ℕ\nh : n > 0\n⊢ n ≥ 1";
        let (goal, hyps) = parse_goal_text(rendered);
        assert_eq!(goal.expect("should parse goal").0, "n ≥ 1");
        assert_eq!(hyps.len(), 2);
        assert_eq!(hyps[0].name, "n");
        assert_eq!(hyps[0].ty, "ℕ");
        assert_eq!(hyps[1].name, "h");
        assert_eq!(hyps[1].ty, "n > 0");
    }

    #[test]
    fn parse_goal_text_returns_none_when_no_goal_marker() {
        let rendered = "no proof state available";
        let (goal, hyps) = parse_goal_text(rendered);
        assert!(goal.is_none());
        assert!(hyps.is_empty());
    }
}
