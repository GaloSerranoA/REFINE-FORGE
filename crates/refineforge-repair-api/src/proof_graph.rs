//! Proof-state graph extraction + prompt renderer for repair strategies.
//!
//! Inspired by InstructGLM (Ye et al., EACL 2024, "Language is All a Graph
//! Needs"): represent the proof state as a structured graph (current goal,
//! hypotheses, tactic history, related-lemma neighborhood) and render it as
//! natural language before passing to the LLM repair strategy.
//!
//! Scope (honest):
//!   - Data shapes for [`ProofState`], [`Hypothesis`], [`TacticInvocation`],
//!     [`DiagnosticAnchor`], [`LemmaRef`], [`LemmaNeighborhood`] — REAL.
//!   - [`PromptTemplate`] + [`render`] against a [`ProofState`] — REAL.
//!   - [`ProofGraphExtractor`] trait — STUB. A real implementation must
//!     integrate with `refineforge-cli`'s LSP client to query Lean's
//!     `$/lean/plainGoal` for goal/hypothesis state, and with a Mathlib
//!     lemma index (out of scope here) for the lemma neighborhood. This
//!     crate ships only [`DiagnosticOnlyExtractor`], which fills only the
//!     diagnostic anchor.
//!
//! Owned by Section 1: Lean 4 Specialist (see `ARCHITECTURE.md`). The data
//! shapes here are part of the cross-section interface alongside
//! [`crate::RepairStrategy`].

use crate::{Diagnostic, Severity};
use anyhow::Result;
use serde::{Deserialize, Serialize};

// ─── ProofState ────────────────────────────────────────────────────────

/// Structured snapshot of the proof state at the diagnostic site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofState {
    /// Current unsolved goal at the diagnostic site, if known.
    /// `None` when extraction failed or the diagnostic is non-proof.
    #[serde(default)]
    pub current_goal: Option<GoalText>,
    /// In-scope hypotheses at the diagnostic site. Order is source
    /// order; renderers must not reorder.
    #[serde(default)]
    pub hypotheses: Vec<Hypothesis>,
    /// Recent tactic invocations leading up to the diagnostic.
    /// Bounded by the extractor; the renderer truncates further per
    /// template if needed.
    #[serde(default)]
    pub tactic_history: Vec<TacticInvocation>,
    /// The diagnostic that triggered the repair attempt.
    pub diagnostic_anchor: DiagnosticAnchor,
    /// Lemmas in the dependency neighborhood of the goal.
    #[serde(default)]
    pub lemma_neighborhood: LemmaNeighborhood,
}

/// Lean goal text (e.g., `"P → Q"`, `"∀ n : ℕ, n + 0 = n"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalText(pub String);

/// One in-scope hypothesis (`name : type`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hypothesis {
    pub name: String,
    pub ty: String,
}

/// One tactic invocation in the proof history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TacticInvocation {
    pub line: u32,
    pub tactic: String,
    #[serde(default)]
    pub goal_before: Option<GoalText>,
    #[serde(default)]
    pub goal_after: Option<GoalText>,
}

/// Diagnostic anchored to the proof state.
///
/// Mirrors [`Diagnostic`] but pinned to the rendering layer so the
/// renderer can quote line numbers and severities directly without
/// re-walking the full diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticAnchor {
    pub line: u32,
    pub message: String,
    pub severity: Severity,
}

impl From<&Diagnostic> for DiagnosticAnchor {
    fn from(d: &Diagnostic) -> Self {
        Self {
            line: d.range.start.line,
            message: d.message.clone(),
            severity: d.severity,
        }
    }
}

/// Local neighborhood of related lemmas.
///
/// Distance is graph hops in the Mathlib lemma dependency graph
/// (1 = direct premise of the goal's type, 2 = premise of a premise,
/// etc.). Renderers sort by distance ascending then by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LemmaNeighborhood {
    #[serde(default)]
    pub lemmas: Vec<LemmaRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LemmaRef {
    /// Fully-qualified Mathlib name, e.g., `"Nat.add_zero"`.
    pub fully_qualified_name: String,
    /// One-line signature, e.g., `"∀ n : ℕ, n + 0 = n"`.
    pub signature: String,
    /// Graph distance from the current goal. Renderers usually cap at
    /// distance 2 or 3.
    pub distance: u8,
}

// ─── Extractor trait ────────────────────────────────────────────────────

