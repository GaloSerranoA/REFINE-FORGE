use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::ValueEnum;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

const POLICY_SCHEMA: &str = "refineforge-approval-policy-v1";
const REVIEW_REQUEST_SCHEMA: &str = "refineforge-human-review-request-v1";
const HUMAN_APPROVAL_SCHEMA: &str = "refineforge-human-approval-v1";
const DEFAULT_POLICY: &str = "approval-policy.yaml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalRole {
    Training,
    Kernel,
    Lean,
}

impl ApprovalRole {
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalRole::Training => "training",
            ApprovalRole::Kernel => "kernel",
            ApprovalRole::Lean => "lean",
        }
    }

    fn draft_file(self) -> &'static str {
        match self {
            ApprovalRole::Training => "training.draft.json",
            ApprovalRole::Kernel => "kernel.draft.json",
            ApprovalRole::Lean => "lean.draft.json",
        }
    }

    fn final_file(self) -> &'static str {
        match self {
            ApprovalRole::Training => "training.json",
            ApprovalRole::Kernel => "kernel.json",
            ApprovalRole::Lean => "lean.json",
        }
    }

    fn request_file(self) -> &'static str {
        match self {
            ApprovalRole::Training => "training.review-request.json",
            ApprovalRole::Kernel => "kernel.review-request.json",
            ApprovalRole::Lean => "lean.review-request.json",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DraftOptions {
    pub role: Option<ApprovalRole>,
    pub evidence_dir: Option<PathBuf>,
    pub review_request: Option<PathBuf>,
    pub agent_report: Option<PathBuf>,
    pub policy: Option<PathBuf>,
    pub operator: String,
    pub emit_json: bool,
}

#[derive(Debug, Clone)]
pub struct ApproveOptions {
    pub role: Option<ApprovalRole>,
    pub evidence_dir: Option<PathBuf>,
    pub review_request: Option<PathBuf>,
    pub draft: Option<PathBuf>,
    pub agent_report: Option<PathBuf>,
    pub policy: Option<PathBuf>,
    pub operator: String,
    pub i_reviewed_this_evidence: bool,
    pub emit_json: bool,
}

#[derive(Debug, Deserialize)]
struct ApprovalPolicy {
    schema_version: String,
    #[serde(default)]
    allowed_operator_names: Vec<String>,
    #[serde(default)]
    allowed_operators: Vec<String>,
    roles: BTreeMap<String, RolePolicy>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RolePolicy {
    #[serde(default)]
    allow_smoke_runs: bool,
    #[serde(
        default,
        alias = "required_evidence_files",
        alias = "evidence_required"
    )]
    required_evidence: Vec<String>,
    #[serde(default)]
    candidate_files_required: Vec<String>,
    #[serde(default)]
    #[serde(alias = "regression")]
    required_metrics: BTreeMap<String, MetricRule>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MetricRule {
    #[serde(default)]
    min_delta: Option<f64>,
    #[serde(default)]
    min: Option<f64>,
}

struct ApprovalContext {
    role: ApprovalRole,
    evidence_dir: PathBuf,
    policy_path: PathBuf,
    review_request_path: PathBuf,
    draft_path: PathBuf,
    final_path: PathBuf,
    operator: String,
    candidate_id: String,
    required_evidence: Vec<String>,
    required_metrics: BTreeMap<String, f64>,
    metric_deltas: BTreeMap<String, f64>,
    checkpoint_sha256: Option<String>,
    conversion_manifest_sha256: Option<String>,
}

pub fn draft(root: &Path, opts: DraftOptions) -> Result<()> {
    let ctx = validate_context(
        root,
        opts.role,
        opts.evidence_dir.as_deref(),
        opts.review_request.as_deref(),
        opts.agent_report.as_deref(),
        opts.policy.as_deref(),
        &opts.operator,
    )?;
    write_approval_artifacts(&ctx, false, opts.emit_json)
}

pub fn approve(root: &Path, opts: ApproveOptions) -> Result<()> {
    if !opts.i_reviewed_this_evidence {
        let role = opts
            .role
            .map(ApprovalRole::as_str)
            .unwrap_or("role-specific");
        bail!("refusing to write approvals/{role}.json without --i-reviewed-this-evidence");
    }

    let mut role = opts.role;
    let mut evidence_dir = opts.evidence_dir;
    let mut review_request = opts.review_request;
    if let Some(draft) = opts.draft.as_deref() {
        let draft = resolve_path(root, draft);
        if !draft.exists() {
            bail!("approval draft is missing: {}", draft.display());
        }
        let draft_value = load_json(&draft)?;
        if role.is_none() {
            role = draft_value
                .get("role")
                .and_then(Value::as_str)
                .map(role_from_str)
                .transpose()?;
        }
        if evidence_dir.is_none() {
            evidence_dir = draft_value
                .get("evidence_dir")
                .and_then(Value::as_str)
                .map(PathBuf::from);
        }
        if review_request.is_none() {
            review_request = draft_value
                .get("review_request_path")
                .and_then(Value::as_str)
                .map(PathBuf::from);
        }
    }

    let ctx = validate_context(
        root,
        role,
        evidence_dir.as_deref(),
        review_request.as_deref(),
        opts.agent_report.as_deref(),
        opts.policy.as_deref(),
        &opts.operator,
    )?;
    write_approval_artifacts(&ctx, true, opts.emit_json)
}

fn validate_context(
    root: &Path,
    role_arg: Option<ApprovalRole>,
    evidence_dir_arg: Option<&Path>,
    review_request_arg: Option<&Path>,
    agent_report_arg: Option<&Path>,
    policy_arg: Option<&Path>,
    operator: &str,
) -> Result<ApprovalContext> {
    let policy_path = resolve_policy_path(root, policy_arg)?;
    let policy = load_policy(&policy_path)?;
    validate_operator(operator, &policy)?;

    let explicit_request_path = review_request_arg.map(|path| resolve_path(root, path));
    let explicit_request_value = explicit_request_path
        .as_deref()
        .filter(|path| path.exists())
        .map(load_json)
        .transpose()?;
    let role = resolve_role(role_arg, explicit_request_value.as_ref())?;
    let role_policy = policy
        .roles
        .get(role.as_str())
        .with_context(|| format!("approval policy has no role section for {}", role.as_str()))?
        .clone();
    if role_policy.required_evidence.is_empty() {
        bail!(
            "approval policy role {} must list required_evidence",
            role.as_str()
        );
    }

    let evidence_dir =
        resolve_evidence_dir(root, evidence_dir_arg, explicit_request_value.as_ref())?;
    let approvals_dir = evidence_dir.join("approvals");
    let review_request_path =
        explicit_request_path.unwrap_or_else(|| approvals_dir.join(role.request_file()));
    let request_value = match explicit_request_value {
        Some(value) => Some(value),
        None if review_request_path.exists() => Some(load_json(&review_request_path)?),
        None => None,
    };
    let draft_path = approvals_dir.join(role.draft_file());
    let final_path = approvals_dir.join(role.final_file());
    validate_required_evidence(&evidence_dir, &role_policy.required_evidence)?;

    let mut required_metrics = BTreeMap::new();
    let mut metric_deltas = BTreeMap::new();
    let mut checkpoint_sha256 = None;
    let mut conversion_manifest_sha256 = None;
    let candidate_id = match role {
        ApprovalRole::Kernel => validate_kernel_evidence(&evidence_dir, request_value.as_ref())?,
        ApprovalRole::Lean => {
            validate_lean_evidence(&evidence_dir, request_value.as_ref(), &role_policy)?
        }
        ApprovalRole::Training => {
            let agent_report = agent_report_arg
                .map(|path| resolve_path(root, path))
                .unwrap_or_else(|| evidence_dir.join("train-agent-report.stdout.json"));
            validate_training_agent_report(&agent_report)?;
            let training = validate_training_evidence(&evidence_dir, &role_policy)?;
            required_metrics = training.required_metrics;
            metric_deltas = training.metric_deltas;
            checkpoint_sha256 = Some(training.checkpoint_sha256);
            conversion_manifest_sha256 = Some(training.conversion_manifest_sha256);
            training.candidate_model_id
        }
    };

    Ok(ApprovalContext {
        role,
        evidence_dir,
        policy_path,
        review_request_path,
        draft_path,
        final_path,
        operator: operator.trim().to_string(),
        candidate_id,
        required_evidence: role_policy.required_evidence,
        required_metrics,
        metric_deltas,
        checkpoint_sha256,
        conversion_manifest_sha256,
    })
}

fn write_approval_artifacts(
    ctx: &ApprovalContext,
    final_approval: bool,
    emit_json: bool,
) -> Result<()> {
    if let Some(parent) = ctx.draft_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create approval directory {}", parent.display()))?;
    }
    let now = Utc::now().to_rfc3339();
    let approval = approval_json(ctx, &now);
    if final_approval {
        write_json(&ctx.final_path, &approval)?;
        write_json(
            &ctx.review_request_path,
            &review_request_json(ctx, &now, "approved", Some(&ctx.final_path))?,
        )?;
    } else {
        write_json(&ctx.draft_path, &approval)?;
        write_json(
            &ctx.review_request_path,
            &review_request_json(ctx, &now, "draft-ready", None)?,
        )?;
    }

    let summary = json!({
        "schema_version": "refineforge-approval-command-v1",
        "command": if final_approval { "approve" } else { "draft" },
        "role": ctx.role.as_str(),
        "status": if final_approval { "approved" } else { "ready-for-human-approval" },
        "trust_boundary": if final_approval {
            "explicit human operator approval written"
        } else {
            "draft-only; does not create final approval"
        },
        "evidence_dir": display_path(&ctx.evidence_dir),
        "draft_path": display_path(&ctx.draft_path),
        "approval_path": display_path(&ctx.final_path),
        "review_request_path": display_path(&ctx.review_request_path),
        "final_approval_exists": ctx.final_path.exists()
    });
    print_summary(&summary, emit_json);
    Ok(())
}

