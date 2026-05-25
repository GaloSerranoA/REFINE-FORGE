use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

const POLICY_SCHEMA: &str = "refineforge-training-approval-policy-v1";
const REVIEW_REQUEST_SCHEMA: &str = "refineforge-training-review-request-v1";
const HUMAN_APPROVAL_DRAFT_SCHEMA: &str = "refineforge-human-approval-draft-v1";
const HUMAN_APPROVAL_SCHEMA: &str = "refineforge-human-approval-v1";
const DEFAULT_POLICY: &str = "training/approval-policy.yaml";
const DEFAULT_AGENT_REPORT: &str = "train-agent-report.stdout.json";

const EXPECTED_REQUIREMENTS: &[&str] = &[
    "train.dataset_hashes",
    "train.reproducible_config",
    "train.live_checkpoint",
    "train.benchmark_eval",
    "train.baseline_regression",
    "train.compute_ledger",
    "train.conversion_manifest",
    "train.promotion_manifest",
    "train.human_promotion_approval",
];

const HUMAN_APPROVAL_REQUIREMENT: &str = "train.human_promotion_approval";

#[derive(Debug, Clone)]
pub struct DraftOptions {
    pub evidence_dir: PathBuf,
    pub agent_report: Option<PathBuf>,
    pub policy: Option<PathBuf>,
    pub operator: String,
    pub emit_json: bool,
}

#[derive(Debug, Clone)]
pub struct ApproveOptions {
    pub evidence_dir: PathBuf,
    pub agent_report: Option<PathBuf>,
    pub policy: Option<PathBuf>,
    pub operator: String,
    pub i_reviewed_this_evidence: bool,
    pub emit_json: bool,
}

#[derive(Debug, Deserialize)]
struct ApprovalPolicy {
    schema_version: String,
    allowed_operators: Vec<String>,
    #[serde(default)]
    allow_smoke_runs: bool,
    #[serde(default)]
    required_evidence: Vec<String>,
    #[serde(default)]
    required_metrics: BTreeMap<String, MetricRule>,
}

#[derive(Debug, Deserialize)]
struct MetricRule {
    #[serde(default)]
    min_delta: Option<f64>,
}

struct ApprovalContext {
    evidence_dir: PathBuf,
    agent_report_path: PathBuf,
    policy_path: PathBuf,
    operator: String,
    candidate_model_id: String,
    checkpoint_sha256: String,
    conversion_manifest_sha256: String,
    metric_deltas: BTreeMap<String, f64>,
    required_metrics: BTreeMap<String, f64>,
    required_evidence: Vec<String>,
}

pub fn draft(root: &Path, opts: DraftOptions) -> Result<()> {
    let ctx = validate_context(
        root,
        &opts.evidence_dir,
        opts.agent_report.as_deref(),
        opts.policy.as_deref(),
        &opts.operator,
    )?;
    let approvals_dir = ctx.evidence_dir.join("approvals");
    std::fs::create_dir_all(&approvals_dir).with_context(|| {
        format!(
            "could not create training approval directory {}",
            approvals_dir.display()
        )
    })?;

    let now = Utc::now().to_rfc3339();
    let draft_path = approvals_dir.join("training.draft.json");
    let request_path = approvals_dir.join("training.review-request.json");
    let final_path = approvals_dir.join("training.json");

    let approval = approval_draft_json(&ctx, &now, &request_path, &final_path);
    let request = review_request_json(&ctx, &now, &draft_path, &final_path);

    write_json(&draft_path, &approval)?;
    write_json(&request_path, &request)?;

    let summary = json!({
        "schema_version": "refineforge-training-approval-command-v1",
        "command": "draft",
        "status": "ready-for-human-approval",
        "trust_boundary": "draft-only; does not create approvals/training.json",
        "candidate_model_id": ctx.candidate_model_id,
        "draft_path": display_path(&draft_path),
        "review_request_path": display_path(&request_path),
        "final_approval_exists": final_path.exists()
    });
    print_summary(&summary, opts.emit_json);
    Ok(())
}

