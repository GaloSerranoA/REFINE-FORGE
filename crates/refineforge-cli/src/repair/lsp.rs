//! Minimal Lean LSP client: spawn `lake env lean --server`, talk
//! JSON-RPC over stdio, collect `textDocument/publishDiagnostics`.
//!
//! Why hand-roll this instead of using a crate: the `lsp-types`
//! crate gives us message *types* but not framing or transport.
//! The Lean LSP server is a normal LSP server; the framing is
//! Content-Length-prefixed JSON-RPC 2.0.
//!
//! Threading model: a reader thread parses incoming messages and
//! pushes them to a `mpsc::channel`. The main thread sends requests
//! by writing to stdin, then drains the channel for the matching
//! response. Notifications (incl. diagnostics) are drained
//! separately by `collect_diagnostics`.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use super::diagnostic::Diagnostic;

/// Convert a filesystem path to a `file://` URI. ASCII-only; for
/// non-ASCII paths use the `url` crate (out of scope for skeleton).
pub fn path_to_uri(p: &Path) -> String {
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = abs.to_string_lossy().replace('\\', "/");
    let s = s.trim_start_matches('/');
    format!("file:///{s}")
}

pub struct LeanLspClient {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    _reader: JoinHandle<()>,
    next_id: i64,
    doc_version: i64,
}

impl LeanLspClient {
    /// Spawn `lake env lean --server` rooted at `lean_dir`.
    pub fn spawn(lean_dir: &Path) -> Result<Self> {
        let mut child = Command::new("lake")
            .arg("env")
            .arg("lean")
            .arg("--server")
            .current_dir(lean_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawning `lake env lean --server` (is elan / lake on PATH?)")?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("no stdin handle"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("no stdout handle"))?;

        let (tx, rx) = channel();
        let reader = thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            while let Ok(v) = read_message(&mut r) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            rx,
            _reader: reader,
            next_id: 1,
            doc_version: 1,
        })
    }

    fn send_request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        write_message(&mut self.stdin, &req)?;
        self.wait_for_response(id, timeout)
    }

    fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let n = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        write_message(&mut self.stdin, &n)
    }

    fn wait_for_response(&mut self, id: i64, timeout: Duration) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        while let Some(remaining) = checked_duration_until(deadline) {
            let msg = self
                .rx
                .recv_timeout(remaining)
                .map_err(|_| anyhow!("LSP response timeout (id={id}, method)"))?;
            if msg.get("id").and_then(|v| v.as_i64()) == Some(id) {
                if let Some(err) = msg.get("error") {
                    return Err(anyhow!("LSP error: {err}"));
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
            // not our response — discard (notifications go through
            // collect_diagnostics, not here)
        }
        Err(anyhow!("LSP response timeout (id={id})"))
    }

    pub fn initialize(&mut self) -> Result<()> {
        let params = json!({
            "processId": std::process::id(),
            "rootUri": Value::Null,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": { "relatedInformation": false }
                }
            },
        });
        let _ = self.send_request("initialize", params, Duration::from_secs(30))?;
        self.send_notification("initialized", json!({}))?;
        Ok(())
    }

    pub fn did_open(&mut self, uri: &str, text: &str) -> Result<()> {
        self.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "lean4",
                    "version": self.doc_version,
                    "text": text,
                }
            }),
        )
    }

    pub fn did_change(&mut self, uri: &str, new_text: &str) -> Result<()> {
        self.doc_version += 1;
        self.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": self.doc_version },
                "contentChanges": [{ "text": new_text }],
            }),
        )
    }

    /// Drain notifications until we collect a complete
    /// `publishDiagnostics` batch for the given URI, or hit timeout.
    /// Lean tends to send multiple diagnostic batches as it
    /// elaborates; we take the latest within a 500ms tail window
    /// after the first batch arrives.
    pub fn collect_diagnostics(&mut self, uri: &str, timeout: Duration) -> Result<Vec<Diagnostic>> {
        let deadline = Instant::now() + timeout;
        let tail = Duration::from_millis(500);
        let mut latest: Vec<Diagnostic> = Vec::new();
        let mut got_any = false;
        let mut last_event = Instant::now();
        while let Some(remaining) = checked_duration_until(deadline) {
            let wait = if got_any {
                tail.min(remaining)
            } else {
                remaining
            };
            match self.rx.recv_timeout(wait) {
                Ok(msg) => {
                    if msg.get("method").and_then(|v| v.as_str())
                        == Some("textDocument/publishDiagnostics")
                    {
                        let params = msg.get("params").cloned().unwrap_or(Value::Null);
                        let msg_uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                        if msg_uri == uri {
                            let diags_raw = params
                                .get("diagnostics")
                                .cloned()
                                .unwrap_or(Value::Array(vec![]));
                            let diags: Vec<lsp_types::Diagnostic> =
                                serde_json::from_value(diags_raw).unwrap_or_default();
                            latest = diags.into_iter().map(Diagnostic::from).collect();
                            got_any = true;
                            last_event = Instant::now();
                        }
                    }
                }
                Err(_) => {
                    if got_any && last_event.elapsed() >= tail {
                        break;
                    }
                }
            }
        }
        Ok(latest)
    }

    /// Query the Lean 4 LSP extension `$/lean/plainGoal` for the
    /// rendered proof state at `(line, character)` in `uri`.
    ///
    /// Returns `Ok(None)` when Lean has no goal at that position
    /// (the response's `goals` array is empty or the request returns
    /// `null`). Returns `Err` on LSP-level errors or timeout.
    ///
    /// The rendered text is the same format Lean prints into
    /// diagnostic messages — `name : type` lines followed by `⊢ goal`,
    /// optionally prefixed with a `case` clause. The proof_graph
    /// crate's `parse_goal_text` consumes this directly.
    pub fn plain_goal(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
        timeout: Duration,
    ) -> Result<Option<String>> {
        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
        });
        let result = self.send_request("$/lean/plainGoal", params, timeout)?;
        if result.is_null() {
            return Ok(None);
        }
        // Lean returns { "rendered": "...", "goals": [...] } in
        // modern versions. We prefer `rendered` (the human-readable
        // form) when present; fall back to the first goal in the
        // `goals` array.
        if let Some(s) = result.get("rendered").and_then(|v| v.as_str()) {
            if s.is_empty() {
                return Ok(None);
            }
            return Ok(Some(s.to_string()));
        }
        if let Some(arr) = result.get("goals").and_then(|v| v.as_array()) {
            if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                return Ok(Some(first.to_string()));
            }
        }
        Ok(None)
    }

    pub fn shutdown(mut self) -> Result<()> {
        let _ = self.send_request("shutdown", Value::Null, Duration::from_secs(5));
        let _ = self.send_notification("exit", Value::Null);
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }
}

