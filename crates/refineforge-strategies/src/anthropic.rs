//! Anthropic strategy: turns Lean diagnostics into proposed patches
//! via Anthropic's `/v1/messages` endpoint.
//!
//! Architecture:
//!
//! ```
//!  AnthropicStrategy<T>   ─uses─►   T: AnthropicTransport
//!         │                                │
//!         │ build_request                  │ send (HTTP)
//!         │ parse_response_into_patch      │
//!         ▼                                ▼
//!   prompt + parsing                 MockTransport  (canned responses)
//!   are PURE FUNCTIONS,           or ReqwestTransport (real HTTP, retry)
//!   fully unit-tested
//! ```
//!
//! The wire types (`MessagesRequest`, `Message`, `SystemBlock`,
//! `UserBlock`, `CacheControl`) mirror Anthropic's API exactly so
//! that swapping the transport requires zero changes to the
//! request/response shape.
//!
//! Prompt caching: the system prompt and the file_content block are
//! marked `cache_control: ephemeral`. Across iterations within a
//! session, only the diagnostic block changes — saves ~90% of cost
//! on long files.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use refineforge_repair_api::{Diagnostic, Patch, Position, Range, RepairStrategy};

// ─── Transport ──────────────────────────────────────────────────────────

/// Abstraction over the HTTP call to Anthropic's `/v1/messages`.
/// Implementations: `MockTransport` (canned responses, ships in this
/// crate) and `ReqwestTransport` (real HTTP, also in this crate).
pub trait AnthropicTransport: Send + Sync {
    fn send(&self, request: &MessagesRequest) -> Result<MessagesResponse>;
}

/// Canned-response transport. The CLI's `anthropic-mock` strategy
/// uses [`MockTransport::declines`] so it always returns `None`
/// and the repair loop reports `NoProposal` — same observable
/// behaviour as `mock`, but exercises the full prompt construction
/// + response parsing pipeline (which the real transport then
/// reuses unchanged).
pub struct MockTransport {
    pub canned_text: String,
}

impl MockTransport {
    /// Returns an empty JSON object — parses to `None`, so the
    /// strategy declines every proposal.
    pub fn declines() -> Self {
        Self { canned_text: "{}".into() }
    }

    /// Returns the supplied JSON. Useful for unit tests; not exposed
    /// via the CLI.
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
            stop_reason: Some("end_turn".into()),
            usage: None,
        })
    }
}

