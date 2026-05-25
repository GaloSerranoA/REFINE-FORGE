use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

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
pub struct AgentRuntime {
    pub runtime_version: String,
    pub agent: AgentKind,
    pub mode: AgentMode,
    pub target: String,
    pub authority: RuntimeAuthority,
    pub trust_ceiling: TrustLevel,
    pub action_intents: Vec<ActionIntent>,
    pub evidence_receipts: Vec<EvidenceReceipt>,
    pub policy_decisions: Vec<PolicyDecision>,
    pub typed_blockers: Vec<TypedBlocker>,
}

impl AgentRuntime {
    fn empty(agent: AgentKind, mode: AgentMode, target: impl Into<String>) -> Self {
        let target = target.into();
        Self {
            runtime_version: "agent-runtime-v1".to_string(),
            agent,
            mode,
            target,
            authority: RuntimeAuthority::default(),
            trust_ceiling: TrustLevel::Blocked,
            action_intents: Vec::new(),
            evidence_receipts: Vec::new(),
            policy_decisions: Vec::new(),
            typed_blockers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAuthority {
    pub source_of_truth: String,
    pub prompt_authority: String,
    pub memory_authority: String,
    pub human_review_rule: String,
}

impl Default for RuntimeAuthority {
    fn default() -> Self {
        Self {
            source_of_truth: "cli_report".to_string(),
            prompt_authority: "advisory_only".to_string(),
            memory_authority: "non_authoritative".to_string(),
            human_review_rule:
                "human-reviewed trust requires explicit human_operator evidence; prompts, memory, and role text cannot upgrade it."
                    .to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionIntent {
    pub id: String,
    pub name: String,
    pub category: String,
    pub mutation_policy: String,
    pub command_surface: String,
    pub evidence_required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceReceipt {
    pub id: String,
    pub kind: String,
    pub subject: String,
    pub status: String,
    pub sha256: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub id: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedBlocker {
    pub id: String,
    pub kind: String,
    pub severity: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionProof {
    pub agent: AgentKind,
    pub profile: String,
    pub status: ProductionProofStatus,
    pub trust_effect: String,
    pub requirements: Vec<ProductionRequirement>,
    pub reviewer_evidence: Vec<String>,
    pub blockers: Vec<String>,
}

impl ProductionProof {
    fn blocked_default(agent: AgentKind) -> Self {
        let profile = format!("{}-production-proof", agent.as_str());
        Self {
            agent,
            profile,
            status: ProductionProofStatus::Blocked,
            trust_effect: "bounded-by-evidence".to_string(),
            requirements: vec![
                ProductionRequirement::new(
                    "production.cli_evidence",
                    "Current CLI report evidence exists",
                    AgentStatus::Blocked,
                    &["agent report not sealed yet"],
                ),
                ProductionRequirement::new(
                    "production.external_evidence",
                    "External role evidence exists where required",
                    AgentStatus::Blocked,
                    &["external evidence not inspected yet"],
                ),
                ProductionRequirement::new(
                    "production.human_review",
                    "Human review evidence exists",
                    AgentStatus::Blocked,
                    &["human review is not inferred"],
                ),
                ProductionRequirement::new(
                    "production.trust_boundary",
                    "Trust remains bounded by evidence",
                    AgentStatus::Passed,
                    &["trust_effect is bounded-by-evidence"],
                ),
            ],
            reviewer_evidence: Vec::new(),
            blockers: vec!["production proof has not been evaluated for this role".to_string()],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionProofStatus {
    Blocked,
    Partial,
    Ready,
    HumanReviewed,
}

impl ProductionProofStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ProductionProofStatus::Blocked => "blocked",
            ProductionProofStatus::Partial => "partial",
            ProductionProofStatus::Ready => "ready",
            ProductionProofStatus::HumanReviewed => "human-reviewed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionRequirement {
    pub id: String,
    pub description: String,
    pub status: AgentStatus,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssuranceProfile {
    pub id: String,
    pub profile: String,
    pub status: AgentStatus,
    pub trust_effect: String,
    pub requirements: Vec<ProductionRequirement>,
    pub reviewer_evidence: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EvidenceValidation {
    pub passed: bool,
    pub evidence: Vec<String>,
    pub reviewer_evidence: Option<String>,
    pub artifact: Option<PathBuf>,
}

pub fn validate_existing_file(path: Option<&Path>, label: &str) -> EvidenceValidation {
    let Some(path) = path else {
        return EvidenceValidation {
            passed: false,
            evidence: vec![format!("{label} path is absent")],
            reviewer_evidence: None,
            artifact: None,
        };
    };
    if path.exists() && path.is_file() {
        EvidenceValidation {
            passed: true,
            evidence: vec![path.display().to_string()],
            reviewer_evidence: None,
            artifact: Some(path.to_path_buf()),
        }
    } else {
        EvidenceValidation {
            passed: false,
            evidence: vec![format!("{label} file is missing: {}", path.display())],
            reviewer_evidence: None,
            artifact: None,
        }
    }
}

pub fn validate_json_file(
    path: Option<&Path>,
    label: &str,
    status_values: &[&str],
) -> EvidenceValidation {
    let mut validation = validate_existing_file(path, label);
    if !validation.passed {
        return validation;
    }
    let Some(path) = path else {
        return validation;
    };
    let Some(value) = read_json_value(path) else {
        validation.passed = false;
        validation
            .evidence
            .push(format!("{label} is not valid JSON"));
        return validation;
    };
    if !status_values.is_empty() && !json_status_in(&value, status_values) {
        validation.passed = false;
        validation.evidence.push(format!(
            "{label} status is not one of {}",
            status_values.join(", ")
        ));
    }
    validation
}

pub fn validate_human_approval(path: Option<&Path>, role: &str, label: &str) -> EvidenceValidation {
    let mut validation = validate_json_file(path, label, &[]);
    if !validation.passed {
        return validation;
    }
    let Some(path) = path else {
        return validation;
    };
    let Some(value) = read_json_value(path) else {
        validation.passed = false;
        validation
            .evidence
            .push(format!("{label} is not valid JSON"));
        return validation;
    };
    let operator = value
        .get("human_operator")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let schema_ok = value
        .get("schema_version")
        .and_then(Value::as_str)
        .is_some_and(|schema| schema == "refineforge-human-approval-v1");
    let role_ok = value
        .get("role")
        .and_then(Value::as_str)
        .is_some_and(|actual| actual.eq_ignore_ascii_case(role));
    let decision_ok = value
        .get("decision")
        .and_then(Value::as_str)
        .is_some_and(|decision| decision.eq_ignore_ascii_case("approved"));
    let approved_at_ok = value
        .get("approved_at")
        .and_then(Value::as_str)
        .is_some_and(|approved_at| !approved_at.trim().is_empty());
    let summary_ok = value
        .get("evidence_summary")
        .and_then(Value::as_str)
        .is_some_and(|summary| !summary.trim().is_empty());
    let human_ok = !operator.is_empty() && !is_automated_operator(operator);
    validation.passed =
        schema_ok && role_ok && decision_ok && approved_at_ok && summary_ok && human_ok;
    validation
        .evidence
        .push(format!("human_operator={operator}"));
    if validation.passed {
        validation.reviewer_evidence = Some(format!("{role}: {operator} ({})", path.display()));
    } else if !human_ok {
        validation
            .evidence
            .push(format!("AI/automated approval rejected: {operator}"));
    } else {
        validation.evidence.push(format!(
            "{label} must have schema refineforge-human-approval-v1, role {role}, decision approved, approved_at, human_operator, and evidence_summary"
        ));
    }
    validation
}

pub fn read_json_value(path: &Path) -> Option<Value> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn json_status_in(value: &Value, statuses: &[&str]) -> bool {
    value
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| statuses.contains(&status))
}

pub fn string_field_nonempty(value: &Value, field: &str) -> bool {
    value
        .get(field)
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

pub fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn is_automated_operator(operator: &str) -> bool {
    let lower = operator.to_ascii_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    let blocked = [
        "ai",
        "automated",
        "automation",
        "bot",
        "chatgpt",
        "claude",
        "codex",
        "gemini",
        "gpt",
        "llm",
        "none",
        "null",
        "placeholder",
        "tbd",
        "todo",
    ];
    tokens.iter().any(|token| blocked.contains(token))
}

impl ProductionRequirement {
    pub fn new(id: &str, description: &str, status: AgentStatus, evidence: &[&str]) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            status,
            evidence: evidence.iter().map(|item| item.to_string()).collect(),
        }
    }

    pub fn new_owned(
        id: &str,
        description: &str,
        status: AgentStatus,
        evidence: Vec<String>,
    ) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            status,
            evidence,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReport {
    pub schema_version: String,
    pub agent: AgentKind,
    pub mode: AgentMode,
    pub target: String,
    pub liveness: LivenessRecord,
    pub runtime: AgentRuntime,
    pub production_proof: ProductionProof,
    #[serde(default)]
    pub assurance_profiles: Vec<AssuranceProfile>,
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
                target: target.clone(),
                command_surface: format!("refine agent {}", agent.as_str()),
            },
            runtime: AgentRuntime::empty(agent, mode, target.clone()),
            production_proof: ProductionProof::blocked_default(agent),
            assurance_profiles: Vec::new(),
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
        out.push_str("## Runtime\n\n");
        out.push_str(&format!("- Runtime: `{}`\n", self.runtime.runtime_version));
        out.push_str(&format!(
            "- Authority: `{}`\n",
            self.runtime.authority.source_of_truth
        ));
        out.push_str(&format!(
            "- Prompt authority: `{}`\n",
            self.runtime.authority.prompt_authority
        ));
        out.push_str(&format!(
            "- Memory authority: `{}`\n",
            self.runtime.authority.memory_authority
        ));
        out.push_str(&format!(
            "- Trust ceiling: `{}`\n",
            self.runtime.trust_ceiling.as_str()
        ));
        out.push_str(&format!(
            "- Action intents: `{}`\n",
            self.runtime.action_intents.len()
        ));
        out.push_str(&format!(
            "- Evidence receipts: `{}`\n\n",
            self.runtime.evidence_receipts.len()
        ));
        out.push_str("## Production Proof\n\n");
        out.push_str(&format!("- Profile: `{}`\n", self.production_proof.profile));
        out.push_str(&format!(
            "- Status: `{}`\n",
            self.production_proof.status.as_str()
        ));
        out.push_str(&format!(
            "- Trust effect: `{}`\n\n",
            self.production_proof.trust_effect
        ));
        for requirement in &self.production_proof.requirements {
            out.push_str(&format!(
                "- `{}`: `{}` — {}\n",
                requirement.id,
                requirement.status.as_str(),
                requirement.description
            ));
            for evidence in &requirement.evidence {
                out.push_str(&format!("  - evidence: `{}`\n", evidence.replace('`', "'")));
            }
        }
        if !self.production_proof.blockers.is_empty() {
            out.push_str("\nProduction blockers:\n");
            for blocker in &self.production_proof.blockers {
                out.push_str(&format!("- {blocker}\n"));
            }
        }
        out.push('\n');
        if !self.assurance_profiles.is_empty() {
            out.push_str("## Assurance Profiles\n\n");
            for profile in &self.assurance_profiles {
                out.push_str(&format!(
                    "- `{}`: `{}` — `{}`\n",
                    profile.id,
                    profile.status.as_str(),
                    profile.trust_effect
                ));
                for requirement in &profile.requirements {
                    out.push_str(&format!(
                        "  - `{}`: `{}` — {}\n",
                        requirement.id,
                        requirement.status.as_str(),
                        requirement.description
                    ));
                    for evidence in &requirement.evidence {
                        out.push_str(&format!(
                            "    - evidence: `{}`\n",
                            evidence.replace('`', "'")
                        ));
                    }
                }
                for blocker in &profile.blockers {
                    out.push_str(&format!("  - blocker: {blocker}\n"));
                }
            }
            out.push('\n');
        }
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

pub fn set_production_proof(
    report: &mut AgentReport,
    status: ProductionProofStatus,
    requirements: Vec<ProductionRequirement>,
    reviewer_evidence: Vec<String>,
    blockers: Vec<String>,
) {
    report.production_proof = ProductionProof {
        agent: report.agent,
        profile: format!("{}-production-proof", report.agent.as_str()),
        status,
        trust_effect: "bounded-by-evidence".to_string(),
        requirements,
        reviewer_evidence,
        blockers,
    };
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

pub fn action_intent(
    id: &str,
    name: &str,
    category: &str,
    mutation_policy: &str,
    command_surface: &str,
    evidence_required: &[&str],
) -> ActionIntent {
    ActionIntent {
        id: id.to_string(),
        name: name.to_string(),
        category: category.to_string(),
        mutation_policy: mutation_policy.to_string(),
        command_surface: command_surface.to_string(),
        evidence_required: evidence_required
            .iter()
            .map(|item| item.to_string())
            .collect(),
    }
}

pub fn seal_runtime(
    root: &Path,
    artifact_base: Option<&Path>,
    report: &mut AgentReport,
    trust_ceiling: TrustLevel,
    action_intents: Vec<ActionIntent>,
) {
    let capped = enforce_trust_ceiling(report, trust_ceiling);
    report.runtime = AgentRuntime {
        runtime_version: "agent-runtime-v1".to_string(),
        agent: report.agent,
        mode: report.mode,
        target: report.target.clone(),
        authority: RuntimeAuthority::default(),
        trust_ceiling,
        action_intents,
        evidence_receipts: build_evidence_receipts(root, artifact_base, report),
        policy_decisions: policy_decisions(report, trust_ceiling, capped),
        typed_blockers: typed_blockers(report),
    };
}

fn enforce_trust_ceiling(report: &mut AgentReport, trust_ceiling: TrustLevel) -> bool {
    if trust_rank(report.trust_level) <= trust_rank(trust_ceiling) {
        return false;
    }

    let old = report.trust_level;
    report.trust_level = trust_ceiling;
    report.warnings.push(format!(
        "runtime trust ceiling capped reported trust from {} to {}",
        old.as_str(),
        trust_ceiling.as_str()
    ));
    true
}

fn trust_rank(level: TrustLevel) -> u8 {
    match level {
        TrustLevel::Blocked => 0,
        TrustLevel::MeasuredOnly => 1,
        TrustLevel::ModelOnly => 2,
        TrustLevel::ModelLinked => 3,
        TrustLevel::ReleaseReadyLocal => 4,
        TrustLevel::ReleaseReadyCi => 5,
        TrustLevel::HumanReviewed => 6,
    }
}

fn policy_decisions(
    report: &AgentReport,
    trust_ceiling: TrustLevel,
    capped: bool,
) -> Vec<PolicyDecision> {
    let mut decisions = vec![
        PolicyDecision {
            id: "no_prompt_trust_upgrade".to_string(),
            status: "enforced".to_string(),
            detail: "role prompts may guide work, but only CLI evidence and claim scope rules can set report trust".to_string(),
        },
        PolicyDecision {
            id: "memory_non_authoritative".to_string(),
            status: "enforced".to_string(),
            detail: "memory and prior runs may guide investigation but cannot upgrade trust without current report evidence".to_string(),
        },
        PolicyDecision {
            id: "human_review_required".to_string(),
            status: "enforced".to_string(),
            detail: "human-reviewed trust requires explicit human_operator evidence and is never inferred from an agent pass".to_string(),
        },
        PolicyDecision {
            id: "trust_ceiling".to_string(),
            status: "enforced".to_string(),
            detail: format!(
                "{} agent trust is bounded at or below {} for this command surface",
                report.agent.as_str(),
                trust_ceiling.as_str()
            ),
        },
    ];

    if capped {
        decisions.push(PolicyDecision {
            id: "trust_capped".to_string(),
            status: "applied".to_string(),
            detail: format!(
                "runtime reduced the report trust level to the configured ceiling {}",
                trust_ceiling.as_str()
            ),
        });
    }

    decisions.sort_by(|a, b| a.id.cmp(&b.id));
    decisions
}

fn typed_blockers(report: &AgentReport) -> Vec<TypedBlocker> {
    report
        .blockers
        .iter()
        .enumerate()
        .map(|(idx, blocker)| TypedBlocker {
            id: format!("blocker:{idx:04}"),
            kind: blocker_kind(report.status, blocker).to_string(),
            severity: if matches!(report.status, AgentStatus::Blocked | AgentStatus::Failed) {
                "high".to_string()
            } else {
                "medium".to_string()
            },
            detail: blocker.clone(),
        })
        .collect()
}

fn blocker_kind(status: AgentStatus, blocker: &str) -> &'static str {
    let lower = blocker.to_ascii_lowercase();
    if lower.contains("missing") || lower.contains("unavailable") || lower.contains("could not") {
        "missing_prerequisite"
    } else if status == AgentStatus::Failed {
        "gate_failure"
    } else {
        "operator_action_required"
    }
}

fn build_evidence_receipts(
    root: &Path,
    artifact_base: Option<&Path>,
    report: &AgentReport,
) -> Vec<EvidenceReceipt> {
    let mut receipts = Vec::new();
    let mut artifact_subjects: Vec<String> = report
        .artifacts
        .iter()
        .map(|path| normalize_path(path))
        .collect();
    artifact_subjects.sort();
    artifact_subjects.dedup();

    for subject_path in artifact_subjects {
        let path = resolve_artifact_path(root, artifact_base, &subject_path);
        let (status, sha256, detail) = hash_artifact(&path);
        let subject = format!("artifact:{subject_path}");
        receipts.push(EvidenceReceipt {
            id: subject.clone(),
            kind: "artifact".to_string(),
            subject,
            status,
            sha256,
            detail,
        });
    }

    let mut command_receipts: Vec<EvidenceReceipt> = report
        .commands
        .iter()
        .map(|command| {
            let payload = format!(
                "{}\0{}\0{}\0{}\0{}\0{}",
                command.name,
                command.command.join("\0"),
                command.status.as_str(),
                command
                    .exit_code
                    .map_or_else(String::new, |code| code.to_string()),
                command.stdout_tail.as_deref().unwrap_or_default(),
                command.stderr_tail.as_deref().unwrap_or_default()
            );
            let subject = format!("command:{}", command.name);
            EvidenceReceipt {
                id: subject.clone(),
                kind: "command".to_string(),
                subject,
                status: command.status.as_str().to_string(),
                sha256: hash_text(&payload),
                detail: command.command.join(" "),
            }
        })
        .collect();
    command_receipts.sort_by(|a, b| a.id.cmp(&b.id));
    receipts.extend(command_receipts);

    let mut tool_receipts: Vec<EvidenceReceipt> = report
        .tool_checks
        .iter()
        .map(|check| {
            let payload = format!(
                "{}\0{}\0{}\0{}",
                check.name, check.required, check.status, check.detail
            );
            let subject = format!("tool_check:{}", check.name);
            EvidenceReceipt {
                id: subject.clone(),
                kind: "tool_check".to_string(),
                subject,
                status: check.status.clone(),
                sha256: hash_text(&payload),
                detail: check.detail.clone(),
            }
        })
        .collect();
    tool_receipts.sort_by(|a, b| a.id.cmp(&b.id));
    receipts.extend(tool_receipts);

    receipts.sort_by(|a, b| a.id.cmp(&b.id));
    receipts
}

fn resolve_artifact_path(root: &Path, artifact_base: Option<&Path>, subject_path: &str) -> PathBuf {
    let path = PathBuf::from(subject_path);
    if path.is_absolute() {
        return path;
    }

    let root_path = root.join(&path);
    if root_path.exists() {
        return root_path;
    }

    if let Some(base) = artifact_base {
        let base_path = base.join(&path);
        if base_path.exists() {
            return base_path;
        }
    }

    root_path
}

fn hash_artifact(path: &Path) -> (String, String, String) {
    if path.is_file() {
        match hash_file(path) {
            Ok(hash) => (
                "present_file".to_string(),
                hash,
                format!("file evidence at {}", path.display()),
            ),
            Err(err) => (
                "unreadable".to_string(),
                hash_text(&format!("unreadable:{}:{err}", path.display())),
                format!("could not read file evidence at {}: {err}", path.display()),
            ),
        }
    } else if path.is_dir() {
        match hash_dir(path) {
            Ok(hash) => (
                "present_dir".to_string(),
                hash,
                format!("directory evidence at {}", path.display()),
            ),
            Err(err) => (
                "unreadable".to_string(),
                hash_text(&format!("unreadable:{}:{err}", path.display())),
                format!(
                    "could not read directory evidence at {}: {err}",
                    path.display()
                ),
            ),
        }
    } else {
        (
            "missing".to_string(),
            hash_text(&format!("missing:{}", path.display())),
            format!("evidence path is missing: {}", path.display()),
        )
    }
}

fn hash_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(hash_bytes(&bytes))
}

fn hash_dir(path: &Path) -> std::io::Result<String> {
    let mut files: Vec<PathBuf> = WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .collect();
    files.sort_by_key(|file| normalize_relative(path, file));

    let mut hasher = Sha256::new();
    for file in files {
        let rel = normalize_relative(path, &file);
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        let bytes = std::fs::read(&file)?;
        hasher.update(bytes);
        hasher.update([0xff]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn hash_text(text: &str) -> String {
    hash_bytes(text.as_bytes())
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn normalize_relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
