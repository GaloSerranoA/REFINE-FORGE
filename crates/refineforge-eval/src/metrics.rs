//! Aggregation: per-corpus metrics.
//!
//! Bootstrap confidence intervals are documented in
//! `docs/repair-evaluation.md` but not implemented in v1 because the
//! current corpus (5-10 entries) is too small for the CIs to be
//! meaningful. Add them once the mathlib mutation pipeline produces
//! N >= 1000.

#[derive(Debug, Clone)]
pub struct Summary {
    pub total: usize,
    pub fixed_count: usize,
    pub already_clean_count: usize,
    pub no_proposal_count: usize,
    pub unrecoverable_count: usize,
    pub max_iter_count: usize,
    pub error_count: usize,
    pub median_duration_ms: u64,
    pub p95_duration_ms: u64,
}

pub fn summarise(per_entry_outcomes: &[(String, u64)]) -> Summary {
    let total = per_entry_outcomes.len();
    let mut fixed = 0;
    let mut already = 0;
    let mut no_prop = 0;
    let mut unrecover = 0;
    let mut max_iter = 0;
    let mut errored = 0;
    let mut durations: Vec<u64> = Vec::with_capacity(total);

    for (outcome, dur) in per_entry_outcomes {
        durations.push(*dur);
        match outcome.as_str() {
            "Fixed" => fixed += 1,
            "AlreadyClean" => already += 1,
            "NoProposal" => no_prop += 1,
            "UnrecoverableError" => unrecover += 1,
            "MaxIterationsReached" => max_iter += 1,
            "Error" => errored += 1,
            _ => {}
        }
    }

    durations.sort_unstable();
    let median = percentile(&durations, 50);
    let p95 = percentile(&durations, 95);

    Summary {
        total,
        fixed_count: fixed,
        already_clean_count: already,
        no_proposal_count: no_prop,
        unrecoverable_count: unrecover,
        max_iter_count: max_iter,
        error_count: errored,
        median_duration_ms: median,
        p95_duration_ms: p95,
    }
}

fn percentile(sorted: &[u64], pct: u32) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    // Nearest-rank method; good enough for headline numbers.
    let idx = ((pct as f64 / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[idx.saturating_sub(1).min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarise_empty() {
        let s = summarise(&[]);
        assert_eq!(s.total, 0);
        assert_eq!(s.fixed_count, 0);
        assert_eq!(s.median_duration_ms, 0);
    }

    #[test]
    fn summarise_mixed() {
        let data = vec![
            ("Fixed".into(), 100),
            ("Fixed".into(), 200),
            ("NoProposal".into(), 50),
            ("UnrecoverableError".into(), 300),
            ("AlreadyClean".into(), 10),
        ];
        let s = summarise(&data);
        assert_eq!(s.total, 5);
        assert_eq!(s.fixed_count, 2);
        assert_eq!(s.no_proposal_count, 1);
        assert_eq!(s.unrecoverable_count, 1);
        assert_eq!(s.already_clean_count, 1);
        assert_eq!(s.median_duration_ms, 100);
    }

    #[test]
    fn percentile_correctness() {
        let v = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&v, 50), 50);
        assert_eq!(percentile(&v, 95), 100);
        assert_eq!(percentile(&v, 100), 100);
    }
}
