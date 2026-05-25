//! Hyperparameter sweep generator.
//!
//! Two strategies:
//!   - `cartesian`: full grid (all combinations of all axes)
//!   - `random:N`: N samples drawn from the cartesian product
//!
//! Output: a list of experiments where each is the base experiment
//! with `hyperparameters.<key>` overridden. The runner then executes
//! each experiment as a normal run.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::experiment::Experiment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sweep {
    /// Stable identifier for the sweep (becomes the parent run dir).
    pub id: String,
    #[serde(default)]
    pub description: String,
    /// Path (relative to the sweep file) to the base experiment YAML.
    pub base_experiment: PathBuf,
    /// Map of dotted-key → list of values to sweep over.
    /// Currently only `hyperparameters.<name>` keys are supported.
    pub grid: BTreeMap<String, Vec<Value>>,
    #[serde(default = "default_strategy")]
    pub strategy: String,
}

fn default_strategy() -> String {
    "cartesian".to_string()
}

impl Sweep {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let s: Sweep = serde_yaml::from_str(&text)
            .with_context(|| format!("parsing sweep YAML {}", path.display()))?;
        s.validate()?;
        Ok(s)
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            anyhow::bail!("sweep.id may not be empty");
        }
        for key in self.grid.keys() {
            if !key.starts_with("hyperparameters.") {
                anyhow::bail!(
                    "sweep.grid key {key:?} must start with `hyperparameters.` (only that namespace is supported in v1)"
                );
            }
        }
        match self.strategy.as_str() {
            "cartesian" => {}
            other if other.starts_with("random:") => {
                let n: usize = other
                    .trim_start_matches("random:")
                    .parse()
                    .with_context(|| format!("invalid random sweep count in {other:?}"))?;
                if n == 0 {
                    anyhow::bail!("random sweep count must be > 0");
                }
            }
            other => {
                anyhow::bail!("unknown sweep strategy {other:?} — supported: cartesian, random:N")
            }
        }
        Ok(())
    }

    /// Generate the concrete list of experiments. Each gets a unique
    /// `id` of the form `<sweep_id>/<run-NNNN>`.
    pub fn expand(&self, base: &Experiment) -> Vec<Experiment> {
        let combos = match self.strategy.as_str() {
            s if s.starts_with("random:") => {
                let n: usize = s.trim_start_matches("random:").parse().unwrap_or(0);
                random_sample(&self.grid, n)
            }
            _ => cartesian(&self.grid),
        };
        combos
            .into_iter()
            .enumerate()
            .map(|(i, overrides)| {
                let mut exp = base.clone();
                exp.id = format!("{}/run-{:04}", self.id, i + 1);
                for (key, val) in overrides {
                    let suffix = key.trim_start_matches("hyperparameters.");
                    exp.hyperparameters.insert(suffix.to_string(), val);
                }
                exp
            })
            .collect()
    }
}

/// Full cartesian product of the grid. Returns a list of dicts
/// where each dict has one value chosen for every axis.
fn cartesian(grid: &BTreeMap<String, Vec<Value>>) -> Vec<BTreeMap<String, Value>> {
    let keys: Vec<&String> = grid.keys().collect();
    let mut out: Vec<BTreeMap<String, Value>> = vec![BTreeMap::new()];
    for key in keys {
        let values = &grid[key];
        let mut next = Vec::with_capacity(out.len() * values.len());
        for existing in &out {
            for v in values {
                let mut clone = existing.clone();
                clone.insert(key.clone(), v.clone());
                next.push(clone);
            }
        }
        out = next;
    }
    out
}

/// Random sample of up to N combos. Uses a deterministic LCG seeded
/// by the grid keys + values so two equal sweeps produce equal
/// samples (reproducibility > true randomness for ML eval).
fn random_sample(grid: &BTreeMap<String, Vec<Value>>, n: usize) -> Vec<BTreeMap<String, Value>> {
    let all = cartesian(grid);
    if all.len() <= n {
        return all;
    }
    let mut seed: u64 = 0xCBF29CE484222325;
    for k in grid.keys() {
        for byte in k.bytes() {
            seed = seed.wrapping_mul(0x100000001B3) ^ byte as u64;
        }
    }
    let mut indices: Vec<usize> = (0..all.len()).collect();
    // Simple Fisher-Yates with our LCG.
    let mut state = seed;
    for i in (1..indices.len()).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (state >> 33) as usize % (i + 1);
        indices.swap(i, j);
    }
    indices
        .into_iter()
        .take(n)
        .map(|i| all[i].clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: i64) -> Value {
        Value::Number(serde_yaml::Number::from(x))
    }

    #[test]
    fn cartesian_3x2_gives_6() {
        let mut grid = BTreeMap::new();
        grid.insert("hyperparameters.lr".into(), vec![v(1), v(2), v(3)]);
        grid.insert("hyperparameters.bs".into(), vec![v(4), v(8)]);
        let out = cartesian(&grid);
        assert_eq!(out.len(), 6);
        // Every combo has both keys set.
        for combo in &out {
            assert!(combo.contains_key("hyperparameters.lr"));
            assert!(combo.contains_key("hyperparameters.bs"));
        }
    }

    #[test]
    fn cartesian_single_axis() {
        let mut grid = BTreeMap::new();
        grid.insert("hyperparameters.lr".into(), vec![v(1), v(2), v(3)]);
        let out = cartesian(&grid);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn random_sample_under_threshold_returns_full() {
        let mut grid = BTreeMap::new();
        grid.insert("hyperparameters.lr".into(), vec![v(1), v(2)]);
        let out = random_sample(&grid, 10);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn random_sample_is_deterministic() {
        let mut grid = BTreeMap::new();
        grid.insert(
            "hyperparameters.lr".into(),
            vec![v(1), v(2), v(3), v(4), v(5)],
        );
        grid.insert("hyperparameters.bs".into(), vec![v(8), v(16), v(32)]);
        let a = random_sample(&grid, 3);
        let b = random_sample(&grid, 3);
        assert_eq!(a, b, "random sweep must be reproducible");
    }

    #[test]
    fn validate_rejects_wrong_namespace() {
        let mut s = Sweep {
            id: "x".into(),
            description: "".into(),
            base_experiment: "x.yaml".into(),
            grid: BTreeMap::new(),
            strategy: "cartesian".into(),
        };
        s.grid.insert("base_model.name".into(), vec![v(1)]);
        let err = s.validate().unwrap_err();
        assert!(err.to_string().contains("hyperparameters."), "{err}");
    }

    #[test]
    fn validate_accepts_random_strategy() {
        let s = Sweep {
            id: "x".into(),
            description: "".into(),
            base_experiment: "x.yaml".into(),
            grid: BTreeMap::new(),
            strategy: "random:5".into(),
        };
        s.validate().unwrap();
    }

    #[test]
    fn validate_rejects_random_zero() {
        let s = Sweep {
            id: "x".into(),
            description: "".into(),
            base_experiment: "x.yaml".into(),
            grid: BTreeMap::new(),
            strategy: "random:0".into(),
        };
        assert!(s.validate().is_err());
    }
}
