//! Anthropic strategy SKELETON.
//!
//! Real:
//! - The `RepairStrategy` trait impl.
//! - Prompt construction (`build_request`).
//! - Response parsing (`parse_response_into_patch`).
//! - The transport trait (`AnthropicTransport`).
//!
//! Mocked:
//! - The HTTP transport itself. `MockTransport` returns a canned
//!   string; no network is touched. Swap for a real `ReqwestTransport`
//!   to make this useful (see crate README §"Wiring a real transport").

use anyhow::Result;
use serde::{Deserialize, Serialize};

use refineforge_repair_api::{Diagnostic, Patch, Position, Range, RepairStrategy};

// ─── Transport ──────────────────────────────────────────────────────────

/// Abstraction over the HTTP call to Anthropic's `/v1/messages`.
/// The skeleton ships only `MockTransport`; a real implementation
/// would use `reqwest` (async) or `ureq` (sync).
pub trait AnthropicTransport: Send + Sync {
    fn send(&self, request: &MessagesRequest) -> Result<MessagesResponse>;
}

/// Canned-response transport. The CLI's `anthropic-mock` strategy
/// uses [`MockTransport::declines`] so it always returns `None` and
/// the repair loop reports `NoProposal` — same observable behaviour
/// as `mock`, but exercises the prompt / parsing / trait wiring.
pub struct MockTransport {
    pub canned_text: String,
}

impl MockTransport {
    /// A transport that returns an empty JSON object — parses to
    /// `None`, so the strategy declines every proposal.
    pub fn declines() -> Self {
        Self { canned_text: "{}".into() }
    }

    /// A transport that returns a specific JSON patch. Useful for
    /// unit tests; not exposed via the CLI.
    pub fn returns(json_patch: impl Into<String>) -> Self {
        Self { canned_text: json_patch.into() }
    }
}

impl AnthropicTransport for MockTransport {
    fn send(&self, _request: &MessagesRequest) -> Result<MessagesResponse> {
        Ok(MessagesResponse {
            content: vec![ContentBlock {
                kind: "text".into(),
                text: Some(self.canned_text.clone()),
            }],
        })
    }
}

// ─── Wire types (mirror Anthropic /v1/messages shape) ───────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MessagesRequest {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessagesResponse {
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: Option<String>,
}

// ─── Strategy ───────────────────────────────────────────────────────────

pub struct AnthropicStrategy<T: AnthropicTransport> {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    transport: T,
}

impl<T: AnthropicTransport> AnthropicStrategy<T> {
    pub fn new(api_key: String, model: impl Into<String>, transport: T) -> Self {
        Self {
            api_key,
            model: model.into(),
            max_tokens: 4096,
            transport,
        }
    }

    fn build_request(&self, d: &Diagnostic, file: &str) -> MessagesRequest {
        let user = format!(
            "DIAGNOSTIC:\n  severity: {:?}\n  range: line {}, col {} -- line {}, col {}\n  message: {}\n\nFILE (lean):\n```lean\n{}\n```\n\nPropose ONE minimal patch as a single JSON object with keys: start_line, start_char, end_line, end_char, new_text, rationale. Do NOT use sorry/admit/axiom — the policy gate will reject those. Respond with ONLY the JSON object, no prose, no markdown fences.",
            d.severity,
            d.range.start.line, d.range.start.character,
            d.range.end.line, d.range.end.character,
            d.message,
            file,
        );
        MessagesRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            system: Some(SYSTEM_PROMPT.to_string()),
            messages: vec![Message {
                role: "user".into(),
                content: user,
            }],
        }
    }
}

impl<T: AnthropicTransport> RepairStrategy for AnthropicStrategy<T> {
    fn propose_patch(
        &self,
        diagnostic: &Diagnostic,
        file_content: &str,
    ) -> Result<Option<Patch>> {
        let request = self.build_request(diagnostic, file_content);
        let response = self.transport.send(&request)?;
        Ok(parse_response_into_patch(&response))
    }

    fn name(&self) -> &'static str { "anthropic" }
}

const SYSTEM_PROMPT: &str = "You are an expert Lean 4 proof engineer assisting refineforge's bounded repair loop. \
The user will give you ONE diagnostic and the full source of one Lean file. \
Propose ONE minimal patch that, when applied, may fix the diagnostic. \
You MUST NOT use sorry, admit, or non-core axiom declarations — the policy gate will reject those and the loop will halt. \
Prefer the smallest possible patch range. If you cannot propose a fix in good faith, return the empty object `{}` to decline.";

// ─── Parsing (pure function, fully testable) ────────────────────────────

#[derive(Deserialize)]
struct PatchJson {
    start_line: u32,
    start_char: u32,
    end_line: u32,
    end_char: u32,
    new_text: String,
    #[serde(default)]
    rationale: String,
}

