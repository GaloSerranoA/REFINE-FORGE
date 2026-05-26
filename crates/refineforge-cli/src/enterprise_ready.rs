use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct EnterpriseReadyOptions {
    pub out_dir: PathBuf,
    pub hosted_ci_evidence: Option<PathBuf>,
    pub signed_release_evidence: Option<PathBuf>,
    pub checkpoint_manifest: Option<PathBuf>,
    pub helyx_integration_evidence: Option<PathBuf>,
    pub cleanup_report: Option<PathBuf>,
    pub emit_json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnterpriseGateStatus {
    Passed,
    Blocked,
}

impl EnterpriseGateStatus {
    fn as_str(self) -> &'static str {
        match self {
            EnterpriseGateStatus::Passed => "passed",
            EnterpriseGateStatus::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseGate {
    pub id: String,
    pub name: String,
    pub status: EnterpriseGateStatus,
    pub evidence_path: Option<PathBuf>,
    pub message: String,
    pub blocker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseReadinessReport {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub status: String,
    pub public_claim: String,
    pub root: PathBuf,
    pub out_dir: PathBuf,
    pub gates: Vec<EnterpriseGate>,
    pub blockers: Vec<String>,
}

impl EnterpriseReadinessReport {
    fn new(root: &Path, out_dir: &Path, gates: Vec<EnterpriseGate>) -> Self {
        let blockers: Vec<String> = gates
            .iter()
            .filter_map(|gate| gate.blocker.clone())
            .collect();
        let ready = blockers.is_empty();
        Self {
            schema_version: "refineforge-enterprise-readiness-v1".to_string(),
            generated_at: Utc::now(),
            status: if ready { "ready" } else { "blocked" }.to_string(),
            public_claim: if ready {
                "enterprise_readiness_evidence_complete_local_check"
            } else {
                "enterprise_readiness_blocked_until_external_evidence_present"
            }
            .to_string(),
            root: root.to_path_buf(),
            out_dir: out_dir.to_path_buf(),
            gates,
            blockers,
        }
    }

    fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Refine-Forge Enterprise Readiness\n\n");
        out.push_str(&format!("- Status: `{}`\n", self.status));
        out.push_str(&format!("- Public claim: `{}`\n", self.public_claim));
        out.push_str(&format!(
            "- Generated at: `{}`\n",
            self.generated_at.to_rfc3339()
        ));
        out.push_str(&format!("- Root: `{}`\n\n", self.root.display()));

        out.push_str("## Gates\n\n");
        out.push_str("| Gate | Status | Evidence | Message |\n");
        out.push_str("|---|---|---|---|\n");
        for gate in &self.gates {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                gate.name,
                gate.status.as_str(),
                gate.evidence_path
                    .as_ref()
                    .map(|path| format!("`{}`", path.display()))
                    .unwrap_or_else(|| "required".to_string()),
                gate.blocker.as_deref().unwrap_or(&gate.message)
            ));
        }

        out.push_str("\n## Blockers\n\n");
        if self.blockers.is_empty() {
            out.push_str("- none\n");
        } else {
            for blocker in &self.blockers {
                out.push_str(&format!("- {blocker}\n"));
            }
        }

        out.push_str("\n## Boundary\n\n");
        out.push_str(
            "This report is a local evidence gate. It does not prove remote CI, \
             signing, checkpoint acceptance, HELYX integration, or cleanup unless \
             the corresponding evidence files are present and pass validation.\n",
        );
        out
    }
}

pub fn ready(root: &Path, opts: EnterpriseReadyOptions) -> Result<()> {
    let report = build_report(root, &opts);
    std::fs::create_dir_all(&opts.out_dir)
        .with_context(|| format!("creating {}", opts.out_dir.display()))?;
    let json_path = opts.out_dir.join("enterprise-readiness.json");
    let md_path = opts.out_dir.join("enterprise-readiness.md");
    std::fs::write(&json_path, serde_json::to_vec_pretty(&report)?)
        .with_context(|| format!("writing {}", json_path.display()))?;
    std::fs::write(&md_path, report.to_markdown())
        .with_context(|| format!("writing {}", md_path.display()))?;

    if opts.emit_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "enterprise readiness report: {} ({})",
            md_path.display(),
            report.status
        );
    }
    Ok(())
}

fn build_report(root: &Path, opts: &EnterpriseReadyOptions) -> EnterpriseReadinessReport {
    let gates = vec![
        status_json_gate(
            "remote_ci_proof",
            "Remote CI proof",
            opts.hosted_ci_evidence.as_deref(),
            |_| Ok(()),
        ),
        status_json_gate(
            "signed_release",
            "Signed release proof",
            opts.signed_release_evidence.as_deref(),
            validate_signed_release,
        ),
        status_json_gate(
            "accepted_model_checkpoint",
            "Accepted real model checkpoint",
            opts.checkpoint_manifest.as_deref(),
            validate_checkpoint_manifest,
        ),
        status_json_gate(
            "live_helyx_integration",
            "Live HELYX integration",
            opts.helyx_integration_evidence.as_deref(),
            |_| Ok(()),
        ),
        docs_polish_gate(root),
        status_json_gate(
            "complexity_cleanup",
            "Complexity cleanup report",
            opts.cleanup_report.as_deref(),
            |_| Ok(()),
        ),
    ];
    EnterpriseReadinessReport::new(root, &opts.out_dir, gates)
}