pub fn approve(root: &Path, opts: ApproveOptions) -> Result<()> {
    if !opts.i_reviewed_this_evidence {
        bail!("refusing to write approvals/training.json without --i-reviewed-this-evidence");
    }

    let ctx = validate_context(
        root,
        &opts.evidence_dir,
        opts.agent_report.as_deref(),
        opts.policy.as_deref(),
        &opts.operator,
    )?;
    let approvals_dir = ctx.evidence_dir.join("approvals");
    std::fs::create_dir_all(&approvals_dir).with_context(|| {
        format!(
            "could not create training approval directory {}",
            approvals_dir.display()
        )
    })?;

    let now = Utc::now().to_rfc3339();
    let approval_path = approvals_dir.join("training.json");
    let request_path = approvals_dir.join("training.review-request.json");
    let approval = approval_json(&ctx, &now, Some(&request_path));
    let request = resolved_review_request_json(&ctx, &now, &approval_path, &request_path)?;
    write_json(&approval_path, &approval)?;
    write_json(&request_path, &request)?;

    let summary = json!({
        "schema_version": "refineforge-training-approval-command-v1",
        "command": "approve",
        "status": "approved",
        "trust_boundary": "explicit human operator approval written",
        "candidate_model_id": ctx.candidate_model_id,
        "approval_path": display_path(&approval_path),
        "review_request_path": display_path(&request_path)
    });
    print_summary(&summary, opts.emit_json);
    Ok(())
}

fn validate_context(
    root: &Path,
    evidence_dir: &Path,
    agent_report: Option<&Path>,
    policy: Option<&Path>,
    operator: &str,
) -> Result<ApprovalContext> {
    let evidence_dir = resolve_path(root, evidence_dir);
    let agent_report_path = agent_report
        .map(|path| resolve_path(root, path))
        .unwrap_or_else(|| evidence_dir.join(DEFAULT_AGENT_REPORT));
    let policy_path = resolve_policy_path(root, policy)?;

    let policy = load_policy(&policy_path)?;
    validate_operator(operator, &policy)?;
    validate_agent_report(&agent_report_path)?;

    for relative in &policy.required_evidence {
        let path = safe_join(&evidence_dir, relative)
            .with_context(|| format!("policy required_evidence path is unsafe: {relative}"))?;
        if !path.exists() {
            bail!("required evidence file is missing: {}", path.display());
        }
    }

    let checkpoint_path = safe_join(&evidence_dir, "training/checkpoint.safetensors")?;
    let eval_path = safe_join(&evidence_dir, "training/eval-report.json")?;
    let regression_path = safe_join(&evidence_dir, "training/regression-report.json")?;
    let compute_path = safe_join(&evidence_dir, "training/compute-ledger.json")?;
    let conversion_path = safe_join(&evidence_dir, "training/conversion-manifest.json")?;
    let promotion_path = safe_join(&evidence_dir, "training/promotion-manifest.json")?;

    if !checkpoint_path.exists() {
        bail!(
            "training checkpoint is missing: {}",
            checkpoint_path.display()
        );
    }
    let checkpoint_sha256 = hash_file(&checkpoint_path)?;
    let eval = load_json(&eval_path)?;
    let regression = load_json(&regression_path)?;
    let compute = load_json(&compute_path)?;
    let conversion = load_json(&conversion_path)?;
    let promotion = load_json(&promotion_path)?;

    require_status(&eval, "passed", &eval_path)?;
    require_status(&regression, "passed", &regression_path)?;
    require_status(&compute, "passed", &compute_path)?;
    require_status(&conversion, "passed", &conversion_path)?;
    require_promotion_ready(&promotion, &promotion_path)?;

    require_checkpoint_hash(&conversion, &checkpoint_sha256, &conversion_path)?;
    require_checkpoint_hash(&promotion, &checkpoint_sha256, &promotion_path)?;
    let conversion_manifest_sha256 = hash_file(&conversion_path)?;
    require_conversion_manifest_hash(&promotion, &conversion_manifest_sha256, &promotion_path)?;

    let candidate_model_id = promotion
        .get("model_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model_id| !model_id.is_empty())
        .context("training promotion manifest must include non-empty model_id")?
        .to_string();
    if !policy.allow_smoke_runs && candidate_model_id.to_lowercase().contains("smoke") {
        bail!("policy rejects smoke-run candidate model_id: {candidate_model_id}");
    }

    let metric_deltas = metric_deltas(&regression)?;
    let mut required_metrics = BTreeMap::new();
    for (metric, rule) in &policy.required_metrics {
        let actual = metric_deltas.get(metric).with_context(|| {
            format!("regression report is missing required metric delta {metric}")
        })?;
        let min_delta = rule.min_delta.unwrap_or(0.0);
        if *actual < min_delta {
            bail!("regression metric {metric} delta {actual} is below policy minimum {min_delta}");
        }
        required_metrics.insert(metric.clone(), min_delta);
    }

    Ok(ApprovalContext {
        evidence_dir,
        agent_report_path,
        policy_path,
        operator: operator.trim().to_string(),
        candidate_model_id,
        checkpoint_sha256,
        conversion_manifest_sha256,
        metric_deltas,
        required_metrics,
        required_evidence: policy.required_evidence,
    })
}

fn load_policy(path: &Path) -> Result<ApprovalPolicy> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("could not read approval policy {}", path.display()))?;
    let policy: ApprovalPolicy = serde_yaml::from_str(&content)
        .with_context(|| format!("could not parse approval policy {}", path.display()))?;
    if policy.schema_version != POLICY_SCHEMA {
        bail!(
            "approval policy {} has schema_version {}, expected {POLICY_SCHEMA}",
            path.display(),
            policy.schema_version
        );
    }
    if policy.allowed_operators.is_empty() {
        bail!("approval policy must list at least one allowed operator");
    }
    if policy.required_evidence.is_empty() {
        bail!("approval policy must list required evidence paths");
    }
    if policy.required_metrics.is_empty() {
        bail!("approval policy must list required regression metrics");
    }
    Ok(policy)
}

