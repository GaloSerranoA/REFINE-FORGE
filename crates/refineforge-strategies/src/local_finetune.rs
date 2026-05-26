//! Local fine-tuned model strategy.
//!
//! The trained checkpoint is not in this repository. This module
//! wires the `RepairStrategy` surface to a local runtime manifest in
//! the weights directory:
//!
//! ```json
//! {
//!   "runtime": "command",
//!   "model_id": "qwen-proof-repair-v1",
//!   "command": ["path/to/infer-once", "--weights", "..."]
//! }
//! ```
//!
//! The command receives a single JSON request on stdin and writes
//! either a raw patch object or an envelope containing `patch`,
//! `usage`, and `stop_reason` on stdout. A native candle backend can
//! replace the command runtime behind the same strategy contract once
//! real checkpoint artifacts exist.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use refineforge_repair_api::proof_graph::{
    self, LeanTextHeuristicExtractor, PromptTemplate, ProofGraphExtractor,
};
use refineforge_repair_api::{Diagnostic, Patch, Position, Range, RepairStrategy};

use crate::{anthropic::UsageStats, StrategyWithUsage};

const MANIFEST_NAME: &str = "refineforge-local-finetune.json";

#[derive(Debug, Clone, Deserialize)]
struct RuntimeManifest {
    runtime: String,
    #[serde(default)]
    model_id: Option<String>,
    command: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ModelRequest<'a> {
    weights_path: &'a str,
    model_id: Option<&'a str>,
    diagnostic: &'a Diagnostic,
    file_content: &'a str,
    /// When the operator selected a prompt template, the strategy
    /// renders it against a ProofState and ships the rendered text
    /// here. Runtime commands that understand the field use it
    /// directly; older runtimes that don't can ignore it and fall
    /// back to constructing their own prompt from `diagnostic` +
    /// `file_content`. Always serialized (even when `None`) so
    /// runtime adapters can branch on `prompt === null`.
    #[serde(default)]
    prompt: Option<&'a str>,
    /// The template's expected output format, if any. Lets a runtime
    /// know whether to emit a Patch JSON object or a single Lean
    /// tactic. Same null-safe contract as `prompt`.
    #[serde(default)]
    expected_output_format: Option<&'a str>,
    /// Stable id of the chosen template for audit logging.
    #[serde(default)]
    template_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct PatchJson {
    start_line: u32,
    start_char: u32,
    end_line: u32,
    end_char: u32,
    new_text: String,
    #[serde(default)]
    rationale: String,
}

impl PatchJson {
    fn into_patch(self) -> Patch {
        Patch {
            range: Range {
                start: Position {
                    line: self.start_line,
                    character: self.start_char,
                },
                end: Position {
                    line: self.end_line,
                    character: self.end_char,
                },
            },
            new_text: self.new_text,
            rationale: if self.rationale.is_empty() {
                "local-finetune-proposed".into()
            } else {
                self.rationale
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LocalUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Debug)]
struct ModelOutput {
    patch: Option<Patch>,
    usage: Option<LocalUsage>,
    stop_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct CommandRuntime {
    weights_path: PathBuf,
    model_id: Option<String>,
    command: Vec<String>,
}

impl CommandRuntime {
    fn from_weights_path(weights_path: &Path) -> Result<Self> {
        if !weights_path.exists() {
            return Err(anyhow!(
                "local-finetune weights path does not exist: {}",
                weights_path.display()
            ));
        }
        if !weights_path.is_dir() {
            return Err(anyhow!(
                "local-finetune weights path must be a directory containing {MANIFEST_NAME}: {}",
                weights_path.display()
            ));
        }

        let manifest_path = weights_path.join(MANIFEST_NAME);
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let manifest: RuntimeManifest = serde_json::from_str(&manifest_text)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;
        if manifest.runtime != "command" {
            return Err(anyhow!(
                "unsupported local-finetune runtime {:?}; this build supports runtime=\"command\" until the candle backend lands",
                manifest.runtime
            ));
        }
        if manifest.command.is_empty() {
            return Err(anyhow!(
                "{MANIFEST_NAME} must provide a non-empty command array"
            ));
        }

        Ok(Self {
            weights_path: weights_path.to_path_buf(),
            model_id: manifest.model_id,
            command: manifest.command,
        })
    }

    fn infer(
        &self,
        diagnostic: &Diagnostic,
        file_content: &str,
        proof_template: Option<&PromptTemplate>,
    ) -> Result<ModelOutput> {
        let (program, args) = self
            .command
            .split_first()
            .ok_or_else(|| anyhow!("local-finetune command is empty"))?;

        // Render the chosen template against a heuristic ProofState
        // when set. On render failure (e.g., template needs hypotheses
        // the heuristic can't supply) the rendered prompt is omitted
        // and the runtime falls back to its native prompt path.
        let rendered = proof_template.and_then(|t| {
            let state = LeanTextHeuristicExtractor::default()
                .extract(file_content, diagnostic)
                .ok()?;
            proof_graph::render(t, &state).ok()
        });
        let weights_path_str = self.weights_path.display().to_string();
        let format_str = proof_template.map(|t| match t.expected_output_format {
            proof_graph::OutputFormat::PatchJson => "patch_json",
            proof_graph::OutputFormat::SingleTactic => "single_tactic",
            proof_graph::OutputFormat::FreeForm => "free_form",
            proof_graph::OutputFormat::VerifierVerdict => "verifier_verdict",
        });
        let request = ModelRequest {
            weights_path: &weights_path_str,
            model_id: self.model_id.as_deref(),
            diagnostic,
            file_content,
            prompt: rendered.as_deref(),
            expected_output_format: format_str,
            template_id: proof_template.map(|t| t.id.as_str()),
        };
        let request_json = serde_json::to_vec(&request)?;

        let mut child = Command::new(program)
            .args(args)
            .env("REFINEFORGE_LOCAL_FINETUNE_WEIGHTS", &self.weights_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawning local-finetune runtime `{program}`"))?;

        if let Some(mut stdin) = child.stdin.take() {
            match stdin.write_all(&request_json) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::BrokenPipe => {}
                Err(e) => return Err(e).context("writing local-finetune request to stdin"),
            }
        }

        let output = child
            .wait_with_output()
            .context("waiting for local-finetune runtime")?;
        if !output.status.success() {
            return Err(anyhow!(
                "local-finetune runtime exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let stdout = String::from_utf8(output.stdout)
            .context("local-finetune runtime wrote non-UTF8 stdout")?;
        parse_model_output(&stdout)
    }
}

/// `RepairStrategy` implementation for a locally hosted fine-tuned
/// proof-repair model.
pub struct LocalFinetuneStrategy {
    runtime: CommandRuntime,
    usage_stats: Arc<Mutex<UsageStats>>,
    /// Optional prompt template — renders against a heuristic
    /// ProofState and ships the result in `ModelRequest.prompt`.
    /// Default `None` preserves the old runtime-contract behaviour.
    proof_template: Option<PromptTemplate>,
}

impl LocalFinetuneStrategy {
    fn new(runtime: CommandRuntime, usage_stats: Arc<Mutex<UsageStats>>) -> Self {
        Self {
            runtime,
            usage_stats,
            proof_template: None,
        }
    }

    /// Configure the strategy to ship a rendered prompt (via
    /// `proof_graph::render`) alongside the diagnostic on every
    /// inference. Compatible with any
    /// [`OutputFormat`](refineforge_repair_api::proof_graph::OutputFormat):
    /// the runtime command receives the format and emits the
    /// appropriate output shape (Patch JSON or single tactic — the
    /// single-tactic case is converted to a Patch by a future
    /// `SingleTacticPatchAdapter` in this crate; today the runtime
    /// must emit Patch JSON itself).
    pub fn with_proof_template(mut self, template: PromptTemplate) -> Self {
        self.proof_template = Some(template);
        self
    }
}

impl RepairStrategy for LocalFinetuneStrategy {
    fn propose_patch(&self, diagnostic: &Diagnostic, file_content: &str) -> Result<Option<Patch>> {
        let output = self
            .runtime
            .infer(diagnostic, file_content, self.proof_template.as_ref())?;
        if let Ok(mut stats) = self.usage_stats.lock() {
            stats.calls += 1;
            if let Some(u) = &output.usage {
                stats.input_tokens += u.input_tokens as u64;
                stats.output_tokens += u.output_tokens as u64;
            }
            stats.record_stop_reason(output.stop_reason.clone());
        }
        Ok(output.patch)
    }

    fn name(&self) -> &'static str {
        "local-finetune"
    }
}

/// Build the local fine-tune strategy from a weights/runtime
/// directory.
pub fn local_finetune_from_path(path: impl AsRef<Path>) -> Result<Box<dyn RepairStrategy>> {
    local_finetune_from_path_with_usage(path).map(|(strategy, _)| strategy)
}

/// Like [`local_finetune_from_path`] but also returns the local usage
/// accumulator.
pub fn local_finetune_from_path_with_usage(path: impl AsRef<Path>) -> Result<StrategyWithUsage> {
    let runtime = CommandRuntime::from_weights_path(path.as_ref())?;
    let usage = Arc::new(Mutex::new(UsageStats::default()));
    let strategy = LocalFinetuneStrategy::new(runtime, usage.clone());
    Ok((Box::new(strategy), usage))
}

/// Typed constructor that returns a concrete [`LocalFinetuneStrategy`]
/// with `proof_template` already applied. The CLI uses this when
/// `--prompt-template` is set so the builder method is reachable; for
/// trait-object construction, prefer [`local_finetune_from_path`].
pub fn local_finetune_typed_with_template(
    path: impl AsRef<Path>,
    template: PromptTemplate,
) -> Result<LocalFinetuneStrategy> {
    let runtime = CommandRuntime::from_weights_path(path.as_ref())?;
    let usage = Arc::new(Mutex::new(UsageStats::default()));
    Ok(LocalFinetuneStrategy::new(runtime, usage).with_proof_template(template))
}

fn parse_model_output(stdout: &str) -> Result<ModelOutput> {
    let trimmed = strip_markdown_fences(stdout.trim());
    if trimmed.is_empty() {
        return Ok(ModelOutput {
            patch: None,
            usage: None,
            stop_reason: None,
        });
    }

    let value: Value = serde_json::from_str(trimmed)
        .with_context(|| format!("parsing local-finetune stdout as JSON: {trimmed}"))?;
    let usage = value
        .get("usage")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .context("parsing local-finetune usage")?;
    let stop_reason = value
        .get("stop_reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let patch_value = if value.get("patch").is_some() {
        value.get("patch").cloned().unwrap_or(Value::Null)
    } else {
        value
    };

    if patch_value.is_null()
        || patch_value
            .as_object()
            .map(|obj| obj.is_empty())
            .unwrap_or(false)
    {
        return Ok(ModelOutput {
            patch: None,
            usage,
            stop_reason,
        });
    }

    let patch = serde_json::from_value::<PatchJson>(patch_value)
        .context("parsing local-finetune patch")?
        .into_patch();
    Ok(ModelOutput {
        patch: Some(patch),
        usage,
        stop_reason,
    })
}

fn strip_markdown_fences(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix("```json").unwrap_or(s);
    let s = s.strip_prefix("```").unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s);
    s.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_patch_json() {
        let out = parse_model_output(
            r#"{"start_line":1,"start_char":2,"end_line":3,"end_char":4,"new_text":"exact h","rationale":""}"#,
        )
        .unwrap();
        let patch = out.patch.expect("patch");
        assert_eq!(patch.range.start.line, 1);
        assert_eq!(patch.new_text, "exact h");
        assert_eq!(patch.rationale, "local-finetune-proposed");
    }

    #[test]
    fn parses_empty_object_as_decline() {
        let out = parse_model_output("{}").unwrap();
        assert!(out.patch.is_none());
    }
}
