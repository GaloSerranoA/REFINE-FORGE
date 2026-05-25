//! `refine-train` — training control-plane CLI.
//!
//! Runs the built-in `refineforge_native` proof-repair smoke trainer
//! or wraps external backends (axolotl, HuggingFace Trainer, HELYX,
//! custom) with run tracking, checkpoint resume, failure recovery,
//! and JSON reports.
//!
//! Subcommands:
//!
//! - `run <exp.yaml>` — execute one experiment, with retries per config
//! - `sweep <sweep.yaml>` — expand a grid/random sweep and run each
//! - `resume <run_dir>` — re-run from the latest checkpoint
//! - `monitor <run_dir>` — tail progress + show latest metrics
//! - `report <run_dir>` — emit final report.json
//! - `promote <run_dir>` — write a local-finetune manifest for a successful checkpoint
//!
//! Owned by Section 2 ([../../ARCHITECTURE.md](../../ARCHITECTURE.md)).

mod checkpoint;
mod dataset;
mod experiment;
mod failure;
mod native;
mod progress;
mod promotion;
mod report;
mod runner;
mod sweep;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "refine-train",
    version,
    about = "Training control plane for refineforge (Section 2)",
    long_about = "Runs the built-in refineforge_native proof-repair smoke trainer \
                  or external backends (axolotl, HuggingFace Trainer, HELYX, custom) \
                  with run tracking, checkpoint resume, retry-with-backoff failure \
                  recovery, checkpoints, and JSON training reports."
)]
struct Cli {
    /// Root for run directories. Defaults to `training/runs/`.
    #[arg(long, default_value = "training/runs")]
    runs_root: PathBuf,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Dataset utilities.
    Data {
        #[command(subcommand)]
        cmd: DataCmd,
    },
    /// Run one experiment (with retries per its `retry` config).
    Run {
        /// Path to the experiment YAML.
        experiment: PathBuf,
        /// Don't actually spawn the training subprocess — print
        /// the resolved command + run dir and exit. Useful for
        /// validating config without burning compute.
        #[arg(long)]
        dry_run: bool,
    },
    /// Expand a sweep config and run each generated experiment in
    /// sequence (parallel sweeps are out of scope for v1; orchestrate
    /// externally via `xargs -P` if needed).
    Sweep {
        /// Path to the sweep YAML.
        sweep: PathBuf,
        /// Stop after the first failure (default: keep going).
        #[arg(long)]
        fail_fast: bool,
    },
    /// Tail a run's progress.jsonl and print the latest metrics.
    Monitor {
        /// Run directory (the dir containing progress.jsonl).
        run_dir: PathBuf,
        /// Number of lines to print initially. Default 20.
        #[arg(long, default_value_t = 20)]
        tail: usize,
        /// Don't follow new lines; just print the current tail.
        #[arg(long)]
        no_follow: bool,
    },
    /// Build / refresh the report.json for a run.
    Report {
        /// Run directory.
        run_dir: PathBuf,
    },
    /// Promote a successful checkpoint into a local-finetune runtime directory.
    Promote {
        /// Run directory containing report.json.
        run_dir: PathBuf,
        /// Output directory for refineforge-local-finetune.json and promotion-report.json.
        #[arg(long)]
        out_dir: PathBuf,
        /// Model id written into the local-finetune manifest.
        #[arg(long)]
        model_id: String,
        /// Runtime command program written into the manifest.
        #[arg(long)]
        command: String,
        /// Runtime command argument. May be repeated.
        #[arg(long = "command-arg", allow_hyphen_values = true)]
        command_args: Vec<String>,
        /// Trainer/runtime producer label.
        #[arg(long, default_value = "manual")]
        producer: String,
        /// Require report.json final_outcome == "success".
        #[arg(long)]
        require_success: bool,
    },
    /// List all checkpoints in a run dir.
    Checkpoints { run_dir: PathBuf },
}

#[derive(Subcommand)]
enum DataCmd {
    /// Audit proof-repair SFT JSONL before training.
    Audit {
        /// Path to the JSONL dataset.
        path: PathBuf,
        /// Expected non-comment row count.
        #[arg(long)]
        expect_rows: Option<usize>,
        /// Expected split count, e.g. --expect-split train=800.
        #[arg(long = "expect-split")]
        expect_splits: Vec<String>,
        /// Optional path for JSON audit output.
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Data { cmd } => cmd_data(cmd),
        Cmd::Run {
            experiment,
            dry_run,
        } => cmd_run(&cli.runs_root, &experiment, dry_run),
        Cmd::Sweep { sweep, fail_fast } => cmd_sweep(&cli.runs_root, &sweep, fail_fast),
        Cmd::Monitor {
            run_dir,
            tail,
            no_follow,
        } => cmd_monitor(&run_dir, tail, no_follow),
        Cmd::Report { run_dir } => cmd_report(&run_dir),
        Cmd::Promote {
            run_dir,
            out_dir,
            model_id,
            command,
            command_args,
            producer,
            require_success,
        } => cmd_promote(
            run_dir,
            out_dir,
            model_id,
            command,
            command_args,
            producer,
            require_success,
        ),
        Cmd::Checkpoints { run_dir } => cmd_checkpoints(&run_dir),
    }
}

