//! The no-sorry gate.
//!
//! Lean accepts `sorry` and `admit` as valid syntax during development.
//! For verification artifacts those tokens MUST NOT appear in source.
//! We run this gate BEFORE `lake build` so a failing claim fails fast
//! and so we can report distinct error categories (policy vs build).
//!
//! Implementation note: we strip Lean comments before scanning so a
//! note like `-- TODO: replace sorry below` doesn't trip the gate.
//! Strings and char literals are NOT stripped because Lean source
//! that contains the literal token `"sorry"` inside a string is
//! still suspicious and a human reviewer should look at it. A future
//! version may add a more precise lexer; for MVP, comment stripping
//! is the right precision/recall tradeoff.

use crate::claim::Policy;
use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct GateResult {
    pub ok: bool,
    pub sorry_count: usize,
    pub admit_count: usize,
    pub axiom_count: usize,
    pub notes: Vec<String>,
}

/// Strip Lean comments. Handles `--` line comments and `/- ... -/`
/// nested block comments.
fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut depth = 0usize;
    while i < bytes.len() {
        if depth == 0 && i + 1 < bytes.len() && &bytes[i..i + 2] == b"/-" {
            depth = 1;
            i += 2;
            continue;
        }
        if depth > 0 {
            if i + 1 < bytes.len() && &bytes[i..i + 2] == b"-/" {
                depth -= 1;
                i += 2;
                continue;
            }
            if i + 1 < bytes.len() && &bytes[i..i + 2] == b"/-" {
                depth += 1;
                i += 2;
                continue;
            }
            // Preserve newlines inside block comments so line numbers
            // remain meaningful for any downstream tooling.
            if bytes[i] == b'\n' {
                out.push('\n');
            }
            i += 1;
            continue;
        }
        if i + 1 < bytes.len() && &bytes[i..i + 2] == b"--" {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

pub fn check(src: &str, policy: &Policy) -> GateResult {
    let clean = strip_comments(src);

    // `\b` word boundaries avoid catching `sorryNotSorry` or similar.
    let re_sorry = Regex::new(r"\bsorry\b").unwrap();
    let re_admit = Regex::new(r"\badmit\b").unwrap();
    // Match top-level `axiom` declarations only (after leading whitespace).
    let re_axiom = Regex::new(r"(?m)^\s*axiom\b").unwrap();

    let sorry_count = re_sorry.find_iter(&clean).count();
    let admit_count = re_admit.find_iter(&clean).count();
    let axiom_count = re_axiom.find_iter(&clean).count();

    let mut notes = Vec::new();
    let mut ok = true;
    if policy.no_sorry && sorry_count > 0 {
        ok = false;
        notes.push(format!(
            "policy violation: no_sorry, found {sorry_count} occurrence(s)"
        ));
    }
    if policy.no_admit && admit_count > 0 {
        ok = false;
        notes.push(format!(
            "policy violation: no_admit, found {admit_count} occurrence(s)"
        ));
    }
    if policy.no_axioms_beyond_lean_core && axiom_count > 0 {
        ok = false;
        notes.push(format!(
            "policy violation: no_axioms_beyond_lean_core, found {axiom_count} axiom declaration(s)"
        ));
    }

    GateResult {
        ok,
        sorry_count,
        admit_count,
        axiom_count,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> Policy {
        Policy::default()
    }

    #[test]
    fn clean_source_passes() {
        let src = "theorem t : True := trivial\n";
        let r = check(src, &p());
        assert!(r.ok);
        assert_eq!(r.sorry_count, 0);
    }

    #[test]
    fn sorry_in_proof_fails() {
        let src = "theorem t : False := by sorry\n";
        let r = check(src, &p());
        assert!(!r.ok);
        assert_eq!(r.sorry_count, 1);
    }

    #[test]
    fn sorry_in_line_comment_passes() {
        let src = "-- TODO: replace sorry here\ntheorem t : True := trivial\n";
        let r = check(src, &p());
        assert!(r.ok);
        assert_eq!(r.sorry_count, 0);
    }

    #[test]
    fn sorry_in_block_comment_passes() {
        let src = "/- old proof had sorry -/\ntheorem t : True := trivial\n";
        let r = check(src, &p());
        assert!(r.ok);
        assert_eq!(r.sorry_count, 0);
    }

    #[test]
    fn nested_block_comment_passes() {
        let src = "/- outer /- inner sorry -/ still in -/ theorem t : True := trivial\n";
        let r = check(src, &p());
        assert!(r.ok);
    }

    #[test]
    fn word_boundary_excludes_substrings() {
        let src = "def sorryNotSorry := 1\n";
        let r = check(src, &p());
        assert_eq!(r.sorry_count, 0);
    }

    #[test]
    fn axiom_declaration_fails() {
        let src = "axiom foo : True\n";
        let r = check(src, &p());
        assert!(!r.ok);
        assert_eq!(r.axiom_count, 1);
    }
}
