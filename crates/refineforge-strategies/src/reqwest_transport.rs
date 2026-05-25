//! Real HTTP transport for [`AnthropicStrategy`](crate::anthropic::AnthropicStrategy).
//!
//! Features:
//!
//! - Blocking `reqwest` client with `rustls-tls` (no OpenSSL).
//! - Retry-with-exponential-backoff for HTTP 429 (rate limit) and
//!   5xx (server error) responses. Default: up to 3 retries with
//!   1s, 2s, 4s delays.
//! - Distinct error reporting for auth (401/403), bad-request (400),
//!   and other non-success statuses (no retry).
//! - Configurable base URL so unit tests can point at a local
//!   `tiny_http` stub server.
//! - Prompt-caching beta header always sent. The strategy decides
//!   whether to mark blocks `cache_control: ephemeral`; the
//!   transport just speaks HTTP.

use anyhow::{anyhow, Context, Result};
use std::time::Duration;

use crate::anthropic::{AnthropicTransport, MessagesRequest, MessagesResponse};

/// Real HTTP transport. `new(api_key)` returns a client ready to
/// hit the real Anthropic API; use [`Self::with_base_url`] in tests.
pub struct ReqwestTransport {
    api_key: String,
    base_url: String,
    client: reqwest::blocking::Client,
    max_retries: u32,
    /// Base for exponential backoff in milliseconds. Default 1000.
    /// Tests set this very low (1) so retry tests don't sleep.
    backoff_base_ms: u64,
}

impl ReqwestTransport {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.anthropic.com".to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("static reqwest client config must build"),
            max_retries: 3,
            backoff_base_ms: 1000,
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn with_max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    pub fn with_backoff_base_ms(mut self, ms: u64) -> Self {
        self.backoff_base_ms = ms;
        self
    }

    fn backoff(&self, attempt: u32) -> Duration {
        // 1×, 2×, 4×, 8× the base.
        Duration::from_millis(self.backoff_base_ms.saturating_mul(1u64 << attempt))
    }
}