fn validate_operator(operator: &str, policy: &ApprovalPolicy) -> Result<()> {
    let operator = operator.trim();
    if operator.is_empty() {
        bail!("operator must be a non-empty human name");
    }
    if is_automated_operator(operator) {
        bail!("AI/automated approval rejected: {operator}");
    }
    if !policy
        .allowed_operators
        .iter()
        .any(|allowed| allowed.trim() == operator)
    {
        bail!("operator {operator} is not listed in approval policy allowed_operators");
    }
    Ok(())
}

fn validate_agent_report(path: &Path) -> Result<()> {
    let value = load_json(path)?;
    let agent = value
        .get("agent")
        .and_then(Value::as_str)
        .context("agent report must include agent")?;
    if agent != "train" {
        bail!("training approval requires a train agent report, found {agent}");
    }
    let proof = value
        .get("production_proof")
        .context("agent report is missing production_proof")?;
    let requirements = proof
        .get("requirements")
        .and_then(Value::as_array)
        .context("agent report production_proof.requirements must be an array")?;
    let mut statuses = BTreeMap::new();
    for requirement in requirements {
        let Some(id) = requirement.get("id").and_then(Value::as_str) else {
            continue;
        };
        let status = requirement
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("missing");
        statuses.insert(id.to_string(), status.to_string());
    }

    let expected: BTreeSet<&str> = EXPECTED_REQUIREMENTS.iter().copied().collect();
    for requirement in expected {
        if !statuses.contains_key(requirement) {
            bail!("agent report is missing production requirement {requirement}");
        }
    }
    for (id, status) in statuses {
        if id == HUMAN_APPROVAL_REQUIREMENT {
            if status != "blocked" && status != "passed" {
                bail!("human approval requirement has unexpected status {status}");
            }
        } else if status != "passed" {
            bail!("production requirement {id} must be passed before approval, found {status}");
        }
    }

    let blockers = proof
        .get("blockers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let non_human_blockers = blockers
        .iter()
        .filter(|blocker| !is_human_approval_blocker(blocker))
        .collect::<Vec<_>>();
    if !non_human_blockers.is_empty() {
        bail!(
            "agent report has non-human approval blockers: {}",
            non_human_blockers
                .iter()
                .map(|blocker| blocker.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }

    Ok(())
}

fn require_status(value: &Value, expected: &str, path: &Path) -> Result<()> {
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .with_context(|| format!("{} is missing status", path.display()))?;
    if status != expected {
        bail!(
            "{} has status {status}, expected {expected}",
            path.display()
        );
    }
    Ok(())
}

fn require_promotion_ready(value: &Value, path: &Path) -> Result<()> {
    let status_ready = value
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status == "approved" || status == "passed");
    let decision_ready = value
        .get("decision")
        .and_then(Value::as_str)
        .is_some_and(|decision| decision == "promote");
    if !status_ready && !decision_ready {
        bail!(
            "{} must have status approved/passed or decision promote",
            path.display()
        );
    }
    Ok(())
}

fn require_checkpoint_hash(value: &Value, expected: &str, path: &Path) -> Result<()> {
    let actual = value
        .get("checkpoint_sha256")
        .and_then(Value::as_str)
        .with_context(|| format!("{} is missing checkpoint_sha256", path.display()))?;
    if actual != expected {
        bail!(
            "{} checkpoint_sha256 {actual} does not match checkpoint file sha256 {expected}",
            path.display()
        );
    }
    Ok(())
}

fn require_conversion_manifest_hash(value: &Value, expected: &str, path: &Path) -> Result<()> {
    let actual = value
        .get("conversion")
        .and_then(|conversion| conversion.get("manifest_sha256"))
        .and_then(Value::as_str)
        .with_context(|| format!("{} is missing conversion.manifest_sha256", path.display()))?;
    if actual != expected {
        bail!(
            "{} conversion.manifest_sha256 {actual} does not match conversion manifest sha256 {expected}",
            path.display()
        );
    }
    Ok(())
}

fn metric_deltas(regression: &Value) -> Result<BTreeMap<String, f64>> {
    let deltas = regression
        .get("metric_deltas")
        .and_then(Value::as_object)
        .context("regression report must include metric_deltas object")?;
    let mut parsed = BTreeMap::new();
    for (metric, value) in deltas {
        let delta = value
            .as_f64()
            .with_context(|| format!("regression metric delta {metric} must be numeric"))?;
        parsed.insert(metric.clone(), delta);
    }
    Ok(parsed)
}

fn approval_draft_json(
    ctx: &ApprovalContext,
    drafted_at: &str,
    request_path: &Path,
    final_path: &Path,
) -> Value {
    json!({
        "schema_version": HUMAN_APPROVAL_DRAFT_SCHEMA,
        "draft_operator": ctx.operator,
        "role": "training",
        "decision": "draft-ready",
        "drafted_at": drafted_at,
        "candidate_model_id": ctx.candidate_model_id,
        "checkpoint_sha256": ctx.checkpoint_sha256,
        "conversion_manifest_sha256": ctx.conversion_manifest_sha256,
        "metric_deltas": ctx.metric_deltas,
        "required_metric_minimums": ctx.required_metrics,
        "required_evidence": ctx.required_evidence,
        "evidence_dir": display_path(&ctx.evidence_dir),
        "agent_report_path": display_path(&ctx.agent_report_path),
        "policy_path": display_path(&ctx.policy_path),
        "review_request_path": display_path(request_path),
        "final_approval_schema": HUMAN_APPROVAL_SCHEMA,
        "final_approval_path": display_path(final_path),
        "not_approval": true,
        "trust_boundary": "draft-only; does not create final approval",
        "evidence_summary": format!(
            "Training production-proof evidence validated for human review for candidate model {}",
            ctx.candidate_model_id
        )
    })
}

fn approval_json(ctx: &ApprovalContext, approved_at: &str, request_path: Option<&Path>) -> Value {
    let mut value = json!({
        "schema_version": HUMAN_APPROVAL_SCHEMA,
        "human_operator": ctx.operator,
        "role": "training",
        "decision": "approved",
        "approved_at": approved_at,
        "candidate_model_id": ctx.candidate_model_id,
        "checkpoint_sha256": ctx.checkpoint_sha256,
        "conversion_manifest_sha256": ctx.conversion_manifest_sha256,
        "metric_deltas": ctx.metric_deltas,
        "required_metric_minimums": ctx.required_metrics,
        "required_evidence": ctx.required_evidence,
        "evidence_dir": display_path(&ctx.evidence_dir),
        "agent_report_path": display_path(&ctx.agent_report_path),
        "policy_path": display_path(&ctx.policy_path),
        "evidence_summary": format!(
            "Training production-proof evidence reviewed for candidate model {}",
            ctx.candidate_model_id
        )
    });
    if let Some(request_path) = request_path {
        value["review_request_path"] = json!(display_path(request_path));
    }
    value
}

fn review_request_json(
    ctx: &ApprovalContext,
    requested_at: &str,
    draft_path: &Path,
    final_path: &Path,
) -> Value {
    json!({
        "schema_version": REVIEW_REQUEST_SCHEMA,
        "status": "pending-human-review",
        "role": "training",
        "candidate_model_id": ctx.candidate_model_id,
        "requested_at": requested_at,
        "requested_by": "refine training-approval draft",
        "operator": ctx.operator,
        "approval_draft_path": display_path(draft_path),
        "final_approval_path": display_path(final_path),
        "evidence_dir": display_path(&ctx.evidence_dir),
        "agent_report_path": display_path(&ctx.agent_report_path),
        "policy_path": display_path(&ctx.policy_path),
        "checkpoint_sha256": ctx.checkpoint_sha256,
        "conversion_manifest_sha256": ctx.conversion_manifest_sha256,
        "metric_deltas": ctx.metric_deltas,
        "required_metric_minimums": ctx.required_metrics,
        "required_evidence": ctx.required_evidence,
        "review_instructions": [
            "Inspect the agent report, regression report, compute ledger, conversion manifest, promotion manifest, and draft approval.",
            "Run refine training-approval approve only after a named human operator has reviewed the evidence."
        ]
    })
}

fn resolved_review_request_json(
    ctx: &ApprovalContext,
    resolved_at: &str,
    approval_path: &Path,
    request_path: &Path,
) -> Result<Value> {
    let mut request = if request_path.exists() {
        load_json(request_path)?
    } else {
        review_request_json(
            ctx,
            resolved_at,
            &approval_path.with_file_name("training.draft.json"),
            approval_path,
        )
    };
    let object = request
        .as_object_mut()
        .context("training review request must be a JSON object")?;
    object.insert("schema_version".to_string(), json!(REVIEW_REQUEST_SCHEMA));
    object.insert("status".to_string(), json!("approved"));
    object.insert("resolved_at".to_string(), json!(resolved_at));
    object.insert("resolved_by".to_string(), json!(ctx.operator));
    object.insert(
        "approval_path".to_string(),
        json!(display_path(approval_path)),
    );
    object.insert(
        "resolution_summary".to_string(),
        json!(format!(
            "Human operator {} approved training evidence for {}",
            ctx.operator, ctx.candidate_model_id
        )),
    );
    Ok(request)
}

fn resolve_policy_path(root: &Path, policy: Option<&Path>) -> Result<PathBuf> {
    if let Some(policy) = policy {
        return Ok(resolve_path(root, policy));
    }

    let primary = resolve_path(root, Path::new(DEFAULT_POLICY));
    if primary.exists() {
        return Ok(primary);
    }
    bail!(
        "approval policy not found; pass --policy or copy training/approval-policy.example.yaml to {DEFAULT_POLICY}"
    );
}

fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute() {
        bail!("path must be relative: {relative}");
    }
    if relative_path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        bail!("path must stay inside evidence directory: {relative}");
    }
    Ok(base.join(relative_path))
}

fn load_json(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("could not read JSON file {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("could not parse JSON file {}", path.display()))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create directory {}", parent.display()))?;
    }
    let content = serde_json::to_vec_pretty(value)?;
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("json")
    ));
    std::fs::write(&tmp, content)
        .with_context(|| format!("could not write temporary file {}", tmp.display()))?;
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("could not replace existing file {}", path.display()))?;
    }
    std::fs::rename(&tmp, path)
        .with_context(|| format!("could not move {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let bytes =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn is_human_approval_blocker(blocker: &str) -> bool {
    let normalized = blocker.to_ascii_lowercase();
    normalized.contains("human")
        && normalized.contains("approval")
        && !normalized.contains("regression")
}

fn is_automated_operator(operator: &str) -> bool {
    let lower = operator.to_ascii_lowercase();
    let tokens = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
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

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn print_summary(summary: &Value, emit_json: bool) {
    if emit_json {
        println!(
            "{}",
            serde_json::to_string_pretty(summary).expect("serialize command summary")
        );
    } else {
        println!(
            "training approval {}: {}",
            summary
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("command"),
            summary
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
    }
}
