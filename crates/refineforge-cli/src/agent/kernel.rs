use super::common::{
    existing_artifact, AgentMode, AgentReport, AgentStatus, CommandRecord, TrustLevel,
};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub fn build(root: &Path, mode: AgentMode, target: &str, out_dir: &Path) -> AgentReport {
    let mut report = AgentReport::new(super::common::AgentKind::Kernel, mode, target);
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
        "kernels/configs/helyx-bitexact-smoke.yaml".to_string(),
        "--json".to_string(),
        "--output".to_string(),
        lint_json.display().to_string(),
    ]);
    let record = run_command(root, "bitexact-lint", &spec);
    report.commands.push(record);
    report.artifacts.push(lint_json);
    match report.commands.last().map(|c| c.status) {
        Some(AgentStatus::Passed) => report.finish(
            AgentStatus::Passed,
            TrustLevel::MeasuredOnly,
            "Bit-exact kernel lint passed. This records deterministic evidence readiness, not CUDA correctness.",
        ),
        Some(AgentStatus::Blocked) => report.finish(
            AgentStatus::Blocked,
            TrustLevel::Blocked,
            "Bit-exact kernel lint could not run because refine-bitexact was unavailable.",
        ),
        _ => report.finish(
            AgentStatus::Failed,
            TrustLevel::Blocked,
            "Bit-exact kernel lint ran and failed. See command stderr/stdout evidence.",
        ),
    }
    report
}

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
