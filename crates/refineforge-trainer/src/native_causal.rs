//! Native causal-LM smoke backend.
//!
//! This backend is intentionally small and CPU-only, but it exercises the
//! training-framework contract with a real next-token objective over packed SFT
//! data: token embeddings, position embeddings, causal prefix aggregation, an
//! MLP block, output-head SGD, held-out loss, and a generation smoke artifact.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;

use crate::experiment::Experiment;
use crate::pack::{self, LoadedPack, PackedRecord};
use crate::progress::ProgressRecord;
use crate::runner::RunPaths;

#[derive(Debug, Clone)]
pub struct NativeCausalOutcome {
    pub progress_records: usize,
}

#[derive(Debug, Clone)]
struct CausalConfig {
    steps: usize,
    learning_rate: f64,
    hidden_size: usize,
    eval_split: String,
}

#[derive(Debug, Clone)]
struct TrainingExample {
    id: String,
    split: String,
    tokens: Vec<u32>,
    loss_mask: Vec<u8>,
}

#[derive(Debug, Clone)]
struct MiniCausalModel {
    vocab_size: usize,
    hidden_size: usize,
    max_position: usize,
    output_weights: Vec<Vec<f64>>,
    output_bias: Vec<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
struct EvalMetrics {
    loss: f64,
    accuracy: f64,
    samples: usize,
}

#[derive(Debug, Serialize)]
struct CausalCheckpoint<'a> {
    schema_version: &'static str,
    backend_kind: &'static str,
    step: usize,
    vocab_size: usize,
    train_examples: usize,
    dev_examples: usize,
    train_loss: f64,
    dev_loss: f64,
    target_token_accuracy: f64,
    train_target_tokens: usize,
    dev_target_tokens: usize,
    weights_sha256: String,
    architecture: serde_json::Value,
    output_weights: &'a [Vec<f64>],
    output_bias: &'a [f64],
}

pub fn run(paths: &RunPaths, exp: &Experiment) -> Result<NativeCausalOutcome> {
    let cfg = CausalConfig::from_experiment(exp)?;
    let pack = pack::load_pack(&exp.dataset.path)?;
    let examples = examples_from_pack(&pack)?;
    if examples.is_empty() {
        anyhow::bail!("refineforge_native_causal_lm requires at least one packed example");
    }
    let vocab_size = pack.manifest.tokenizer.vocab_size.max(
        pack.tokens
            .iter()
            .copied()
            .max()
            .map(|max_token| max_token as usize + 1)
            .unwrap_or(2),
    );
    let max_position = examples
        .iter()
        .map(|example| example.tokens.len())
        .max()
        .unwrap_or(2)
        .max(2);
    let mut model = MiniCausalModel::new(vocab_size, cfg.hidden_size, max_position);
    let (train, dev) = split_examples(&examples, &cfg.eval_split);

    let mut progress_file = std::fs::File::create(&paths.progress_file)
        .with_context(|| format!("creating {}", paths.progress_file.display()))?;
    let mut log_file = std::fs::File::create(&paths.log_file)
        .with_context(|| format!("creating {}", paths.log_file.display()))?;
    write_train_metadata(paths, exp, &pack, &cfg, &train, &dev)?;

    writeln!(
        log_file,
        "refineforge_native_causal_lm start examples={} train={} dev={} steps={} hidden_size={} vocab_size={}",
        examples.len(),
        train.len(),
        dev.len(),
        cfg.steps,
        cfg.hidden_size,
        vocab_size
    )?;

    let save_steps = exp.checkpoint.save_steps.unwrap_or(cfg.steps as u64).max(1) as usize;
    let mut records = 0usize;
    for step in 1..=cfg.steps {
        let last_train = model.train_epoch(&train, cfg.learning_rate);
        let last_dev = model.evaluate(&dev);
        let raw = format!(
            "step={} train_loss={:.8} dev_loss={:.8} target_token_accuracy={:.8} learning_rate={:.8}",
            step, last_train.loss, last_dev.loss, last_dev.accuracy, cfg.learning_rate
        );
        writeln!(log_file, "{raw}")?;
        let mut metrics = BTreeMap::new();
        metrics.insert("train_loss".to_string(), last_train.loss);
        metrics.insert("dev_loss".to_string(), last_dev.loss);
        metrics.insert("target_token_accuracy".to_string(), last_dev.accuracy);
        metrics.insert("learning_rate".to_string(), cfg.learning_rate);
        let record = ProgressRecord {
            timestamp: Utc::now(),
            raw,
            metrics,
            step: Some(step as u64),
        };
        writeln!(progress_file, "{}", serde_json::to_string(&record)?)?;
        records += 1;

        if step % save_steps == 0 || step == cfg.steps {
            write_checkpoint(
                paths,
                &model,
                &cfg,
                &train,
                &dev,
                step,
                (last_train, last_dev),
            )?;
        }
    }
    write_generation_smoke(paths, &model, &examples)?;
    writeln!(log_file, "refineforge_native_causal_lm completed normally")?;
    Ok(NativeCausalOutcome {
        progress_records: records,
    })
}