/// Build a [`ProofState`] from a Lean source + diagnostic.
///
/// Real implementations need LSP integration to query the elaborator
/// state and a Mathlib lemma index for the neighborhood. This trait is
/// the boundary; the api crate ships only the [`DiagnosticOnlyExtractor`]
/// fallback that fills the diagnostic anchor and leaves the rest empty.
pub trait ProofGraphExtractor {
    fn extract(&self, file_content: &str, diagnostic: &Diagnostic) -> Result<ProofState>;
}

/// Trivial extractor: fills only the diagnostic anchor; leaves goal,
/// hypotheses, tactic history, and lemma neighborhood empty.
///
/// Useful as a baseline for templates that don't require graph context
/// (e.g., `fix_proof_direct`) and as a fallback when richer extraction
/// fails.
pub struct DiagnosticOnlyExtractor;

impl ProofGraphExtractor for DiagnosticOnlyExtractor {
    fn extract(&self, _file_content: &str, diagnostic: &Diagnostic) -> Result<ProofState> {
        Ok(ProofState {
            current_goal: None,
            hypotheses: Vec::new(),
            tactic_history: Vec::new(),
            diagnostic_anchor: diagnostic.into(),
            lemma_neighborhood: LemmaNeighborhood::default(),
        })
    }
}

/// Text-heuristic extractor that needs no LSP integration.
///
/// Parses the diagnostic message for the unsolved goal and any
/// hypotheses Lean printed alongside it (e.g., a Lean 4 error of the
/// form `"unsolved goals\nh1 : P\nh2 : P → Q\n⊢ Q"`), and scans the
/// file content for the last `history_lines` non-empty lines preceding
/// the diagnostic site, keeping the ones that look like a tactic
/// invocation.
///
/// Honest limitations:
///   - Cannot see hypotheses introduced by `intro`/`rintro` if Lean
///     didn't echo them into the diagnostic message. The richer LSP
///     extractor that queries `$/lean/plainGoal` is the way to fill
///     that gap (out of scope here).
///   - Cannot fill `lemma_neighborhood` — that needs a Mathlib lemma
///     index.
///   - Tactic detection is keyword-anchored against a fixed list and
///     will miss user-defined or macro tactics. Unknown lines fall
///     through silently (not added to history).
pub struct LeanTextHeuristicExtractor {
    /// How many lines preceding the diagnostic to scan for tactic
    /// history. Default: 6.
    pub history_lines: usize,
}

impl Default for LeanTextHeuristicExtractor {
    fn default() -> Self {
        Self { history_lines: 6 }
    }
}

impl ProofGraphExtractor for LeanTextHeuristicExtractor {
    fn extract(&self, file_content: &str, diagnostic: &Diagnostic) -> Result<ProofState> {
        let (goal, hypotheses) = parse_goal_and_hypotheses(&diagnostic.message);
        let tactic_history = parse_tactic_history(
            file_content,
            diagnostic.range.start.line,
            self.history_lines,
        );
        Ok(ProofState {
            current_goal: goal,
            hypotheses,
            tactic_history,
            diagnostic_anchor: diagnostic.into(),
            lemma_neighborhood: LemmaNeighborhood::default(),
        })
    }
}

/// Tactic keywords the text heuristic accepts. Conservative — only
/// includes core/standard Mathlib tactics whose lines reliably start
/// with the keyword.
const TACTIC_KEYWORDS: &[&str] = &[
    "apply",
    "assumption",
    "by_cases",
    "by_contra",
    "cases",
    "constructor",
    "contradiction",
    "decide",
    "exact",
    "exists",
    "field_simp",
    "have",
    "induction",
    "intro",
    "intros",
    "left",
    "linarith",
    "obtain",
    "omega",
    "push_neg",
    "rcases",
    "refine",
    "refl",
    "rewrite",
    "rfl",
    "right",
    "ring",
    "rintro",
    "rw",
    "show",
    "simp",
    "sorry",
    "split_ifs",
    "suffices",
    "trivial",
    "use",
];

fn line_looks_like_tactic(trimmed: &str) -> bool {
    if trimmed.is_empty() || trimmed.starts_with("--") {
        return false;
    }
    // Strip leading `· ` / `<;> ` / `;` style combinators.
    let body = trimmed
        .trim_start_matches('·')
        .trim_start_matches(';')
        .trim_start();
    let first_token: &str = body
        .split(|c: char| c.is_whitespace() || c == ';')
        .next()
        .unwrap_or("");
    TACTIC_KEYWORDS.contains(&first_token)
}

