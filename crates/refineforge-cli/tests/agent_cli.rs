use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn run_refine(args: &[&str], out: &Path) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_refine"));
    cmd.current_dir(workspace_root())
        .args(["--root", "."])
        .args(args)
        .arg("--out")
        .arg(out)
        .arg("--json")
        .output()
        .expect("run refine agent command")
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read_json(path: &Path) -> Value {
    let content = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&content).unwrap()
}

fn assert_enterprise_report(report: &Value, expected_agent: &str) {
    assert_eq!(report["liveness"]["state"], "alive");
    assert_eq!(report["liveness"]["agent"], expected_agent);
    assert!(
        report["capabilities"].as_array().unwrap().len() >= 3,
        "{expected_agent} report should declare enterprise capabilities"
    );
    assert!(
        report["tool_checks"].as_array().unwrap().len() >= 1,
        "{expected_agent} report should declare tool-gate checks"
    );
}

fn write_stub(dir: &Path, name: &str, success: bool) -> std::path::PathBuf {
    #[cfg(windows)]
    let path = dir.join(format!("{name}.cmd"));
    #[cfg(not(windows))]
    let path = dir.join(name);

    #[cfg(windows)]
    let content = if success {
        "@echo off\r\necho stub ok\r\nexit /b 0\r\n"
    } else {
        "@echo off\r\necho stub failed 1>&2\r\nexit /b 7\r\n"
    };
    #[cfg(not(windows))]
    let content = if success {
        "#!/bin/sh\necho stub ok\nexit 0\n"
    } else {
        "#!/bin/sh\necho stub failed >&2\nexit 7\n"
    };

    std::fs::write(&path, content).unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path
}

fn write_kernel_output_stub(dir: &Path) -> std::path::PathBuf {
    #[cfg(windows)]
    let path = dir.join("refine-bitexact-output.cmd");
    #[cfg(not(windows))]
    let path = dir.join("refine-bitexact-output");

    #[cfg(windows)]
    let content = "@echo off\r\necho {\"status\":\"pass\"} > \"%5\"\r\nif errorlevel 1 exit /b 1\r\nexit /b 0\r\n";
    #[cfg(not(windows))]
    let content = "#!/bin/sh\necho '{\"status\":\"pass\"}' > \"$5\"\n";

    std::fs::write(&path, content).unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path
}

fn write_kernel_enterprise_stub(dir: &Path) -> std::path::PathBuf {
    #[cfg(windows)]
    let path = dir.join("refine-bitexact-enterprise.cmd");
    #[cfg(not(windows))]
    let path = dir.join("refine-bitexact-enterprise");

    #[cfg(windows)]
    let content = "@echo off\r\nif \"%1\"==\"lint\" (\r\n  echo {\"status\":\"pass\"} > \"%5\"\r\n  exit /b 0\r\n)\r\necho bitexact run ok\r\nexit /b 0\r\n";
    #[cfg(not(windows))]
    let content = "#!/bin/sh\nif [ \"$1\" = \"lint\" ]; then\n  echo '{\"status\":\"pass\"}' > \"$5\"\n  exit 0\nfi\necho bitexact run ok\nexit 0\n";

    std::fs::write(&path, content).unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path
}

#[test]
fn agent_lean_inspect_writes_report() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("lean-agent");
    let output = run_refine(
        &["agent", "lean", "--mode", "inspect", "--target", "helyx"],
        &out,
    );

    assert_success(&output);
    let report = read_json(&out.join("lean.json"));
    assert_eq!(report["schema_version"], "agent-report-v1");
    assert_eq!(report["agent"], "lean");
    assert_eq!(report["mode"], "inspect");
    assert_eq!(report["target"], "helyx");
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "model-only");
    assert_enterprise_report(&report, "lean");
    assert!(out.join("lean.md").exists());
}

#[test]
fn agent_lean_check_keeps_model_only_scope_as_trust_floor() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("lean-check");
    let output = run_refine(
        &["agent", "lean", "--mode", "check", "--target", "helyx"],
        &out,
    );

    assert_success(&output);
    let report = read_json(&out.join("lean.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "model-only");
    assert!(
        report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("model-only")),
        "Lean check must explain why passing gates did not upgrade trust to model-linked"
    );
}

#[test]
fn agent_devops_train_and_kernel_inspect_reports_are_truth_bounded() {
    for (name, trust_level) in [
        ("devops", "release-ready-local"),
        ("train", "measured-only"),
        ("kernel", "measured-only"),
    ] {
        let td = tempfile::tempdir().unwrap();
        let out = td.path().join(format!("{name}-agent"));
        let output = run_refine(
            &["agent", name, "--mode", "inspect", "--target", "helyx"],
            &out,
        );

        assert_success(&output);
        let report = read_json(&out.join(format!("{name}.json")));
        assert_eq!(report["schema_version"], "agent-report-v1");
        assert_eq!(report["agent"], name);
        assert_eq!(report["mode"], "inspect");
        assert_eq!(report["status"], "passed");
        assert_eq!(report["trust_level"], trust_level);
        assert_enterprise_report(&report, name);
        assert!(
            report["artifacts"].as_array().unwrap().len() >= 1,
            "{name} report should record at least one inspected artifact"
        );
        assert!(out.join(format!("{name}.md")).exists());
    }
}