impl AnthropicTransport for ReqwestTransport {
    fn send(&self, request: &MessagesRequest) -> Result<MessagesResponse> {
        let url = format!("{}/v1/messages", self.base_url);
        let body = serde_json::to_string(request).context("serialize MessagesRequest")?;

        let mut last_transient: Option<String> = None;

        for attempt in 0..=self.max_retries {
            let result = self
                .client
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", "prompt-caching-2024-07-31")
                .header("content-type", "application/json")
                .body(body.clone())
                .send();

            match result {
                Ok(resp) => {
                    let status = resp.status();
                    let code = status.as_u16();

                    if status.is_success() {
                        let parsed: MessagesResponse = resp
                            .json()
                            .context("parse Anthropic /v1/messages response")?;
                        return Ok(parsed);
                    }

                    // Read body for the error message (best-effort).
                    let text = resp.text().unwrap_or_default();

                    match code {
                        400 => {
                            return Err(anyhow!("Anthropic bad request (HTTP 400): {text}"));
                        }
                        401 | 403 => {
                            return Err(anyhow!(
                                "Anthropic auth error (HTTP {code}). Check ANTHROPIC_API_KEY. Body: {text}"
                            ));
                        }
                        413 => {
                            return Err(anyhow!(
                                "Anthropic request too large (HTTP 413). Try a smaller file or fewer cache blocks. Body: {text}"
                            ));
                        }
                        404 => {
                            return Err(anyhow!(
                                "Anthropic 404 — model name '{}' likely wrong, or wrong base URL '{}'. Body: {text}",
                                request.model, self.base_url
                            ));
                        }
                        429 | 500..=599 => {
                            last_transient = Some(format!("HTTP {code}: {text}"));
                            if attempt < self.max_retries {
                                std::thread::sleep(self.backoff(attempt));
                                continue;
                            }
                            return Err(anyhow!(
                                "Anthropic transient error after {} retries — {}",
                                self.max_retries,
                                last_transient.as_deref().unwrap_or("(no detail)")
                            ));
                        }
                        _ => {
                            return Err(anyhow!("Anthropic unexpected HTTP {code}: {text}"));
                        }
                    }
                }
                Err(e) => {
                    // Network-level errors: retry timeouts and connect
                    // errors; surface everything else immediately.
                    if (e.is_timeout() || e.is_connect()) && attempt < self.max_retries {
                        last_transient = Some(format!("network: {e}"));
                        std::thread::sleep(self.backoff(attempt));
                        continue;
                    }
                    return Err(anyhow!("reqwest error: {e}"));
                }
            }
        }

        // Loop fall-through shouldn't happen — every branch either
        // returns or continues.
        Err(anyhow!(
            "exhausted {} retries; last error: {}",
            self.max_retries,
            last_transient.as_deref().unwrap_or("(unknown)")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::{AnthropicStrategy, MockTransport};
    use refineforge_repair_api::{Diagnostic, Position, Range, Severity};
    use std::net::TcpListener;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::thread::{self, JoinHandle};

    struct StubResponse {
        status: u16,
        body: String,
    }

    /// Start a tiny_http server on an ephemeral port. Returns the
    /// base URL and a handle that joins when N requests are served.
    /// Handler is called with the 0-indexed request number.
    fn start_stub_server<F>(num_requests: usize, handler: F) -> (String, JoinHandle<()>)
    where
        F: Fn(usize) -> StubResponse + Send + 'static,
    {
        // Bind first to grab a port, then hand the listener to tiny_http.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("local_addr").port();
        drop(listener); // release; tiny_http will rebind

        let server = tiny_http::Server::http(format!("127.0.0.1:{port}")).expect("tiny_http bind");
        let url = format!("http://127.0.0.1:{port}");

        let handle = thread::spawn(move || {
            for (i, request) in server.incoming_requests().take(num_requests).enumerate() {
                let stub = handler(i);
                let resp =
                    tiny_http::Response::from_string(stub.body).with_status_code(stub.status);
                let _ = request.respond(resp);
            }
        });

        (url, handle)
    }

    fn make_request() -> MessagesRequest {
        let s = AnthropicStrategy::new("k".into(), "claude-opus-4-7", MockTransport::declines());
        s.build_request(
            &Diagnostic {
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
            },
            "file",
        )
    }

    const SUCCESS_BODY: &str = r#"{"content":[{"type":"text","text":"{\"start_line\":0,\"start_char\":0,\"end_line\":0,\"end_char\":1,\"new_text\":\"x\",\"rationale\":\"y\"}"}],"stop_reason":"end_turn"}"#;

    #[test]
    fn returns_parsed_response_on_200() {
        let (url, handle) = start_stub_server(1, |_| StubResponse {
            status: 200,
            body: SUCCESS_BODY.into(),
        });
        let t = ReqwestTransport::new("k".into())
            .with_base_url(url)
            .with_max_retries(0);
        let resp = t.send(&make_request()).expect("must succeed");
        assert_eq!(resp.content[0].kind, "text");
        assert!(resp.content[0].text.as_ref().unwrap().contains("new_text"));
        handle.join().unwrap();
    }

    #[test]
    fn auth_error_does_not_retry_and_surfaces_helpfully() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();
        let (url, handle) = start_stub_server(1, move |_| {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            StubResponse {
                status: 401,
                body: r#"{"error":{"type":"authentication_error","message":"invalid key"}}"#.into(),
            }
        });
        let t = ReqwestTransport::new("bad-key".into())
            .with_base_url(url)
            .with_max_retries(3)
            .with_backoff_base_ms(1);
        let err = t.send(&make_request()).expect_err("must error");
        let msg = err.to_string();
        assert!(msg.contains("HTTP 401"), "msg: {msg}");
        assert!(
            msg.contains("ANTHROPIC_API_KEY"),
            "msg should hint at env var: {msg}"
        );
        // Must NOT have retried.
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        handle.join().unwrap();
    }

    #[test]
    fn rate_limit_retries_then_succeeds() {
        let (url, handle) = start_stub_server(2, |i| {
            if i == 0 {
                StubResponse {
                    status: 429,
                    body: "{\"error\":\"rate_limited\"}".into(),
                }
            } else {
                StubResponse {
                    status: 200,
                    body: SUCCESS_BODY.into(),
                }
            }
        });
        let t = ReqwestTransport::new("k".into())
            .with_base_url(url)
            .with_max_retries(3)
            .with_backoff_base_ms(1);
        let resp = t.send(&make_request()).expect("must eventually succeed");
        assert_eq!(resp.content.len(), 1);
        handle.join().unwrap();
    }

    #[test]
    fn server_error_retries_then_succeeds() {
        let (url, handle) = start_stub_server(3, |i| {
            if i < 2 {
                StubResponse {
                    status: 503,
                    body: "service unavailable".into(),
                }
            } else {
                StubResponse {
                    status: 200,
                    body: SUCCESS_BODY.into(),
                }
            }
        });
        let t = ReqwestTransport::new("k".into())
            .with_base_url(url)
            .with_max_retries(3)
            .with_backoff_base_ms(1);
        let resp = t.send(&make_request()).expect("must eventually succeed");
        assert!(resp.content[0].text.as_ref().unwrap().contains("new_text"));
        handle.join().unwrap();
    }

    #[test]
    fn exhausted_retries_returns_error() {
        let (url, handle) = start_stub_server(4, |_| StubResponse {
            status: 503,
            body: "still broken".into(),
        });
        let t = ReqwestTransport::new("k".into())
            .with_base_url(url)
            .with_max_retries(3)
            .with_backoff_base_ms(1);
        let err = t.send(&make_request()).expect_err("must exhaust");
        let msg = err.to_string();
        assert!(
            msg.contains("transient error after 3 retries"),
            "msg: {msg}"
        );
        handle.join().unwrap();
    }

    #[test]
    fn bad_request_does_not_retry() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();
        let (url, handle) = start_stub_server(1, move |_| {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            StubResponse {
                status: 400,
                body: r#"{"error":"invalid_request"}"#.into(),
            }
        });
        let t = ReqwestTransport::new("k".into())
            .with_base_url(url)
            .with_max_retries(3)
            .with_backoff_base_ms(1);
        let err = t.send(&make_request()).expect_err("must error");
        assert!(err.to_string().contains("HTTP 400"));
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            1,
            "bad request must not retry"
        );
        handle.join().unwrap();
    }