fn approval_json(ctx: &ApprovalContext, approved_at: &str) -> Value {
    let mut value = json!({
        "schema_version": HUMAN_APPROVAL_SCHEMA,
        "human_operator": ctx.operator,
        "role": ctx.role.as_str(),
        "decision": "approved",
        "approved_at": approved_at,
        "candidate_id": ctx.candidate_id,
        "required_evidence": ctx.required_evidence,
        "evidence_dir": display_path(&ctx.evidence_dir),
        "policy_path": display_path(&ctx.policy_path),
        "review_request_path": display_path(&ctx.review_request_path),
        "evidence_summary": format!(
            "{} production-proof evidence reviewed for {}",
            ctx.role.as_str(),
            ctx.candidate_id
        )
    });
    match ctx.role {
        ApprovalRole::Training => {
            value["candidate_model_id"] = json!(ctx.candidate_id);
            value["metric_deltas"] = json!(ctx.metric_deltas);
            value["required_metric_minimums"] = json!(ctx.required_metrics);
            if let Some(sha) = &ctx.checkpoint_sha256 {
                value["checkpoint_sha256"] = json!(sha);
            }
            if let Some(sha) = &ctx.conversion_manifest_sha256 {
                value["conversion_manifest_sha256"] = json!(sha);
            }
        }
        ApprovalRole::Kernel => {
            value["kernel_id"] = json!(ctx.candidate_id);
        }
        ApprovalRole::Lean => {
            value["claim_id"] = json!(ctx.candidate_id);
        }
    }
    value
}

