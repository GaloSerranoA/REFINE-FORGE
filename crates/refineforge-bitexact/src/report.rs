//! Final bit-exact gate report.
//!
//! `Outcome::Pass` = every run produced an output and all hashes
//! matched. `Outcome::Fail` = either some run errored, or hashes
//! disagreed. The report includes the per-run results so a human
//! can see WHICH runs disagreed and by how much.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::experiment::{KernelExperiment, KernelReferenceKind, KernelSourceKind};
use crate::hash::all_equal;
use crate::manifest::InputArtifact;
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
    pub expected_sha256: Option<String>,
    pub observed_sha256: Option<String>,
    pub input_manifest: Vec<InputArtifact>,
    pub production_contract: KernelProductionContract,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelProductionContract {
    pub source_kind: KernelSourceKind,
    pub source_path: Option<String>,
    pub reference_kind: KernelReferenceKind,
    pub reference_path: Option<String>,
    pub requires_real_cuda: bool,
    pub hardware: BTreeMap<String, String>,
    pub compiler_runtime_metadata: BTreeMap<String, String>,
    pub performance_baseline: Option<String>,
}

impl Report {
    pub fn build_with_input_manifest(
        exp: &KernelExperiment,
        runs: Vec<RunResult>,
        input_manifest: Vec<InputArtifact>,
    ) -> Self {
        let hashes: Vec<String> = runs.iter().filter_map(|r| r.output_hash.clone()).collect();
        let any_error = runs.iter().any(|r| r.error.is_some());
        let mut unique: Vec<String> = hashes.to_vec();
        unique.sort();
        unique.dedup();
        let observed_sha256 = (unique.len() == 1).then(|| unique[0].clone());
        let expected_sha256 = exp.expected_sha256.clone();
        let baseline_matches = match (&expected_sha256, &observed_sha256) {
            (Some(expected), Some(observed)) => expected == observed,
            (Some(_), None) => false,
            (None, _) => true,
        };
        let outcome =
            if hashes.len() == runs.len() && all_equal(&hashes) && !any_error && baseline_matches {
                Outcome::Pass
            } else {
                Outcome::Fail
            };
        let summary = match &outcome {
            Outcome::Pass => format!(
                "PASS: all {} runs produced identical SHA-256 = {}",
                runs.len(),
                hashes
                    .first()
                    .map(|s| &s[..16.min(s.len())])
                    .unwrap_or("(none)")
            ),
            Outcome::Fail => {
                let err_count = runs.iter().filter(|r| r.error.is_some()).count();
                if let (Some(expected), Some(observed)) = (&expected_sha256, &observed_sha256) {
                    if expected != observed {
                        format!(
                            "FAIL: expected SHA-256 {}, observed {}",
                            &expected[..16.min(expected.len())],
                            &observed[..16.min(observed.len())]
                        )
                    } else {
                        format!(
                            "FAIL: {} runs, {} errored, {} unique hash(es)",
                            runs.len(),
                            err_count,
                            unique.len()
                        )
                    }
                } else {
                    format!(
                        "FAIL: {} runs, {} errored, {} unique hash(es)",
                        runs.len(),
                        err_count,
                        unique.len()
                    )
                }
            }
        };
        Self {
            experiment: exp.clone(),
            finished_at: Utc::now(),
            outcome,
            summary,
            runs,
            unique_hashes: unique,
            expected_sha256,
            observed_sha256,
            input_manifest,
            production_contract: production_contract(exp),
        }
    }

