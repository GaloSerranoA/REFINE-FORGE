//! Retry-with-backoff wrapper around `runner::run_once`.
//!
//! Classifies failure causes (OOM / interrupt / network / unknown)
//! by scanning the captured log file. Resumes from the latest
//! checkpoint when `retry.resume_from_checkpoint` is true and a
//! checkpoint exists.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

use crate::experiment::Experiment;
use crate::runner::{self, RunOutcome};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRecord {
    pub timestamp: DateTime<Utc>,
    pub attempt: u32,
    pub exit_code: Option<i32>,
    pub category: FailureCategory,
    pub message: String,
    pub action: RecoveryAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureCategory {
    OutOfMemory,
    Interrupted,
    Network,
    BackendError,
    Unknown,
    /// Success — recorded for symmetry with attempts log.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryAction {
    /// Will retry from the latest checkpoint after backoff.
    ResumeFromCheckpoint,
    /// Will retry from scratch after backoff (no checkpoint exists).
    RetryFromScratch,
    /// Permanent failure — no more retries.
    Abort,
    /// Final success.
    Done,
}

#[derive(Debug)]
pub struct RetryOutcome {
    pub attempts: u32,
    pub final_outcome: Option<RunOutcome>,
    pub failures: Vec<FailureRecord>,
}

/// Run an experiment with retries per the experiment's `retry`
/// config. Appends one `FailureRecord` to `failures.jsonl` in the
/// run dir per attempt (success-terminating attempt categorised as
/// `None` + `Done`).
pub fn run_with_retries(runs_root: &Path, exp: &Experiment) -> Result<RetryOutcome> {
    let mut failures = Vec::new();
    let mut final_outcome: Option<RunOutcome> = None;
    let max_attempts = exp.retry.max_attempts.max(1);
    let backoff = std::time::Duration::from_secs(exp.retry.backoff_seconds);

    for attempt in 1..=max_attempts {
        let outcome = runner::run_once(runs_root, exp)?;
        let exit_code = outcome.exit_status.code();
        let log_path = outcome.paths.log_file.clone();
        let log_tail = read_log_tail(&log_path, 200);

        if outcome.exit_status.success() {
            failures.push(FailureRecord {
                timestamp: Utc::now(),
                attempt,
                exit_code,
                category: FailureCategory::None,
                message: "training process exited successfully".into(),
                action: RecoveryAction::Done,
            });
            final_outcome = Some(outcome);
            break;
        }

        // Non-zero exit. Classify + decide next action.
        let category = classify_failure(&log_tail, exit_code);
        let action = decide_action(&exp, attempt, max_attempts, category, &outcome);
        let record = FailureRecord {
            timestamp: Utc::now(),
            attempt,
            exit_code,
            category,
            message: format!("exit {:?}; tail: {}", exit_code, summarise(&log_tail, 200)),
            action,
        };
        failures.push(record.clone());

        // Persist the failure log alongside the run.
        let failures_path = outcome.paths.run_dir.join("failures.jsonl");
        append_jsonl(&failures_path, &record)?;

        if matches!(action, RecoveryAction::Abort) {
            break;
        }

        if attempt < max_attempts {
            std::thread::sleep(backoff);
        }
    }

    Ok(RetryOutcome {
        attempts: failures.len() as u32,
        final_outcome,
        failures,
    })
}

fn classify_failure(log_tail: &str, exit_code: Option<i32>) -> FailureCategory {
    let lower = log_tail.to_lowercase();
    if lower.contains("out of memory") || lower.contains("cuda out of memory") || lower.contains("oom") {
        return FailureCategory::OutOfMemory;
    }
    if exit_code == Some(130) || lower.contains("keyboardinterrupt") || lower.contains("sigint") {
        return FailureCategory::Interrupted;
    }
    if lower.contains("connection refused")
        || lower.contains("network is unreachable")
        || lower.contains("ssl error")
        || lower.contains("sslerror")          // Python's SSLError class
        || lower.contains("connectionerror")   // Python's ConnectionError
        || lower.contains("timeout")
        || lower.contains("dns")
    {
        return FailureCategory::Network;
    }
    if lower.contains("traceback") || lower.contains("error:") {
        return FailureCategory::BackendError;
    }
    FailureCategory::Unknown
}

fn decide_action(
    exp: &Experiment,
    attempt: u32,
    max_attempts: u32,
    category: FailureCategory,
    outcome: &RunOutcome,
) -> RecoveryAction {
    if attempt >= max_attempts {
        return RecoveryAction::Abort;
    }
    // OOM with no checkpoint → abort (retrying won't help).
    if category == FailureCategory::OutOfMemory {
        let has_ckpt = crate::checkpoint::latest(&outcome.paths.checkpoint_dir)
            .ok()
            .flatten()
            .is_some();
        if !has_ckpt {
            return RecoveryAction::Abort;
        }
    }
    if exp.retry.resume_from_checkpoint {
        if crate::checkpoint::latest(&outcome.paths.checkpoint_dir)
            .ok()
            .flatten()
            .is_some()
        {
            return RecoveryAction::ResumeFromCheckpoint;
        }
    }
    RecoveryAction::RetryFromScratch
}

fn read_log_tail(path: &Path, lines: usize) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    text.lines()
        .rev()
        .take(lines)
        .collect::<Vec<&str>>()
        .into_iter()
        .rev()
        .collect::<Vec<&str>>()
        .join("\n")
}

fn summarise(text: &str, max: usize) -> String {
    let cleaned = text.replace('\n', " | ");
    if cleaned.len() <= max { cleaned } else { format!("{}…", &cleaned[..max.saturating_sub(1)]) }
}

fn append_jsonl<T: Serialize>(path: &Path, val: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{}", serde_json::to_string(val)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_oom() {
        assert_eq!(
            classify_failure("CUDA out of memory. Tried to allocate 2.00 GiB", None),
            FailureCategory::OutOfMemory
        );
        assert_eq!(
            classify_failure("RuntimeError: OOM at step 1234", None),
            FailureCategory::OutOfMemory
        );
    }

    #[test]
    fn classify_interrupt() {
        assert_eq!(
            classify_failure("KeyboardInterrupt", Some(130)),
            FailureCategory::Interrupted
        );
        assert_eq!(classify_failure("", Some(130)), FailureCategory::Interrupted);
    }

    #[test]
    fn classify_network() {
        assert_eq!(
            classify_failure("requests.exceptions.SSLError: bad cert", None),
            FailureCategory::Network
        );
    }

    #[test]
    fn classify_backend_error() {
        assert_eq!(
            classify_failure("Traceback (most recent call last):\nValueError: foo", None),
            FailureCategory::BackendError
        );
    }

    #[test]
    fn classify_unknown_when_no_signals() {
        assert_eq!(classify_failure("training completed", None), FailureCategory::Unknown);
    }
}
