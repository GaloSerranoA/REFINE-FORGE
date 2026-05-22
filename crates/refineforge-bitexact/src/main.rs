//! `refine-bitexact` — bit-exact reproducibility gate CLI.
//!
//! Run a kernel-experiment N times, hash each output, FAIL the
//! process if any hash disagrees. Use as a CI gate alongside
//! `refine lean check-all`.

mod experiment;
mod hash;
mod lint;
mod manifest;
mod report;
mod run_all;
mod runner;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "refine-bitexact",
    version,
    about = "Bit-exact reproducibility gate for GPU kernels (refineforge Section 4)",
    long_about = "Runs a kernel-experiment N times, hashes each output, fails if \
                  any hash disagrees. Use as a CI gate. Does NOT enforce \
                  determinism — only detects when it's been broken. See \
                  docs/bit-exact-reproducibility.md for mitigations."
)]
struct Cli {
    /// Root for run directories. Defaults to `kernels/runs/`.
    #[arg(long, default_value = "kernels/runs")]
    runs_root: PathBuf,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the gate against one kernel-experiment.
    Run {
        /// Path to the kernel-experiment YAML.
        experiment: PathBuf,
        /// Don't actually run; print the resolved command(s) and exit.
        #[arg(long)]
        dry_run: bool,
    },
    /// Rebuild report.json for a previous run (does NOT re-execute
    /// the kernel; just re-summarises whatever's on disk in the
    /// runs_root/<id>/ tree).
    Report { run_dir: PathBuf },
    /// Lint one kernel-experiment YAML for enterprise readiness.
    Lint {
        /// Path to the kernel-experiment YAML.
        experiment: PathBuf,
        /// Emit JSON to stdout instead of human text.
        #[arg(long)]
        json: bool,
        /// Optional path to write the JSON lint report.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Run every kernel-experiment YAML in a directory.
    RunAll {
        /// Directory containing kernel-experiment YAML files.
        config_dir: PathBuf,
        /// Include example-* configs. By default real-kernel CI skips examples.
        #[arg(long)]
        include_examples: bool,
        /// Optional path to write aggregate summary JSON.
        #[arg(long)]
        summary_json: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run {
            experiment,
            dry_run,
        } => cmd_run(&cli.runs_root, &experiment, dry_run),
        Cmd::Report { run_dir } => cmd_report(&run_dir),
        Cmd::Lint {
            experiment,
            json,
            output,
        } => cmd_lint(&experiment, json, output.as_deref()),
        Cmd::RunAll {
            config_dir,
            include_examples,
            summary_json,
        } => cmd_run_all(
            &cli.runs_root,
            &config_dir,
            include_examples,
            summary_json.as_deref(),
        ),
    }
}

fn cmd_run(runs_root: &Path, exp_path: &Path, dry_run: bool) -> Result<()> {
    let exp = experiment::KernelExperiment::load(exp_path)?;
    let paths = runner::RunPaths::for_experiment(runs_root, &exp);
    std::fs::create_dir_all(&paths.run_dir)?;

    if dry_run {
        println!("DRY-RUN: would execute the following {} time(s):", exp.runs);
        for i in 0..exp.runs {
            let cmd = exp.substitute(&exp.command, &paths.run_dir, i);
            println!("  [{i}] {cmd}");
        }
        if let experiment::OutputSource::File(path) = &exp.output {
            println!(
                "  hashing file: {}",
                exp.substitute(path, &paths.run_dir, 0)
            );
        } else {
            println!("  hashing stdout of each run");
        }
        if !exp.env.is_empty() {
            println!("  env:");
            for (k, v) in &exp.env {
                println!("    {k}={v}");
            }
        }
        return Ok(());
    }

    eprintln!(
        "refine-bitexact: experiment '{}' — running {} time(s)",
        exp.id, exp.runs
    );
    let runs = runner::run_all(runs_root, &exp)?;
    for r in &runs {
        match (&r.output_hash, &r.error) {
            (Some(h), _) => eprintln!(
                "  [{}] {} ms — sha256 {}",
                r.run_index,
                r.duration_ms,
                &h[..16]
            ),
            (None, Some(e)) => eprintln!("  [{}] ERROR: {e}", r.run_index),
            (None, None) => eprintln!("  [{}] (no hash, no error — bug)", r.run_index),
        }
    }
    let input_manifest = manifest::build_input_manifest(&exp.input_files)?;
    let report = report::Report::build_with_input_manifest(&exp, runs, input_manifest);
    report.write(&paths.run_dir)?;
    eprintln!();
    eprintln!("{}", report.summary);
    eprintln!(
        "report: {}",
        paths.run_dir.join("bitexact-report.json").display()
    );
    match report.outcome {
        report::Outcome::Pass => Ok(()),
        report::Outcome::Fail => Err(anyhow::anyhow!("bit-exact gate FAILED")),
    }
}

fn cmd_report(run_dir: &Path) -> Result<()> {
    // The report is built from the runs slice; without re-executing
    // we don't have a runs slice. This subcommand is reserved for a
    // future enhancement that persists per-run hashes to a JSONL.
    // For now: read the existing report.json and re-pretty-print.
    let p = run_dir.join("bitexact-report.json");
    let content =
        std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
    let v: serde_json::Value = serde_json::from_str(&content)?;
    println!("{}", serde_json::to_string_pretty(&v)?);
    Ok(())
}

fn cmd_lint(exp_path: &Path, json: bool, output: Option<&Path>) -> Result<()> {
    let exp = experiment::KernelExperiment::load(exp_path)?;
    let report = lint::lint_experiment(&exp);
    let json_text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = output {
        std::fs::write(path, &json_text).with_context(|| format!("writing {}", path.display()))?;
    }
    if json {
        println!("{json_text}");
    } else {
        println!(
            "lint {:?}: {} issue(s) for {}",
            report.status,
            report.issues.len(),
            report.experiment_id
        );
        for issue in &report.issues {
            println!("  {}: {}", issue.field, issue.message);
        }
    }
    match report.status {
        lint::LintStatus::Pass => Ok(()),
        lint::LintStatus::Fail => Err(anyhow::anyhow!("bit-exact lint FAILED")),
    }
}

fn cmd_run_all(
    runs_root: &Path,
    config_dir: &Path,
    include_examples: bool,
    summary_json: Option<&Path>,
) -> Result<()> {
    let summary = run_all::run_directory(&run_all::RunAllOptions {
        config_dir: config_dir.to_path_buf(),
        runs_root: runs_root.to_path_buf(),
        include_examples,
        summary_json: summary_json.map(Path::to_path_buf),
    })?;
    println!(
        "run-all: {} config(s), {} passed, {} failed",
        summary.included_configs, summary.passed, summary.failed
    );
    for entry in &summary.entries {
        let status = entry
            .outcome
            .as_ref()
            .map(|outcome| format!("{outcome:?}"))
            .unwrap_or_else(|| "Error".into());
        println!("  {status}: {}", entry.config_path);
        if let Some(report_path) = &entry.report_path {
            println!("    report: {report_path}");
        }
        if let Some(error) = &entry.error {
            println!("    error: {error}");
        }
    }
    if let Some(path) = summary_json {
        println!("summary: {}", path.display());
    }
    if summary.has_failures() {
        Err(anyhow::anyhow!("one or more bit-exact gates failed"))
    } else {
        Ok(())
    }
}