fn status_json_gate(
    id: &str,
    name: &str,
    path: Option<&Path>,
    validate: fn(&Value) -> std::result::Result<(), String>,
) -> EnterpriseGate {
    let Some(path) = path else {
        return blocked_gate(id, name, None, format!("{id} evidence path is required"));
    };

    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            return blocked_gate(
                id,
                name,
                Some(path.to_path_buf()),
                format!("{id} evidence could not be read: {err}"),
            );
        }
    };
    let json: Value = match serde_json::from_str(&content) {
        Ok(json) => json,
        Err(err) => {
            return blocked_gate(
                id,
                name,
                Some(path.to_path_buf()),
                format!("{id} evidence is not valid JSON: {err}"),
            );
        }
    };
    if !status_is_accepted(&json) {
        let blocker = blocked_evidence_summary(id, &json);
        return blocked_gate(
            id,
            name,
            Some(path.to_path_buf()),
            blocker.unwrap_or_else(|| {
                format!(
                    "{id} evidence status must be passed, success, ready, approved, or human-reviewed"
                )
            }),
        );
    }
    if let Err(err) = validate(&json) {
        return blocked_gate(id, name, Some(path.to_path_buf()), err);
    }
    EnterpriseGate {
        id: id.to_string(),
        name: name.to_string(),
        status: EnterpriseGateStatus::Passed,
        evidence_path: Some(path.to_path_buf()),
        message: "evidence accepted".to_string(),
        blocker: None,
    }
}

fn docs_polish_gate(root: &Path) -> EnterpriseGate {
    let required = [
        ("README.md", "enterprise readiness"),
        ("STRUCTURE.md", "enterprise readiness"),
        ("CHANGELOG.md", "enterprise readiness"),
        ("docs/enterprise-readiness.md", "enterprise readiness"),
    ];
    let mut missing = Vec::new();
    for (relative, needle) in required {
        let path = root.join(relative);
        let Ok(content) = std::fs::read_to_string(&path) else {
            missing.push(format!("{relative} is missing"));
            continue;
        };
        if !content.to_ascii_lowercase().contains(needle) {
            missing.push(format!("{relative} does not mention {needle}"));
        }
    }

    if missing.is_empty() {
        EnterpriseGate {
            id: "docs_polish".to_string(),
            name: "Documentation polish".to_string(),
            status: EnterpriseGateStatus::Passed,
            evidence_path: Some(root.join("docs/enterprise-readiness.md")),
            message: "enterprise readiness docs are linked from README, STRUCTURE, and CHANGELOG"
                .to_string(),
            blocker: None,
        }
    } else {
        blocked_gate(
            "docs_polish",
            "Documentation polish",
            Some(root.join("docs/enterprise-readiness.md")),
            format!("docs_polish blocked: {}", missing.join("; ")),
        )
    }
}

fn validate_signed_release(json: &Value) -> std::result::Result<(), String> {
    let has_signature = json.get("signature").and_then(Value::as_str).is_some()
        || json.get("signature_mode").and_then(Value::as_str).is_some()
        || json.get("sigstore").is_some()
        || json
            .get("signed_bundle_path")
            .and_then(Value::as_str)
            .is_some();
    if !has_signature {
        return Err("signed_release evidence must include a signature marker".to_string());
    }
    let sha = json
        .get("bundle_sha256")
        .or_else(|| json.get("artifact_sha256"))
        .and_then(Value::as_str);
    if !sha.is_some_and(is_hex_sha256) {
        return Err("signed_release evidence must include a 64-hex bundle_sha256".to_string());
    }
    Ok(())
}

fn validate_checkpoint_manifest(json: &Value) -> std::result::Result<(), String> {
    let sha = json
        .get("checkpoint_sha256")
        .or_else(|| json.pointer("/checkpoint/sha256"))
        .or_else(|| json.pointer("/checkpoint/checkpoint_sha256"))
        .and_then(Value::as_str);
    if !sha.is_some_and(is_hex_sha256) {
        return Err(
            "accepted_model_checkpoint evidence must include a checkpoint sha256".to_string(),
        );
    }
    let requires_hash = json
        .pointer("/helyx_handoff/requires_hash_verification")
        .or_else(|| json.get("requires_hash_verification"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !requires_hash {
        return Err(
            "accepted_model_checkpoint evidence must require HELYX hash verification".to_string(),
        );
    }
    Ok(())
}

fn status_is_accepted(json: &Value) -> bool {
    json.get("status")
        .and_then(Value::as_str)
        .map(|status| {
            matches!(
                status.to_ascii_lowercase().as_str(),
                "passed" | "pass" | "success" | "ready" | "approved" | "human-reviewed"
            )
        })
        .unwrap_or(false)
}

fn blocked_evidence_summary(id: &str, json: &Value) -> Option<String> {
    if let Some(blocker) = json.get("blocker").and_then(Value::as_str) {
        return Some(format!("{id} evidence blocked: {blocker}"));
    }

    let blockers = json.get("blockers")?.as_array()?;
    let mut parts = Vec::new();
    for blocker in blockers {
        if let Some(text) = blocker.as_str() {
            parts.push(text.to_string());
            continue;
        }
        if let Some(object) = blocker.as_object() {
            let blocker_id = object
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("unnamed_blocker");
            let detail = object
                .get("impact")
                .or_else(|| object.get("observed"))
                .and_then(Value::as_str)
                .unwrap_or("blocked");
            parts.push(format!("{blocker_id}: {detail}"));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("{id} evidence blocked: {}", parts.join("; ")))
    }
}

fn is_hex_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn blocked_gate(
    id: &str,
    name: &str,
    evidence_path: Option<PathBuf>,
    blocker: String,
) -> EnterpriseGate {
    EnterpriseGate {
        id: id.to_string(),
        name: name.to_string(),
        status: EnterpriseGateStatus::Blocked,
        evidence_path,
        message: "blocked until evidence is provided".to_string(),
        blocker: Some(blocker),
    }
}
