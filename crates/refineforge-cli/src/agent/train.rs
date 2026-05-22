use super::common::{
    capability, existing_artifact, repo_tool_check, AgentMode, AgentReport, AgentStatus,
    CommandRecord, TrustLevel,
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

    if !mode.runs_checks() {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::MeasuredOnly,
            "Training agent inspected local configs and dataset fixtures. No checkpoint improvement is claimed.",
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
    report.artifacts.push(audit_json);

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
    report
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
                "Training dataset audit and trainer dry-run passed. This proves orchestration readiness, not model improvement."
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
    let default = PathBuf::from("training/configs/lean-proof-repair-smoke-stub.yaml");
    if root.join(&default).exists() {
        default
    } else {
        target_path
    }
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
