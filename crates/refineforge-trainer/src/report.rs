//! Final training-report builder.
//!
//! Reads `progress.jsonl`, `failures.jsonl`, and the checkpoint dir;
//! emits a single `report.json` with: experiment config, per-metric
//! summary stats, checkpoint manifest, failure timeline, and final
//! outcome.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::checkpoint;
use crate::experiment::Experiment;
use crate::failure::FailureRecord;
use crate::progress::ProgressRecord;
use crate::runner::RunPaths;

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub experiment: Experiment,
    pub finished_at: DateTime<Utc>,
    pub final_outcome: String,
    pub attempts: usize,
    pub dataset_lineage: DatasetLineage,
    pub compute_ledger: ComputeLedger,
    pub progress_record_count: usize,
    pub metric_summary: BTreeMap<String, MetricStats>,
    pub checkpoints: Vec<CheckpointInfo>,
    pub failures: Vec<FailureRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetLineage {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeLedger {
    pub backend_kind: String,
    pub device: Option<String>,
    pub attempts: usize,
    pub run_budget: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetricStats {
    pub samples: usize,
    pub first: f64,
    pub last: f64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointInfo {
    pub step: u64,
    pub path: String,
    /// On-disk size in bytes (best-effort; 0 if dir-walk fails).
    pub size_bytes: u64,
}

pub fn build(
    exp: &Experiment,
    paths: &RunPaths,
    final_outcome: &str,
    attempts: usize,
) -> Result<Report> {
    let progress = read_progress(&paths.progress_file)?;
    let failures = read_failures(&paths.run_dir.join("failures.jsonl"))?;

    let mut metric_summary = BTreeMap::new();
    for metric_name in &exp.monitoring.metrics_to_track {
        let values: Vec<f64> = progress
            .iter()
            .filter_map(|r| r.metrics.get(metric_name).copied())
            .collect();
        if let Some(stats) = stats_for(&values) {
            metric_summary.insert(metric_name.clone(), stats);
        }
    }

    let checkpoints = checkpoint::list_checkpoints(&paths.checkpoint_dir)?
        .into_iter()
        .map(|c| CheckpointInfo {
            step: c.step,
            path: c.path.display().to_string(),
            size_bytes: dir_size(&c.path).unwrap_or(0),
        })
        .collect();

    Ok(Report {
        experiment: exp.clone(),
        finished_at: Utc::now(),
        final_outcome: final_outcome.to_string(),
        attempts,
        dataset_lineage: dataset_lineage(exp)?,
        compute_ledger: compute_ledger(exp, attempts),
        progress_record_count: progress.len(),
        metric_summary,
        checkpoints,
        failures,
    })
}

pub fn write(report: &Report, paths: &RunPaths) -> Result<()> {
    let out = paths.run_dir.join("report.json");
    std::fs::write(&out, serde_json::to_string_pretty(report)?)
        .with_context(|| format!("writing {}", out.display()))?;
    Ok(())
}

fn read_progress(path: &Path) -> Result<Vec<ProgressRecord>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let f = std::fs::File::open(path)?;
    let r = BufReader::new(f);
    let mut out = Vec::new();
    for line in r.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<ProgressRecord>(&line) {
            out.push(rec);
        }
    }
    Ok(out)
}

fn read_failures(path: &Path) -> Result<Vec<FailureRecord>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let f = std::fs::File::open(path)?;
    let r = BufReader::new(f);
    let mut out = Vec::new();
    for line in r.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<FailureRecord>(&line) {
            out.push(rec);
        }
    }
    Ok(out)
}

fn stats_for(values: &[f64]) -> Option<MetricStats> {
    if values.is_empty() {
        return None;
    }
    let first = values[0];
    let last = *values.last().unwrap();
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let sum: f64 = values.iter().sum();
    Some(MetricStats {
        samples: values.len(),
        first,
        last,
        min,
        max,
        mean: sum / values.len() as f64,
    })
}

fn dir_size(dir: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            total = total.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(total)
}

