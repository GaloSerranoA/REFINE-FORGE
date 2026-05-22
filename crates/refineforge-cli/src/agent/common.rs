use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Lean,
    Devops,
    Train,
    Kernel,
    RunAll,
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Lean => "lean",
            AgentKind::Devops => "devops",
            AgentKind::Train => "train",
            AgentKind::Kernel => "kernel",
            AgentKind::RunAll => "run_all",
        }
    }

    pub fn report_stem(self) -> &'static str {
        match self {
            AgentKind::RunAll => "summary",
            _ => self.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    Inspect,
    Check,
    Repair,
    Execute,
}

impl AgentMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentMode::Inspect => "inspect",
            AgentMode::Check => "check",
            AgentMode::Repair => "repair",
            AgentMode::Execute => "execute",
        }
    }

    pub fn runs_checks(self) -> bool {
        matches!(
            self,
            AgentMode::Check | AgentMode::Repair | AgentMode::Execute
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Passed,
    Failed,
    Blocked,
    Partial,
}

impl AgentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentStatus::Passed => "passed",
            AgentStatus::Failed => "failed",
            AgentStatus::Blocked => "blocked",
            AgentStatus::Partial => "partial",
        }
    }

    pub fn is_success(self) -> bool {
        matches!(self, AgentStatus::Passed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrustLevel {
    Blocked,
    MeasuredOnly,
    ModelOnly,
    ModelLinked,
    ReleaseReadyLocal,
    ReleaseReadyCi,
    HumanReviewed,
}

impl TrustLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            TrustLevel::Blocked => "blocked",
            TrustLevel::MeasuredOnly => "measured-only",
            TrustLevel::ModelOnly => "model-only",
            TrustLevel::ModelLinked => "model-linked",
            TrustLevel::ReleaseReadyLocal => "release-ready-local",
            TrustLevel::ReleaseReadyCi => "release-ready-ci",
            TrustLevel::HumanReviewed => "human-reviewed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    pub name: String,
    pub command: Vec<String>,
    pub status: AgentStatus,
    pub duration_ms: u128,
    pub exit_code: Option<i32>,
    pub stdout_tail: Option<String>,
    pub stderr_tail: Option<String>,
}

impl CommandRecord {
    pub fn internal(name: &str, command: &[&str], started: Instant, result: &Result<()>) -> Self {
        Self {
            name: name.to_string(),
            command: command.iter().map(|s| s.to_string()).collect(),
            status: if result.is_ok() {
                AgentStatus::Passed
            } else {
                AgentStatus::Failed
            },
            duration_ms: started.elapsed().as_millis(),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: result.as_ref().err().map(|e| e.to_string()),
        }
    }

    pub fn internal_owned(
        name: &str,
        command: Vec<String>,
        started: Instant,
        result: &Result<()>,
    ) -> Self {
        Self {
            name: name.to_string(),
            command,
            status: if result.is_ok() {
                AgentStatus::Passed
            } else {
                AgentStatus::Failed
            },
            duration_ms: started.elapsed().as_millis(),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: result.as_ref().err().map(|e| e.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessRecord {
    pub state: String,
    pub checked_at: DateTime<Utc>,
    pub agent: AgentKind,
    pub mode: AgentMode,
    pub target: String,
    pub command_surface: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRecord {
    pub name: String,
    pub status: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCheck {
    pub name: String,
    pub required: bool,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReport {
    pub schema_version: String,
    pub agent: AgentKind,
    pub mode: AgentMode,
    pub target: String,
    pub liveness: LivenessRecord,
    pub capabilities: Vec<CapabilityRecord>,
    pub tool_checks: Vec<ToolCheck>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub status: AgentStatus,
    pub trust_level: TrustLevel,
    pub commands: Vec<CommandRecord>,
    pub changed_files: Vec<PathBuf>,
    pub artifacts: Vec<PathBuf>,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub summary: String,
}

impl AgentReport {
    pub fn new(agent: AgentKind, mode: AgentMode, target: impl Into<String>) -> Self {
        let now = Utc::now();
        let target = target.into();
        Self {
            schema_version: "agent-report-v1".to_string(),
            agent,
            mode,
            target: target.clone(),
            liveness: LivenessRecord {
                state: "alive".to_string(),
                checked_at: now,
                agent,
                mode,
                target,
                command_surface: format!("refine agent {}", agent.as_str()),
            },
            capabilities: Vec::new(),
            tool_checks: Vec::new(),
            started_at: now,
            finished_at: now,
            status: AgentStatus::Passed,
            trust_level: TrustLevel::MeasuredOnly,
            commands: Vec::new(),
            changed_files: Vec::new(),
            artifacts: Vec::new(),
            blockers: Vec::new(),
            warnings: Vec::new(),
            summary: String::new(),
        }
    }

    pub fn finish(
        &mut self,
        status: AgentStatus,
        trust_level: TrustLevel,
        summary: impl Into<String>,
    ) {
        self.finished_at = Utc::now();
        self.status = status;
        self.trust_level = trust_level;
        self.summary = summary.into();
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {} agent report\n\n", self.agent.as_str()));
        out.push_str(&format!("- Schema: `{}`\n", self.schema_version));
        out.push_str(&format!("- Mode: `{}`\n", self.mode.as_str()));
        out.push_str(&format!("- Target: `{}`\n", self.target));
        out.push_str(&format!("- Liveness: `{}`\n", self.liveness.state));
        out.push_str(&format!("- Status: `{}`\n", self.status.as_str()));
        out.push_str(&format!("- Trust level: `{}`\n", self.trust_level.as_str()));
        out.push_str(&format!("- Started: `{}`\n", self.started_at.to_rfc3339()));
        out.push_str(&format!(
            "- Finished: `{}`\n\n",
            self.finished_at.to_rfc3339()
        ));
        out.push_str("## Summary\n\n");
        out.push_str(&self.summary);
        out.push_str("\n\n");
        out.push_str("## Capabilities\n\n");
        if self.capabilities.is_empty() {
            out.push_str("- None recorded.\n\n");
        } else {
            for capability in &self.capabilities {
                out.push_str(&format!(
                    "- `{}`: `{}` — {}\n",
                    capability.name, capability.status, capability.evidence
                ));
            }
            out.push('\n');
        }
        out.push_str("## Tool Checks\n\n");
        if self.tool_checks.is_empty() {
            out.push_str("- None recorded.\n\n");
        } else {
            for check in &self.tool_checks {
                out.push_str(&format!(
                    "- `{}`: `{}` (required={}) — {}\n",
                    check.name, check.status, check.required, check.detail
                ));
            }
            out.push('\n');
        }
        write_list(&mut out, "Artifacts", &self.artifacts);
        write_string_list(&mut out, "Blockers", &self.blockers);
        write_string_list(&mut out, "Warnings", &self.warnings);
        out.push_str("## Commands\n\n");
        if self.commands.is_empty() {
            out.push_str("- None recorded.\n");
        } else {
            for command in &self.commands {
                out.push_str(&format!(
                    "- `{}`: `{}` in {} ms\n",
                    command.name,
                    command.status.as_str(),
                    command.duration_ms
                ));
                out.push_str(&format!("  - argv: `{}`\n", command.command.join(" ")));
                if let Some(stderr) = &command.stderr_tail {
                    out.push_str(&format!("  - stderr: `{}`\n", stderr.replace('`', "'")));
                }
            }
        }
        out
    }
}

fn write_list(out: &mut String, title: &str, values: &[PathBuf]) {
    out.push_str(&format!("## {title}\n\n"));
    if values.is_empty() {
        out.push_str("- None recorded.\n\n");
    } else {
        for value in values {
            out.push_str(&format!("- `{}`\n", value.display()));
        }
        out.push('\n');
    }
}

fn write_string_list(out: &mut String, title: &str, values: &[String]) {
    out.push_str(&format!("## {title}\n\n"));
    if values.is_empty() {
        out.push_str("- None recorded.\n\n");
    } else {
        for value in values {
            out.push_str(&format!("- {value}\n"));
        }
        out.push('\n');
    }
}

pub fn capability(name: &str, status: &str, evidence: &str) -> CapabilityRecord {
    CapabilityRecord {
        name: name.to_string(),
        status: status.to_string(),
        evidence: evidence.to_string(),
    }
}

pub fn tool_check(name: &str, required: bool, status: &str, detail: &str) -> ToolCheck {
    ToolCheck {
        name: name.to_string(),
        required,
        status: status.to_string(),
        detail: detail.to_string(),
    }
}

pub fn repo_tool_check(root: &Path, tool: &str, required: bool) -> ToolCheck {
    if root.join("Cargo.toml").exists()
        && matches!(tool, "refine" | "refine-train" | "refine-bitexact")
    {
        return tool_check(
            tool,
            required,
            "available",
            "provided by the local Cargo workspace command surface",
        );
    }
    tool_check(
        tool,
        required,
        "not_checked",
        "external tool availability is environment-specific",
    )
}

pub fn write_reports(out_dir: &Path, stem: &str, report: &AgentReport) -> Result<()> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let json_path = out_dir.join(format!("{stem}.json"));
    let md_path = out_dir.join(format!("{stem}.md"));
    std::fs::write(&json_path, serde_json::to_vec_pretty(report)?)
        .with_context(|| format!("writing {}", json_path.display()))?;
    std::fs::write(&md_path, report.to_markdown())
        .with_context(|| format!("writing {}", md_path.display()))?;
    Ok(())
}

pub fn existing_artifact(root: &Path, rel: &str, report: &mut AgentReport) {
    let path = root.join(rel);
    if path.exists() {
        report.artifacts.push(PathBuf::from(rel));
    } else {
        report
            .warnings
            .push(format!("expected artifact is missing: {rel}"));
    }
}
