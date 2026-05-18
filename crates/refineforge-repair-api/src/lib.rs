//! The contract between `refineforge-cli` (which drives the repair
//! loop) and `refineforge-strategies` (which implements strategies
//! against this trait). Exists as its own crate so neither side
//! depends on the other — avoids the circular dep that would happen
//! if the trait lived in `refineforge-cli`.
//!
//! Owned by **Section 1: Lean 4 Specialist** ([../../ARCHITECTURE.md](../../ARCHITECTURE.md)).
//! This trait surface is one of the two stable cross-section
//! interfaces (the other is the bundle manifest schema).

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ─── Diagnostic types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Severity,
    pub message: String,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Diagnostic {
    pub fn severity_is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }
}

// ─── LSP conversions (kept here so strategies can also speak LSP if
//     they need to) ───────────────────────────────────────────────────────

impl From<lsp_types::Diagnostic> for Diagnostic {
    fn from(d: lsp_types::Diagnostic) -> Self {
        Self {
            range: d.range.into(),
            severity: d.severity.map(Severity::from).unwrap_or(Severity::Unknown),
            message: d.message,
            source: d.source,
        }
    }
}

impl From<lsp_types::DiagnosticSeverity> for Severity {
    fn from(s: lsp_types::DiagnosticSeverity) -> Self {
        match s {
            lsp_types::DiagnosticSeverity::ERROR => Severity::Error,
            lsp_types::DiagnosticSeverity::WARNING => Severity::Warning,
            lsp_types::DiagnosticSeverity::INFORMATION => Severity::Information,
            lsp_types::DiagnosticSeverity::HINT => Severity::Hint,
            _ => Severity::Unknown,
        }
    }
}

impl From<lsp_types::Range> for Range {
    fn from(r: lsp_types::Range) -> Self {
        Self {
            start: r.start.into(),
            end: r.end.into(),
        }
    }
}

impl From<lsp_types::Position> for Position {
    fn from(p: lsp_types::Position) -> Self {
        Self {
            line: p.line,
            character: p.character,
        }
    }
}

// ─── Patch ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patch {
    pub range: Range,
    pub new_text: String,
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
/// (which Lean files generally are).
fn char_offset(line_offsets: &[usize], source: &str, line: u32, character: u32) -> usize {
    let line = line as usize;
    if line >= line_offsets.len() {
        return source.len();
    }
    line_offsets[line] + character as usize
}

// ─── Trait ──────────────────────────────────────────────────────────────

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

/// Default impl: declines every diagnostic. Lives in the api crate
/// so any consumer can use it without depending on the cli.
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
    fn severity_error_classification() {
        assert!(d().severity_is_error());
    }

    #[test]
    fn lsp_severity_conversion() {
        assert_eq!(Severity::from(lsp_types::DiagnosticSeverity::ERROR), Severity::Error);
        assert_eq!(Severity::from(lsp_types::DiagnosticSeverity::WARNING), Severity::Warning);
        assert_eq!(Severity::from(lsp_types::DiagnosticSeverity::INFORMATION), Severity::Information);
        assert_eq!(Severity::from(lsp_types::DiagnosticSeverity::HINT), Severity::Hint);
    }

    #[test]
    fn mock_strategy_declines_all() {
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
            rationale: "test".into(),
        };
        assert_eq!(patch.apply(source), "theorem t : False := by trivial\n");
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
        assert_eq!(patch.apply(source), "line XYZ\nline 2\n");
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
        assert_eq!(patch.apply(source), "aXbc\n");
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
        assert_eq!(patch.apply(source), "abcignored");
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
