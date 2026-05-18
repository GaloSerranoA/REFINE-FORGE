//! Report builder: combines per-entry results + corpus metadata
//! into the JSON written to disk.

use anyhow::Error;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::corpus::CorpusEntry;
use crate::metrics::{summarise, Summary};
use crate::runner::EntryResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub corpus_path: String,
    pub strategy: String,
    pub max_iterations: usize,
    pub run_started: DateTime<Utc>,
    pub run_finished: DateTime<Utc>,
    pub entries: Vec<EntryReport>,
    pub summary: SummaryReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryReport {
    pub id: String,
    pub claim_id: String,
    pub mutation: String,
    pub result: Option<EntryResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryReport {
    pub total: usize,
    pub fixed_count: usize,
    pub already_clean_count: usize,
    pub no_proposal_count: usize,
    pub unrecoverable_count: usize,
    pub max_iter_count: usize,
    pub error_count: usize,
    pub median_duration_ms: u64,
    pub p95_duration_ms: u64,
    pub repair_rate: f64,
}

impl From<Summary> for SummaryReport {
    fn from(s: Summary) -> Self {
        let denom = s.total.max(1);
        Self {
            repair_rate: s.fixed_count as f64 / denom as f64,
            total: s.total,
            fixed_count: s.fixed_count,
            already_clean_count: s.already_clean_count,
            no_proposal_count: s.no_proposal_count,
            unrecoverable_count: s.unrecoverable_count,
            max_iter_count: s.max_iter_count,
            error_count: s.error_count,
            median_duration_ms: s.median_duration_ms,
            p95_duration_ms: s.p95_duration_ms,
        }
    }
}

impl Report {
    pub fn build(
        corpus_path: &Path,
        strategy: &str,
        max_iterations: usize,
        run_started: DateTime<Utc>,
        results: Vec<(CorpusEntry, Result<EntryResult, Error>)>,
    ) -> Self {
        let mut entries = Vec::with_capacity(results.len());
        let mut outcomes_for_summary: Vec<(String, u64)> = Vec::with_capacity(results.len());

        for (entry, res) in results {
            let (result_opt, error_opt, outcome_for_summary, duration) = match res {
                Ok(r) => {
                    let outcome = r.outcome.clone();
                    let dur = r.duration_ms;
                    (Some(r), None, outcome, dur)
                }
                Err(e) => (None, Some(e.to_string()), "Error".into(), 0),
            };
            outcomes_for_summary.push((outcome_for_summary, duration));
            entries.push(EntryReport {
                id: entry.id.clone(),
                claim_id: entry.claim_id.clone(),
                mutation: entry.mutation.clone(),
                result: result_opt,
                error: error_opt,
            });
        }

        let summary: SummaryReport = summarise(&outcomes_for_summary).into();

        Self {
            corpus_path: corpus_path.display().to_string(),
            strategy: strategy.to_string(),
            max_iterations,
            run_started,
            run_finished: Utc::now(),
            entries,
            summary,
        }
    }
}