fn parse_tactic_history(
    file_content: &str,
    diagnostic_line: u32,
    history_lines: usize,
) -> Vec<TacticInvocation> {
    let lines: Vec<&str> = file_content.lines().collect();
    let anchor = (diagnostic_line as usize).min(lines.len());
    let start = anchor.saturating_sub(history_lines);
    let mut out = Vec::new();
    for (idx, raw) in lines.iter().enumerate().take(anchor).skip(start) {
        let trimmed = raw.trim();
        if line_looks_like_tactic(trimmed) {
            out.push(TacticInvocation {
                line: idx as u32,
                tactic: trimmed.to_string(),
                goal_before: None,
                goal_after: None,
            });
        }
    }
    out
}

/// Parse a Lean-style proof-state text block (either a diagnostic
/// message body or a `$/lean/plainGoal` rendered field) into
/// `(goal, hypotheses)`. Public so the LSP-driven extractor in
/// `refineforge-cli` can reuse the same parsing rules as the
/// heuristic extractor.
pub fn parse_goal_text(message: &str) -> (Option<GoalText>, Vec<Hypothesis>) {
    parse_goal_and_hypotheses(message)
}

// ─── SingleTactic → Patch adapter ───────────────────────────────────────

/// Convert a single-tactic LLM output (e.g., `"exact h2"`,
/// `"apply Nat.add_zero"`) into a [`crate::Patch`] anchored at the
/// diagnostic's range.
///
/// Strategy:
///   - If the diagnostic range covers a non-empty span (start != end),
///     the patch REPLACES that span with the tactic. Common case: Lean
///     points at a placeholder like `sorry` or a broken tactic line.
///   - If the range is a single point (start == end), the patch INSERTS
///     the tactic at that position, prefixed with a newline and the
///     surrounding indentation if any.
///
/// Honest limitations:
///   - Indentation detection is heuristic: takes the leading
///     whitespace of the diagnostic line in `file_content`. Lean
///     proofs that mix tactic styles (`by exact h` vs structured
///     blocks) may need post-edit hand cleanup.
///   - The tactic is trimmed of surrounding whitespace and any
///     trailing semicolons; markdown fences and prose preambles are
///     also stripped before insertion.
///   - This adapter does NOT verify the tactic compiles — that's the
///     repair loop's job (apply + lake re-check).
pub fn single_tactic_to_patch(
    tactic: &str,
    diagnostic: &Diagnostic,
    file_content: &str,
) -> crate::Patch {
    let cleaned = clean_single_tactic_text(tactic);
    let start = diagnostic.range.start;
    let end = diagnostic.range.end;
    let is_insertion = start.line == end.line && start.character == end.character;
    let new_text = if is_insertion {
        let indent = line_leading_indent(file_content, start.line);
        format!("\n{indent}{cleaned}")
    } else {
        cleaned.clone()
    };
    crate::Patch {
        range: crate::Range {
            start: crate::Position {
                line: start.line,
                character: start.character,
            },
            end: crate::Position {
                line: end.line,
                character: end.character,
            },
        },
        new_text,
        rationale: format!("single-tactic adapter: {cleaned}"),
    }
}

fn clean_single_tactic_text(raw: &str) -> String {
    let mut s = raw.trim();
    // Strip enclosing markdown fences (```lean ... ```)
    if let Some(stripped) = s.strip_prefix("```") {
        s = stripped.trim_start();
        if let Some(idx) = s.find('\n') {
            s = &s[idx + 1..];
        }
        if let Some(idx) = s.rfind("```") {
            s = &s[..idx];
        }
    }
    let s = s.trim();
    // Take only the first non-empty line (single tactic = one line).
    let first = s.lines().find(|l| !l.trim().is_empty()).unwrap_or(s);
    let first = first.trim();
    // Strip a trailing semicolon if present (`; ` tactic combinator
    // should be left to the model when it actually wants chaining).
    let trimmed = first.trim_end_matches(|c: char| c == ';' || c.is_whitespace());
    trimmed.to_string()
}

