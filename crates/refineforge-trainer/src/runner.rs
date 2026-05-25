//! Subprocess wrapper for the training-tool backend.
//!
//! Responsibilities:
//!   1. Build the run directory (`training/runs/<exp_id>/`).
//!   2. Write the resolved experiment config there.
//!   3. Substitute template tokens in `backend.command`.
//!   4. Spawn the subprocess, capturing stdout+stderr into `log_file`.
//!   5. Tail the log file and feed lines to the progress parser,
//!      appending JSONL records to `progress.jsonl`.
//!   6. Return the exit status.
//!
//! Does NOT handle retries — that's `failure.rs` wrapping this.

use anyhow::{anyhow, Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::checkpoint;
use crate::experiment::{Backend, Experiment};
use crate::progress::parser_for;
use crate::{native, native_causal};

#[derive(Debug, Clone)]
pub struct RunPaths {
    pub run_dir: PathBuf,
    pub log_file: PathBuf,
    pub progress_file: PathBuf,
    pub checkpoint_dir: PathBuf,
}

impl RunPaths {
    pub fn for_experiment(runs_root: &Path, exp: &Experiment) -> Self {
        let run_dir = runs_root.join(&exp.id);
        Self {
            log_file: run_dir.join(&exp.monitoring.log_file),
            progress_file: run_dir.join("progress.jsonl"),
            checkpoint_dir: run_dir.join(&exp.checkpoint.dir),
            run_dir,
        }
    }

    pub fn ensure_created(&self) -> Result<()> {
        std::fs::create_dir_all(&self.run_dir)?;
        std::fs::create_dir_all(&self.checkpoint_dir)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct RunOutcome {
    pub exit_status: std::process::ExitStatus,
    pub progress_records: usize,
    pub paths: RunPaths,
}

/// Build the command line for the given backend by substituting
/// template tokens. Defaults are provided for `axolotl`, `helyx_train`,
/// HRM-Text, and the in-process native dry-run labels; `custom`
/// and `pytorch_baseline` usually provide `command`.
pub fn build_command(backend: &Backend, paths: &RunPaths, exp: &Experiment) -> Result<Vec<String>> {
    let resume_from = checkpoint::latest(&paths.checkpoint_dir)?
        .map(|c| c.path.display().to_string())
        .unwrap_or_default();
    let config_file = backend
        .config_file
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    let template = match (backend.kind.as_str(), backend.command.as_deref()) {
        (_, Some(cmd)) => cmd.to_string(),
        ("axolotl", None) => {
            let cfg = backend.config_file.as_ref().ok_or_else(|| {
                anyhow!("backend.kind=axolotl with no command requires backend.config_file")
            })?;
            format!("axolotl train {} --output-dir {{run_dir}}", cfg.display())
        }
        ("hf_trainer", None) => {
            // Generic placeholder — real users override this with their
            // own training script path. We emit a clear error rather
            // than a silent miscompile.
            return Err(anyhow!(
                "backend.kind=hf_trainer with no command requires `backend.command` template — point at your training script"
            ));
        }
        ("refineforge_native", None) => {
            "refineforge-native run --dataset {dataset_path} --output {run_dir} --checkpoint-dir {checkpoint_dir}".to_string()
        }
        ("refineforge_native_causal_lm", None) => {
            "refineforge-native-causal-lm run --dataset {dataset_path} --output {run_dir} --checkpoint-dir {checkpoint_dir}".to_string()
        }
        ("helyx_train", None) => {
            let cfg = backend.config_file.as_ref().ok_or_else(|| {
                anyhow!("backend.kind=helyx_train with no command requires backend.config_file")
            })?;
            format!(
                "helyx-train run --config {} --dataset {{dataset_path}} --output {{run_dir}} --checkpoint-dir {{checkpoint_dir}}",
                cfg.display()
            )
        }
        ("hrm_text", None) => {
            let cfg = backend.config_file.as_ref().ok_or_else(|| {
                anyhow!("backend.kind=hrm_text with no command requires backend.config_file")
            })?;
            format!(
                "torchrun --nproc_per_node=1 pretrain.py --config-name {} data.path={{dataset_path}} +checkpoint_path={{checkpoint_dir}}",
                cfg.display()
            )
        }
        ("pytorch_baseline", None) => {
            return Err(anyhow!(
                "backend.kind=pytorch_baseline with no command requires `backend.command` template"
            ));
        }
        ("custom", None) => {
            return Err(anyhow!("backend.kind=custom requires command template"));
        }
        (other, _) => return Err(anyhow!("unknown backend kind {other:?}")),
    };

    let substituted = template
        .replace("{run_dir}", &paths.run_dir.display().to_string())
        .replace(
            "{checkpoint_dir}",
            &paths.checkpoint_dir.display().to_string(),
        )
        .replace("{config_file}", &config_file)
        .replace("{dataset_path}", &exp.dataset.path.display().to_string())
        .replace("{dataset_format}", &exp.dataset.format)
        .replace("{base_model}", &exp.base_model.name)
        .replace(
            "{base_revision}",
            exp.base_model.revision.as_deref().unwrap_or(""),
        )
        .replace("{resume_from}", &resume_from);

    let mut argv: Vec<String> = shell_split(&substituted);
    argv.extend(backend.extra_args.iter().cloned());
    if argv.is_empty() {
        return Err(anyhow!("empty command after substitution"));
    }
    Ok(argv)
}

/// Naive shell-split: splits on whitespace, no quoting. Adequate for
/// our tool-launch templates which are simple `tool arg --flag val`.
/// For more complex needs, the operator can shell out via a wrapper
/// script and point `command` at the wrapper.
fn shell_split(s: &str) -> Vec<String> {
    s.split_whitespace().map(|s| s.to_string()).collect()
}

/// Run the experiment once (no retry). Streams stdout/stderr to the
/// log file and parses progress lines into `progress.jsonl`.
pub fn run_once(runs_root: &Path, exp: &Experiment) -> Result<RunOutcome> {
    let paths = RunPaths::for_experiment(runs_root, exp);
    paths.ensure_created()?;

    // Write the resolved config alongside the logs (audit trail).
    let cfg_path = paths.run_dir.join("config.yaml");
    std::fs::write(&cfg_path, serde_yaml::to_string(exp)?)?;

    if exp.backend.kind == "refineforge_native" {
        let native_outcome = native::run(&paths, exp)?;
        if let Some(keep_last) = exp.checkpoint.keep_last {
            let _ = checkpoint::prune(&paths.checkpoint_dir, keep_last);
        }
        return Ok(RunOutcome {
            exit_status: successful_exit_status(),
            progress_records: native_outcome.progress_records,
            paths,
        });
    }
    if exp.backend.kind == "refineforge_native_causal_lm" {
        let native_outcome = native_causal::run(&paths, exp)?;
        if let Some(keep_last) = exp.checkpoint.keep_last {
            let _ = checkpoint::prune(&paths.checkpoint_dir, keep_last);
        }
        return Ok(RunOutcome {
            exit_status: successful_exit_status(),
            progress_records: native_outcome.progress_records,
            paths,
        });
    }

    let argv = build_command(&exp.backend, &paths, exp)?;
    let (program, args) = argv.split_first().unwrap();

    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning training backend `{program}`"))?;

    // Capture both stdout and stderr. We interleave them into the log
    // file in the order received.
    let log_handle = std::fs::File::create(&paths.log_file)?;
    let log = Arc::new(Mutex::new(log_handle));
    let progress = Arc::new(Mutex::new(std::fs::File::create(&paths.progress_file)?));
    let parser = parser_for(&exp.monitoring.progress_format);
    let parser = Arc::new(parser);
    let records = Arc::new(Mutex::new(0usize));

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("no child stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("no child stderr"))?;

    let log_for_stdout = Arc::clone(&log);
    let progress_for_stdout = Arc::clone(&progress);
    let records_for_stdout = Arc::clone(&records);
    let parser_for_stdout = Arc::clone(&parser);
    let stdout_handle = std::thread::spawn(move || -> Result<()> {
        let r = BufReader::new(stdout);
        for line in r.lines() {
            let line = line?;
            {
                let mut f = log_for_stdout.lock().unwrap();
                writeln!(f, "{line}")?;
            }
            if let Some(rec) = parser_for_stdout.parse_line(&line) {
                let json = serde_json::to_string(&rec)?;
                let mut f = progress_for_stdout.lock().unwrap();
                writeln!(f, "{json}")?;
                *records_for_stdout.lock().unwrap() += 1;
            }
        }
        Ok(())
    });

    let log_for_stderr = Arc::clone(&log);
    let stderr_handle = std::thread::spawn(move || -> Result<()> {
        let r = BufReader::new(stderr);
        for line in r.lines() {
            let line = line?;
            let mut f = log_for_stderr.lock().unwrap();
            writeln!(f, "STDERR: {line}")?;
        }
        Ok(())
    });

    let exit_status = child.wait()?;
    stdout_handle.join().unwrap_or(Ok(()))?;
    stderr_handle.join().unwrap_or(Ok(()))?;

    // Optional checkpoint pruning.
    if let Some(keep_last) = exp.checkpoint.keep_last {
        let _ = checkpoint::prune(&paths.checkpoint_dir, keep_last);
    }

    let progress_records = *records.lock().unwrap();
    Ok(RunOutcome {
        exit_status,
        progress_records,
        paths,
    })
}

#[cfg(unix)]
fn successful_exit_status() -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(0)
}

#[cfg(windows)]
fn successful_exit_status() -> std::process::ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::*;
    use std::collections::BTreeMap;

    fn minimal_exp_with_command(cmd: &str) -> Experiment {
        Experiment {
            id: "test-exp".into(),
            description: "".into(),
            base_model: BaseModel {
                name: "x".into(),
                source: "huggingface".into(),
                revision: None,
            },
            dataset: Dataset {
                path: "data.jsonl".into(),
                format: "jsonl".into(),
                fields: BTreeMap::new(),
            },
            backend: Backend {
                kind: "custom".into(),
                config_file: None,
                command: Some(cmd.into()),
                extra_args: vec![],
            },
            hyperparameters: BTreeMap::new(),
            checkpoint: CheckpointConfig::default(),
            monitoring: MonitoringConfig::default(),
            retry: RetryConfig::default(),
        }
    }

    #[test]
    fn build_command_substitutes_tokens() {
        let exp = minimal_exp_with_command("script --output {run_dir} --data {dataset_path}");
        let td = tempfile::tempdir().unwrap();
        let paths = RunPaths::for_experiment(td.path(), &exp);
        paths.ensure_created().unwrap();
        let argv = build_command(&exp.backend, &paths, &exp).unwrap();
        assert_eq!(argv[0], "script");
        // run_dir + dataset_path are substituted somewhere in the args:
        let joined = argv.join(" ");
        assert!(joined.contains("test-exp"));
        assert!(joined.contains("data.jsonl"));
    }

    #[test]
    fn build_command_appends_extra_args() {
        let mut exp = minimal_exp_with_command("script");
        exp.backend.extra_args = vec!["--seed".into(), "42".into()];
        let td = tempfile::tempdir().unwrap();
        let paths = RunPaths::for_experiment(td.path(), &exp);
        paths.ensure_created().unwrap();
        let argv = build_command(&exp.backend, &paths, &exp).unwrap();
        assert_eq!(argv, vec!["script", "--seed", "42"]);
    }

    #[test]
    fn build_command_rejects_custom_without_command() {
        let exp = Experiment {
            backend: Backend {
                kind: "custom".into(),
                config_file: None,
                command: None,
                extra_args: vec![],
            },
            ..minimal_exp_with_command("ignored")
        };
        let td = tempfile::tempdir().unwrap();
        let paths = RunPaths::for_experiment(td.path(), &exp);
        let err = build_command(&exp.backend, &paths, &exp).unwrap_err();
        assert!(err.to_string().contains("command template"));
    }

    #[test]
    fn build_command_defaults_helyx_train_backend() {
        let mut exp = minimal_exp_with_command("ignored");
        exp.backend = Backend {
            kind: "helyx_train".into(),
            config_file: Some("training/configs/helyx-proof-repair.yaml".into()),
            command: None,
            extra_args: vec![],
        };
        let td = tempfile::tempdir().unwrap();
        let paths = RunPaths::for_experiment(td.path(), &exp);
        paths.ensure_created().unwrap();

        let argv = build_command(&exp.backend, &paths, &exp).unwrap();

        assert_eq!(argv[0], "helyx-train");
        assert_eq!(argv[1], "run");
        let joined = argv.join(" ");
        assert!(
            joined.contains("--config training/configs/helyx-proof-repair.yaml"),
            "{joined}"
        );
        assert!(joined.contains("--dataset data.jsonl"), "{joined}");
        assert!(joined.contains("--output"), "{joined}");
        assert!(joined.contains("test-exp"), "{joined}");
        assert!(joined.contains("--checkpoint-dir"), "{joined}");
        assert!(joined.contains("checkpoints"), "{joined}");
    }

    #[test]
    fn build_command_describes_native_backend_dry_run() {
        let mut exp = minimal_exp_with_command("ignored");
        exp.backend = Backend {
            kind: "refineforge_native".into(),
            config_file: None,
            command: None,
            extra_args: vec![],
        };
        let td = tempfile::tempdir().unwrap();
        let paths = RunPaths::for_experiment(td.path(), &exp);
        paths.ensure_created().unwrap();

        let argv = build_command(&exp.backend, &paths, &exp).unwrap();

        assert_eq!(argv[0], "refineforge-native");
        let joined = argv.join(" ");
        assert!(joined.contains("--dataset data.jsonl"), "{joined}");
        assert!(joined.contains("--output"), "{joined}");
        assert!(joined.contains("test-exp"), "{joined}");
        assert!(joined.contains("--checkpoint-dir"), "{joined}");
    }

    #[test]
    fn run_paths_layout_is_correct() {
        let exp = minimal_exp_with_command("script");
        let td = tempfile::tempdir().unwrap();
        let p = RunPaths::for_experiment(td.path(), &exp);
        assert!(p.run_dir.ends_with("test-exp"));
        assert!(p.log_file.ends_with("train.log"));
        assert!(p.progress_file.ends_with("progress.jsonl"));
        assert!(p.checkpoint_dir.ends_with("checkpoints"));
    }
}