fn dataset_lineage(exp: &Experiment) -> Result<DatasetLineage> {
    let (sha256, size_bytes) = if exp.dataset.path.is_dir() {
        (dir_sha256(&exp.dataset.path)?, dir_size(&exp.dataset.path)?)
    } else {
        let bytes = std::fs::read(&exp.dataset.path)
            .with_context(|| format!("reading dataset {}", exp.dataset.path.display()))?;
        let size_bytes = bytes.len() as u64;
        (hex_sha256(&bytes), size_bytes)
    };
    Ok(DatasetLineage {
        path: exp.dataset.path.display().to_string(),
        sha256,
        size_bytes,
        format: exp.dataset.format.clone(),
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn dir_sha256(path: &Path) -> Result<String> {
    let mut files: Vec<std::path::PathBuf> = walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .collect();
    files.sort_by_key(|file| {
        file.strip_prefix(path)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/")
    });
    let mut hasher = Sha256::new();
    for file in files {
        let relative = file
            .strip_prefix(path)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(std::fs::read(&file)?);
        hasher.update([0xff]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn compute_ledger(exp: &Experiment, attempts: usize) -> ComputeLedger {
    ComputeLedger {
        backend_kind: exp.backend.kind.clone(),
        device: std::env::var("REFINEFORGE_TRAIN_DEVICE")
            .ok()
            .or_else(|| std::env::var("CUDA_VISIBLE_DEVICES").ok()),
        attempts,
        run_budget: std::env::var("REFINEFORGE_TRAIN_BUDGET").ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_basic() {
        let s = stats_for(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
        assert_eq!(s.samples, 5);
        assert_eq!(s.first, 1.0);
        assert_eq!(s.last, 5.0);
        assert_eq!(s.min, 1.0);
        assert_eq!(s.max, 5.0);
        assert_eq!(s.mean, 3.0);
    }

    #[test]
    fn stats_empty_returns_none() {
        assert!(stats_for(&[]).is_none());
    }

    #[test]
    fn stats_single_value() {
        let s = stats_for(&[42.0]).unwrap();
        assert_eq!(s.samples, 1);
        assert_eq!(s.first, 42.0);
        assert_eq!(s.last, 42.0);
        assert_eq!(s.mean, 42.0);
    }

    #[test]
    fn report_records_dataset_hashes_and_compute_ledger() {
        use crate::experiment::{
            Backend, BaseModel, CheckpointConfig, Dataset, Experiment, MonitoringConfig,
            RetryConfig,
        };
        use crate::runner::RunPaths;
        use std::collections::BTreeMap;

        let td = tempfile::tempdir().unwrap();
        let dataset = td.path().join("dataset.jsonl");
        std::fs::write(&dataset, b"{\"id\":\"one\"}\n").unwrap();
        let run_dir = td.path().join("run");
        let paths = RunPaths {
            run_dir: run_dir.clone(),
            log_file: run_dir.join("train.log"),
            progress_file: run_dir.join("progress.jsonl"),
            checkpoint_dir: run_dir.join("checkpoints"),
        };
        std::fs::create_dir_all(&paths.checkpoint_dir).unwrap();
        std::fs::write(&paths.progress_file, "").unwrap();

        let exp = Experiment {
            id: "lineage-fixture".into(),
            description: String::new(),
            base_model: BaseModel {
                name: "fixture-base".into(),
                source: "local".into(),
                revision: None,
            },
            dataset: Dataset {
                path: dataset.clone(),
                format: "jsonl".into(),
                fields: BTreeMap::new(),
            },
            backend: Backend {
                kind: "custom".into(),
                config_file: None,
                command: Some("true".into()),
                extra_args: vec![],
                runtime: BTreeMap::new(),
            },
            hyperparameters: BTreeMap::new(),
            checkpoint: CheckpointConfig::default(),
            monitoring: MonitoringConfig::default(),
            retry: RetryConfig::default(),
        };

        let report = build(&exp, &paths, "success", 1).unwrap();

        assert_eq!(report.dataset_lineage.path, dataset.display().to_string());
        assert_eq!(report.dataset_lineage.sha256.len(), 64);
        assert_eq!(report.dataset_lineage.size_bytes, 13);
        assert_eq!(report.compute_ledger.backend_kind, "custom");
        assert_eq!(report.compute_ledger.attempts, 1);
    }
}