fn line_leading_indent(file_content: &str, line: u32) -> String {
    let lines: Vec<&str> = file_content.lines().collect();
    let idx = (line as usize).min(lines.len().saturating_sub(1));
    let raw = lines.get(idx).copied().unwrap_or("");
    let mut indent = String::new();
    for c in raw.chars() {
        if c == ' ' || c == '\t' {
            indent.push(c);
        } else {
            break;
        }
    }
    indent
}

fn parse_goal_and_hypotheses(message: &str) -> (Option<GoalText>, Vec<Hypothesis>) {
    let mut goal: Option<GoalText> = None;
    let mut hypotheses: Vec<Hypothesis> = Vec::new();
    // Scan lines; collect candidate-hypotheses in a buffer, flush
    // them only if a `⊢ <goal>` line follows.
    let mut buffered: Vec<Hypothesis> = Vec::new();
    for raw in message.lines() {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("⊢ ") {
            goal = Some(GoalText(rest.trim().to_string()));
            hypotheses.append(&mut buffered);
            break;
        }
        if let Some(h) = parse_hypothesis_line(trimmed) {
            buffered.push(h);
        } else {
            buffered.clear();
        }
    }
    (goal, hypotheses)
}

fn parse_hypothesis_line(line: &str) -> Option<Hypothesis> {
    // Lean error format prints hypotheses as `name : type`. Multiple
    // names sharing one type (e.g., `n m : ℕ`) collapse into one
    // entry per name with the shared type. We accept both shapes.
    let colon_at = line.find(" : ")?;
    let (names_part, type_part) = line.split_at(colon_at);
    let type_part = &type_part[3..]; // skip " : "
    let names_part = names_part.trim();
    if names_part.is_empty() || type_part.is_empty() {
        return None;
    }
    // Reject lines that don't look like identifier(s).
    let first_name = names_part.split_whitespace().next()?;
    if !is_lean_identifier(first_name) {
        return None;
    }
    // For multi-name lines we keep only the first name to stay simple;
    // callers that need every name can split themselves.
    Some(Hypothesis {
        name: first_name.to_string(),
        ty: type_part.trim().to_string(),
    })
}

fn is_lean_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !(first.is_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '\'')
}

// ─── Prompt template + library ──────────────────────────────────────────

/// Versioned library of prompt templates loaded from
/// `training/prompt_templates/*.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateLibrary {
    pub schema_version: u32,
    pub templates: Vec<PromptTemplate>,
}

impl TemplateLibrary {
    pub fn find(&self, id: &str) -> Option<&PromptTemplate> {
        self.templates.iter().find(|t| t.id == id)
    }
}

/// One prompt template variant. See
/// `training/prompt_templates/README.md` for the placeholder grammar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub id: String,
    pub variant_name: String,
    #[serde(default)]
    pub requires: TemplateRequirements,
    pub user_template: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    pub expected_output_format: OutputFormat,
}

/// Declares which [`ProofState`] fields a template depends on.
///
/// The renderer fails fast with [`RenderError::MissingField`] when a
/// required field is empty. Lets the training-time sampler skip
/// templates the current extractor can't satisfy without trying to
/// render them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateRequirements {
    #[serde(default)]
    pub needs_goal: bool,
    #[serde(default)]
    pub needs_hypotheses: bool,
    #[serde(default)]
    pub needs_tactic_history: bool,
    #[serde(default)]
    pub needs_lemma_neighborhood: bool,
}

/// Expected shape of the LLM's reply for a given template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Free natural language; the consumer parses heuristically.
    FreeForm,
    /// A single Lean tactic on one line.
    SingleTactic,
    /// JSON matching the [`crate::Patch`] shape.
    PatchJson,
    /// `"ACCEPT"` or `"REVISE: <reason>"`. For Verifier-role prompts.
    VerifierVerdict,
}

/// Errors raised by [`render`].
#[derive(Debug)]
pub enum RenderError {
    /// Template referenced a placeholder this renderer doesn't support.
    UnknownPlaceholder(String),
    /// Template's [`TemplateRequirements`] aren't satisfied by the state.
    MissingField(&'static str),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPlaceholder(p) => write!(f, "unknown placeholder: {{{p}}}"),
            Self::MissingField(s) => write!(f, "template requires field: {s}"),
        }
    }
}

impl std::error::Error for RenderError {}

// ─── Renderer ───────────────────────────────────────────────────────────