pub fn parse_response_into_patch(response: &MessagesResponse) -> Option<Patch> {
    let text = response.content.iter().find_map(|c| c.text.as_ref())?;
    let trimmed = strip_markdown_fences(text.trim());
    let p: PatchJson = serde_json::from_str(trimmed).ok()?;
    Some(Patch {
        range: Range {
            start: Position {
                line: p.start_line,
                character: p.start_char,
            },
            end: Position {
                line: p.end_line,
                character: p.end_char,
            },
        },
        new_text: p.new_text,
        rationale: if p.rationale.is_empty() {
            "anthropic-proposed".into()
        } else {
            p.rationale
        },
    })
}

fn strip_markdown_fences(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix("```json").unwrap_or(s);
    let s = s.strip_prefix("```").unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s);
    s.trim()
}

// ─── Convenience factory for the CLI ────────────────────────────────────

/// Returns an `AnthropicStrategy` wired to `MockTransport::declines()`.
/// The CLI's `--strategy anthropic-mock` uses this. Useful for proving
/// the prompt-and-parsing path runs end-to-end without an API key.
pub fn anthropic_mock_strategy() -> Box<dyn RepairStrategy> {
    Box::new(AnthropicStrategy::new(
        "MOCK-KEY-NOT-USED".to_string(),
        "claude-opus-4-7",
        MockTransport::declines(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use refineforge_repair_api::Severity;

    fn d() -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position {
                    line: 5,
                    character: 12,
                },
                end: Position {
                    line: 5,
                    character: 18,
                },
            },
            severity: Severity::Error,
            message: "unsolved goals".into(),
            source: Some("lean".into()),
        }
    }

    #[test]
    fn build_request_includes_diagnostic_message_and_file() {
        let s = AnthropicStrategy::new(
            "k".into(),
            "claude-opus-4-7",
            MockTransport::declines(),
        );
        let req = s.build_request(&d(), "theorem t : True := by sorry");
        assert_eq!(req.model, "claude-opus-4-7");
        assert_eq!(req.max_tokens, 4096);
        assert!(req.system.as_deref().unwrap().contains("MUST NOT use sorry"));
        let user = &req.messages[0].content;
        assert!(user.contains("unsolved goals"));
        assert!(user.contains("theorem t : True := by sorry"));
        assert!(user.contains("line 5, col 12"));
    }

    #[test]
    fn parse_response_into_patch_succeeds_on_valid_json() {
        let response = MessagesResponse {
            content: vec![ContentBlock {
                kind: "text".into(),
                text: Some(
                    r#"{"start_line":5,"start_char":12,"end_line":5,"end_char":18,"new_text":"trivial","rationale":"closes by trivial"}"#.into(),
                ),
            }],
        };
        let p = parse_response_into_patch(&response).expect("must parse");
        assert_eq!(p.range.start.line, 5);
        assert_eq!(p.range.start.character, 12);
        assert_eq!(p.new_text, "trivial");
        assert_eq!(p.rationale, "closes by trivial");
    }

    #[test]
    fn parse_response_strips_markdown_fences() {
        let response = MessagesResponse {
            content: vec![ContentBlock {
                kind: "text".into(),
                text: Some(
                    "```json\n{\"start_line\":0,\"start_char\":0,\"end_line\":0,\"end_char\":1,\"new_text\":\"x\",\"rationale\":\"\"}\n```"
                        .into(),
                ),
            }],
        };
        let p = parse_response_into_patch(&response).expect("must parse");
        assert_eq!(p.new_text, "x");
        // Empty rationale gets a default attribution.
        assert_eq!(p.rationale, "anthropic-proposed");
    }

    #[test]
    fn parse_response_returns_none_on_empty_object() {
        let response = MessagesResponse {
            content: vec![ContentBlock {
                kind: "text".into(),
                text: Some("{}".into()),
            }],
        };
        assert!(parse_response_into_patch(&response).is_none());
    }

    #[test]
    fn parse_response_returns_none_on_malformed_json() {
        let response = MessagesResponse {
            content: vec![ContentBlock {
                kind: "text".into(),
                text: Some("not even json".into()),
            }],
        };
        assert!(parse_response_into_patch(&response).is_none());
    }

    #[test]
    fn end_to_end_with_mock_transport_returning_patch() {
        let canned = r#"{"start_line":1,"start_char":2,"end_line":3,"end_char":4,"new_text":"foo","rationale":"bar"}"#;
        let s = AnthropicStrategy::new(
            "k".into(),
            "claude-opus-4-7",
            MockTransport::returns(canned),
        );
        let p = s.propose_patch(&d(), "file contents").unwrap().expect("must propose");
        assert_eq!(p.new_text, "foo");
        assert_eq!(p.rationale, "bar");
        assert_eq!(s.name(), "anthropic");
    }

    #[test]
    fn anthropic_mock_strategy_factory_declines() {
        let s = anthropic_mock_strategy();
        assert_eq!(s.propose_patch(&d(), "anything").unwrap(), None);
        assert_eq!(s.name(), "anthropic");
    }
}
