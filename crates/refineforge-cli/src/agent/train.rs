use super::common::{
    action_intent, capability, existing_artifact, repo_tool_check, seal_runtime,
    set_production_proof, ActionIntent, AgentMode, AgentReport, AgentStatus, CommandRecord,
    ProductionProofStatus, ProductionRequirement, TrustLevel,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

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
        apply_train_production_proof(&mut report, false, false, allow_expensive, None);
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
    apply_train_production_proof(
        &mut report,
        dataset_audit_passed,
        live_training_passed,
        allow_expensive,
        Some(&audit_json),
    );
    seal_runtime(
        root,
        Some(out_dir),
        &mut report,
        TrustLevel::MeasuredOnly,
        train_action_intents(mode, allow_expensive),
    );
    report
}

fn apply_train_production_proof(
    report: &mut AgentReport,
    dataset_audit_passed: bool,
    live_training_passed: bool,
    allow_expensive: bool,
    audit_json: Option<&Path>,
) {
    let mut blockers = Vec::new();
    let mut requirements = Vec::new();
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

    requirements.push(ProductionRequirement::new(
        "train.reproducible_config",
        "Training config is explicit and tied to immutable dataset evidence",
        if dataset_audit_passed {
            AgentStatus::Partial
        } else {
            AgentStatus::Blocked
        },
        &[if dataset_audit_passed {
            "local config and dataset audit exist; config hash promotion still required"
        } else {
            "config cannot be promoted until dataset audit passes"
        }],
    ));

    requirements.push(ProductionRequirement::new(
        "train.live_checkpoint",
        "Live backend training run produced checkpoint metadata",
        if live_training_passed && allow_expensive {
            AgentStatus::Partial
        } else {
            AgentStatus::Blocked
        },
        &[if live_training_passed && allow_expensive {
            "live training command passed; checkpoint metadata still must be promoted"
        } else if allow_expensive {
            "live training was requested but no successful checkpoint evidence was found"
        } else {
            "training execute defaults to dry-run; live checkpoint evidence is absent"
        }],
    ));
    if !live_training_passed || !allow_expensive {
        blockers.push("live checkpoint evidence is missing".to_string());
    }

    let eval_report = std::env::var("REFINEFORGE_TRAINING_EVAL_REPORT").ok();
    requirements.push(ProductionRequirement::new_owned(
        "train.benchmark_eval",
        "Benchmark evaluation compares baseline and candidate model quality",
        if eval_report.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![eval_report.unwrap_or_else(|| {
            "evaluation report not provided via REFINEFORGE_TRAINING_EVAL_REPORT".to_string()
        })],
    ));
    if std::env::var("REFINEFORGE_TRAINING_EVAL_REPORT").is_err() {
        blockers.push("evaluation evidence is missing".to_string());
    }

    let regression_report = std::env::var("REFINEFORGE_TRAINING_REGRESSION_REPORT").ok();
    requirements.push(ProductionRequirement::new_owned(
        "train.baseline_regression",
        "Required benchmark metrics do not regress below threshold",
        if regression_report.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![regression_report.unwrap_or_else(|| {
            "baseline regression report not provided via REFINEFORGE_TRAINING_REGRESSION_REPORT"
                .to_string()
        })],
    ));
    if std::env::var("REFINEFORGE_TRAINING_REGRESSION_REPORT").is_err() {
        blockers.push("baseline regression evidence is missing".to_string());
    }

    let compute_ledger = std::env::var("REFINEFORGE_TRAINING_COMPUTE_LEDGER").ok();
    requirements.push(ProductionRequirement::new_owned(
        "train.compute_ledger",
        "Compute ledger records backend, device, duration, and run budget",
        if compute_ledger.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![compute_ledger.unwrap_or_else(|| {
            "compute ledger not provided via REFINEFORGE_TRAINING_COMPUTE_LEDGER".to_string()
        })],
    ));
    if std::env::var("REFINEFORGE_TRAINING_COMPUTE_LEDGER").is_err() {
        blockers.push("compute ledger evidence is missing".to_string());
    }

    let promotion_manifest = std::env::var("REFINEFORGE_TRAINING_PROMOTION_MANIFEST").ok();
    requirements.push(ProductionRequirement::new_owned(
        "train.promotion_manifest",
        "Promotion manifest records model id, checkpoint hash, metrics, and rollback path",
        if promotion_manifest.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![promotion_manifest.unwrap_or_else(|| {
            "promotion manifest not provided via REFINEFORGE_TRAINING_PROMOTION_MANIFEST"
                .to_string()
        })],
    ));
    if std::env::var("REFINEFORGE_TRAINING_PROMOTION_MANIFEST").is_err() {
        blockers.push("promotion manifest evidence is missing".to_string());
    }

    let approval = std::env::var("REFINEFORGE_TRAINING_HUMAN_APPROVAL").ok();
    requirements.push(ProductionRequirement::new_owned(
        "train.human_promotion_approval",
        "Named human reviewer approved model promotion",
        if approval.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![approval
            .clone()
            .unwrap_or_else(|| "human promotion approval is absent".to_string())],
    ));
    if approval.is_none() {
        blockers.push("human promotion approval is missing".to_string());
    }

    set_production_proof(
        report,
        if blockers.is_empty() {
            ProductionProofStatus::HumanReviewed
        } else {
            ProductionProofStatus::Blocked
        },
        requirements,
        approval.into_iter().collect(),
        blockers,
    );
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