/// Render a [`ProofState`] into a prompt body using `template`.
///
/// Supported placeholders (written as `{name}` in `user_template`).
/// Literal braces are escaped as `{{` and `}}`.
/// | placeholder            | source                                     |
/// |------------------------|--------------------------------------------|
/// | `{goal}`               | `current_goal`, or `(unknown)` when empty  |
/// | `{hypotheses}`         | newline-joined `name : type` lines         |
/// | `{tactic_history}`     | newline-joined `L<line>: <tactic>` lines   |
/// | `{lemmas}`             | newline-joined `<distance>-hop  <name>  :  <signature>` |
/// | `{diagnostic_line}`    | diagnostic anchor line number              |
/// | `{diagnostic_message}` | diagnostic message text                    |
/// | `{diagnostic_severity}`| severity word: `error`, `warning`, etc.    |
pub fn render(template: &PromptTemplate, state: &ProofState) -> Result<String, RenderError> {
    if template.requires.needs_goal && state.current_goal.is_none() {
        return Err(RenderError::MissingField("current_goal"));
    }
    if template.requires.needs_hypotheses && state.hypotheses.is_empty() {
        return Err(RenderError::MissingField("hypotheses"));
    }
    if template.requires.needs_tactic_history && state.tactic_history.is_empty() {
        return Err(RenderError::MissingField("tactic_history"));
    }
    if template.requires.needs_lemma_neighborhood && state.lemma_neighborhood.lemmas.is_empty() {
        return Err(RenderError::MissingField("lemma_neighborhood"));
    }

    let goal = state
        .current_goal
        .as_ref()
        .map(|g| g.0.as_str())
        .unwrap_or("(unknown)");
    let hypotheses = render_hypotheses(&state.hypotheses);
    let tactic_history = render_tactic_history(&state.tactic_history);
    let lemmas = render_lemmas(&state.lemma_neighborhood);
    let diagnostic_line = state.diagnostic_anchor.line.to_string();
    let diagnostic_message = state.diagnostic_anchor.message.as_str();
    let diagnostic_severity = severity_word(state.diagnostic_anchor.severity);

    let mut out = String::with_capacity(template.user_template.len() + 256);
    let mut rest = template.user_template.as_str();
    while let Some(open) = rest.find('{') {
        push_template_literal(&mut out, &rest[..open]);
        if rest[open..].starts_with("{{") {
            out.push('{');
            rest = &rest[open + 2..];
            continue;
        }
        let close = match rest[open..].find('}') {
            Some(off) => open + off,
            None => {
                push_template_literal(&mut out, &rest[open..]);
                rest = "";
                break;
            }
        };
        let placeholder = &rest[open + 1..close];
        let value: &str = match placeholder {
            "goal" => goal,
            "hypotheses" => &hypotheses,
            "tactic_history" => &tactic_history,
            "lemmas" => &lemmas,
            "diagnostic_line" => &diagnostic_line,
            "diagnostic_message" => diagnostic_message,
            "diagnostic_severity" => diagnostic_severity,
            other => return Err(RenderError::UnknownPlaceholder(other.to_string())),
        };
        out.push_str(value);
        rest = &rest[close + 1..];
    }
    push_template_literal(&mut out, rest);
    Ok(out)
}

fn push_template_literal(out: &mut String, literal: &str) {
    let mut rest = literal;
    while let Some(close) = rest.find("}}") {
        out.push_str(&rest[..close]);
        out.push('}');
        rest = &rest[close + 2..];
    }
    out.push_str(rest);
}

