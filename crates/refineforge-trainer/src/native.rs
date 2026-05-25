//! Built-in deterministic native trainer for local proof-repair smoke runs.
//!
//! This is deliberately small, but it performs real gradient-based training:
//! proof-repair prompts are hashed into feature vectors, patch `new_text`
//! values become deterministic target buckets, and a linear softmax model is
//! trained with SGD. It is a native smoke backend, not an LLM trainer.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use crate::experiment::Experiment;
use crate::progress::ProgressRecord;
use crate::runner::RunPaths;

#[derive(Debug, Clone)]
pub struct NativeRunOutcome {
    pub progress_records: usize,
}

#[derive(Debug, Clone)]
struct NativeConfig {
    steps: usize,
    learning_rate: f64,
    feature_buckets: usize,
    target_buckets: usize,
}

#[derive(Debug, Clone)]
struct TrainingRow {
    id: Option<String>,
    prompt: String,
    target_text: String,
}

#[derive(Debug, Deserialize)]
struct RawTrainingRow {
    id: Option<String>,
    prompt: String,
    response: String,
}

#[derive(Debug, Serialize)]
struct NativeCheckpoint<'a> {
    schema_version: &'static str,
    backend_kind: &'static str,
    step: usize,
    train_rows: usize,
    row_ids_sha256: String,
    feature_buckets: usize,
    target_buckets: usize,
    learning_rate: f64,
    loss: f64,
    accuracy: f64,
    weights_sha256: String,
    weights: &'a [Vec<f64>],
    bias: &'a [f64],
}

pub fn run(paths: &RunPaths, exp: &Experiment) -> Result<NativeRunOutcome> {
    let cfg = NativeConfig::from_experiment(exp)?;
    let rows = load_rows(&exp.dataset.path)?;
    if rows.is_empty() {
        anyhow::bail!(
            "refineforge_native requires at least one training row in {}",
            exp.dataset.path.display()
        );
    }

    let mut model = NativeLinearModel::new(cfg.feature_buckets, cfg.target_buckets);
    let mut progress_file = std::fs::File::create(&paths.progress_file)
        .with_context(|| format!("creating {}", paths.progress_file.display()))?;
    let mut log_file = std::fs::File::create(&paths.log_file)
        .with_context(|| format!("creating {}", paths.log_file.display()))?;

    writeln!(
        log_file,
        "refineforge_native start rows={} steps={} feature_buckets={} target_buckets={}",
        rows.len(),
        cfg.steps,
        cfg.feature_buckets,
        cfg.target_buckets
    )?;

    let mut records = 0usize;
    let save_steps = exp.checkpoint.save_steps.unwrap_or(cfg.steps as u64).max(1) as usize;
    for step in 1..=cfg.steps {
        let metrics = model.train_step(&rows, cfg.learning_rate);
        let raw = format!(
            "step={} loss={:.8} accuracy={:.8} learning_rate={:.8}",
            step, metrics.loss, metrics.accuracy, cfg.learning_rate
        );
        writeln!(log_file, "{raw}")?;

        let mut progress_metrics = BTreeMap::new();
        progress_metrics.insert("loss".to_string(), metrics.loss);
        progress_metrics.insert("accuracy".to_string(), metrics.accuracy);
        progress_metrics.insert("learning_rate".to_string(), cfg.learning_rate);
        let record = ProgressRecord {
            timestamp: Utc::now(),
            raw,
            metrics: progress_metrics,
            step: Some(step as u64),
        };
        writeln!(progress_file, "{}", serde_json::to_string(&record)?)?;
        records += 1;

        if step % save_steps == 0 || step == cfg.steps {
            write_checkpoint(paths, &cfg, &model, &metrics, &rows, step)?;
        }
    }

    writeln!(log_file, "refineforge_native completed normally")?;
    Ok(NativeRunOutcome {
        progress_records: records,
    })
}

