use super::common::{
    capability, existing_artifact, repo_tool_check, AgentMode, AgentReport, AgentStatus,
    CommandRecord, TrustLevel,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

pub fn build(root: &Path, mode: AgentMode, target: &str, out_dir: &Path) -> AgentReport {
    let mut report = AgentReport::new(super::common::AgentKind::Kernel, mode, target);
    report.capabilities.extend([
        capability(
            "bitexact-contract-lint",
            "available",
            "validates HELYX-compatible kernel metadata before execution",
        ),
        capability(
            "deterministic-run-gate",
            "available",
            "execute mode runs refine-bitexact and writes per-run evidence",
        ),
        capability(
            "baseline-hash-enforcement",
            "available",
            "expected SHA-256 baselines fail stable-but-wrong outputs",
        ),
        capability(
            "helyx-kernels-boundary",
            "tool_gated",
            "real CUDA kernels remain external; Refine-Forge owns the evidence gate",
        ),
    ]);
    report.tool_checks.extend([
        repo_tool_check(root, "refine-bitexact", true),
        repo_tool_check(root, "helyx-kernels", false),
    ]);
    existing_artifact(
        root,
        "kernels/configs/helyx-bitexact-smoke.yaml",
        &mut report,
    );
    existing_artifact(
        root,
        "kernels/fixtures/helyx-bitexact-input.txt",
        &mut report,
    );
    existing_artifact(root, "kernels/README.md", &mut report);

    if !mode.runs_checks() {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::MeasuredOnly,
            "Kernel agent inspected bit-exact configs and fixtures. CUDA semantic correctness is not claimed.",
        );
        return report;
    }

    let lint_json = out_dir.join("bitexact-lint.json");
    let runs_root = out_dir.join("kernel-runs");
    let config = config_for_target(root, target);
    if let Err(err) = std::fs::create_dir_all(out_dir) {
        report.blockers.push(format!(
            "could not create kernel evidence directory {}: {err}",
            out_dir.display()
        ));
        report.finish(
            AgentStatus::Blocked,
            TrustLevel::Blocked,
            "Bit-exact kernel lint could not run because the evidence directory could not be created.",
        );
        return report;
    }
    let mut spec = bitexact_command(root);
    spec.args.extend([
        "lint".to_string(),
        config.display().to_string(),
        "--json".to_string(),
        "--output".to_string(),
        lint_json.display().to_string(),
    ]);
    let record = run_command(root, "bitexact-lint", &spec);
    report.commands.push(record);
    report.artifacts.push(lint_json);

    if mode == AgentMode::Execute
        && report
            .commands
            .last()
            .is_some_and(|c| c.status == AgentStatus::Passed)
    {
        let mut run_spec = bitexact_command(root);
        run_spec.args.extend([
            "--runs-root".to_string(),
            runs_root.display().to_string(),
            "run".to_string(),
            config.display().to_string(),
        ]);
        let record = run_command(root, "bitexact-run", &run_spec);
        report.commands.push(record);
        report.artifacts.push(runs_root);
    } else if mode == AgentMode::Repair {
        report.warnings.push(
            "kernel repair mode is evidence-only; no kernel config or source files were mutated."
                .to_string(),
        );
    }

    finish_from_commands(&mut report, mode);
    report
}

fn finish_from_commands(report: &mut AgentReport, mode: AgentMode) {
    if report
        .commands
        .iter()
        .any(|command| command.status == AgentStatus::Blocked)
    {
        report.finish(
            AgentStatus::Blocked,
            TrustLevel::Blocked,
            "Bit-exact kernel command could not run because refine-bitexact was unavailable.",
        );
    } else if report
        .commands
        .iter()
        .any(|command| command.status == AgentStatus::Failed)
    {
        report.finish(
            AgentStatus::Failed,
            TrustLevel::Blocked,
            "Bit-exact kernel command ran and failed. See command stderr/stdout evidence.",
        );
    } else if mode == AgentMode::Execute {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::MeasuredOnly,
            "Bit-exact kernel lint and run passed. This records deterministic gate evidence, not CUDA semantic correctness.",
        );
    } else {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::MeasuredOnly,
            "Bit-exact kernel lint passed. This records deterministic evidence readiness, not CUDA correctness.",
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
    let default = PathBuf::from("kernels/configs/helyx-bitexact-smoke.yaml");
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

fn bitexact_command(root: &Path) -> CommandSpec {
    if let Ok(bin) = std::env::var("REFINEFORGE_REFINE_BITEXACT_BIN") {
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
                "refineforge-bitexact".to_string(),
                "--bin".to_string(),
                "refine-bitexact".to_string(),
                "--".to_string(),
            ],
        };
    }
    CommandSpec {
        program: "refine-bitexact".to_string(),
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
