use super::common::{
    action_intent, capability, existing_artifact, repo_tool_check, seal_runtime,
    set_production_proof, ActionIntent, AgentMode, AgentReport, AgentStatus, CommandRecord,
    ProductionProofStatus, ProductionRequirement, TrustLevel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use walkdir::WalkDir;

const TRAINING_EVIDENCE_DIR_ENV: &str = "REFINEFORGE_TRAINING_EVIDENCE_DIR";
const TRAINING_CHECKPOINT_ENV: &str = "REFINEFORGE_TRAINING_CHECKPOINT";
const TRAINING_EVAL_REPORT_ENV: &str = "REFINEFORGE_TRAINING_EVAL_REPORT";
const TRAINING_REGRESSION_REPORT_ENV: &str = "REFINEFORGE_TRAINING_REGRESSION_REPORT";
const TRAINING_COMPUTE_LEDGER_ENV: &str = "REFINEFORGE_TRAINING_COMPUTE_LEDGER";
const TRAINING_PROMOTION_MANIFEST_ENV: &str = "REFINEFORGE_TRAINING_PROMOTION_MANIFEST";
const TRAINING_CONVERSION_MANIFEST_ENV: &str = "REFINEFORGE_TRAINING_CONVERSION_MANIFEST";
const TRAINING_HUMAN_APPROVAL_ENV: &str = "REFINEFORGE_TRAINING_HUMAN_APPROVAL";

pub fn build(
    root: &Path,
    mode: AgentMode,
    target: &str,
    out_dir: &Path,
    allow_expensive: bool,
) -> AgentReport {
    let mut report = AgentReport::new(super::common::AgentKind::Train, mode, target);
    report.capabilities.extend([
        capability(
            "dataset-audit",
            "available",
            "validates proof-repair SFT JSONL before any training execution",
        ),
        capability(
            "native-smoke-training",
            "available",
            "runs refineforge_native proof-repair smoke training without external trainer binaries",
        ),
        capability(
            "trainer-orchestration",
            "available",
            "invokes refine-train with explicit run directories and command evidence",
        ),
        capability(
            "training-production-proof-validation",
            "available",
            "validates checkpoint, eval, regression, compute ledger, conversion, lineage, promotion, and human approval files before any trust upgrade",
        ),
        capability(
            "loss-only-eval-linter",
            "available",
            "blocks model-quality trust when evaluation evidence contains only loss or perplexity metrics",
        ),
        capability(
            "safe-execute-default",
            "available",
            "execute mode runs trainer dry-run unless --allow-expensive is set",
        ),
        capability(
            "helyx-train-compatibility",
            "tool_gated",
            "HELYX backend execution is delegated to helyx-train when the config selects it",
        ),
    ]);
    report.tool_checks.extend([
        repo_tool_check(root, "refine-train", true),
        repo_tool_check(root, "helyx-train", false),
    ]);
    existing_artifact(
        root,
        "training/configs/refineforge-native-proof-repair-smoke.yaml",
        &mut report,
    );
    existing_artifact(
        root,
        "training/configs/lean-proof-repair-smoke-stub.yaml",
        &mut report,
    );
    existing_artifact(
        root,
        "training/data/mathlib-proof-repair-v1/anthropic-sft.jsonl",
        &mut report,
    );
    existing_artifact(
        root,
        "training/configs/helyx-mathlib-proof-repair-smoke.yaml",
        &mut report,
    );
    existing_artifact(
        root,
        "docs/training/training-production-proof.md",
        &mut report,
    );
    existing_artifact(root, "training/evals/proof-repair-smoke.yaml", &mut report);

    if !mode.runs_checks() {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::MeasuredOnly,
            "Training agent inspected local configs and dataset fixtures. No checkpoint improvement is claimed.",
        );
        apply_train_production_proof(
            root,
            target,
            &mut report,
            false,
            false,
            allow_expensive,
            None,
        );
        seal_runtime(
            root,
            Some(out_dir),
            &mut report,
            TrustLevel::MeasuredOnly,
            train_action_intents(mode, allow_expensive),
        );
        return report;
    }

    let audit_json = out_dir.join("training-data-audit.json");
    let runs_root = out_dir.join("training-runs");
    if let Err(err) = std::fs::create_dir_all(out_dir) {
        report.blockers.push(format!(
            "could not create training evidence directory {}: {err}",
            out_dir.display()
        ));
        report.finish(
            AgentStatus::Blocked,
            TrustLevel::Blocked,
            "Training dataset audit could not run because the evidence directory could not be created.",
        );
        seal_runtime(
            root,
            Some(out_dir),
            &mut report,
            TrustLevel::MeasuredOnly,
            train_action_intents(mode, allow_expensive),
        );
        return report;
    }
    let mut spec = train_command(root);
    spec.args.extend([
        "data".to_string(),
        "audit".to_string(),
        "training/data/mathlib-proof-repair-v1/anthropic-sft.jsonl".to_string(),
        "--output".to_string(),
        audit_json.display().to_string(),
    ]);
    let record = run_command(root, "training-data-audit", &spec);
    report.commands.push(record);
    report.artifacts.push(audit_json.clone());

    if mode == AgentMode::Execute
        && report
            .commands
            .last()
            .is_some_and(|c| c.status == AgentStatus::Passed)
    {
        let config = config_for_target(root, target);
        let mut run_spec = train_command(root);
        run_spec.args.extend([
            "--runs-root".to_string(),
            runs_root.display().to_string(),
            "run".to_string(),
            config.display().to_string(),
        ]);
        if !allow_expensive {
            run_spec.args.push("--dry-run".to_string());
        }
        let name = if allow_expensive {
            "training-run"
        } else {
            "training-run-dry-run"
        };
        let record = run_command(root, name, &run_spec);
        report.commands.push(record);
        report.artifacts.push(runs_root);
        if !allow_expensive {
            report.warnings.push(
                "training execute used --dry-run; rerun with --allow-expensive for a live backend run."
                    .to_string(),
            );
        }
    } else if mode == AgentMode::Repair {
        report.warnings.push(
            "training repair mode is evidence-only; no dataset or checkpoint files were mutated."
                .to_string(),
        );
    }

    finish_from_commands(&mut report, mode, allow_expensive);
    let dataset_audit_passed = report.commands.iter().any(|command| {
        command.name == "training-data-audit" && command.status == AgentStatus::Passed
    });
    let live_training_passed = report
        .commands
        .iter()
        .any(|command| command.name == "training-run" && command.status == AgentStatus::Passed);
    let production_status = apply_train_production_proof(
        root,
        target,
        &mut report,
        dataset_audit_passed,
        live_training_passed,
        allow_expensive,
        Some(&audit_json),
    );
    if report.status == AgentStatus::Passed
        && production_status == ProductionProofStatus::HumanReviewed
    {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::HumanReviewed,
            "Training dataset audit, live run, checkpoint, evaluation, regression, compute ledger, conversion manifest, promotion manifest, and named human approval evidence all passed.",
        );
    } else if report.status == AgentStatus::Passed
        && production_status == ProductionProofStatus::Blocked
        && train_production_only_missing_human_approval(&report)
    {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::MeasuredOnly,
            "Training live run and checkpoint, evaluation, regression, compute ledger, conversion, and promotion evidence passed. Production proof remains blocked until a named human approval file is provided.",
        );
    }
    let trust_ceiling = if production_status == ProductionProofStatus::HumanReviewed {
        TrustLevel::HumanReviewed
    } else {
        TrustLevel::MeasuredOnly
    };
    seal_runtime(
        root,
        Some(out_dir),
        &mut report,
        trust_ceiling,
        train_action_intents(mode, allow_expensive),
    );
    report
}