impl NativeConfig {
    fn from_experiment(exp: &Experiment) -> Result<Self> {
        let steps = hyper_usize(exp, "steps", 10)?;
        let learning_rate = hyper_f64(exp, "learning_rate", 0.1)?;
        let feature_buckets = hyper_usize(exp, "feature_buckets", 64)?;
        let target_buckets = hyper_usize(exp, "target_buckets", 16)?;
        if steps == 0 {
            anyhow::bail!("refineforge_native hyperparameters.steps must be greater than 0");
        }
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            anyhow::bail!(
                "refineforge_native hyperparameters.learning_rate must be finite and positive"
            );
        }
        if feature_buckets < 2 {
            anyhow::bail!("refineforge_native hyperparameters.feature_buckets must be at least 2");
        }
        if target_buckets < 2 {
            anyhow::bail!("refineforge_native hyperparameters.target_buckets must be at least 2");
        }
        Ok(Self {
            steps,
            learning_rate,
            feature_buckets,
            target_buckets,
        })
    }
}

fn hyper_usize(exp: &Experiment, key: &str, default: usize) -> Result<usize> {
    let Some(value) = exp.hyperparameters.get(key) else {
        return Ok(default);
    };
    let value = value
        .as_u64()
        .with_context(|| format!("hyperparameters.{key} must be an unsigned integer"))?;
    usize::try_from(value).with_context(|| format!("hyperparameters.{key} is too large"))
}

fn hyper_f64(exp: &Experiment, key: &str, default: f64) -> Result<f64> {
    let Some(value) = exp.hyperparameters.get(key) else {
        return Ok(default);
    };
    value
        .as_f64()
        .with_context(|| format!("hyperparameters.{key} must be a number"))
}

fn load_rows(path: &Path) -> Result<Vec<TrainingRow>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut rows = Vec::new();
    for (index, raw_line) in text.lines().enumerate() {
        let line_no = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let raw: RawTrainingRow =
            serde_json::from_str(line).with_context(|| format!("parsing row {line_no}"))?;
        let target_text = target_text_from_response(&raw.response)
            .with_context(|| format!("extracting target patch text at row {line_no}"))?;
        rows.push(TrainingRow {
            id: raw.id,
            prompt: raw.prompt,
            target_text,
        });
    }
    Ok(rows)
}

fn target_text_from_response(response: &str) -> Result<String> {
    let value: Value = serde_json::from_str(response).context("response is not JSON")?;
    let patch = value.get("patch").unwrap_or(&value);
    patch
        .get("new_text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .context("response patch is missing new_text")
}

#[derive(Debug, Clone)]
struct NativeLinearModel {
    feature_buckets: usize,
    target_buckets: usize,
    weights: Vec<Vec<f64>>,
    bias: Vec<f64>,
}

#[derive(Debug, Clone)]
struct StepMetrics {
    loss: f64,
    accuracy: f64,
}

impl NativeLinearModel {
    fn new(feature_buckets: usize, target_buckets: usize) -> Self {
        Self {
            feature_buckets,
            target_buckets,
            weights: vec![vec![0.0; feature_buckets]; target_buckets],
            bias: vec![0.0; target_buckets],
        }
    }

    fn train_step(&mut self, rows: &[TrainingRow], learning_rate: f64) -> StepMetrics {
        let mut loss_sum = 0.0f64;
        let mut correct = 0usize;
        for row in rows {
            let features = feature_vector(&row.prompt, self.feature_buckets);
            let target = target_bucket(&row.target_text, self.target_buckets);
            let logits = self.logits(&features);
            let mut probabilities = softmax(&logits);
            let predicted = argmax(&probabilities);
            if predicted == target {
                correct += 1;
            }
            loss_sum += -probabilities[target].max(1.0e-12).ln();

            probabilities[target] -= 1.0;
            for (class, gradient) in probabilities.iter().copied().enumerate() {
                for (bucket, feature) in features.iter().copied().enumerate() {
                    if feature != 0.0 {
                        self.weights[class][bucket] -= learning_rate * gradient * feature;
                    }
                }
                self.bias[class] -= learning_rate * gradient;
            }
        }
        StepMetrics {
            loss: loss_sum / rows.len() as f64,
            accuracy: correct as f64 / rows.len() as f64,
        }
    }

    fn logits(&self, features: &[f64]) -> Vec<f64> {
        let mut logits = self.bias.clone();
        for (class, logit) in logits.iter_mut().enumerate() {
            for (bucket, feature) in features.iter().copied().enumerate() {
                *logit += self.weights[class][bucket] * feature;
            }
        }
        logits
    }
}

fn feature_vector(prompt: &str, buckets: usize) -> Vec<f64> {
    let mut features = vec![0.0; buckets];
    let mut token = String::new();
    for ch in prompt.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            token.push(ch.to_ascii_lowercase());
        } else {
            flush_token(&mut token, &mut features);
        }
    }
    flush_token(&mut token, &mut features);
    if features.iter().all(|value| *value == 0.0) {
        let bucket = stable_bucket(prompt, buckets);
        features[bucket] = 1.0;
    }
    let norm = features
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt()
        .max(1.0);
    for value in &mut features {
        *value /= norm;
    }
    features
}

