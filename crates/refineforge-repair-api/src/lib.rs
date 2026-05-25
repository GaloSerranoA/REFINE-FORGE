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
        let start = char_offset(
            &offsets,
            source,
            self.range.start.line,
            self.range.start.character,
        );
        let end = char_offset(
            &offsets,
            source,
            self.range.end.line,
            self.range.end.character,
        );
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
            self.range.start.line,
            self.range.start.character,
            self.range.end.line,
            self.range.end.character
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

/// Compute byte offset for an LSP position.
///
/// LSP `character` is nominally UTF-16 code units; this
/// implementation treats it as a byte offset, which is identical to
/// UTF-16 for ASCII content. Lean source files within a *single
/// line* are usually ASCII even when the file as a whole contains
/// multi-byte characters (`≥`, `λ`, etc.) — patches that target
/// such lines would need a UTF-16-aware converter; documented as
/// future work.
///
/// Clamps `character` to the line's content length (excluding `\r\n`
/// or `\n` terminator). Without this clamp, an over-the-line-end
/// `character` value silently consumes the line terminator, which
/// concatenates adjacent lines and produces "stvalueture"-style
/// corruption when a multi-line patch follows. Observed in the
/// `refineforge-eval` baseline run on counter-swap-lemma (2026-05-18).
fn char_offset(line_offsets: &[usize], source: &str, line: u32, character: u32) -> usize {
    let line = line as usize;
    if line >= line_offsets.len() {
        return source.len();
    }
    let line_start = line_offsets[line];
    let next_line_start = line_offsets.get(line + 1).copied().unwrap_or(source.len());
    // Strip the line terminator (\n, or \r\n) when computing the
    // line's "end of content" position.
    let bytes = source.as_bytes();
    let mut content_end = next_line_start;
    if content_end > line_start && bytes.get(content_end - 1) == Some(&b'\n') {
        content_end -= 1;
        if content_end > line_start && bytes.get(content_end - 1) == Some(&b'\r') {
            content_end -= 1;
        }
    }
    let max_char = content_end - line_start;
    line_start + (character as usize).min(max_char)
}

// ─── Trait ──────────────────────────────────────────────────────────────

pub trait RepairStrategy {
    /// Propose a patch for the given diagnostic. Return `None` to
    /// decline (caller treats as `RepairOutcome::NoProposal`).
    fn propose_patch(&self, diagnostic: &Diagnostic, file_content: &str) -> Result<Option<Patch>>;

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
    fn name(&self) -> &'static str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
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
        assert_eq!(
            Severity::from(lsp_types::DiagnosticSeverity::ERROR),
            Severity::Error
        );
        assert_eq!(
            Severity::from(lsp_types::DiagnosticSeverity::WARNING),
            Severity::Warning
        );
        assert_eq!(
            Severity::from(lsp_types::DiagnosticSeverity::INFORMATION),
            Severity::Information
        );
        assert_eq!(
            Severity::from(lsp_types::DiagnosticSeverity::HINT),
            Severity::Hint
        );
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
                start: Position {
                    line: 0,
                    character: 24,
                },
                end: Position {
                    line: 0,
                    character: 29,
                },
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
                start: Position {
                    line: 0,
                    character: 5,
                },
                end: Position {
                    line: 2,
                    character: 0,
                },
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
                start: Position {
                    line: 0,
                    character: 1,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
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
                start: Position {
                    line: 99,
                    character: 0,
                },
                end: Position {
                    line: 99,
                    character: 5,
                },
            },
            new_text: "ignored".into(),
            rationale: "test".into(),
        };
        assert_eq!(patch.apply(source), "abcignored");
    }

    /// Regression: a character offset past the end of a line must
    /// NOT consume the line terminator and slurp the next line.
    /// Bug discovered in refineforge-eval baseline (2026-05-18) —
    /// without the fix, patches end up with "stvalueture" / duplicated
    /// content from concatenation of adjacent lines.
    #[test]
    fn patch_apply_clamps_character_to_line_length_lf() {
        let source = "  simp [Nat.mul_comm]\nnext line\n";
        // Range goes past the end-of-line character (line is 21 chars).
        let patch = Patch {
            range: Range {
                start: Position {
                    line: 0,
                    character: 2,
                },
                end: Position {
                    line: 0,
                    character: 99,
                },
            },
            new_text: "simp [incr]".into(),
            rationale: "test".into(),
        };
        // Expected: only the content of line 0 (after col 2) is
        // replaced; the \n and "next line" are untouched.
        assert_eq!(patch.apply(source), "  simp [incr]\nnext line\n");
    }

    #[test]
    fn patch_apply_clamps_character_to_line_length_crlf() {
        let source = "  simp [Nat.mul_comm]\r\nnext line\r\n";
        let patch = Patch {
            range: Range {
                start: Position {
                    line: 0,
                    character: 2,
                },
                end: Position {
                    line: 0,
                    character: 99,
                },
            },
            new_text: "simp [incr]".into(),
            rationale: "test".into(),
        };
        assert_eq!(patch.apply(source), "  simp [incr]\r\nnext line\r\n");
    }

    #[test]
    fn patch_apply_multi_line_replacement_preserves_following_line() {
        let source = "line a\nline b\nline c\nline d\n";
        // Replace lines 0..2 with new text; line c and beyond must remain.
        let patch = Patch {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 99,
                },
            },
            new_text: "NEW".into(),
            rationale: "test".into(),
        };
        // After clamp: end of line 1 content (char 6 = "line b"), so
        // we replace "line a\nline b" with "NEW".
        assert_eq!(patch.apply(source), "NEW\nline c\nline d\n");
    }

    #[test]
    fn new_text_summary_truncates_long_strings() {
        let p = Patch {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            new_text: "x".repeat(100),
            rationale: "".into(),
        };
        let s = p.new_text_summary();
        assert!(s.ends_with("..."));
        assert_eq!(s.len(), 40);
    }
}