fn train_production_only_missing_human_approval(report: &AgentReport) -> bool {
    let requirements = &report.production_proof.requirements;
    !requirements.is_empty()
        && requirements.iter().all(|requirement| {
            requirement.status == AgentStatus::Passed
                || (requirement.id == "train.human_promotion_approval"
                    && requirement.status == AgentStatus::Blocked)
        })
        && report
            .production_proof
            .blockers
            .iter()
            .all(|blocker| blocker.contains("human promotion approval"))
}

fn apply_train_production_proof(
    root: &Path,
    target: &str,
    report: &mut AgentReport,
    dataset_audit_passed: bool,
    live_training_passed: bool,
    allow_expensive: bool,
    audit_json: Option<&Path>,
) -> ProductionProofStatus {
    let mut blockers = Vec::new();
    let mut requirements = Vec::new();
    let mut reviewer_evidence = Vec::new();
    let evidence_paths = TrainingEvidencePaths::from_env();
    let dataset_evidence = audit_json
        .filter(|_| dataset_audit_passed)
        .and_then(dataset_audit_evidence)
        .unwrap_or_else(|| "training-data-audit not run or failed".to_string());

    requirements.push(ProductionRequirement::new_owned(
        "train.dataset_hashes",
        "Dataset audit passes and records deterministic dataset hashes",
        if dataset_audit_passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![dataset_evidence],
    ));
    if !dataset_audit_passed {
        blockers.push(
            "dataset hash evidence is missing because training-data-audit did not pass".to_string(),
        );
    }

    let config_path = config_for_target(root, target);
    let config_evidence = config_hash_evidence(root, &config_path);
    let config_passed = dataset_audit_passed && config_evidence.passed;
    if let Some(path) = &config_evidence.artifact {
        report.artifacts.push(path.clone());
    }
    requirements.push(ProductionRequirement::new_owned(
        "train.reproducible_config",
        "Training config is explicit and tied to immutable dataset evidence",
        if config_passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        config_evidence.evidence,
    ));
    if !config_passed {
        blockers.push("reproducible config hash evidence is missing".to_string());
    }

    let checkpoint = validate_path(
        evidence_paths.checkpoint.as_deref(),
        "training checkpoint",
        true,
        TRAINING_CHECKPOINT_ENV,
        Some("training/checkpoint.safetensors"),
    );
    if let Some(path) = &checkpoint.artifact {
        report.artifacts.push(path.clone());
    }
    let checkpoint_passed = live_training_passed && allow_expensive && checkpoint.passed;
    let mut checkpoint_evidence = vec![if live_training_passed && allow_expensive {
        "live training command passed with --allow-expensive".to_string()
    } else if allow_expensive {
        "live training was requested but no successful training-run command was recorded"
            .to_string()
    } else {
        "training execute defaults to dry-run; live checkpoint evidence is absent".to_string()
    }];
    checkpoint_evidence.extend(checkpoint.evidence);
    requirements.push(ProductionRequirement::new_owned(
        "train.live_checkpoint",
        "Live backend training run produced checkpoint metadata",
        if checkpoint_passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        checkpoint_evidence,
    ));
    if !checkpoint_passed {
        blockers.push("live checkpoint evidence is missing".to_string());
    }

    let mut eval_report = validate_json_evidence(
        evidence_paths.eval_report.as_deref(),
        "training evaluation report",
        TRAINING_EVAL_REPORT_ENV,
        Some("training/eval-report.json"),
        eval_report_is_complete,
        "evaluation report must be passed and include baseline, candidate, and non-loss quality metrics",
    );
    let eval_loss_only = evidence_paths
        .eval_report
        .as_deref()
        .and_then(read_json)
        .is_some_and(|value| eval_report_is_loss_only(&value));
    if eval_loss_only {
        eval_report.evidence.push(
            "evaluation report is loss-only; held-out quality metrics are required".to_string(),
        );
    }
    if let Some(path) = &eval_report.artifact {
        report.artifacts.push(path.clone());
    }
    requirements.push(ProductionRequirement::new_owned(
        "train.benchmark_eval",
        "Benchmark evaluation compares baseline and candidate model quality",
        if eval_report.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        eval_report.evidence,
    ));
    if !eval_report.passed {
        if eval_loss_only {
            blockers.push(
                "loss-only evaluation evidence cannot support model-quality production proof"
                    .to_string(),
            );
        } else {
            blockers.push("evaluation evidence is missing".to_string());
        }
    }

    let regression_report = validate_json_evidence(
        evidence_paths.regression_report.as_deref(),
        "training regression report",
        TRAINING_REGRESSION_REPORT_ENV,
        Some("training/regression-report.json"),
        regression_report_is_complete,
        "regression report must be passed and include baseline, candidate, and metric delta data",
    );
    if let Some(path) = &regression_report.artifact {
        report.artifacts.push(path.clone());
    }
    requirements.push(ProductionRequirement::new_owned(
        "train.baseline_regression",
        "Required benchmark metrics do not regress below threshold",
        if regression_report.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        regression_report.evidence,
    ));
    if !regression_report.passed {
        blockers.push("baseline regression evidence is missing".to_string());
    }

    let compute_ledger = validate_json_evidence(
        evidence_paths.compute_ledger.as_deref(),
        "training compute ledger",
        TRAINING_COMPUTE_LEDGER_ENV,
        Some("training/compute-ledger.json"),
        compute_ledger_is_complete,
        "compute ledger must be passed and include backend, device, and duration/budget data",
    );
    if let Some(path) = &compute_ledger.artifact {
        report.artifacts.push(path.clone());
    }
    requirements.push(ProductionRequirement::new_owned(
        "train.compute_ledger",
        "Compute ledger records backend, device, duration, and run budget",
        if compute_ledger.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        compute_ledger.evidence,
    ));
    if !compute_ledger.passed {
        blockers.push("compute ledger evidence is missing".to_string());
    }

    let mut conversion_manifest = validate_json_evidence(
        evidence_paths.conversion_manifest.as_deref(),
        "training conversion manifest",
        TRAINING_CONVERSION_MANIFEST_ENV,
        Some("training/conversion-manifest.json"),
        conversion_manifest_is_complete,
        "conversion manifest must be passed and include source/target formats, checkpoint hash, and converted artifacts",
    );
    if conversion_manifest.passed {
        match (
            checkpoint.sha256.as_deref(),
            conversion_checkpoint_sha256(evidence_paths.conversion_manifest.as_deref()).as_deref(),
        ) {
            (Some(actual), Some(declared)) if actual.eq_ignore_ascii_case(declared) => {
                conversion_manifest
                    .evidence
                    .push("conversion checkpoint_sha256 matches checkpoint artifact".to_string());
            }
            (Some(actual), Some(declared)) => {
                conversion_manifest.passed = false;
                conversion_manifest.evidence.push(format!(
                    "conversion checkpoint_sha256 mismatch: manifest={declared} checkpoint={actual}"
                ));
            }
            _ => {
                conversion_manifest.passed = false;
                conversion_manifest.evidence.push(
                    "conversion checkpoint_sha256 could not be compared to checkpoint artifact"
                        .to_string(),
                );
            }
        }
    }
    if let Some(path) = &conversion_manifest.artifact {
        report.artifacts.push(path.clone());
    }
    requirements.push(ProductionRequirement::new_owned(
        "train.conversion_manifest",
        "Checkpoint conversion evidence records source format, target format, checkpoint hash, and output artifacts",
        if checkpoint_passed && conversion_manifest.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        conversion_manifest.evidence,
    ));
    if !(checkpoint_passed && conversion_manifest.passed) {
        blockers.push(
            "conversion manifest evidence is missing or checkpoint hash mismatch".to_string(),
        );
    }

    let mut promotion_manifest = validate_json_evidence(
        evidence_paths.promotion_manifest.as_deref(),
        "training promotion manifest",
        TRAINING_PROMOTION_MANIFEST_ENV,
        Some("training/promotion-manifest.json"),
        promotion_manifest_is_complete,
        "promotion manifest must be approved/promote and include model id, checkpoint hash, rollback, lineage, and conversion data",
    );
    if promotion_manifest.passed {
        match (
            checkpoint.sha256.as_deref(),
            promotion_checkpoint_sha256(evidence_paths.promotion_manifest.as_deref()).as_deref(),
        ) {
            (Some(actual), Some(declared)) if actual.eq_ignore_ascii_case(declared) => {
                promotion_manifest
                    .evidence
                    .push("promotion checkpoint_sha256 matches checkpoint artifact".to_string());
            }
            (Some(actual), Some(declared)) => {
                promotion_manifest.passed = false;
                promotion_manifest.evidence.push(format!(
                    "promotion checkpoint_sha256 mismatch: manifest={declared} checkpoint={actual}"
                ));
            }
            _ => {
                promotion_manifest.passed = false;
                promotion_manifest.evidence.push(
                    "promotion checkpoint_sha256 could not be compared to checkpoint artifact"
                        .to_string(),
                );
            }
        }
        match (
            conversion_manifest.sha256.as_deref(),
            promotion_conversion_manifest_sha256(evidence_paths.promotion_manifest.as_deref())
                .as_deref(),
        ) {
            (Some(actual), Some(declared)) if actual.eq_ignore_ascii_case(declared) => {
                promotion_manifest.evidence.push(
                    "promotion conversion manifest_sha256 matches conversion artifact".to_string(),
                );
            }
            (Some(actual), Some(declared)) => {
                promotion_manifest.passed = false;
                promotion_manifest.evidence.push(format!(
                    "promotion conversion manifest_sha256 mismatch: manifest={declared} conversion={actual}"
                ));
            }
            _ => {
                promotion_manifest.passed = false;
                promotion_manifest.evidence.push(
                    "promotion conversion manifest_sha256 could not be compared to conversion artifact"
                        .to_string(),
                );
            }
        }
    }
    if let Some(path) = &promotion_manifest.artifact {
        report.artifacts.push(path.clone());
    }
    requirements.push(ProductionRequirement::new_owned(
        "train.promotion_manifest",
        "Promotion manifest records model id, checkpoint hash, lineage, conversion, metrics, and rollback path",
        if checkpoint_passed && conversion_manifest.passed && promotion_manifest.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        promotion_manifest.evidence,
    ));
    if !(checkpoint_passed && conversion_manifest.passed && promotion_manifest.passed) {
        blockers.push(
            "promotion manifest evidence is missing, lacks lineage/conversion, or checkpoint hash / conversion hash evidence mismatches"
                .to_string(),
        );
    }

    let approval = validate_training_approval(evidence_paths.approval.as_deref());
    if let Some(path) = &approval.artifact {
        report.artifacts.push(path.clone());
    }
    if let Some(reviewer) = &approval.reviewer_evidence {
        reviewer_evidence.push(reviewer.clone());
    }
    requirements.push(ProductionRequirement::new_owned(
        "train.human_promotion_approval",
        "Named human reviewer approved model promotion",
        if approval.passed {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        approval.evidence,
    ));
    if !approval.passed {
        blockers.push("human promotion approval is missing".to_string());
    }

    let status = if blockers.is_empty() {
        ProductionProofStatus::HumanReviewed
    } else {
        ProductionProofStatus::Blocked
    };
    set_production_proof(report, status, requirements, reviewer_evidence, blockers);
    status
}

fn train_action_intents(mode: AgentMode, allow_expensive: bool) -> Vec<ActionIntent> {
    let run_policy = if allow_expensive {
        "writes_evidence_and_may_run_backend_compute"
    } else {
        "writes_evidence_and_forces_dry_run"
    };
    vec![
        action_intent(
            "train.inspect.fixtures",
            "Inspect training configs, datasets, and HELYX compatibility surfaces",
            "inspect",
            "read_only",
            "refine agent train --mode inspect",
            &[
                "training/configs/*.yaml",
                "training/data/mathlib-proof-repair-v1/anthropic-sft.jsonl",
            ],
        ),
        action_intent(
            "train.audit.dataset",
            "Audit dataset records before any trainer execution",
            "verify",
            "writes_evidence",
            "refine agent train --mode check",
            &["training-data-audit", "training-data-audit.json"],
        ),
        action_intent(
            "train.execute.run",
            "Run trainer orchestration with explicit dry-run or live-compute policy",
            "execute",
            run_policy,
            &format!("refine agent train --mode {}", mode.as_str()),
            &[
                "training-runs",
                "refine-train command record",
                "--allow-expensive policy",
            ],
        ),
        action_intent(
            "train.promote.guard",
            "Block model-quality claims without benchmark and checkpoint promotion evidence",
            "audit",
            "evidence_only",
            "refine agent train --mode execute",
            &[
                "evaluation metrics",
                "checkpoint metadata",
                "promotion approval",
            ],
        ),
    ]
}

fn finish_from_commands(report: &mut AgentReport, mode: AgentMode, allow_expensive: bool) {
    if report
        .commands
        .iter()
        .any(|command| command.status == AgentStatus::Blocked)
    {
        report.finish(
            AgentStatus::Blocked,
            TrustLevel::Blocked,
            "Training command could not run because a required trainer binary was unavailable.",
        );
    } else if report
        .commands
        .iter()
        .any(|command| command.status == AgentStatus::Failed)
    {
        report.finish(
            AgentStatus::Failed,
            TrustLevel::Blocked,
            "Training command ran and failed. See command stderr/stdout evidence.",
        );
    } else if mode == AgentMode::Execute {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::MeasuredOnly,
            if allow_expensive {
                "Training dataset audit and live trainer execution passed. Model quality still requires evaluation evidence."
            } else {
                "Training dataset audit and trainer dry-run passed. This proves training control-plane readiness, not model improvement."
            },
        );
    } else {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::MeasuredOnly,
            "Training dataset audit passed. This is measured training readiness, not model improvement evidence.",
        );
    }
}