fn flush_token(token: &mut String, features: &mut [f64]) {
    if token.is_empty() {
        return;
    }
    let bucket = stable_bucket(token, features.len());
    features[bucket] += 1.0;
    token.clear();
}

fn target_bucket(text: &str, buckets: usize) -> usize {
    stable_bucket(text, buckets)
}

fn stable_bucket(text: &str, buckets: usize) -> usize {
    let digest = Sha256::digest(text.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    (u64::from_be_bytes(bytes) % buckets as u64) as usize
}

fn softmax(logits: &[f64]) -> Vec<f64> {
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut exps: Vec<f64> = logits.iter().map(|value| (value - max).exp()).collect();
    let sum: f64 = exps.iter().sum();
    for value in &mut exps {
        *value /= sum;
    }
    exps
}

fn argmax(values: &[f64]) -> usize {
    values
        .iter()
        .copied()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn write_checkpoint(
    paths: &RunPaths,
    cfg: &NativeConfig,
    model: &NativeLinearModel,
    metrics: &StepMetrics,
    rows: &[TrainingRow],
    step: usize,
) -> Result<()> {
    let checkpoint_dir = paths.checkpoint_dir.join(format!("step-{step}"));
    std::fs::create_dir_all(&checkpoint_dir)
        .with_context(|| format!("creating {}", checkpoint_dir.display()))?;
    let weights_sha256 = weights_sha256(&model.weights, &model.bias)?;
    let checkpoint = NativeCheckpoint {
        schema_version: "refineforge-native-checkpoint-v0",
        backend_kind: "refineforge_native",
        step,
        train_rows: rows.len(),
        row_ids_sha256: row_ids_sha256(rows),
        feature_buckets: cfg.feature_buckets,
        target_buckets: cfg.target_buckets,
        learning_rate: cfg.learning_rate,
        loss: metrics.loss,
        accuracy: metrics.accuracy,
        weights_sha256,
        weights: &model.weights,
        bias: &model.bias,
    };
    let checkpoint_path = checkpoint_dir.join("native-checkpoint.json");
    std::fs::write(&checkpoint_path, serde_json::to_string_pretty(&checkpoint)?)
        .with_context(|| format!("writing {}", checkpoint_path.display()))?;
    Ok(())
}

fn row_ids_sha256(rows: &[TrainingRow]) -> String {
    let mut hasher = Sha256::new();
    for row in rows {
        hasher.update(row.id.as_deref().unwrap_or(""));
        hasher.update(b"\n");
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn weights_sha256(weights: &[Vec<f64>], bias: &[f64]) -> Result<String> {
    let bytes = serde_json::to_vec(&(weights, bias))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_text_accepts_direct_patch_response() {
        let response = r#"{"new_text":"simp","rationale":"use simplifier"}"#;
        assert_eq!(target_text_from_response(response).unwrap(), "simp");
    }

    #[test]
    fn target_text_accepts_nested_patch_response() {
        let response = r#"{"patch":{"new_text":"exact h","rationale":"reuse hypothesis"}}"#;
        assert_eq!(target_text_from_response(response).unwrap(), "exact h");
    }

    #[test]
    fn feature_vector_is_deterministic_and_normalized() {
        let left = feature_vector("alpha beta alpha", 16);
        let right = feature_vector("alpha beta alpha", 16);
        assert_eq!(left, right);
        let norm = left.iter().map(|value| value * value).sum::<f64>().sqrt();
        assert!((norm - 1.0).abs() < 1.0e-9);
    }
}