fn cmd_data(cmd: DataCmd) -> Result<()> {
    match cmd {
        DataCmd::Audit {
            path,
            expect_rows,
            expect_splits,
            output,
        } => {
            let expectations = dataset::AuditExpectations {
                rows: expect_rows,
                splits: parse_split_expectations(&expect_splits)?,
            };
            let audit = dataset::audit_jsonl(&path, &expectations)?;
            println!(
                "dataset audit OK: rows={} unique_ids={} valid_patches={} sha256={}",
                audit.total_rows, audit.unique_ids, audit.valid_patch_rows, audit.sha256
            );
            if !audit.split_counts.is_empty() {
                let parts: Vec<String> = audit
                    .split_counts
                    .iter()
                    .map(|(split, count)| format!("{split}={count}"))
                    .collect();
                println!("  splits: {}", parts.join(", "));
            }
            if let Some(out) = output {
                dataset::write_audit_json(&audit, &out)?;
                println!("  wrote {}", out.display());
            }
            Ok(())
        }
    }
}

fn parse_split_expectations(items: &[String]) -> Result<std::collections::BTreeMap<String, usize>> {
    let mut out = std::collections::BTreeMap::new();
    for item in items {
        let Some((split, count)) = item.split_once('=') else {
            anyhow::bail!("--expect-split must be NAME=COUNT, got {item:?}");
        };
        if split.trim().is_empty() {
            anyhow::bail!("--expect-split name may not be empty");
        }
        out.insert(
            split.to_string(),
            count
                .parse::<usize>()
                .with_context(|| format!("parsing split count in {item:?}"))?,
        );
    }
    Ok(out)
}

fn cmd_run(runs_root: &Path, exp_path: &Path, dry_run: bool) -> Result<()> {
    let exp = experiment::Experiment::load(exp_path)?;
    if dry_run {
        let paths = runner::RunPaths::for_experiment(runs_root, &exp);
        paths.ensure_created()?;
        let argv = runner::build_command(&exp.backend, &paths, &exp)?;
        println!("DRY-RUN: would execute the following:");
        println!("  cwd:     {}", std::env::current_dir()?.display());
        println!("  run_dir: {}", paths.run_dir.display());
        println!("  argv:    {}", argv.join(" "));
        println!("  log:     {}", paths.log_file.display());
        println!("  ckpts:   {}", paths.checkpoint_dir.display());
        return Ok(());
    }
    println!(
        "Running experiment '{}' (max {} attempts)",
        exp.id, exp.retry.max_attempts
    );
    let outcome = failure::run_with_retries(runs_root, &exp)?;
    let paths = runner::RunPaths::for_experiment(runs_root, &exp);
    let final_kind = if outcome
        .final_outcome
        .as_ref()
        .is_some_and(|o| o.exit_status.success())
    {
        "success"
    } else {
        "failure"
    };
    let r = report::build(&exp, &paths, final_kind, outcome.attempts as usize)?;
    report::write(&r, &paths)?;
    let runner_progress_records = outcome
        .final_outcome
        .as_ref()
        .map(|run| run.progress_records)
        .unwrap_or_default();
    println!("  attempts:           {}", outcome.attempts);
    println!("  progress records:   {}", r.progress_record_count);
    println!("  runner records:     {}", runner_progress_records);
    println!("  failure records:    {}", outcome.failures.len());
    println!("  checkpoints:        {}", r.checkpoints.len());
    println!("  final:              {}", final_kind);
    println!(
        "  report:             {}",
        paths.run_dir.join("report.json").display()
    );
    if final_kind != "success" {
        anyhow::bail!(
            "training did not succeed after {} attempts",
            outcome.attempts
        );
    }
    Ok(())
}