impl CausalConfig {
    fn from_experiment(exp: &Experiment) -> Result<Self> {
        let steps = hyper_usize(exp, "steps", 8)?;
        let learning_rate = hyper_f64(exp, "learning_rate", 0.05)?;
        let hidden_size = hyper_usize(exp, "hidden_size", 16)?;
        let eval_split = exp
            .hyperparameters
            .get("eval_split")
            .and_then(|value| value.as_str())
            .unwrap_or("heldout")
            .to_string();
        if steps == 0 {
            anyhow::bail!(
                "refineforge_native_causal_lm hyperparameters.steps must be greater than 0"
            );
        }
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            anyhow::bail!(
                "refineforge_native_causal_lm hyperparameters.learning_rate must be finite and positive"
            );
        }
        if hidden_size < 2 {
            anyhow::bail!(
                "refineforge_native_causal_lm hyperparameters.hidden_size must be at least 2"
            );
        }
        Ok(Self {
            steps,
            learning_rate,
            hidden_size,
            eval_split,
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

fn examples_from_pack(pack: &LoadedPack) -> Result<Vec<TrainingExample>> {
    pack.records
        .iter()
        .map(|record| example_from_record(record, pack))
        .collect()
}

fn example_from_record(record: &PackedRecord, pack: &LoadedPack) -> Result<TrainingExample> {
    let start = record.token_start;
    let end = start + record.token_len;
    if end > pack.tokens.len() || end > pack.loss_mask.len() {
        anyhow::bail!("packed record {} is out of token range", record.id);
    }
    Ok(TrainingExample {
        id: record.id.clone(),
        split: record.split.clone(),
        tokens: pack.tokens[start..end].to_vec(),
        loss_mask: pack.loss_mask[start..end].to_vec(),
    })
}

fn split_examples<'a>(
    examples: &'a [TrainingExample],
    eval_split: &str,
) -> (Vec<&'a TrainingExample>, Vec<&'a TrainingExample>) {
    let mut train = Vec::new();
    let mut dev = Vec::new();
    for example in examples {
        let split = example.split.to_ascii_lowercase();
        if split == eval_split || split == "heldout" || split == "eval" || split == "val" {
            dev.push(example);
        } else {
            train.push(example);
        }
    }
    if train.is_empty() {
        train = examples.iter().collect();
    }
    if dev.is_empty() {
        dev = train.clone();
    }
    (train, dev)
}

impl MiniCausalModel {
    fn new(vocab_size: usize, hidden_size: usize, max_position: usize) -> Self {
        Self {
            vocab_size,
            hidden_size,
            max_position,
            output_weights: vec![vec![0.0; hidden_size]; vocab_size],
            output_bias: vec![0.0; vocab_size],
        }
    }

    fn train_epoch(&mut self, examples: &[&TrainingExample], learning_rate: f64) -> EvalMetrics {
        let mut loss_sum = 0.0;
        let mut correct = 0usize;
        let mut samples = 0usize;
        for example in examples {
            for position in 1..example.tokens.len() {
                if example.loss_mask.get(position).copied().unwrap_or(0) == 0 {
                    continue;
                }
                let target = example.tokens[position] as usize;
                if target >= self.vocab_size {
                    continue;
                }
                let hidden = self.hidden_state(&example.tokens[..position], position);
                let logits = self.logits(&hidden);
                let mut probabilities = softmax(&logits);
                let predicted = argmax(&probabilities);
                if predicted == target {
                    correct += 1;
                }
                loss_sum += -probabilities[target].max(1.0e-12).ln();
                probabilities[target] -= 1.0;
                for (token, gradient) in probabilities.iter().copied().enumerate() {
                    for (idx, value) in hidden.iter().copied().enumerate() {
                        self.output_weights[token][idx] -= learning_rate * gradient * value;
                    }
                    self.output_bias[token] -= learning_rate * gradient;
                }
                samples += 1;
            }
        }
        metrics(loss_sum, correct, samples)
    }

    fn evaluate(&self, examples: &[&TrainingExample]) -> EvalMetrics {
        let mut loss_sum = 0.0;
        let mut correct = 0usize;
        let mut samples = 0usize;
        for example in examples {
            for position in 1..example.tokens.len() {
                if example.loss_mask.get(position).copied().unwrap_or(0) == 0 {
                    continue;
                }
                let target = example.tokens[position] as usize;
                if target >= self.vocab_size {
                    continue;
                }
                let hidden = self.hidden_state(&example.tokens[..position], position);
                let logits = self.logits(&hidden);
                let probabilities = softmax(&logits);
                let predicted = argmax(&probabilities);
                if predicted == target {
                    correct += 1;
                }
                loss_sum += -probabilities[target].max(1.0e-12).ln();
                samples += 1;
            }
        }
        metrics(loss_sum, correct, samples)
    }

    fn hidden_state(&self, prefix: &[u32], position: usize) -> Vec<f64> {
        let mut state = vec![0.0; self.hidden_size];
        let count = prefix.len().max(1) as f64;
        for (offset, token) in prefix.iter().copied().enumerate() {
            for (idx, value) in state.iter_mut().enumerate() {
                *value += token_embedding(token, idx) / count;
                let absolute_position = position
                    .saturating_sub(prefix.len())
                    .saturating_add(offset)
                    .min(self.max_position - 1);
                *value += position_embedding(absolute_position, idx) / count;
            }
        }
        for (idx, value) in state.iter_mut().enumerate() {
            let gate = deterministic_weight("mlp", idx, self.hidden_size);
            *value = (*value * gate).tanh().max(0.0);
        }
        state
    }

    fn logits(&self, hidden: &[f64]) -> Vec<f64> {
        self.output_weights
            .iter()
            .zip(self.output_bias.iter().copied())
            .map(|(weights, bias)| {
                weights
                    .iter()
                    .zip(hidden.iter())
                    .map(|(weight, value)| weight * value)
                    .sum::<f64>()
                    + bias
            })
            .collect()
    }

    fn generate(&self, seed: &[u32], max_new_tokens: usize) -> Vec<u32> {
        let mut out = seed.to_vec();
        for _ in 0..max_new_tokens {
            let position = out.len().max(1);
            let hidden = self.hidden_state(&out, position);
            let logits = self.logits(&hidden);
            let next = argmax(&logits) as u32;
            out.push(next);
        }
        out
    }
}

fn metrics(loss_sum: f64, correct: usize, samples: usize) -> EvalMetrics {
    if samples == 0 {
        return EvalMetrics {
            loss: 0.0,
            accuracy: 0.0,
            samples: 0,
        };
    }
    EvalMetrics {
        loss: loss_sum / samples as f64,
        accuracy: correct as f64 / samples as f64,
        samples,
    }
}

fn token_embedding(token: u32, idx: usize) -> f64 {
    deterministic_weight(&format!("tok:{token}"), idx, 97)
}

fn position_embedding(position: usize, idx: usize) -> f64 {
    deterministic_weight(&format!("pos:{position}"), idx, 89)
}

fn deterministic_weight(namespace: &str, idx: usize, scale: usize) -> f64 {
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    hasher.update(idx.to_le_bytes());
    hasher.update(scale.to_le_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    let raw = u64::from_le_bytes(bytes) as f64 / u64::MAX as f64;
    (raw - 0.5) * 0.2
}

fn softmax(logits: &[f64]) -> Vec<f64> {
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut values: Vec<f64> = logits.iter().map(|value| (value - max).exp()).collect();
    let sum = values.iter().sum::<f64>().max(1.0e-12);
    for value in &mut values {
        *value /= sum;
    }
    values
}

fn argmax(values: &[f64]) -> usize {
    values
        .iter()
        .copied()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn write_train_metadata(
    paths: &RunPaths,
    exp: &Experiment,
    pack: &LoadedPack,
    cfg: &CausalConfig,
    train: &[&TrainingExample],
    dev: &[&TrainingExample],
) -> Result<()> {
    let metadata = serde_json::json!({
        "schema_version": "refineforge-native-causal-lm-train-metadata-v1",
        "backend_kind": "refineforge_native_causal_lm",
        "experiment_id": exp.id,
        "pack_sha256": pack.manifest.pack_sha256,
        "pack_root": pack.root.display().to_string(),
        "tokenizer_sha256": pack.manifest.tokenizer.sha256,
        "train_examples": train.len(),
        "dev_examples": dev.len(),
        "train_example_ids": train.iter().map(|example| example.id.as_str()).collect::<Vec<_>>(),
        "dev_example_ids": dev.iter().map(|example| example.id.as_str()).collect::<Vec<_>>(),
        "steps": cfg.steps,
        "hidden_size": cfg.hidden_size,
        "created_at": Utc::now()
    });
    std::fs::write(
        paths.run_dir.join("train-metadata.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    Ok(())
}

fn write_checkpoint(
    paths: &RunPaths,
    model: &MiniCausalModel,
    cfg: &CausalConfig,
    train: &[&TrainingExample],
    dev: &[&TrainingExample],
    step: usize,
    metrics: (EvalMetrics, EvalMetrics),
) -> Result<()> {
    let checkpoint_dir = paths.checkpoint_dir.join(format!("step-{step}"));
    std::fs::create_dir_all(&checkpoint_dir)?;
    let weights_sha256 = weights_sha256(&model.output_weights, &model.output_bias)?;
    let (train_metrics, dev_metrics) = metrics;
    let checkpoint = CausalCheckpoint {
        schema_version: "refineforge-native-causal-lm-checkpoint-v1",
        backend_kind: "refineforge_native_causal_lm",
        step,
        vocab_size: model.vocab_size,
        train_examples: train.len(),
        dev_examples: dev.len(),
        train_loss: train_metrics.loss,
        dev_loss: dev_metrics.loss,
        target_token_accuracy: dev_metrics.accuracy,
        train_target_tokens: train_metrics.samples,
        dev_target_tokens: dev_metrics.samples,
        weights_sha256,
        architecture: serde_json::json!({
            "token_embedding": {
                "kind": "deterministic_hash_embedding",
                "hidden_size": cfg.hidden_size
            },
            "position_embedding": {
                "kind": "deterministic_hash_embedding",
                "max_position": model.max_position
            },
            "causal_attention": {
                "kind": "prefix_mean_causal_attention",
                "mask": "strictly_prefix_only"
            },
            "mlp_block": {
                "kind": "deterministic_tanh_relu_projection",
                "trainable": false
            },
            "output_head": {
                "kind": "trainable_softmax_head",
                "trainable": true
            }
        }),
        output_weights: &model.output_weights,
        output_bias: &model.output_bias,
    };
    std::fs::write(
        checkpoint_dir.join("causal-lm-checkpoint.json"),
        serde_json::to_vec_pretty(&checkpoint)?,
    )?;
    Ok(())
}

fn write_generation_smoke(
    paths: &RunPaths,
    model: &MiniCausalModel,
    examples: &[TrainingExample],
) -> Result<()> {
    let seed = examples
        .first()
        .map(|example| {
            let keep = example.tokens.len().min(4);
            example.tokens[..keep].to_vec()
        })
        .unwrap_or_else(|| vec![0]);
    let generated = model.generate(&seed, 8);
    let smoke = serde_json::json!({
        "schema_version": "refineforge-generation-smoke-v1",
        "backend_kind": "refineforge_native_causal_lm",
        "prompt_token_count": seed.len(),
        "generated_token_count": generated.len().saturating_sub(seed.len()),
        "prompt_sha256": pack::hex_sha256(&tokens_as_bytes(&seed)),
        "output_sha256": pack::hex_sha256(&tokens_as_bytes(&generated)),
        "tokens": generated
    });
    std::fs::write(
        paths.run_dir.join("generation-smoke.json"),
        serde_json::to_vec_pretty(&smoke)?,
    )?;
    Ok(())
}

fn tokens_as_bytes(tokens: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(tokens.len() * 4);
    for token in tokens {
        bytes.extend(token.to_le_bytes());
    }
    bytes
}

fn weights_sha256(weights: &[Vec<f64>], bias: &[f64]) -> Result<String> {
    let bytes = serde_json::to_vec(&(weights, bias))?;
    Ok(pack::hex_sha256(&bytes))
}
