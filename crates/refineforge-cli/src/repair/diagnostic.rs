//! LSP diagnostic → repair-friendly form.

use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_error_classification() {
        let d = Diagnostic {
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 1 },
            },
            severity: Severity::Error,
            message: "unsolved goals".into(),
            source: Some("lean".into()),
        };
        assert!(d.severity_is_error());
    }

    #[test]
    fn severity_warning_is_not_error() {
        let d = Diagnostic {
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 1 },
            },
            severity: Severity::Warning,
            message: "unused variable".into(),
            source: None,
        };
        assert!(!d.severity_is_error());
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
}