    #[test]
    fn model_404_message_includes_model_name() {
        let (url, handle) = start_stub_server(1, |_| StubResponse {
            status: 404,
            body: r#"{"error":"not_found"}"#.into(),
        });
        let t = ReqwestTransport::new("k".into())
            .with_base_url(url)
            .with_max_retries(0);
        let err = t.send(&make_request()).expect_err("must error");
        let msg = err.to_string();
        assert!(msg.contains("404"), "msg: {msg}");
        assert!(
            msg.contains("claude-opus-4-7"),
            "msg should mention model: {msg}"
        );
        handle.join().unwrap();
    }

    #[test]
    fn request_headers_are_sent_correctly() {
        // Verify the request actually carries the auth + version headers.
        // We use a stub that records the headers it sees.
        let captured = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let captured_clone = captured.clone();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let server = tiny_http::Server::http(format!("127.0.0.1:{port}")).unwrap();
        let url = format!("http://127.0.0.1:{port}");

        let handle = thread::spawn(move || {
            for request in server.incoming_requests().take(1) {
                let mut headers: Vec<String> = request
                    .headers()
                    .iter()
                    .map(|h| format!("{}: {}", h.field, h.value))
                    .collect();
                headers.sort();
                captured_clone.lock().unwrap().extend(headers);
                let _ = request
                    .respond(tiny_http::Response::from_string(SUCCESS_BODY).with_status_code(200));
            }
        });

        let t = ReqwestTransport::new("my-test-key".into())
            .with_base_url(url)
            .with_max_retries(0);
        let _ = t.send(&make_request()).unwrap();
        handle.join().unwrap();

        let seen = captured.lock().unwrap().join("\n").to_lowercase();
        assert!(
            seen.contains("x-api-key: my-test-key"),
            "missing api key: {seen}"
        );
        assert!(
            seen.contains("anthropic-version: 2023-06-01"),
            "missing version: {seen}"
        );
        assert!(
            seen.contains("anthropic-beta: prompt-caching-2024-07-31"),
            "missing beta header: {seen}"
        );
    }
}
