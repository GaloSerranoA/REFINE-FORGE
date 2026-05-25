//! Parser for the operator's `## Human decision` markdown
//! section. Recognises four response forms:
//!
//! - `APPROVED: <reason>`
//! - `REJECTED: <reason>`
//! - `EDIT_AND_RESUBMIT: <suggestions>`
//! - Partial (batched only): `APPROVED: 1-5,7; REJECTED: 6,8 [reasons]`
//!
//! See criteria-doc v0.3 "Batch escalations" subsection for the
//! response-form grammar.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome")]
pub enum DecisionOutcome {
    Approved {
        reason: Option<String>,
    },
    Rejected {
        reason: String,
    },
    EditAndResubmit {
        suggestions: String,
    },
    /// Batched-only: per-item decisions in a single packet.
    Partial(PartialDecision),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialDecision {
    /// 1-based item indices the operator approved.
    pub approved_indices: Vec<u32>,
    /// 1-based item indices the operator rejected, with reasons,
    /// in the order the operator wrote them in the packet.
    pub rejected_indices: Vec<(u32, String)>,
}

impl PartialDecision {
    pub fn rejection_reason(&self, idx: u32) -> Option<&str> {
        self.rejected_indices
            .iter()
            .find(|(i, _)| *i == idx)
            .map(|(_, r)| r.as_str())
    }