#[test]
fn agent_run_all_inspect_writes_dashboard_and_role_reports() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("helyx-readiness");
    let output = run_refine(
        &["agent", "run-all", "--mode", "inspect", "--target", "helyx"],
        &out,
    );

    assert_success(&output);
    let summary = read_json(&out.join("summary.json"));
    assert_eq!(summary["schema_version"], "agent-report-v1");
    assert_eq!(summary["agent"], "run_all");
    assert_eq!(summary["target"], "helyx");
    assert_eq!(summary["status"], "passed");
    assert_enterprise_report(&summary, "run_all");
    assert!(out.join("summary.md").exists());
    for name in ["lean", "devops", "train", "kernel"] {
        let role_report_path = out.join(format!("{name}.json"));
        assert!(role_report_path.exists());
        assert_enterprise_report(&read_json(&role_report_path), name);
        assert!(out.join(format!("{name}.md")).exists());
    }
}

#[test]
fn agent_docs_and_schema_exist() {
    let root = workspace_root();
    for path in [
        "docs/agents/README.md",
        "docs/agents/lean-agent.md",
        "docs/agents/devops-agent.md",
        "docs/agents/training-agent.md",
        "docs/agents/kernel-agent.md",
        "schemas/agent-report.schema.json",
    ] {
        assert!(root.join(path).exists(), "{path} should exist");
    }
}

#[test]
fn agent_train_check_records_pass_fail_and_blocked_statuses() {
    let td = tempfile::tempdir().unwrap();

    let pass = write_stub(td.path(), "refine-train-pass", true);
    let pass_out = td.path().join("train-pass");
    let pass_output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &pass)
        .args([
            "--root", ".", "agent", "train", "--mode", "check", "--target", "helyx", "--out",
        ])
        .arg(&pass_out)
        .arg("--json")
        .output()
        .expect("run train pass");
    assert_success(&pass_output);
    let report = read_json(&pass_out.join("train.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["commands"][0]["status"], "passed");
    let command = report["commands"][0]["command"].as_array().unwrap();
    assert!(
        command
            .iter()
            .any(|arg| arg == "training/data/mathlib-proof-repair-v1/anthropic-sft.jsonl"),
        "training check should audit the valid Mathlib SFT fixture, got {command:?}"
    );

    let fail = write_stub(td.path(), "refine-train-fail", false);
    let fail_out = td.path().join("train-fail");
    let fail_output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &fail)
        .args([
            "--root", ".", "agent", "train", "--mode", "check", "--target", "helyx", "--out",
        ])
        .arg(&fail_out)
        .arg("--json")
        .output()
        .expect("run train fail");
    assert!(!fail_output.status.success());
    let report = read_json(&fail_out.join("train.json"));
    assert_eq!(report["status"], "failed");
    assert_eq!(report["commands"][0]["status"], "failed");

    let blocked_out = td.path().join("train-blocked");
    let blocked_output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env(
            "REFINEFORGE_REFINE_TRAIN_BIN",
            td.path().join("missing-refine-train"),
        )
        .args([
            "--root", ".", "agent", "train", "--mode", "check", "--target", "helyx", "--out",
        ])
        .arg(&blocked_out)
        .arg("--json")
        .output()
        .expect("run train blocked");
    assert!(!blocked_output.status.success());
    let report = read_json(&blocked_out.join("train.json"));
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["commands"][0]["status"], "blocked");
}

#[test]
fn agent_kernel_check_creates_output_dir_before_lint_command() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_kernel_output_stub(td.path());
    let out = td.path().join("nested").join("kernel-check");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_BITEXACT_BIN", &stub)
        .args([
            "--root", ".", "agent", "kernel", "--mode", "check", "--target", "helyx", "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run kernel check");

    assert_success(&output);
    assert!(out.join("bitexact-lint.json").exists());
    let report = read_json(&out.join("kernel.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["commands"][0]["status"], "passed");
}

#[test]
fn agent_train_execute_runs_data_audit_and_dry_run_training() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_stub(td.path(), "refine-train-execute", true);
    let out = td.path().join("train-execute");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &stub)
        .args([
            "--root", ".", "agent", "train", "--mode", "execute", "--target", "helyx", "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run train execute");

    assert_success(&output);
    let report = read_json(&out.join("train.json"));
    assert_eq!(report["status"], "passed");
    let command_names: Vec<_> = report["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cmd| cmd["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        command_names,
        ["training-data-audit", "training-run-dry-run"]
    );
    let run_command = report["commands"][1]["command"].as_array().unwrap();
    assert!(
        run_command.iter().any(|arg| arg == "--dry-run"),
        "execute mode should default to a safe trainer dry-run without --allow-expensive"
    );
    assert!(
        report["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact.as_str().unwrap().contains("training-runs")),
        "training execute should record its runs_root artifact"
    );
}

#[test]
fn agent_kernel_execute_runs_lint_and_bitexact_gate() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_kernel_enterprise_stub(td.path());
    let out = td.path().join("kernel-execute");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_BITEXACT_BIN", &stub)
        .args([
            "--root", ".", "agent", "kernel", "--mode", "execute", "--target", "helyx", "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run kernel execute");

    assert_success(&output);
    let report = read_json(&out.join("kernel.json"));
    assert_eq!(report["status"], "passed");
    let command_names: Vec<_> = report["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cmd| cmd["name"].as_str().unwrap())
        .collect();
    assert_eq!(command_names, ["bitexact-lint", "bitexact-run"]);
    assert!(
        report["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact.as_str().unwrap().contains("kernel-runs")),
        "kernel execute should record its runs_root artifact"
    );
}
