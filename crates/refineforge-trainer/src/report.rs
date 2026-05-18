//! Final training-report builder.
//!
//! Reads `progress.jsonl`, `failures.jsonl`, and the checkpoint dir;
//! emits a single `report.json` with: experiment config, per-metric
//! summary stats, checkpoint manifest, failure timeline, and final
//! outcome.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    pub progress_record_count: usize,
    pub metric_summary: BTreeMap<String, MetricStats>,
    pub checkpoints: Vec<CheckpointInfo>,
    pub failures: Vec<FailureRecord>,
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

pub fn build(exp: &Experiment, paths: &RunPaths, final_outcome: &str, attempts: usize) -> Result<Report> {
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
}