fn render_hypotheses(hs: &[Hypothesis]) -> String {
    hs.iter()
        .map(|h| format!("{} : {}", h.name, h.ty))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_tactic_history(ts: &[TacticInvocation]) -> String {
    ts.iter()
        .map(|t| format!("L{}: {}", t.line, t.tactic))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_lemmas(n: &LemmaNeighborhood) -> String {
    let mut sorted = n.lemmas.clone();
    sorted.sort_by(|a, b| {
        a.distance
            .cmp(&b.distance)
            .then_with(|| a.fully_qualified_name.cmp(&b.fully_qualified_name))
    });
    sorted
        .iter()
        .map(|l| {
            format!(
                "{}-hop  {}  :  {}",
                l.distance, l.fully_qualified_name, l.signature
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn severity_word(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Information => "information",
        Severity::Hint => "hint",
        Severity::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Position, Range};

    fn diag() -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position {
                    line: 7,
                    character: 2,
                },
                end: Position {
                    line: 7,
                    character: 10,
                },
            },
            severity: Severity::Error,
            message: "unsolved goals\n⊢ Q".into(),
            source: Some("lean".into()),
        }
    }

    fn rich_state() -> ProofState {
        ProofState {
            current_goal: Some(GoalText("Q".into())),
            hypotheses: vec![
                Hypothesis {
                    name: "h1".into(),
                    ty: "P".into(),
                },
                Hypothesis {
                    name: "h2".into(),
                    ty: "P → Q".into(),
                },
            ],
            tactic_history: vec![
                TacticInvocation {
                    line: 5,
                    tactic: "intro h1".into(),
                    goal_before: None,
                    goal_after: None,
                },
                TacticInvocation {
                    line: 6,
                    tactic: "apply h2".into(),
                    goal_before: None,
                    goal_after: None,
                },
            ],
            diagnostic_anchor: (&diag()).into(),
            lemma_neighborhood: LemmaNeighborhood {
                lemmas: vec![LemmaRef {
                    fully_qualified_name: "modus_ponens".into(),
                    signature: "(P → Q) → P → Q".into(),
                    distance: 1,
                }],
            },
        }
    }

    #[test]
    fn diagnostic_only_extractor_fills_only_anchor() {
        let s = DiagnosticOnlyExtractor.extract("", &diag()).unwrap();
        assert!(s.current_goal.is_none());
        assert!(s.hypotheses.is_empty());
        assert!(s.tactic_history.is_empty());
        assert!(s.lemma_neighborhood.lemmas.is_empty());
        assert_eq!(s.diagnostic_anchor.line, 7);
        assert_eq!(s.diagnostic_anchor.severity, Severity::Error);
    }

    #[test]
    fn anchor_from_diagnostic_uses_start_line() {
        let a: DiagnosticAnchor = (&diag()).into();
        assert_eq!(a.line, 7);
        assert_eq!(a.severity, Severity::Error);
        assert!(a.message.starts_with("unsolved goals"));
    }

    #[test]
    fn render_substitutes_goal_and_diagnostic() {
        let t = PromptTemplate {
            id: "goal_focused".into(),
            variant_name: "Goal-focused".into(),
            requires: TemplateRequirements {
                needs_goal: true,
                ..Default::default()
            },
            user_template:
                "Goal: {goal}\nError at L{diagnostic_line} ({diagnostic_severity}): {diagnostic_message}"
                    .into(),
            system_prompt: None,
            expected_output_format: OutputFormat::SingleTactic,
        };
        let out = render(&t, &rich_state()).unwrap();
        assert!(out.contains("Goal: Q"));
        assert!(out.contains("L7"));
        assert!(out.contains("error"));
        assert!(out.contains("unsolved goals"));
    }

    #[test]
    fn render_escapes_literal_json_braces() {
        let t = PromptTemplate {
            id: "json_literal".into(),
            variant_name: "JSON literal".into(),
            requires: TemplateRequirements::default(),
            user_template:
                "Return {{\"new_text\":\"{diagnostic_message}\",\"range\":{{\"line\":{diagnostic_line}}}}}."
                    .into(),
            system_prompt: None,
            expected_output_format: OutputFormat::PatchJson,
        };
        let out = render(&t, &rich_state()).unwrap();
        assert_eq!(
            out,
            "Return {\"new_text\":\"unsolved goals\n⊢ Q\",\"range\":{\"line\":7}}."
        );
    }

    #[test]
    fn render_substitutes_hypotheses_and_lemmas() {
        let t = PromptTemplate {
            id: "graph_aware".into(),
            variant_name: "Graph-aware".into(),
            requires: TemplateRequirements {
                needs_goal: true,
                needs_hypotheses: true,
                needs_lemma_neighborhood: true,
                ..Default::default()
            },
            user_template: "G:{goal}\nH:\n{hypotheses}\nL:\n{lemmas}".into(),
            system_prompt: None,
            expected_output_format: OutputFormat::PatchJson,
        };
        let out = render(&t, &rich_state()).unwrap();
        assert!(out.contains("h1 : P"));
        assert!(out.contains("h2 : P → Q"));
        assert!(out.contains("1-hop"));
        assert!(out.contains("modus_ponens"));
    }

    #[test]
    fn render_unknown_placeholder_errors() {
        let t = PromptTemplate {
            id: "bad".into(),
            variant_name: "Bad".into(),
            requires: TemplateRequirements::default(),
            user_template: "What is {mystery}?".into(),
            system_prompt: None,
            expected_output_format: OutputFormat::FreeForm,
        };
        let s = ProofState {
            current_goal: None,
            hypotheses: vec![],
            tactic_history: vec![],
            diagnostic_anchor: (&diag()).into(),
            lemma_neighborhood: LemmaNeighborhood::default(),
        };
        let err = render(&t, &s).unwrap_err();
        match err {
            RenderError::UnknownPlaceholder(p) => assert_eq!(p, "mystery"),
            other => panic!("expected unknown-placeholder error, got {other:?}"),
        }
    }

    #[test]
    fn render_requires_goal_fails_when_missing() {
        let t = PromptTemplate {
            id: "needs_goal".into(),
            variant_name: "Needs goal".into(),
            requires: TemplateRequirements {
                needs_goal: true,
                ..Default::default()
            },
            user_template: "G:{goal}".into(),
            system_prompt: None,
            expected_output_format: OutputFormat::SingleTactic,
        };
        let mut s = rich_state();
        s.current_goal = None;
        let err = render(&t, &s).unwrap_err();
        assert!(matches!(err, RenderError::MissingField("current_goal")));
    }

    #[test]
    fn heuristic_extracts_goal_from_diagnostic_message() {
        let d = Diagnostic {
            range: Range {
                start: Position {
                    line: 10,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
            severity: Severity::Error,
            message: "unsolved goals\n⊢ P → Q".into(),
            source: None,
        };
        let s = LeanTextHeuristicExtractor::default()
            .extract("", &d)
            .unwrap();
        assert_eq!(s.current_goal.unwrap().0, "P → Q");
        assert!(s.hypotheses.is_empty());
    }

    #[test]
    fn heuristic_extracts_hypotheses_when_lean_echoes_them() {
        let d = Diagnostic {
            range: Range {
                start: Position {
                    line: 10,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
            severity: Severity::Error,
            message: "unsolved goals\nh1 : P\nh2 : P → Q\n⊢ Q".into(),
            source: None,
        };
        let s = LeanTextHeuristicExtractor::default()
            .extract("", &d)
            .unwrap();
        assert_eq!(s.current_goal.unwrap().0, "Q");
        assert_eq!(s.hypotheses.len(), 2);
        assert_eq!(s.hypotheses[0].name, "h1");
        assert_eq!(s.hypotheses[0].ty, "P");
        assert_eq!(s.hypotheses[1].name, "h2");
        assert_eq!(s.hypotheses[1].ty, "P → Q");
    }

    #[test]
    fn heuristic_rejects_non_identifier_lines_as_hypotheses() {
        let d = Diagnostic {
            range: Range {
                start: Position {
                    line: 10,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
            severity: Severity::Error,
            message: "type mismatch\n123 : not an identifier\n⊢ Q".into(),
            source: None,
        };
        let s = LeanTextHeuristicExtractor::default()
            .extract("", &d)
            .unwrap();
        assert_eq!(s.current_goal.unwrap().0, "Q");
        assert!(s.hypotheses.is_empty());
    }

    #[test]
    fn heuristic_extracts_tactic_history_from_file_content() {
        let file = "theorem foo : P → Q := by\n  intro h\n  apply h2\n  rfl\n  sorry\n";
        let d = Diagnostic {
            range: Range {
                start: Position {
                    line: 4,
                    character: 2,
                },
                end: Position {
                    line: 4,
                    character: 7,
                },
            },
            severity: Severity::Error,
            message: "unsolved goals\n⊢ Q".into(),
            source: None,
        };
        let s = LeanTextHeuristicExtractor::default()
            .extract(file, &d)
            .unwrap();
        let tactics: Vec<&str> = s.tactic_history.iter().map(|t| t.tactic.as_str()).collect();
        assert!(tactics.contains(&"intro h"));
        assert!(tactics.contains(&"apply h2"));
        assert!(tactics.contains(&"rfl"));
        // The diagnostic line itself (line 4 with "sorry") is excluded
        // (history is *preceding* lines only).
        assert!(!tactics.contains(&"sorry"));
    }

    #[test]
    fn heuristic_ignores_non_tactic_lines() {
        // Diagnostic on the trailing blank line so `rfl` at line 4 is
        // inside the preceding-lines window.
        let file =
            "-- comment\ntheorem foo : P := by\n  intro h\n  some_macro_call\n  rfl\n  -- end\n";
        let d = Diagnostic {
            range: Range {
                start: Position {
                    line: 5,
                    character: 2,
                },
                end: Position {
                    line: 5,
                    character: 5,
                },
            },
            severity: Severity::Error,
            message: "ok".into(),
            source: None,
        };
        let s = LeanTextHeuristicExtractor::default()
            .extract(file, &d)
            .unwrap();
        let tactics: Vec<&str> = s.tactic_history.iter().map(|t| t.tactic.as_str()).collect();
        assert!(tactics.contains(&"intro h"));
        assert!(tactics.contains(&"rfl"));
        // `some_macro_call` is not a known tactic keyword -> ignored.
        assert!(!tactics.iter().any(|t| t.contains("some_macro_call")));
        // Comment lines ignored.
        assert!(!tactics.iter().any(|t| t.contains("comment")));
        assert!(!tactics.iter().any(|t| t.contains("-- end")));
    }

    #[test]
    fn heuristic_respects_history_lines_window() {
        let file = "intro a\nintro b\nintro c\nintro d\nintro e\nintro f\nintro g\nfinal\n";
        let d = Diagnostic {
            range: Range {
                start: Position {
                    line: 7,
                    character: 0,
                },
                end: Position {
                    line: 7,
                    character: 5,
                },
            },
            severity: Severity::Error,
            message: "ok".into(),
            source: None,
        };
        let s = LeanTextHeuristicExtractor { history_lines: 3 }
            .extract(file, &d)
            .unwrap();
        assert_eq!(s.tactic_history.len(), 3);
        let last_three: Vec<&str> = s.tactic_history.iter().map(|t| t.tactic.as_str()).collect();
        assert_eq!(last_three, vec!["intro e", "intro f", "intro g"]);
    }

    #[test]
    fn single_tactic_adapter_replaces_range_when_span_nonempty() {
        let d = Diagnostic {
            range: Range {
                start: Position {
                    line: 3,
                    character: 2,
                },
                end: Position {
                    line: 3,
                    character: 7,
                },
            },
            severity: Severity::Error,
            message: "...".into(),
            source: None,
        };
        let p = single_tactic_to_patch("exact h2", &d, "");
        assert_eq!(p.new_text, "exact h2");
        assert_eq!(p.range.start.line, 3);
        assert_eq!(p.range.start.character, 2);
        assert_eq!(p.range.end.character, 7);
        assert!(p.rationale.contains("exact h2"));
    }

    #[test]
    fn single_tactic_adapter_inserts_with_indent_when_range_is_point() {
        let file = "theorem t : P := by\n  intro h\n  -- placeholder\n";
        let d = Diagnostic {
            range: Range {
                start: Position {
                    line: 2,
                    character: 0,
                },
                end: Position {
                    line: 2,
                    character: 0,
                },
            },
            severity: Severity::Error,
            message: "unsolved".into(),
            source: None,
        };
        let p = single_tactic_to_patch("exact h", &d, file);
        // Inserts a leading newline + matching indent (line 2 starts
        // with "  -- placeholder", so indent is "  ").
        assert!(p.new_text.starts_with('\n'));
        assert!(p.new_text.contains("  exact h"));
    }

    #[test]
    fn single_tactic_adapter_strips_markdown_fences() {
        let d = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            },
            severity: Severity::Error,
            message: "...".into(),
            source: None,
        };
        let p = single_tactic_to_patch("```lean\nrfl\n```", &d, "");
        assert_eq!(p.new_text, "rfl");
    }

    #[test]
    fn single_tactic_adapter_takes_only_first_line_and_strips_semicolon() {
        let d = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            },
            severity: Severity::Error,
            message: "...".into(),
            source: None,
        };
        let p = single_tactic_to_patch("apply h ;\nsecond line should be ignored", &d, "");
        assert_eq!(p.new_text, "apply h");
    }

    #[test]
    fn template_library_find_by_id() {
        let lib = TemplateLibrary {
            schema_version: 1,
            templates: vec![PromptTemplate {
                id: "foo".into(),
                variant_name: "Foo".into(),
                requires: TemplateRequirements::default(),
                user_template: "bar".into(),
                system_prompt: None,
                expected_output_format: OutputFormat::FreeForm,
            }],
        };
        assert!(lib.find("foo").is_some());
        assert!(lib.find("missing").is_none());
    }
}
