use super::common::{
    action_intent, capability, existing_artifact, repo_tool_check, seal_runtime,
    set_production_proof, ActionIntent, AgentMode, AgentReport, AgentStatus, CommandRecord,
    ProductionProofStatus, ProductionRequirement, TrustLevel,
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
    existing_artifact(root, "kernels/hardware-matrix.example.json", &mut report);
    existing_artifact(root, "kernels/README.md", &mut report);
    existing_artifact(root, "docs/kernels/kernel-production-proof.md", &mut report);

    if !mode.runs_checks() {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::MeasuredOnly,
            "Kernel agent inspected bit-exact configs and fixtures. CUDA semantic correctness is not claimed.",
        );
        apply_kernel_production_proof(&mut report, root, target, false, false);
        seal_runtime(
            root,
            Some(out_dir),
            &mut report,
            TrustLevel::MeasuredOnly,
            kernel_action_intents(mode),
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
        seal_runtime(
            root,
            Some(out_dir),
            &mut report,
            TrustLevel::MeasuredOnly,
            kernel_action_intents(mode),
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
    let lint_passed = report
        .commands
        .iter()
        .any(|command| command.name == "bitexact-lint" && command.status == AgentStatus::Passed);
    let run_passed = report
        .commands
        .iter()
        .any(|command| command.name == "bitexact-run" && command.status == AgentStatus::Passed);
    apply_kernel_production_proof(&mut report, root, target, lint_passed, run_passed);
    seal_runtime(
        root,
        Some(out_dir),
        &mut report,
        TrustLevel::MeasuredOnly,
        kernel_action_intents(mode),
    );
    report
}

fn apply_kernel_production_proof(
    report: &mut AgentReport,
    root: &Path,
    target: &str,
    lint_passed: bool,
    run_passed: bool,
) {
    let mut blockers = Vec::new();
    let mut requirements = Vec::new();
    let config = config_for_target(root, target);
    let source_contract = kernel_source_contract(root, &config);
    let real_source = has_real_kernel_source(root) && source_contract.is_real_source();

    requirements.push(ProductionRequirement::new_owned(
        "kernel.real_source",
        "Real CUDA or HELYX kernel source exists and is cited by the config",
        if real_source {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![if real_source {
            "config source points at real kernel source and kernels/src contains candidate source"
                .to_string()
        } else {
            source_contract.evidence.clone()
        }],
    ));
    if !real_source {
        blockers.push(source_contract.blocker);
    }

    requirements.push(ProductionRequirement::new(
        "kernel.cpu_reference",
        "CPU reference implementation or committed golden output exists",
        if root
            .join("kernels/fixtures/helyx-bitexact-input.txt")
            .exists()
        {
            AgentStatus::Partial
        } else {
            AgentStatus::Blocked
        },
        &[
            if root
                .join("kernels/fixtures/helyx-bitexact-input.txt")
                .exists()
            {
                "smoke fixture exists; production CPU reference still required"
            } else {
                "CPU reference or golden output fixture is missing"
            },
        ],
    ));
    blockers.push("production CPU reference/golden output evidence is missing".to_string());

    requirements.push(ProductionRequirement::new(
        "kernel.bitexact_fixture",
        "Bit-exact lint and run pass on deterministic fixtures",
        if lint_passed && run_passed {
            AgentStatus::Passed
        } else if lint_passed {
            AgentStatus::Partial
        } else {
            AgentStatus::Blocked
        },
        &[if lint_passed && run_passed {
            "bitexact-lint and bitexact-run passed"
        } else if lint_passed {
            "bitexact-lint passed; bitexact-run not present"
        } else {
            "bitexact evidence not run or failed"
        }],
    ));
    if !(lint_passed && run_passed) {
        blockers.push("complete bit-exact run evidence is missing".to_string());
    }

    let hardware_matrix = std::env::var("REFINEFORGE_KERNEL_HARDWARE_MATRIX").ok();
    let hardware_matrix_valid = hardware_matrix
        .as_deref()
        .is_some_and(|path| production_hardware_matrix_passes(Path::new(path)));
    requirements.push(ProductionRequirement::new_owned(
        "kernel.hardware_matrix",
        "Hardware matrix records GPU, driver, CUDA toolkit, OS, and CPU architecture",
        if hardware_matrix_valid {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![hardware_matrix.unwrap_or_else(|| {
            "production hardware matrix not provided via REFINEFORGE_KERNEL_HARDWARE_MATRIX"
                .to_string()
        })],
    ));
    if !hardware_matrix_valid {
        blockers
            .push("CUDA hardware matrix evidence is missing or not production-passed".to_string());
    }

    let compiler_metadata = std::env::var("REFINEFORGE_KERNEL_COMPILER_METADATA").ok();
    requirements.push(ProductionRequirement::new_owned(
        "kernel.compiler_runtime_metadata",
        "Compiler and runtime metadata record rustc, nvcc, driver, and build flags",
        if compiler_metadata.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![compiler_metadata.unwrap_or_else(|| {
            "compiler/runtime metadata not provided via REFINEFORGE_KERNEL_COMPILER_METADATA"
                .to_string()
        })],
    ));
    if std::env::var("REFINEFORGE_KERNEL_COMPILER_METADATA").is_err() {
        blockers.push("compiler/runtime metadata is missing".to_string());
    }

    let performance_baseline = std::env::var("REFINEFORGE_KERNEL_PERFORMANCE_BASELINE").ok();
    requirements.push(ProductionRequirement::new_owned(
        "kernel.performance_baseline",
        "Performance baseline records latency/throughput and regression threshold",
        if performance_baseline.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![performance_baseline.unwrap_or_else(|| {
            "performance baseline not provided via REFINEFORGE_KERNEL_PERFORMANCE_BASELINE"
                .to_string()
        })],
    ));
    if std::env::var("REFINEFORGE_KERNEL_PERFORMANCE_BASELINE").is_err() {
        blockers.push("kernel performance baseline is missing".to_string());
    }

    let handoff = std::env::var("REFINEFORGE_KERNEL_HELYX_HANDOFF").ok();
    requirements.push(ProductionRequirement::new_owned(
        "kernel.helyx_handoff",
        "HELYX handoff records source, config, and evidence bundle hashes",
        if handoff.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![handoff.unwrap_or_else(|| {
            "HELYX kernel handoff not provided via REFINEFORGE_KERNEL_HELYX_HANDOFF".to_string()
        })],
    ));
    if std::env::var("REFINEFORGE_KERNEL_HELYX_HANDOFF").is_err() {
        blockers.push("HELYX kernel handoff evidence is missing".to_string());
    }

    let approval = std::env::var("REFINEFORGE_KERNEL_HUMAN_APPROVAL").ok();
    requirements.push(ProductionRequirement::new_owned(
        "kernel.human_kernel_approval",
        "Named human reviewer approved CUDA correctness and performance evidence",
        if approval.is_some() {
            AgentStatus::Passed
        } else {
            AgentStatus::Blocked
        },
        vec![approval
            .clone()
            .unwrap_or_else(|| "human kernel approval is absent".to_string())],
    ));
    if approval.is_none() {
        blockers.push("human kernel approval is missing".to_string());
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

fn has_real_kernel_source(root: &Path) -> bool {
    let src = root.join("kernels").join("src");
    let Ok(entries) = std::fs::read_dir(src) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        path.is_file()
            && matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("cu" | "cuh" | "rs")
            )
    })
}

struct KernelSourceContract {
    kind: String,
    evidence: String,
    blocker: String,
}

impl KernelSourceContract {
    fn is_real_source(&self) -> bool {
        matches!(self.kind.as_str(), "cuda" | "rust" | "external")
            && !self.evidence.contains("missing")
    }
}

fn kernel_source_contract(root: &Path, config: &Path) -> KernelSourceContract {
    let config_path = if config.is_absolute() {
        config.to_path_buf()
    } else {
        root.join(config)
    };
    let Ok(text) = std::fs::read_to_string(&config_path) else {
        return KernelSourceContract {
            kind: "missing".into(),
            evidence: format!("kernel config missing: {}", config.display()),
            blocker: "kernel.real_source blocked: kernel config is missing".into(),
        };
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&text) else {
        return KernelSourceContract {
            kind: "invalid".into(),
            evidence: format!("kernel config could not be parsed: {}", config.display()),
            blocker: "kernel.real_source blocked: kernel config could not be parsed".into(),
        };
    };
    let kind = yaml_string_at(&value, &["source", "kind"]).unwrap_or_else(|| "stub".into());
    let source_path = yaml_string_at(&value, &["source", "path"]);
    if kind == "stub" {
        return KernelSourceContract {
            kind,
            evidence: format!("{} declares source.kind is stub", config.display()),
            blocker: "kernel.real_source blocked: source.kind is stub".into(),
        };
    }
    let Some(source_path) = source_path else {
        return KernelSourceContract {
            kind,
            evidence: "source.path missing".into(),
            blocker: "kernel.real_source blocked: source.path is missing".into(),
        };
    };
    let resolved = if Path::new(&source_path).is_absolute() {
        PathBuf::from(&source_path)
    } else {
        root.join(&source_path)
    };
    if !resolved.exists() {
        return KernelSourceContract {
            kind,
            evidence: format!("source.path missing: {source_path}"),
            blocker: format!(
                "kernel.real_source blocked: source.path does not exist: {source_path}"
            ),
        };
    }
    KernelSourceContract {
        kind,
        evidence: source_path,
        blocker: "kernel.real_source blocked: real source not found".into(),
    }
}

fn yaml_string_at(value: &serde_yaml::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    match current {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Null => None,
        other => other.as_str().map(str::to_string),
    }
}

fn production_hardware_matrix_passes(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    value
        .get("runs")
        .and_then(|runs| runs.as_array())
        .is_some_and(|runs| runs.iter().any(hardware_run_is_production_pass))
}

fn hardware_run_is_production_pass(run: &serde_json::Value) -> bool {
    let required = ["gpu_name", "gpu_arch", "driver_version", "cuda_toolkit"];
    run.get("status").and_then(|v| v.as_str()) == Some("passed")
        && required.iter().all(|key| {
            run.get(*key)
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.trim().is_empty())
        })
}

fn kernel_action_intents(mode: AgentMode) -> Vec<ActionIntent> {
    vec![
        action_intent(
            "kernel.inspect.fixtures",
            "Inspect kernel configs, fixtures, baselines, and HELYX handoff docs",
            "inspect",
            "read_only",
            "refine agent kernel --mode inspect",
            &[
                "kernels/configs/*.yaml",
                "kernels/fixtures/*",
                "kernels/README.md",
            ],
        ),
        action_intent(
            "kernel.lint.bitexact",
            "Lint bit-exact metadata before execution",
            "verify",
            "writes_evidence",
            "refine agent kernel --mode check",
            &["bitexact-lint", "bitexact-lint.json"],
        ),
        action_intent(
            "kernel.execute.bitexact",
            "Run deterministic bit-exact gate after lint success",
            "execute",
            "writes_evidence",
            &format!("refine agent kernel --mode {}", mode.as_str()),
            &["bitexact-run", "kernel-runs", "expected SHA-256 baselines"],
        ),
        action_intent(
            "kernel.correctness.guard",
            "Block CUDA semantic, portability, and performance claims without hardware evidence",
            "audit",
            "evidence_only",
            "refine agent kernel --mode execute",
            &["CUDA source", "hardware matrix", "benchmark evidence"],
        ),
    ]
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