    pub fn rejected(&self, idx: u32) -> bool {
        self.rejected_indices.iter().any(|(i, _)| *i == idx)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecisionParseError {
    #[error("packet body does not contain `## Human decision` section")]
    MissingSection,
    #[error("`## Human decision` section is still pending (no decision yet)")]
    Pending,
    #[error("could not recognise decision form: {0}")]
    Unrecognised(String),
    #[error("invalid partial-decision index list: {0}")]
    InvalidIndices(String),
    #[error("REJECTED requires a non-empty reason")]
    RejectedWithoutReason,
    #[error("EDIT_AND_RESUBMIT requires non-empty suggestions")]
    EditWithoutSuggestions,
}

/// Parse the operator's decision out of a packet's markdown.
///
/// Strategy:
/// 1. Find the `## Human decision` heading.
/// 2. Collect lines after it that are NOT HTML comments and NOT
///    the literal `(pending)`.
/// 3. The first non-comment, non-blank line is the verdict
///    (APPROVED / REJECTED / EDIT_AND_RESUBMIT). Subsequent
///    lines are appended to the reason / suggestions string.
/// 4. For partial form: split on `;` and parse each side.
pub fn parse_decision(markdown: &str) -> Result<DecisionOutcome, DecisionParseError> {
    let body = decision_body(markdown)?;
    let verdict_line = first_verdict_line(&body)?;

    if let Some(rest) = verdict_line.strip_prefix("APPROVED:") {
        if looks_like_index_list(rest) {
            return parse_partial(&body);
        }
        let reason = clean_reason(rest, &body, 1);
        Ok(DecisionOutcome::Approved {
            reason: if reason.is_empty() {
                None
            } else {
                Some(reason)
            },
        })
    } else if verdict_line.eq_ignore_ascii_case("APPROVED") {
        Ok(DecisionOutcome::Approved { reason: None })
    } else if let Some(rest) = verdict_line.strip_prefix("REJECTED:") {
        if looks_like_index_list(rest) {
            return parse_partial(&body);
        }
        let reason = clean_reason(rest, &body, 1);
        if reason.is_empty() {
            return Err(DecisionParseError::RejectedWithoutReason);
        }
        Ok(DecisionOutcome::Rejected { reason })
    } else if let Some(rest) = verdict_line.strip_prefix("EDIT_AND_RESUBMIT:") {
        let suggestions = clean_reason(rest, &body, 1);
        if suggestions.is_empty() {
            return Err(DecisionParseError::EditWithoutSuggestions);
        }
        Ok(DecisionOutcome::EditAndResubmit { suggestions })
    } else {
        Err(DecisionParseError::Unrecognised(verdict_line))
    }
}

fn decision_body(markdown: &str) -> Result<String, DecisionParseError> {
    let heading = "## Human decision";
    let idx = markdown
        .find(heading)
        .ok_or(DecisionParseError::MissingSection)?;
    let after = &markdown[idx + heading.len()..];
    // Cut at the next heading (## or #), if any.
    let mut body = String::new();
    for line in after.lines().skip_while(|l| l.trim().is_empty()) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("# ") || trimmed.starts_with("## ") {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    Ok(body)
}

fn first_verdict_line(body: &str) -> Result<String, DecisionParseError> {
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("<!--") {
            continue;
        }
        if line.eq_ignore_ascii_case("(pending)") {
            return Err(DecisionParseError::Pending);
        }
        return Ok(line.to_string());
    }
    Err(DecisionParseError::Pending)
}

/// Build the trailing reason/suggestions string from the rest
/// of the first line + any continuation lines (that aren't HTML
/// comments or further headings).
fn clean_reason(first_line_rest: &str, body: &str, skip_lines: usize) -> String {
    let mut out = first_line_rest.trim().to_string();
    let mut skipped = 0usize;
    for raw in body.lines() {
        let line = raw.trim();
        if skipped < skip_lines {
            if !line.is_empty() && !line.starts_with("<!--") {
                skipped += 1;
            }
            continue;
        }
        if line.is_empty() || line.starts_with("<!--") {
            continue;
        }
        if line.starts_with("# ") || line.starts_with("## ") {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(line);
    }
    out
}

/// True when `s` (the text after `APPROVED:` or `REJECTED:`)
/// starts with what looks like an index list — digits / commas /
/// hyphens / whitespace only, up to the first `;` or `[`. Used
/// to distinguish partial-form responses from free-text reasons.
fn looks_like_index_list(s: &str) -> bool {
    let leading: String = s.chars().take_while(|c| *c != ';' && *c != '[').collect();
    let stripped = leading.trim();
    !stripped.is_empty()
        && stripped
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == ',' || c.is_whitespace())
}

fn parse_partial(body: &str) -> Result<DecisionOutcome, DecisionParseError> {
    // Re-collect the full verdict line plus subsequent
    // continuation lines so multi-line partials work.
    let mut full = String::new();
    let mut seen_verdict = false;
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("<!--") {
            continue;
        }
        if line.starts_with("# ") || line.starts_with("## ") {
            break;
        }
        if !seen_verdict && !line.starts_with("APPROVED:") && !line.starts_with("REJECTED:") {
            continue;
        }
        if !full.is_empty() {
            full.push(' ');
        }
        full.push_str(line);
        seen_verdict = true;
    }

    let mut approved: Vec<u32> = Vec::new();
    let mut rejected: Vec<(u32, String)> = Vec::new();

    for seg in full.split(';') {
        let seg = seg.trim();
        if let Some(rest) = seg.strip_prefix("APPROVED:") {
            for idx in parse_index_list(rest.trim())? {
                approved.push(idx);
            }
        } else if let Some(rest) = seg.strip_prefix("REJECTED:") {
            // Format: "6,8 [reason that may have spaces]" — the
            // bracketed reason applies to every rejected index in
            // the segment.
            let (ids_str, reason) = split_indices_and_reason(rest.trim());
            let ids = parse_index_list(ids_str)?;
            for idx in ids {
                rejected.push((idx, reason.clone()));
            }
        }
    }

    if approved.is_empty() && rejected.is_empty() {
        return Err(DecisionParseError::InvalidIndices(full));
    }

    Ok(DecisionOutcome::Partial(PartialDecision {
        approved_indices: approved,
        rejected_indices: rejected,
    }))
}

fn split_indices_and_reason(s: &str) -> (&str, String) {
    if let Some(open) = s.find('[') {
        if let Some(close_rel) = s[open..].find(']') {
            let close = open + close_rel;
            let ids = s[..open].trim();
            let reason = s[open + 1..close].trim().to_string();
            return (ids, reason);
        }
    }
    (s.trim(), String::new())
}

fn parse_index_list(s: &str) -> Result<Vec<u32>, DecisionParseError> {
    let mut out = Vec::new();
    for token in s.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if let Some((a, b)) = token.split_once('-') {
            let a: u32 = a
                .trim()
                .parse()
                .map_err(|_| DecisionParseError::InvalidIndices(s.into()))?;
            let b: u32 = b
                .trim()
                .parse()
                .map_err(|_| DecisionParseError::InvalidIndices(s.into()))?;
            if b < a {
                return Err(DecisionParseError::InvalidIndices(s.into()));
            }
            for i in a..=b {
                out.push(i);
            }
        } else {
            let n: u32 = token
                .parse()
                .map_err(|_| DecisionParseError::InvalidIndices(s.into()))?;
            out.push(n);
        }
    }
    if out.is_empty() {
        return Err(DecisionParseError::InvalidIndices(s.into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(decision: &str) -> String {
        format!(
            "# Escalation: x\n\nsome body\n\n## Human decision\n\n{}\n",
            decision
        )
    }

    #[test]
    fn approved_with_reason() {
        let r = parse_decision(&wrap("APPROVED: looks fine to me")).unwrap();
        assert_eq!(
            r,
            DecisionOutcome::Approved {
                reason: Some("looks fine to me".into())
            }
        );
    }

    #[test]
    fn approved_without_reason_text() {
        let r = parse_decision(&wrap("APPROVED:")).unwrap();
        assert_eq!(r, DecisionOutcome::Approved { reason: None });
    }

    #[test]
    fn approved_bare_word() {
        let r = parse_decision(&wrap("APPROVED")).unwrap();
        assert_eq!(r, DecisionOutcome::Approved { reason: None });
    }

    #[test]
    fn rejected_with_reason() {
        let r = parse_decision(&wrap("REJECTED: u64 → Nat hides real overflow")).unwrap();
        assert_eq!(
            r,
            DecisionOutcome::Rejected {
                reason: "u64 → Nat hides real overflow".into()
            }
        );
    }

    #[test]
    fn rejected_without_reason_errors() {
        let r = parse_decision(&wrap("REJECTED:"));
        assert_eq!(r, Err(DecisionParseError::RejectedWithoutReason));
    }

    #[test]
    fn edit_and_resubmit_carries_suggestions() {
        let r = parse_decision(&wrap("EDIT_AND_RESUBMIT: try BitVec 64 instead")).unwrap();
        assert_eq!(
            r,
            DecisionOutcome::EditAndResubmit {
                suggestions: "try BitVec 64 instead".into()
            }
        );
    }

    #[test]
    fn pending_section_returns_pending_error() {
        let r = parse_decision(&wrap("(pending)"));
        assert_eq!(r, Err(DecisionParseError::Pending));
    }

    #[test]
    fn missing_section_errors() {
        let r = parse_decision("# Some packet\n\nbody with no decision heading\n");
        assert_eq!(r, Err(DecisionParseError::MissingSection));
    }

    #[test]
    fn comments_inside_decision_block_are_ignored() {
        let md = "## Human decision\n\n<!-- some hint -->\n<!-- another hint -->\n\nAPPROVED: ok\n";
        let r = parse_decision(md).unwrap();
        assert_eq!(
            r,
            DecisionOutcome::Approved {
                reason: Some("ok".into())
            }
        );
    }

    #[test]
    fn partial_form_simple() {
        let r = parse_decision(&wrap(
            "APPROVED: 1,2,3; REJECTED: 4 [the eval did not converge]",
        ))
        .unwrap();
        if let DecisionOutcome::Partial(p) = r {
            assert_eq!(p.approved_indices, vec![1, 2, 3]);
            assert_eq!(p.rejection_reason(4), Some("the eval did not converge"));
        } else {
            panic!("expected Partial, got {:?}", r);
        }
    }

    #[test]
    fn partial_form_range_and_individual() {
        let r = parse_decision(&wrap("APPROVED: 1-5,7; REJECTED: 6,8 [losses too lossy]")).unwrap();
        if let DecisionOutcome::Partial(p) = r {
            assert_eq!(p.approved_indices, vec![1, 2, 3, 4, 5, 7]);
            assert_eq!(p.rejected_indices.len(), 2);
            assert!(p.rejected(6));
            assert!(p.rejected(8));
        } else {
            panic!("expected Partial, got {:?}", r);
        }
    }

    #[test]
    fn partial_form_only_approved() {
        let r = parse_decision(&wrap("APPROVED: 1-3,5;")).unwrap();
        if let DecisionOutcome::Partial(p) = r {
            assert_eq!(p.approved_indices, vec![1, 2, 3, 5]);
            assert!(p.rejected_indices.is_empty());
        } else {
            panic!("expected Partial, got {:?}", r);
        }
    }

    #[test]
    fn partial_form_invalid_range_errors() {
        let r = parse_decision(&wrap("APPROVED: 5-1;"));
        assert!(matches!(r, Err(DecisionParseError::InvalidIndices(_))));
    }

    #[test]
    fn unrecognised_verdict_errors() {
        let r = parse_decision(&wrap("MAYBE: I'm not sure"));
        assert!(matches!(r, Err(DecisionParseError::Unrecognised(_))));
    }

    #[test]
    fn reason_continuation_across_lines_is_joined() {
        let md = "## Human decision\n\nREJECTED: the lost-overflow\nbreaks the saturating_add\nargument in the refinement doc\n";
        let r = parse_decision(md).unwrap();
        assert_eq!(
            r,
            DecisionOutcome::Rejected {
                reason:
                    "the lost-overflow breaks the saturating_add argument in the refinement doc"
                        .into()
            }
        );
    }

    #[test]
    fn decision_round_trips_via_json() {
        let cases = [
            DecisionOutcome::Approved { reason: None },
            DecisionOutcome::Approved {
                reason: Some("ok".into()),
            },
            DecisionOutcome::Rejected {
                reason: "no".into(),
            },
            DecisionOutcome::EditAndResubmit {
                suggestions: "try x".into(),
            },
            DecisionOutcome::Partial(PartialDecision {
                approved_indices: vec![1, 2],
                rejected_indices: vec![(3, "nope".into())],
            }),
        ];
        for d in &cases {
            let j = serde_json::to_string(d).expect("ser");
            let back: DecisionOutcome = serde_json::from_str(&j).expect("de");
            assert_eq!(&back, d);
        }
    }
}