// ─── Wire types (mirror Anthropic /v1/messages shape) ───────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MessagesRequest {
    pub model: String,
    pub max_tokens: u32,
    /// System prompt as an array of blocks so we can mark them
    /// `cache_control: ephemeral`. Single-block array is the common
    /// case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<SystemBlock>>,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemBlock {
    #[serde(rename = "type")]
    pub kind: String, // always "text"
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: String, // "user" or "assistant"
    pub content: Vec<UserBlock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserBlock {
    #[serde(rename = "type")]
    pub kind: String, // always "text"
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: String, // "ephemeral"
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessagesResponse {
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

// ─── Strategy ───────────────────────────────────────────────────────────

pub struct AnthropicStrategy<T: AnthropicTransport> {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub enable_caching: bool,
    transport: T,
}

impl<T: AnthropicTransport> AnthropicStrategy<T> {
    /// Default constructor: caching enabled, max_tokens=4096.
    pub fn new(api_key: String, model: impl Into<String>, transport: T) -> Self {
        Self {
            api_key,
            model: model.into(),
            max_tokens: 4096,
            enable_caching: true,
            transport,
        }
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }

    pub fn without_caching(mut self) -> Self {
        self.enable_caching = false;
        self
    }

    pub(crate) fn build_request(&self, d: &Diagnostic, file: &str) -> MessagesRequest {
        let cache = if self.enable_caching {
            Some(CacheControl { kind: "ephemeral".into() })
        } else {
            None
        };
        // System prompt is stable across iterations — cache it.
        let system = vec![SystemBlock {
            kind: "text".into(),
            text: SYSTEM_PROMPT.to_string(),
            cache_control: cache.clone(),
        }];
        // File content is stable across iterations within a repair
        // session — cache it. Diagnostic is the only per-request
        // payload.
        let file_block = UserBlock {
            kind: "text".into(),
            text: format!("FILE (lean):\n```lean\n{}\n```", file),
            cache_control: cache,
        };
        let diag_block = UserBlock {
            kind: "text".into(),
            text: format!(
                "DIAGNOSTIC:\n  severity: {:?}\n  range (0-indexed, LSP convention): line {}, col {} -- line {}, col {}\n  message: {}\n\nPropose ONE minimal patch as a single JSON object with keys: start_line, start_char, end_line, end_char (ALL 0-indexed, LSP convention — the FIRST line of the file is line 0), new_text, rationale.\n\nPatch semantics: the substring of the file from start_line:start_char up to (but not including) end_line:end_char is replaced by new_text. To insert without deleting, set end == start. The patch is applied verbatim — extra context in new_text will be duplicated.\n\nDo NOT use sorry/admit/axiom — the policy gate will reject those. Respond with ONLY the JSON object, no prose, no markdown fences.",
                d.severity,
                d.range.start.line, d.range.start.character,
                d.range.end.line, d.range.end.character,
                d.message,
            ),
            cache_control: None,
        };
        MessagesRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            system: Some(system),
            messages: vec![Message {
                role: "user".into(),
                content: vec![file_block, diag_block],
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
            start: Position { line: p.start_line, character: p.start_char },
            end: Position { line: p.end_line, character: p.end_char },
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
/// the prompt + parsing path runs end-to-end without an API key.
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
                start: Position { line: 5, character: 12 },
                end: Position { line: 5, character: 18 },
            },
            severity: Severity::Error,
            message: "unsolved goals".into(),
            source: Some("lean".into()),
        }
    }

    #[test]
    fn build_request_uses_two_content_blocks_for_caching() {
        let s = AnthropicStrategy::new(
            "k".into(),
            "claude-opus-4-7",
            MockTransport::declines(),
        );
        let req = s.build_request(&d(), "theorem t : True := by sorry");

        assert_eq!(req.model, "claude-opus-4-7");
        assert_eq!(req.max_tokens, 4096);

        // System prompt is cached.
        let sys = req.system.as_ref().expect("system must be set");
        assert_eq!(sys.len(), 1);
        assert!(sys[0].text.contains("MUST NOT use sorry"));
        assert!(sys[0].cache_control.is_some(), "system block must be cached");

        // Two user blocks: file (cached) + diagnostic (not cached).
        assert_eq!(req.messages.len(), 1);
        let blocks = &req.messages[0].content;
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].text.contains("theorem t : True := by sorry"));
        assert!(blocks[0].cache_control.is_some(), "file block must be cached");
        assert!(blocks[1].text.contains("unsolved goals"));
        assert!(blocks[1].text.contains("line 5, col 12"));
        assert!(blocks[1].cache_control.is_none(), "diagnostic block must NOT be cached (changes per iteration)");
    }

    #[test]
    fn without_caching_disables_cache_control_on_all_blocks() {
        let s = AnthropicStrategy::new(
            "k".into(),
            "claude-opus-4-7",
            MockTransport::declines(),
        ).without_caching();
        let req = s.build_request(&d(), "file");
        let sys = req.system.as_ref().unwrap();
        assert!(sys[0].cache_control.is_none());
        for b in &req.messages[0].content {
            assert!(b.cache_control.is_none(), "no block should be cached when caching disabled");
        }
    }

    #[test]
    fn with_max_tokens_overrides_default() {
        let s = AnthropicStrategy::new("k".into(), "claude-opus-4-7", MockTransport::declines())
            .with_max_tokens(1024);
        assert_eq!(s.max_tokens, 1024);
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
            stop_reason: None,
            usage: None,
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
            stop_reason: None,
            usage: None,
        };
        let p = parse_response_into_patch(&response).expect("must parse");
        assert_eq!(p.new_text, "x");
        assert_eq!(p.rationale, "anthropic-proposed");
    }

    #[test]
    fn parse_response_returns_none_on_empty_object() {
        let response = MessagesResponse {
            content: vec![ContentBlock {
                kind: "text".into(),
                text: Some("{}".into()),
            }],
            stop_reason: None,
            usage: None,
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
            stop_reason: None,
            usage: None,
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

    #[test]
    fn request_serializes_with_cache_control_on_marked_blocks() {
        let s = AnthropicStrategy::new("k".into(), "claude-opus-4-7", MockTransport::declines());
        let req = s.build_request(&d(), "file");
        let json = serde_json::to_string(&req).unwrap();
        // The cached blocks include "cache_control":{"type":"ephemeral"}
        assert!(json.contains("\"cache_control\":{\"type\":\"ephemeral\"}"),
                "serialized request must include cache_control marker");
        // The unmarked diagnostic block must NOT include cache_control.
        // (Hard to assert directly without parsing; checked by inspecting
        // count of cache_control occurrences = 2 — system + file.)
        let count = json.matches("\"cache_control\"").count();
        assert_eq!(count, 2, "expected exactly 2 cache_control markers (system + file); got {count}");
    }
}