fn config_for_target(root: &Path, target: &str) -> PathBuf {
    let target_path = PathBuf::from(target);
    if matches!(
        target_path.extension().and_then(|ext| ext.to_str()),
        Some("yaml" | "yml")
    ) {
        return target_path;
    }
    let default = PathBuf::from("training/configs/refineforge-native-proof-repair-smoke.yaml");
    if root.join(&default).exists() {
        default
    } else {
        target_path
    }
}

fn dataset_audit_evidence(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let sha = value
        .get("dataset_sha256")
        .or_else(|| value.get("sha256"))
        .and_then(|v| v.as_str())?;
    let rows = value
        .get("record_count")
        .or_else(|| value.get("total_rows"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Some(format!(
        "{} dataset_sha256={} record_count={}",
        path.display(),
        sha,
        rows
    ))
}

#[derive(Debug, Default)]
struct TrainingEvidencePaths {
    checkpoint: Option<PathBuf>,
    eval_report: Option<PathBuf>,
    regression_report: Option<PathBuf>,
    compute_ledger: Option<PathBuf>,
    promotion_manifest: Option<PathBuf>,
    conversion_manifest: Option<PathBuf>,
    approval: Option<PathBuf>,
}

impl TrainingEvidencePaths {
    fn from_env() -> Self {
        let base = env_path(TRAINING_EVIDENCE_DIR_ENV);
        Self {
            checkpoint: env_or_conventional_path(
                TRAINING_CHECKPOINT_ENV,
                base.as_deref(),
                "training/checkpoint.safetensors",
            ),
            eval_report: env_or_conventional_path(
                TRAINING_EVAL_REPORT_ENV,
                base.as_deref(),
                "training/eval-report.json",
            ),
            regression_report: env_or_conventional_path(
                TRAINING_REGRESSION_REPORT_ENV,
                base.as_deref(),
                "training/regression-report.json",
            ),
            compute_ledger: env_or_conventional_path(
                TRAINING_COMPUTE_LEDGER_ENV,
                base.as_deref(),
                "training/compute-ledger.json",
            ),
            promotion_manifest: env_or_conventional_path(
                TRAINING_PROMOTION_MANIFEST_ENV,
                base.as_deref(),
                "training/promotion-manifest.json",
            ),
            conversion_manifest: env_or_conventional_path(
                TRAINING_CONVERSION_MANIFEST_ENV,
                base.as_deref(),
                "training/conversion-manifest.json",
            ),
            approval: env_or_conventional_path(
                TRAINING_HUMAN_APPROVAL_ENV,
                base.as_deref(),
                "approvals/training.json",
            ),
        }
    }
}

#[derive(Debug, Default)]
struct EvidenceValidation {
    passed: bool,
    evidence: Vec<String>,
    artifact: Option<PathBuf>,
    sha256: Option<String>,
    reviewer_evidence: Option<String>,
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn env_or_conventional_path(key: &str, base: Option<&Path>, relative: &str) -> Option<PathBuf> {
    env_path(key).or_else(|| base.map(|base| base.join(relative)))
}

fn config_hash_evidence(root: &Path, config_path: &Path) -> EvidenceValidation {
    let resolved = resolve_root_path(root, config_path);
    let mut validation = validate_path(Some(&resolved), "training config", true, "target", None);
    if validation.passed {
        validation
            .evidence
            .push(format!("config_path={}", resolved.display()));
    }
    validation
}

fn validate_path(
    path: Option<&Path>,
    label: &str,
    allow_dir: bool,
    env_key: &str,
    conventional_rel: Option<&str>,
) -> EvidenceValidation {
    let Some(path) = path else {
        let conventional = conventional_rel
            .map(|rel| format!(" or {TRAINING_EVIDENCE_DIR_ENV}/{rel}"))
            .unwrap_or_default();
        return EvidenceValidation {
            passed: false,
            evidence: vec![format!("{label} not provided via {env_key}{conventional}")],
            artifact: None,
            sha256: None,
            reviewer_evidence: None,
        };
    };

    let mut evidence = vec![format!("{label} path={}", path.display())];
    let exists = if allow_dir {
        path.exists()
    } else {
        path.is_file()
    };
    let artifact = Some(path.to_path_buf());
    if !exists {
        evidence.push(format!("{label} missing: {}", path.display()));
        return EvidenceValidation {
            passed: false,
            evidence,
            artifact,
            sha256: None,
            reviewer_evidence: None,
        };
    }

    let sha256 = match hash_path(path) {
        Ok(hash) => {
            evidence.push(format!("{} sha256={hash}", path.display()));
            Some(hash)
        }
        Err(err) => {
            evidence.push(format!("{} unreadable: {err}", path.display()));
            return EvidenceValidation {
                passed: false,
                evidence,
                artifact,
                sha256: None,
                reviewer_evidence: None,
            };
        }
    };

    EvidenceValidation {
        passed: true,
        evidence,
        artifact,
        sha256,
        reviewer_evidence: None,
    }
}

fn validate_json_evidence(
    path: Option<&Path>,
    label: &str,
    env_key: &str,
    conventional_rel: Option<&str>,
    predicate: fn(&Value) -> bool,
    predicate_detail: &str,
) -> EvidenceValidation {
    let mut validation = validate_path(path, label, false, env_key, conventional_rel);
    if !validation.passed {
        return validation;
    }

    let Some(path) = path else {
        return validation;
    };
    match read_json(path) {
        Some(value) if predicate(&value) => validation,
        Some(_) => {
            validation.passed = false;
            validation.evidence.push(predicate_detail.to_string());
            validation
        }
        None => {
            validation.passed = false;
            validation
                .evidence
                .push(format!("{label} is not valid JSON"));
            validation
        }
    }
}

fn validate_training_approval(path: Option<&Path>) -> EvidenceValidation {
    let mut validation = validate_path(
        path,
        "training human approval",
        false,
        TRAINING_HUMAN_APPROVAL_ENV,
        Some("approvals/training.json"),
    );
    if !validation.passed {
        return validation;
    }

    let Some(path) = path else {
        return validation;
    };
    let Some(value) = read_json(path) else {
        validation.passed = false;
        validation
            .evidence
            .push("training human approval is not valid JSON".to_string());
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
        .is_some_and(|role| role.eq_ignore_ascii_case("training"));
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
        validation.reviewer_evidence = Some(format!("training: {operator} ({})", path.display()));
    } else if !human_ok {
        validation
            .evidence
            .push(format!("AI/automated approval rejected: {operator}"));
    } else {
        validation.evidence.push(
            "approval must have schema refineforge-human-approval-v1, role training, decision approved, approved_at, and evidence_summary"
                .to_string(),
        );
    }
    validation
}

fn eval_report_is_complete(value: &Value) -> bool {
    json_status_is(value, "passed")
        && object_field_nonempty(value, "metrics")
        && eval_report_has_quality_metrics(value)
        && !eval_report_is_loss_only(value)
        && (object_field_nonempty(value, "baseline")
            || string_field_nonempty(value, "baseline_report"))
        && (object_field_nonempty(value, "candidate")
            || string_field_nonempty(value, "candidate_report"))
}

fn eval_report_has_quality_metrics(value: &Value) -> bool {
    metrics_object(value, "quality_metrics")
        .or_else(|| metrics_object(value, "metrics"))
        .is_some_and(|metrics| {
            metrics
                .iter()
                .any(|(name, value)| value.is_number() && !metric_name_is_loss_only(name))
        })
}

fn eval_report_is_loss_only(value: &Value) -> bool {
    let Some(metrics) =
        metrics_object(value, "quality_metrics").or_else(|| metrics_object(value, "metrics"))
    else {
        return false;
    };
    let numeric_metric_names: Vec<&str> = metrics
        .iter()
        .filter_map(|(name, value)| value.is_number().then_some(name.as_str()))
        .collect();
    !numeric_metric_names.is_empty()
        && numeric_metric_names
            .iter()
            .all(|name| metric_name_is_loss_only(name))
}

fn metrics_object<'a>(value: &'a Value, field: &str) -> Option<&'a serde_json::Map<String, Value>> {
    value.get(field).and_then(Value::as_object)
}

fn metric_name_is_loss_only(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    normalized.contains("loss")
        || normalized.contains("perplexity")
        || normalized == "ppl"
        || normalized == "nll"
}

fn regression_report_is_complete(value: &Value) -> bool {
    json_status_is(value, "passed")
        && (string_field_nonempty(value, "baseline_report")
            || object_field_nonempty(value, "baseline"))
        && (string_field_nonempty(value, "candidate_report")
            || object_field_nonempty(value, "candidate"))
        && (object_field_nonempty(value, "metric_deltas")
            || object_field_nonempty(value, "metrics"))
}

fn compute_ledger_is_complete(value: &Value) -> bool {
    json_status_is(value, "passed")
        && (string_field_nonempty(value, "backend_kind") || string_field_nonempty(value, "backend"))
        && (value.get("device").is_some() || value.get("devices").is_some())
        && (value.get("duration_ms").is_some()
            || value.get("duration_seconds").is_some()
            || value.get("gpu_hours").is_some()
            || value.get("run_budget").is_some())
}

fn promotion_manifest_is_complete(value: &Value) -> bool {
    (json_status_is(value, "approved") || json_field_is(value, "decision", "promote"))
        && string_field_nonempty(value, "model_id")
        && value.get("rollback").is_some_and(Value::is_object)
        && value
            .get("checkpoint_sha256")
            .and_then(Value::as_str)
            .is_some_and(valid_sha256)
        && lineage_is_complete(value.get("lineage"))
        && conversion_reference_is_complete(value.get("conversion"))
}

fn conversion_manifest_is_complete(value: &Value) -> bool {
    json_status_is(value, "passed")
        && string_field_nonempty(value, "source_format")
        && string_field_nonempty(value, "target_format")
        && value
            .get("checkpoint_sha256")
            .and_then(Value::as_str)
            .is_some_and(valid_sha256)
        && value
            .get("artifacts")
            .and_then(Value::as_array)
            .is_some_and(|artifacts| {
                !artifacts.is_empty() && artifacts.iter().all(artifact_is_complete)
            })
}

fn artifact_is_complete(value: &Value) -> bool {
    value
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(|path| !path.trim().is_empty())
        && value
            .get("sha256")
            .and_then(Value::as_str)
            .is_some_and(valid_sha256)
}

fn lineage_is_complete(value: Option<&Value>) -> bool {
    let Some(lineage) = value else {
        return false;
    };
    ["config_sha256", "train_metadata_sha256", "tokenizer_sha256"]
        .iter()
        .all(|field| {
            lineage
                .get(*field)
                .and_then(Value::as_str)
                .is_some_and(valid_sha256)
        })
        && lineage
            .get("checkpoint_shards")
            .and_then(Value::as_array)
            .is_some_and(|shards| !shards.is_empty() && shards.iter().all(artifact_is_complete))
        && string_field_nonempty(lineage, "ema_policy")
        && string_field_nonempty(lineage, "resume_source")
        && lineage
            .get("epoch")
            .is_some_and(|epoch| epoch.is_u64() || epoch.is_i64() || epoch.is_f64())
}

fn conversion_reference_is_complete(value: Option<&Value>) -> bool {
    let Some(conversion) = value else {
        return false;
    };
    string_field_nonempty(conversion, "manifest_path")
        && conversion
            .get("manifest_sha256")
            .and_then(Value::as_str)
            .is_some_and(valid_sha256)
}

fn promotion_checkpoint_sha256(path: Option<&Path>) -> Option<String> {
    read_json(path?)?
        .get("checkpoint_sha256")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn promotion_conversion_manifest_sha256(path: Option<&Path>) -> Option<String> {
    read_json(path?)?
        .pointer("/conversion/manifest_sha256")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn conversion_checkpoint_sha256(path: Option<&Path>) -> Option<String> {
    read_json(path?)?
        .get("checkpoint_sha256")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn json_status_is(value: &Value, expected: &str) -> bool {
    json_field_is(value, "status", expected)
}

fn json_field_is(value: &Value, field: &str, expected: &str) -> bool {
    value
        .get(field)
        .and_then(Value::as_str)
        .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
}

fn string_field_nonempty(value: &Value, field: &str) -> bool {
    value
        .get(field)
        .and_then(Value::as_str)
        .is_some_and(|actual| !actual.trim().is_empty())
}

fn object_field_nonempty(value: &Value, field: &str) -> bool {
    value
        .get(field)
        .and_then(Value::as_object)
        .is_some_and(|object| !object.is_empty())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_automated_operator(operator: &str) -> bool {
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

fn resolve_root_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn hash_path(path: &Path) -> std::io::Result<String> {
    if path.is_dir() {
        return hash_dir(path);
    }
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

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn normalize_relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[derive(Clone)]
struct CommandSpec {
    program: String,
    args: Vec<String>,
}

fn train_command(root: &Path) -> CommandSpec {
    if let Ok(bin) = std::env::var("REFINEFORGE_REFINE_TRAIN_BIN") {
        return CommandSpec {
            program: bin,
            args: Vec::new(),
        };
    }
    if root.join("Cargo.toml").exists() {
        return CommandSpec {
            program: "cargo".to_string(),
            args: vec![
                "run".to_string(),
                "-p".to_string(),
                "refineforge-trainer".to_string(),
                "--bin".to_string(),
                "refine-train".to_string(),
                "--".to_string(),
            ],
        };
    }
    CommandSpec {
        program: "refine-train".to_string(),
        args: Vec::new(),
    }
}

fn run_command(root: &Path, name: &str, spec: &CommandSpec) -> CommandRecord {
    let started = Instant::now();
    let output = Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(root)
        .output();
    match output {
        Ok(output) => CommandRecord {
            name: name.to_string(),
            command: std::iter::once(spec.program.clone())
                .chain(spec.args.iter().cloned())
                .collect(),
            status: if output.status.success() {
                AgentStatus::Passed
            } else {
                AgentStatus::Failed
            },
            duration_ms: started.elapsed().as_millis(),
            exit_code: output.status.code(),
            stdout_tail: Some(tail(&output.stdout)),
            stderr_tail: Some(tail(&output.stderr)),
        },
        Err(err) => CommandRecord {
            name: name.to_string(),
            command: std::iter::once(spec.program.clone())
                .chain(spec.args.iter().cloned())
                .collect(),
            status: AgentStatus::Blocked,
            duration_ms: started.elapsed().as_millis(),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: Some(err.to_string()),
        },
    }
}

fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut lines: Vec<&str> = text.lines().rev().take(20).collect();
    lines.reverse();
    lines.join("\n")
}