fn checked_duration_until(deadline: Instant) -> Option<Duration> {
    let now = Instant::now();
    if now >= deadline {
        None
    } else {
        Some(deadline - now)
    }
}

fn write_message(w: &mut impl Write, msg: &Value) -> Result<()> {
    let body = serde_json::to_string(msg)?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(body.as_bytes())?;
    w.flush()?;
    Ok(())
}

fn read_message(r: &mut impl BufRead) -> Result<Value> {
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line)?;
        if n == 0 {
            return Err(anyhow!("LSP server closed stdout"));
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(v) = trimmed.strip_prefix("Content-Length:") {
            content_length = v.trim().parse().context("invalid Content-Length")?;
        }
        // Ignore other headers (e.g. Content-Type).
    }
    let mut buf = vec![0u8; content_length];
    r.read_exact(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn roundtrip_message_with_simple_payload() {
        let payload = json!({"jsonrpc": "2.0", "id": 1, "method": "test"});
        let mut buf = Vec::new();
        write_message(&mut buf, &payload).unwrap();
        // Header + body
        let s = String::from_utf8(buf.clone()).unwrap();
        assert!(s.starts_with("Content-Length: "));
        assert!(s.contains("\r\n\r\n"));

        let mut r = std::io::BufReader::new(Cursor::new(buf));
        let parsed = read_message(&mut r).unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn read_message_ignores_extra_headers() {
        let body = json!({"a": 1});
        let body_str = serde_json::to_string(&body).unwrap();
        let raw = format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body_str.len(),
            body_str
        );
        let mut r = std::io::BufReader::new(Cursor::new(raw.into_bytes()));
        let parsed = read_message(&mut r).unwrap();
        assert_eq!(parsed, body);
    }

    #[test]
    fn path_to_uri_uses_forward_slashes() {
        // On any platform the URI should use `/` separators after
        // `file:///`. We don't assert the full path because
        // canonicalize() may resolve to a non-existent file as the
        // input — we just check the prefix shape.
        let p = Path::new("nonexistent.txt");
        let uri = path_to_uri(p);
        assert!(uri.starts_with("file:///"));
        assert!(
            !uri.contains('\\'),
            "uri should not contain backslashes: {uri}"
        );
    }
}
