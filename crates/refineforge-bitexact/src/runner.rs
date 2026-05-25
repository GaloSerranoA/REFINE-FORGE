//! Run a kernel-experiment N times and collect output hashes.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::experiment::{KernelExperiment, OutputSource};
use crate::hash::{hash_bytes, hash_file};
use crate::manifest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub run_index: usize,
    pub started_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
    pub output_hash: Option<String>,
    pub error: Option<String>,
}

pub struct RunPaths {
    pub run_dir: PathBuf,
}

impl RunPaths {
    pub fn for_experiment(runs_root: &Path, exp: &KernelExperiment) -> Self {
        Self {
            run_dir: runs_root.join(&exp.id),
        }
    }
}

/// Execute the kernel `exp.runs` times. Returns one RunResult per
/// invocation. The caller decides pass/fail by comparing the hashes.
pub fn run_all(runs_root: &Path, exp: &KernelExperiment) -> Result<Vec<RunResult>> {
    let paths = RunPaths::for_experiment(runs_root, exp);
    std::fs::create_dir_all(&paths.run_dir)?;
    let _input_manifest = manifest::build_input_manifest(&exp.input_files)?;

    let mut out = Vec::with_capacity(exp.runs);
    for i in 0..exp.runs {
        out.push(run_once(&paths.run_dir, exp, i));
    }
    write_runs_jsonl(&paths.run_dir.join("runs.jsonl"), &out)?;
    Ok(out)
}

fn write_runs_jsonl(path: &Path, runs: &[RunResult]) -> Result<()> {
    let mut file =
        std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    for run in runs {
        let json = serde_json::to_string(run)?;
        writeln!(file, "{json}").with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

pub fn run_once(run_dir: &Path, exp: &KernelExperiment, run_index: usize) -> RunResult {
    let started = Utc::now();
    let t0 = Instant::now();
    let cmd_string = exp.substitute(&exp.command, run_dir, run_index);
    let argv: Vec<String> = shell_split(&cmd_string);
    let mut result = RunResult {
        run_index,
        started_at: started,
        duration_ms: 0,
        exit_code: None,
        output_hash: None,
        error: None,
    };

    if argv.is_empty() {
        result.error = Some("empty command after substitution".into());
        return result;
    }

    let (program, args) = argv.split_first().unwrap();
    let mut cmd = Command::new(program);
    cmd.args(args);
    for (k, v) in &exp.env {
        cmd.env(k, v);
    }

    let output = match &exp.output {
        OutputSource::Stdout => {
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd.output()
        }
        OutputSource::File(_) => {
            // Inherit stderr for visibility; we don't need stdout.
            cmd.stdout(Stdio::null()).stderr(Stdio::piped());
            cmd.output()
        }
    };

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            result.error = Some(format!("failed to spawn `{program}`: {e}"));
            result.duration_ms = t0.elapsed().as_millis() as u64;
            return result;
        }
    };

    result.exit_code = output.status.code();
    result.duration_ms = t0.elapsed().as_millis() as u64;

    if !output.status.success() {
        let stderr_tail = String::from_utf8_lossy(&output.stderr);
        result.error = Some(format!(
            "kernel exited {}: {}",
            output.status,
            truncate(&stderr_tail, 200)
        ));
        return result;
    }

    let hash = match &exp.output {
        OutputSource::Stdout => Ok(hash_bytes(&output.stdout)),
        OutputSource::File(path) => {
            let resolved = exp.substitute(path, run_dir, run_index);
            let p = PathBuf::from(&resolved);
            hash_file(&p).with_context(|| format!("hashing output file {resolved}"))
        }
    };

    match hash {
        Ok(h) => result.output_hash = Some(h),
        Err(e) => result.error = Some(format!("hash failed: {e}")),
    }
    result
}

fn shell_split(s: &str) -> Vec<String> {
    s.split_whitespace().map(|s| s.to_string()).collect()
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n.saturating_sub(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::{KernelProduction, KernelProfile, KernelReference, KernelSource};
    use std::collections::BTreeMap;

    fn make_exp(cmd: &str, runs: usize, output: OutputSource) -> KernelExperiment {
        KernelExperiment {
            id: "test-runner".into(),
            template_version: None,
            description: "".into(),
            producer: None,
            kernel_id: None,
            profile: KernelProfile::Generic,
            command: cmd.into(),
            runs,
            output,
            source: KernelSource::default(),
            reference: KernelReference::default(),
            production: KernelProduction::default(),
            expected_sha256: None,
            input_files: vec![],
            tags: vec![],
            env: BTreeMap::new(),
            hardware: BTreeMap::new(),
        }
    }

    #[test]
    #[cfg(unix)]
    fn deterministic_command_yields_identical_hashes() {
        let td = tempfile::tempdir().unwrap();
        let exp = make_exp("echo deterministic-output", 3, OutputSource::Stdout);
        let results = run_all(td.path(), &exp).unwrap();
        assert_eq!(results.len(), 3);
        let hashes: Vec<&String> = results
            .iter()
            .filter_map(|r| r.output_hash.as_ref())
            .collect();
        assert_eq!(hashes.len(), 3);
        assert!(
            hashes.windows(2).all(|w| w[0] == w[1]),
            "all hashes must match: {:?}",
            hashes
        );
    }

    #[test]
    #[cfg(unix)]
    fn nondeterministic_command_yields_different_hashes() {
        let td = tempfile::tempdir().unwrap();
        // RANDOM is a bash-specific variable that changes per invocation.
        let exp = make_exp("bash -c 'echo $RANDOM-$RANDOM'", 3, OutputSource::Stdout);
        let results = run_all(td.path(), &exp).unwrap();
        let hashes: Vec<String> = results
            .iter()
            .filter_map(|r| r.output_hash.clone())
            .collect();
        // It's astronomically unlikely (but theoretically possible) that
        // 3 calls to $RANDOM produce the same value 3 times. If this
        // test ever flakes, that's a story for a winning lottery ticket.
        let unique: std::collections::HashSet<&String> = hashes.iter().collect();
        assert!(
            unique.len() > 1,
            "expected non-equal hashes, got {hashes:?}"
        );
    }

    #[test]
    fn spawn_failure_is_captured_per_run() {
        let td = tempfile::tempdir().unwrap();
        let exp = make_exp("this-binary-does-not-exist-12345", 2, OutputSource::Stdout);
        let results = run_all(td.path(), &exp).unwrap();
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.error.is_some(), "spawn failure must populate error");
            assert!(r.output_hash.is_none());
        }
    }

    #[test]
    fn run_all_writes_per_run_jsonl_audit_stream() {
        let td = tempfile::tempdir().unwrap();
        let exp = make_exp("this-binary-does-not-exist-jsonl", 2, OutputSource::Stdout);

        let _ = run_all(td.path(), &exp).unwrap();

        let jsonl = td.path().join("test-runner").join("runs.jsonl");
        let text = std::fs::read_to_string(&jsonl).expect("runs.jsonl must exist");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["run_index"], 0);
        assert!(first["error"].as_str().unwrap().contains("failed to spawn"));
    }
}