fn cmd_sweep(runs_root: &Path, sweep_path: &Path, fail_fast: bool) -> Result<()> {
    let sweep_cfg = sweep::Sweep::load(sweep_path)?;
    let base_path = sweep_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&sweep_cfg.base_experiment);
    let base = experiment::Experiment::load(&base_path)?;
    let experiments = sweep_cfg.expand(&base);
    println!(
        "Sweep '{}': {} experiment(s); strategy={}",
        sweep_cfg.id,
        experiments.len(),
        sweep_cfg.strategy
    );
    let mut success = 0;
    let mut failure_count = 0;
    for (i, exp) in experiments.iter().enumerate() {
        println!();
        println!("[{}/{}] {}", i + 1, experiments.len(), exp.id);
        match failure::run_with_retries(runs_root, exp) {
            Ok(outcome) => {
                if outcome
                    .final_outcome
                    .as_ref()
                    .is_some_and(|o| o.exit_status.success())
                {
                    success += 1;
                    println!("  ✓ success after {} attempt(s)", outcome.attempts);
                } else {
                    failure_count += 1;
                    println!("  ✗ failed after {} attempt(s)", outcome.attempts);
                    if fail_fast {
                        println!("(--fail-fast — stopping)");
                        break;
                    }
                }
            }
            Err(e) => {
                failure_count += 1;
                println!("  ✗ ERROR: {e}");
                if fail_fast {
                    break;
                }
            }
        }
    }
    println!();
    println!("Sweep summary: {success} success, {failure_count} failure(s)");
    Ok(())
}

fn cmd_monitor(run_dir: &Path, tail_n: usize, no_follow: bool) -> Result<()> {
    let progress_file = run_dir.join("progress.jsonl");
    if !progress_file.exists() {
        anyhow::bail!(
            "no progress.jsonl in {} — is this an active run dir?",
            run_dir.display()
        );
    }
    let initial = std::fs::read_to_string(&progress_file)
        .with_context(|| format!("reading {}", progress_file.display()))?;
    let lines: Vec<&str> = initial.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(tail_n);
    for line in &lines[start..] {
        print_progress_line(line);
    }
    if no_follow {
        return Ok(());
    }
    // Follow mode: poll the file for new lines.
    let mut last_len = lines.len();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let text = match std::fs::read_to_string(&progress_file) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.len() > last_len {
            for line in &lines[last_len..] {
                print_progress_line(line);
            }
            last_len = lines.len();
        }
    }
}

fn print_progress_line(line: &str) {
    match serde_json::from_str::<progress::ProgressRecord>(line) {
        Ok(r) => {
            let mut parts: Vec<String> = vec![format!("[{}]", r.timestamp.format("%H:%M:%S"))];
            if let Some(s) = r.step {
                parts.push(format!("step={s}"));
            }
            for (k, v) in &r.metrics {
                parts.push(format!("{k}={v:.6}"));
            }
            println!("{}", parts.join(" "));
        }
        Err(_) => println!("{line}"),
    }
}

fn cmd_report(run_dir: &Path) -> Result<()> {
    let cfg_path = run_dir.join("config.yaml");
    if !cfg_path.exists() {
        anyhow::bail!(
            "no config.yaml in {} — is this an active run dir?",
            run_dir.display()
        );
    }
    let exp = experiment::Experiment::load(&cfg_path)?;
    let paths = runner::RunPaths {
        log_file: run_dir.join(&exp.monitoring.log_file),
        progress_file: run_dir.join("progress.jsonl"),
        checkpoint_dir: run_dir.join(&exp.checkpoint.dir),
        run_dir: run_dir.to_path_buf(),
    };
    let r = report::build(&exp, &paths, "rebuilt", 0)?;
    report::write(&r, &paths)?;
    println!("Wrote {}", paths.run_dir.join("report.json").display());
    Ok(())
}

fn cmd_promote(
    run_dir: PathBuf,
    out_dir: PathBuf,
    model_id: String,
    command: String,
    command_args: Vec<String>,
    producer: String,
    require_success: bool,
) -> Result<()> {
    let mut manifest_command = vec![command];
    manifest_command.extend(command_args);
    let report = promotion::promote(&promotion::PromotionOptions {
        run_dir,
        out_dir,
        model_id,
        command: manifest_command,
        producer,
        require_success,
    })?;
    println!("promotion {}: model_id={}", report.status, report.model_id);
    if let Some(checkpoint) = &report.checkpoint {
        println!(
            "  checkpoint: step={} path={}",
            checkpoint.step, checkpoint.path
        );
    }
    if let Some(path) = &report.manifest_path {
        println!("  manifest:   {path}");
    }
    println!("  report:     {}/promotion-report.json", report.out_dir);
    Ok(())
}

fn cmd_checkpoints(run_dir: &Path) -> Result<()> {
    let cfg_path = run_dir.join("config.yaml");
    let dir = if cfg_path.exists() {
        let exp = experiment::Experiment::load(&cfg_path)?;
        run_dir.join(&exp.checkpoint.dir)
    } else {
        run_dir.join("checkpoints")
    };
    let ckpts = checkpoint::list_checkpoints(&dir)?;
    if ckpts.is_empty() {
        println!("(no checkpoints under {})", dir.display());
        return Ok(());
    }
    for c in ckpts {
        println!("step {:>8}  {}", c.step, c.path.display());
    }
    Ok(())
}
