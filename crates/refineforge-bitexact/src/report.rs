//! Final bit-exact gate report.
//!
//! `Outcome::Pass` = every run produced an output and all hashes
//! matched. `Outcome::Fail` = either some run errored, or hashes
//! disagreed. The report includes the per-run results so a human
//! can see WHICH runs disagreed and by how much.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::experiment::KernelExperiment;
use crate::hash::all_equal;
use crate::runner::RunResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Outcome {
    /// All N runs produced identical hashes.
    Pass,
    /// At least one run errored OR hashes disagreed.
    Fail,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub experiment: KernelExperiment,
    pub finished_at: DateTime<Utc>,
    pub outcome: Outcome,
    pub summary: String,
    pub runs: Vec<RunResult>,
    pub unique_hashes: Vec<String>,
}

impl Report {
    pub fn build(exp: &KernelExperiment, runs: Vec<RunResult>) -> Self {
        let hashes: Vec<String> = runs.iter().filter_map(|r| r.output_hash.clone()).collect();
        let any_error = runs.iter().any(|r| r.error.is_some());
        let outcome = if hashes.len() == runs.len() && all_equal(&hashes) && !any_error {
            Outcome::Pass
        } else {
            Outcome::Fail
        };
        let mut unique: Vec<String> = hashes.iter().cloned().collect();
        unique.sort();
        unique.dedup();
        let summary = match &outcome {
            Outcome::Pass => format!(
                "PASS: all {} runs produced identical SHA-256 = {}",
                runs.len(),
                hashes.first().map(|s| &s[..16.min(s.len())]).unwrap_or("(none)")
            ),
            Outcome::Fail => {
                let err_count = runs.iter().filter(|r| r.error.is_some()).count();
                format!(
                    "FAIL: {} runs, {} errored, {} unique hash(es)",
                    runs.len(),
                    err_count,
                    unique.len()
                )
            }
        };
        Self {
            experiment: exp.clone(),
            finished_at: Utc::now(),
            outcome,
            summary,
            runs,
            unique_hashes: unique,
        }
    }

    pub fn write(&self, run_dir: &Path) -> Result<()> {
        let out = run_dir.join("bitexact-report.json");
        std::fs::write(&out, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", out.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::OutputSource;
    use std::collections::BTreeMap;

    fn exp() -> KernelExperiment {
        KernelExperiment {
            id: "t".into(),
            description: "".into(),
            command: "echo x".into(),
            runs: 3,
            output: OutputSource::Stdout,
            env: BTreeMap::new(),
            hardware: BTreeMap::new(),
        }
    }

    fn run(idx: usize, hash: Option<&str>, error: Option<&str>) -> RunResult {
        RunResult {
            run_index: idx,
            started_at: Utc::now(),
            duration_ms: 100,
            exit_code: Some(0),
            output_hash: hash.map(String::from),
            error: error.map(String::from),
        }
    }

    #[test]
    fn pass_when_all_hashes_equal_and_no_errors() {
        let runs = vec![
            run(0, Some("aaa"), None),
            run(1, Some("aaa"), None),
            run(2, Some("aaa"), None),
        ];
        let r = Report::build(&exp(), runs);
        assert_eq!(r.outcome, Outcome::Pass);
        assert_eq!(r.unique_hashes.len(), 1);
        assert!(r.summary.starts_with("PASS"));
    }

    #[test]
    fn fail_when_hashes_disagree() {
        let runs = vec![
            run(0, Some("aaa"), None),
            run(1, Some("bbb"), None),
        ];
        let r = Report::build(&exp(), runs);
        assert_eq!(r.outcome, Outcome::Fail);
        assert_eq!(r.unique_hashes.len(), 2);
        assert!(r.summary.starts_with("FAIL"));
    }

    #[test]
    fn fail_when_a_run_errored_even_if_remaining_hashes_match() {
        let runs = vec![
            run(0, Some("aaa"), None),
            run(1, None, Some("kernel crashed")),
            run(2, Some("aaa"), None),
        ];
        let r = Report::build(&exp(), runs);
        assert_eq!(r.outcome, Outcome::Fail);
        assert!(r.summary.contains("1 errored"));
    }

    #[test]
    fn fail_when_zero_runs() {
        let r = Report::build(&exp(), vec![]);
        assert_eq!(r.outcome, Outcome::Fail);
    }
}