    pub fn write(&self, run_dir: &Path) -> Result<()> {
        let out = run_dir.join("bitexact-report.json");
        std::fs::write(&out, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", out.display()))?;
        Ok(())
    }
}

fn production_contract(exp: &KernelExperiment) -> KernelProductionContract {
    let mut compiler_runtime_metadata = BTreeMap::new();
    if let Ok(rustc) = std::process::Command::new("rustc").arg("-Vv").output() {
        if rustc.status.success() {
            compiler_runtime_metadata.insert(
                "rustc".into(),
                String::from_utf8_lossy(&rustc.stdout).trim().to_string(),
            );
        }
    }
    if let Ok(nvcc) = std::env::var("REFINEFORGE_NVCC_VERSION") {
        compiler_runtime_metadata.insert("nvcc".into(), nvcc);
    }

    KernelProductionContract {
        source_kind: exp.source.kind.clone(),
        source_path: exp
            .source
            .path
            .as_ref()
            .map(|path| path.display().to_string()),
        reference_kind: exp.reference.kind.clone(),
        reference_path: exp
            .reference
            .path
            .as_ref()
            .map(|path| path.display().to_string()),
        requires_real_cuda: exp.production.requires_real_cuda,
        hardware: exp.hardware.clone(),
        compiler_runtime_metadata,
        performance_baseline: std::env::var("REFINEFORGE_KERNEL_PERFORMANCE_BASELINE").ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::{
        KernelProduction, KernelProfile, KernelReference, KernelSource, KernelSourceKind,
        OutputSource,
    };
    use crate::manifest::InputArtifact;
    use std::collections::BTreeMap;

    fn exp_with_expected(expected_sha256: Option<&str>) -> KernelExperiment {
        KernelExperiment {
            id: "t".into(),
            template_version: None,
            description: "".into(),
            producer: None,
            kernel_id: None,
            profile: KernelProfile::Generic,
            command: "echo x".into(),
            runs: 3,
            output: OutputSource::Stdout,
            source: KernelSource::default(),
            reference: KernelReference::default(),
            production: KernelProduction::default(),
            expected_sha256: expected_sha256.map(String::from),
            input_files: vec![],
            tags: vec![],
            env: BTreeMap::new(),
            hardware: BTreeMap::new(),
        }
    }

    fn exp() -> KernelExperiment {
        exp_with_expected(None)
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
        let r = Report::build_with_input_manifest(&exp(), runs, vec![]);
        assert_eq!(r.outcome, Outcome::Pass);
        assert_eq!(r.unique_hashes.len(), 1);
        assert!(r.summary.starts_with("PASS"));
    }

    #[test]
    fn fail_when_hashes_disagree() {
        let runs = vec![run(0, Some("aaa"), None), run(1, Some("bbb"), None)];
        let r = Report::build_with_input_manifest(&exp(), runs, vec![]);
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
        let r = Report::build_with_input_manifest(&exp(), runs, vec![]);
        assert_eq!(r.outcome, Outcome::Fail);
        assert!(r.summary.contains("1 errored"));
    }

    #[test]
    fn fail_when_expected_hash_does_not_match_observed_hash() {
        let runs = vec![
            run(0, Some("aaa"), None),
            run(1, Some("aaa"), None),
            run(2, Some("aaa"), None),
        ];
        let r = Report::build_with_input_manifest(&exp_with_expected(Some("bbb")), runs, vec![]);
        assert_eq!(r.outcome, Outcome::Fail);
        assert_eq!(r.expected_sha256.as_deref(), Some("bbb"));
        assert_eq!(r.observed_sha256.as_deref(), Some("aaa"));
        assert!(r.summary.contains("expected SHA-256"), "{}", r.summary);
    }

    #[test]
    fn records_input_manifest_in_report() {
        let inputs = vec![InputArtifact {
            path: "kernels/fixtures/input.bin".into(),
            sha256: "abc".into(),
            size_bytes: 3,
        }];
        let r = Report::build_with_input_manifest(
            &exp(),
            vec![run(0, Some("aaa"), None), run(1, Some("aaa"), None)],
            inputs.clone(),
        );
        assert_eq!(r.outcome, Outcome::Pass);
        assert_eq!(r.input_manifest, inputs);
        assert_eq!(r.production_contract.source_kind, KernelSourceKind::Stub);
        assert!(!r.production_contract.requires_real_cuda);
    }

    #[test]
    fn fail_when_zero_runs() {
        let r = Report::build_with_input_manifest(&exp(), vec![], vec![]);
        assert_eq!(r.outcome, Outcome::Fail);
    }
}
