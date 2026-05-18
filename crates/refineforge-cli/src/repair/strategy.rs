//! Repair strategies: how a diagnostic becomes a proposed patch.
//!
//! The shipped `MockStrategy` is deliberately useless — it always
//! returns `None`. It exists so the rest of the repair loop is
//! testable in CI without an API key.
//!
//! Real strategies (e.g. `AnthropicStrategy` using the official
//! Anthropic SDK with `claude-opus-4-7`) should live in their own
//! crate or be added behind a feature flag here. See
//! [`docs/llm-repair-design.md`] §4 for the recipe.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::diagnostic::{Diagnostic, Range};

/// A proposed change to a Lean source file.
///
/// Patches are LSP-shaped (range + new_text) rather than line-based
/// so they round-trip through `textDocument/didChange` cleanly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub range: Range,
    pub new_text: String,
    /// Human-readable explanation. Surfaced in `RepairReport`.
    pub rationale: String,
}

impl Patch {
    /// Apply this patch to the given source text. Pure function;
    /// does not touch disk.
    pub fn apply(&self, source: &str) -> String {
        let offsets = line_offsets(source);
        let start = char_offset(&offsets, source, self.range.start.line, self.range.start.character);
        let end = char_offset(&offsets, source, self.range.end.line, self.range.end.character);
        let start = start.min(source.len());
        let end = end.min(source.len()).max(start);
        let mut out = String::with_capacity(source.len() - (end - start) + self.new_text.len());
        out.push_str(&source[..start]);
        out.push_str(&self.new_text);
        out.push_str(&source[end..]);
        out
    }

    pub fn range_summary(&self) -> String {
        format!(
            "{}:{}-{}:{}",
            self.range.start.line, self.range.start.character,
            self.range.end.line, self.range.end.character
        )
    }

    pub fn new_text_summary(&self) -> String {
        let s = self.new_text.replace('\n', "\\n");
        if s.len() > 40 {
            format!("{}...", &s[..37])
        } else {
            s
        }
    }
}

fn line_offsets(s: &str) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, c) in s.char_indices() {
        if c == '\n' {
            v.push(i + 1);
        }
    }
    v
}

/// Compute byte offset for an LSP position. LSP characters are
/// UTF-16 code units; this implementation assumes ASCII source
/// (which Lean files generally are). For non-ASCII source, swap
/// in a UTF-16-aware converter.
fn char_offset(line_offsets: &[usize], source: &str, line: u32, character: u32) -> usize {
    let line = line as usize;
    if line >= line_offsets.len() {
        return source.len();
    }
    line_offsets[line] + character as usize
}

pub trait RepairStrategy {
    /// Propose a patch for the given diagnostic. Return `None` to
    /// decline (caller treats as `RepairOutcome::NoProposal`).
    fn propose_patch(
        &self,
        diagnostic: &Diagnostic,
        file_content: &str,
    ) -> Result<Option<Patch>>;

    fn name(&self) -> &'static str;
}

/// Default impl: declines every diagnostic. The repair-loop driver
/// exists; the LLM does not. Swap in your strategy of choice.
pub struct MockStrategy;

impl RepairStrategy for MockStrategy {
    fn propose_patch(
        &self,
        _diagnostic: &Diagnostic,
        _file_content: &str,
    ) -> Result<Option<Patch>> {
        Ok(None)
    }
    fn name(&self) -> &'static str { "mock" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repair::diagnostic::{Position, Severity};

    fn d() -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 1 },
            },
            severity: Severity::Error,
            message: "test".into(),
            source: None,
        }
    }

    #[test]
    fn mock_strategy_declines_all_proposals() {
        let s = MockStrategy;
        assert!(s.propose_patch(&d(), "anything").unwrap().is_none());
        assert_eq!(s.name(), "mock");
    }

    #[test]
    fn patch_apply_replaces_single_line_range() {
        let source = "theorem t : False := by sorry\n";
        let patch = Patch {
            range: Range {
                start: Position { line: 0, character: 24 },
                end: Position { line: 0, character: 29 },
            },
            new_text: "trivial".into(),
            rationale: "replace sorry with trivial (will fail, but tests apply mechanics)".into(),
        };
        let out = patch.apply(source);
        assert_eq!(out, "theorem t : False := by trivial\n");
    }

    #[test]
    fn patch_apply_replaces_across_lines() {
        let source = "line 0\nline 1\nline 2\n";
        let patch = Patch {
            range: Range {
                start: Position { line: 0, character: 5 },
                end: Position { line: 2, character: 0 },
            },
            new_text: "XYZ\n".into(),
            rationale: "test".into(),
        };
        let out = patch.apply(source);
        assert_eq!(out, "line XYZ\nline 2\n");
    }

    #[test]
    fn patch_apply_inserts_at_position() {
        let source = "abc\n";
        let patch = Patch {
            range: Range {
                start: Position { line: 0, character: 1 },
                end: Position { line: 0, character: 1 },
            },
            new_text: "X".into(),
            rationale: "insert".into(),
        };
        let out = patch.apply(source);
        assert_eq!(out, "aXbc\n");
    }

    #[test]
    fn patch_apply_clamps_out_of_bounds() {
        let source = "abc";
        let patch = Patch {
            range: Range {
                start: Position { line: 99, character: 0 },
                end: Position { line: 99, character: 5 },
            },
            new_text: "ignored".into(),
            rationale: "test".into(),
        };
        // Out-of-bounds collapses to (len, len) → append.
        let out = patch.apply(source);
        assert_eq!(out, "abcignored");
    }

    #[test]
    fn new_text_summary_truncates_long_strings() {
        let p = Patch {
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 0 },
            },
            new_text: "x".repeat(100),
            rationale: "".into(),
        };
        let s = p.new_text_summary();
        assert!(s.ends_with("..."));
        assert_eq!(s.len(), 40);
    }
}