fn review_request_json(
    ctx: &ApprovalContext,
    timestamp: &str,
    status: &str,
    approval_path: Option<&Path>,
) -> Result<Value> {
    let mut request = if ctx.review_request_path.exists() {
        load_json(&ctx.review_request_path)?
    } else {
        json!({
            "schema_version": REVIEW_REQUEST_SCHEMA,
            "role": ctx.role.as_str(),
            "decision": "pending",
            "requested_at": timestamp,
            "candidate": {
                "evidence_dir": display_path(&ctx.evidence_dir)
            }
        })
    };
    let object = request
        .as_object_mut()
        .context("review request must be a JSON object")?;
    object.insert("schema_version".to_string(), json!(REVIEW_REQUEST_SCHEMA));
    object.insert("role".to_string(), json!(ctx.role.as_str()));
    object.insert("status".to_string(), json!(status));
    object.insert("operator".to_string(), json!(ctx.operator));
    object.insert(
        "approval_draft_path".to_string(),
        json!(display_path(&ctx.draft_path)),
    );
    object.insert(
        "final_approval_path".to_string(),
        json!(display_path(&ctx.final_path)),
    );
    object.insert(
        "evidence_dir".to_string(),
        json!(display_path(&ctx.evidence_dir)),
    );
    if let Some(approval_path) = approval_path {
        object.insert("resolved_at".to_string(), json!(timestamp));
        object.insert("resolved_by".to_string(), json!(ctx.operator));
        object.insert(
            "approval_path".to_string(),
            json!(display_path(approval_path)),
        );
        object.insert(
            "resolution_summary".to_string(),
            json!(format!(
                "Human operator {} approved {} evidence for {}",
                ctx.operator,
                ctx.role.as_str(),
                ctx.candidate_id
            )),
        );
    }
    Ok(request)
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
    if allowed_operators(&policy).is_empty() {
        bail!("approval policy must list allowed_operator_names");
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
    if !allowed_operators(policy)
        .iter()
        .any(|allowed| allowed.trim() == operator)
    {
        bail!("operator {operator} is not listed in approval policy allowed_operator_names");
    }
    Ok(())
}

fn allowed_operators(policy: &ApprovalPolicy) -> Vec<&str> {
    policy
        .allowed_operator_names
        .iter()
        .chain(policy.allowed_operators.iter())
        .map(String::as_str)
        .collect()
}

fn resolve_role(role_arg: Option<ApprovalRole>, request: Option<&Value>) -> Result<ApprovalRole> {
    if let Some(role) = role_arg {
        return Ok(role);
    }
    let role = request
        .and_then(|value| value.get("role"))
        .and_then(Value::as_str)
        .context("approval role is required when --review-request does not contain role")?;
    role_from_str(role)
}

fn role_from_str(role: &str) -> Result<ApprovalRole> {
    match role {
        "training" => Ok(ApprovalRole::Training),
        "kernel" => Ok(ApprovalRole::Kernel),
        "lean" => Ok(ApprovalRole::Lean),
        other => bail!("unsupported approval role {other}"),
    }
}

fn resolve_evidence_dir(
    root: &Path,
    arg: Option<&Path>,
    request: Option<&Value>,
) -> Result<PathBuf> {
    if let Some(arg) = arg {
        return Ok(resolve_path(root, arg));
    }
    let raw = request
        .and_then(|value| {
            value
                .get("candidate")
                .and_then(|candidate| candidate.get("evidence_dir"))
                .or_else(|| value.get("evidence_dir"))
        })
        .and_then(Value::as_str)
        .context("approval evidence directory is required")?;
    Ok(resolve_path(root, Path::new(raw)))
}

fn resolve_policy_path(root: &Path, policy: Option<&Path>) -> Result<PathBuf> {
    if let Some(policy) = policy {
        return Ok(resolve_path(root, policy));
    }
    let default = resolve_path(root, Path::new(DEFAULT_POLICY));
    if default.exists() {
        return Ok(default);
    }
    bail!("approval policy not found; pass --policy or copy approval-policy.example.yaml to {DEFAULT_POLICY}")
}

fn validate_required_evidence(evidence_dir: &Path, required: &[String]) -> Result<()> {
    for relative in required {
        let path = safe_join(evidence_dir, relative)
            .with_context(|| format!("policy required_evidence path is unsafe: {relative}"))?;
        if !path.exists() {
            bail!("required evidence file is missing: {}", path.display());
        }
    }
    Ok(())
}

fn validate_kernel_evidence(evidence_dir: &Path, request: Option<&Value>) -> Result<String> {
    let source = safe_join(evidence_dir, "kernels/src/hvector_add.cu")?;
    if !source.exists() {
        bail!("kernel CUDA source is missing: {}", source.display());
    }

    let reference_path = safe_join(evidence_dir, "kernels/reference-output.json")?;
    let reference = load_json(&reference_path)?;
    require_json_status_in(&reference, &["passed", "accepted"], &reference_path)?;
    require_any_sha256(
        &reference,
        &["sha256", "output_sha256", "reference_sha256"],
        &reference_path,
    )?;

    let bitexact_path = safe_join(evidence_dir, "kernels/bitexact-report.json")?;
    let bitexact = load_json(&bitexact_path)?;
    let bitexact_ok = bitexact
        .get("outcome")
        .and_then(Value::as_str)
        .is_some_and(|status| status.eq_ignore_ascii_case("pass"))
        || bitexact
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status.eq_ignore_ascii_case("passed"));
    if !bitexact_ok {
        bail!(
            "{} is not a passed bit-exact report",
            bitexact_path.display()
        );
    }

    let hardware_path = safe_join(evidence_dir, "kernels/hardware-matrix.json")?;
    let hardware = load_json(&hardware_path)?;
    let hardware_ok = hardware
        .get("runs")
        .and_then(Value::as_array)
        .is_some_and(|runs| runs.iter().any(hardware_run_passed));
    if !hardware_ok {
        bail!("{} has no passed hardware run", hardware_path.display());
    }

    let compiler_path = safe_join(evidence_dir, "kernels/compiler-metadata.json")?;
    let compiler = load_json(&compiler_path)?;
    require_json_status_in(&compiler, &["passed"], &compiler_path)?;
    if !["nvcc", "rustc", "compiler"]
        .iter()
        .any(|field| string_field_nonempty(&compiler, field))
    {
        bail!(
            "{} must include nvcc, rustc, or compiler",
            compiler_path.display()
        );
    }

    let performance_path = safe_join(evidence_dir, "kernels/performance-baseline.json")?;
    let performance = load_json(&performance_path)?;
    require_json_status_in(&performance, &["passed"], &performance_path)?;

    let handoff_path = safe_join(evidence_dir, "kernels/helyx-handoff.json")?;
    let handoff = load_json(&handoff_path)?;
    require_json_status_in(&handoff, &["passed", "accepted"], &handoff_path)?;
    let handoff_kernel_id = handoff
        .get("kernel_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let request_kernel_id = request
        .and_then(|value| value.get("candidate"))
        .and_then(|candidate| candidate.get("kernel_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    handoff_kernel_id
        .or(request_kernel_id)
        .context("kernel approval requires candidate.kernel_id or HELYX handoff kernel_id")
}

fn validate_lean_evidence(
    evidence_dir: &Path,
    request: Option<&Value>,
    policy: &RolePolicy,
) -> Result<String> {
    let request =
        request.context("Lean approval requires a review request with candidate files")?;
    let candidate = request
        .get("candidate")
        .and_then(Value::as_object)
        .context("Lean review request must include candidate object")?;
    for field in &policy.candidate_files_required {
        let raw = candidate
            .get(field)
            .and_then(Value::as_str)
            .with_context(|| format!("Lean review request candidate is missing {field}"))?;
        let path = PathBuf::from(raw);
        if !path.exists() {
            bail!("Lean candidate file {field} is missing: {}", path.display());
        }
    }

    let proof_report = first_existing(
        evidence_dir,
        &[
            "lean/lean-proof-report.json",
            "lean/proof-report.json",
            "lean/report.json",
        ],
    )
    .context("Lean proof report evidence is missing")?;
    let proof = load_json(&proof_report)?;
    let proof_ok = proof
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status == "verified" || status == "passed");
    if !proof_ok {
        bail!(
            "{} is not a verified Lean proof report",
            proof_report.display()
        );
    }

    let scan_report = first_existing(
        evidence_dir,
        &[
            "lean/rust-symbol-scan.json",
            "lean/refinement-links.json",
            "lean/scan-report.json",
        ],
    )
    .context("Lean Rust symbol scan evidence is missing")?;
    let scan = load_json(&scan_report)?;
    require_json_status_in(&scan, &["passed", "verified"], &scan_report)?;

    let bundle_hashes = first_existing(
        evidence_dir,
        &[
            "lean/exported-bundle-hashes.json",
            "lean/bundle-hashes.json",
        ],
    )
    .context("Lean exported bundle hashes evidence is missing")?;
    let bundles = load_json(&bundle_hashes)?;
    require_json_status_in(&bundles, &["passed"], &bundle_hashes)?;
    let bundle_hashes_ok = bundles
        .get("bundles")
        .and_then(Value::as_array)
        .is_some_and(|bundles| {
            !bundles.is_empty()
                && bundles.iter().all(|bundle| {
                    bundle
                        .get("sha256")
                        .and_then(Value::as_str)
                        .is_some_and(valid_sha256)
                })
        });
    if !bundle_hashes_ok {
        bail!(
            "{} must include non-empty bundles with sha256",
            bundle_hashes.display()
        );
    }

    candidate
        .get("claim_id")
        .and_then(Value::as_str)
        .or_else(|| request.get("claim_id").and_then(Value::as_str))
        .map(str::to_string)
        .context("Lean approval requires candidate.claim_id")
}

fn first_existing(base: &Path, candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .filter_map(|candidate| safe_join(base, candidate).ok())
        .find(|path| path.exists())
}

struct TrainingEvidence {
    candidate_model_id: String,
    checkpoint_sha256: String,
    conversion_manifest_sha256: String,
    metric_deltas: BTreeMap<String, f64>,
    required_metrics: BTreeMap<String, f64>,
}

fn validate_training_evidence(
    evidence_dir: &Path,
    policy: &RolePolicy,
) -> Result<TrainingEvidence> {
    let checkpoint_path = safe_join(evidence_dir, "training/checkpoint.safetensors")?;
    let eval_path = safe_join(evidence_dir, "training/eval-report.json")?;
    let regression_path = safe_join(evidence_dir, "training/regression-report.json")?;
    let compute_path = safe_join(evidence_dir, "training/compute-ledger.json")?;
    let conversion_path = safe_join(evidence_dir, "training/conversion-manifest.json")?;
    let promotion_path = safe_join(evidence_dir, "training/promotion-manifest.json")?;

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

    require_json_status_in(&eval, &["passed"], &eval_path)?;
    require_json_status_in(&regression, &["passed"], &regression_path)?;
    require_json_status_in(&compute, &["passed"], &compute_path)?;
    require_json_status_in(&conversion, &["passed"], &conversion_path)?;
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
        let minimum = rule.min_delta.or(rule.min).unwrap_or(0.0);
        if *actual < minimum {
            bail!("regression metric {metric} delta {actual} is below policy minimum {minimum}");
        }
        required_metrics.insert(metric.clone(), minimum);
    }

    Ok(TrainingEvidence {
        candidate_model_id,
        checkpoint_sha256,
        conversion_manifest_sha256,
        metric_deltas,
        required_metrics,
    })
}

fn validate_training_agent_report(path: &Path) -> Result<()> {
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
    let expected: BTreeSet<&str> = [
        "train.dataset_hashes",
        "train.reproducible_config",
        "train.live_checkpoint",
        "train.benchmark_eval",
        "train.baseline_regression",
        "train.compute_ledger",
        "train.conversion_manifest",
        "train.promotion_manifest",
        "train.human_promotion_approval",
    ]
    .into_iter()
    .collect();
    let mut statuses = BTreeMap::new();
    for requirement in requirements {
        let Some(id) = requirement.get("id").and_then(Value::as_str) else {
            continue;
        };
        statuses.insert(
            id.to_string(),
            requirement
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("missing")
                .to_string(),
        );
    }
    for requirement in expected {
        if !statuses.contains_key(requirement) {
            bail!("agent report is missing production requirement {requirement}");
        }
    }
    for (id, status) in statuses {
        if id == "train.human_promotion_approval" {
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
        .filter(|blocker| !is_human_approval_blocker(blocker))
        .collect::<Vec<_>>();
    if !blockers.is_empty() {
        bail!(
            "agent report has non-human approval blockers: {}",
            blockers.join("; ")
        );
    }
    Ok(())
}

fn require_json_status_in(value: &Value, statuses: &[&str], path: &Path) -> Result<()> {
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .with_context(|| format!("{} is missing status", path.display()))?;
    if !statuses.contains(&status) {
        bail!(
            "{} has status {status}, expected one of {}",
            path.display(),
            statuses.join(", ")
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

fn require_any_sha256(value: &Value, fields: &[&str], path: &Path) -> Result<()> {
    if fields
        .iter()
        .filter_map(|field| value.get(*field).and_then(Value::as_str))
        .any(valid_sha256)
    {
        return Ok(());
    }
    bail!(
        "{} must include one of {} as a SHA-256 hex string",
        path.display(),
        fields.join(", ")
    );
}

fn hardware_run_passed(run: &Value) -> bool {
    run.get("status").and_then(Value::as_str) == Some("passed")
        && ["gpu_name", "gpu_arch", "driver_version", "cuda_toolkit"]
            .iter()
            .all(|field| string_field_nonempty(run, field))
}

fn string_field_nonempty(value: &Value, field: &str) -> bool {
    value
        .get(field)
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
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

fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
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

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn print_summary(summary: &Value, emit_json: bool) {
    if emit_json {
        println!(
            "{}",
            serde_json::to_string_pretty(summary).expect("serialize approval command summary")
        );
    } else {
        println!(
            "approval {} {}: {}",
            summary
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
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
