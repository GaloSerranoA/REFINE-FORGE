//! Native from-scratch GPT transformer backend.
//!
//! A real (small, CPU, f64, deterministic) GPT — trainable token/position
//! embeddings, pre-norm blocks of multi-head causal self-attention + MLP with
//! residuals, final LayerNorm, LM head, cross-entropy, AdamW — realizing the
//! `train-llm-from-scratch` architecture in Rust with no Python/PyTorch. It is
//! a drop-in for the existing evidence/trust pipeline: it writes the same run
//! artifacts as `native_causal`, so `report.rs` and `evidence.rs` consume it
//! unchanged. This is still smoke-grade on tiny data; it does not claim LLM
//! production quality.

pub mod nn;

/// GPU-accelerated variant (`refineforge_native_gpt_cuda`), only with `--features
/// cuda`. Reuses this module's pack loading / config / split + the evidence
/// emission contract, computing on the device via `refineforge-gpu`.
#[cfg(feature = "cuda")]
pub mod cuda;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;

use crate::experiment::Experiment;
use crate::pack::{self, LoadedPack, PackedRecord};
use crate::progress::ProgressRecord;
use crate::runner::RunPaths;
use nn::{AdamW, Block, LayerNorm, Linear, Mat, SplitMix64};

#[derive(Debug, Clone)]
pub struct NativeGptOutcome {
    pub progress_records: usize,
}

#[derive(Debug, Clone)]
struct GptConfig {
    steps: usize,
    learning_rate: f64,
    weight_decay: f64,
    n_embed: usize,
    n_head: usize,
    n_layers: usize,
    context_length: usize,
    seed: u64,
    eval_split: String,
}

impl GptConfig {
    fn from_experiment(exp: &Experiment) -> Result<Self> {
        let steps = hyper_usize(exp, "steps", 8)?;
        let learning_rate = hyper_f64(exp, "learning_rate", 0.01)?;
        let weight_decay = hyper_f64(exp, "weight_decay", 0.01)?;
        let n_embed = hyper_usize(exp, "n_embed", 32)?;
        let n_head = hyper_usize(exp, "n_head", 4)?;
        let n_layers = hyper_usize(exp, "n_layers", 2)?;
        let context_length = hyper_usize(exp, "context_length", 64)?;
        let seed = exp
            .hyperparameters
            .get("seed")
            .and_then(|value| value.as_u64())
            .unwrap_or(1);
        let eval_split = exp
            .hyperparameters
            .get("eval_split")
            .and_then(|value| value.as_str())
            .unwrap_or("heldout")
            .to_string();
        if steps == 0 {
            anyhow::bail!("refineforge_native_gpt hyperparameters.steps must be > 0");
        }
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            anyhow::bail!(
                "refineforge_native_gpt hyperparameters.learning_rate must be finite > 0"
            );
        }
        if !weight_decay.is_finite() || weight_decay < 0.0 {
            anyhow::bail!(
                "refineforge_native_gpt hyperparameters.weight_decay must be finite >= 0"
            );
        }
        if n_embed < 2 || n_head == 0 || n_embed % n_head != 0 {
            anyhow::bail!(
                "refineforge_native_gpt requires n_embed>=2 and n_head dividing n_embed (got n_embed={n_embed}, n_head={n_head})"
            );
        }
        if n_layers == 0 {
            anyhow::bail!("refineforge_native_gpt hyperparameters.n_layers must be > 0");
        }
        if context_length < 2 {
            anyhow::bail!("refineforge_native_gpt hyperparameters.context_length must be >= 2");
        }
        Ok(Self {
            steps,
            learning_rate,
            weight_decay,
            n_embed,
            n_head,
            n_layers,
            context_length,
            seed,
            eval_split,
        })
    }

    fn hidden(&self) -> usize {
        self.n_embed * 4
    }
}

#[derive(Debug, Clone)]
struct TrainingExample {
    id: String,
    split: String,
    tokens: Vec<u32>,
    loss_mask: Vec<u8>,
}

#[derive(Debug, Default, Clone, Copy)]
struct EvalMetrics {
    loss: f64,
    accuracy: f64,
    samples: usize,
}

/// The GPT model: trainable embeddings + N pre-norm blocks + final norm + head.
struct GptModel {
    vocab_size: usize,
    n_embed: usize,
    context_length: usize,
    tok_emb: Mat,
    pos_emb: Mat,
    d_tok_emb: Mat,
    d_pos_emb: Mat,
    blocks: Vec<Block>,
    ln_f: LayerNorm,
    lm_head: Linear,
}

struct GptCache {
    block_caches: Vec<nn::BlockCache>,
    lnf_cache: nn::LnCache,
    lnf_y: Mat,
}

impl GptModel {
    fn new(cfg: &GptConfig, vocab_size: usize) -> Self {
        let mut rng = SplitMix64::new(cfg.seed);
        let std = 0.02;
        let e = cfg.n_embed;
        let tok_emb = Mat::from_fn(vocab_size, e, || rng.next_gaussian() * std);
        let pos_emb = Mat::from_fn(cfg.context_length, e, || rng.next_gaussian() * std);
        let blocks = (0..cfg.n_layers)
            .map(|_| Block::new(&mut rng, e, cfg.n_head, cfg.hidden(), std))
            .collect();
        let ln_f = LayerNorm::new(e);
        let lm_head = Linear::new(&mut rng, e, vocab_size, std);
        Self {
            vocab_size,
            n_embed: e,
            context_length: cfg.context_length,
            d_tok_emb: Mat::zeros(vocab_size, e),
            d_pos_emb: Mat::zeros(cfg.context_length, e),
            tok_emb,
            pos_emb,
            blocks,
            ln_f,
            lm_head,
        }
    }

    fn embed(&self, tokens: &[u32]) -> Mat {
        let t = tokens.len();
        let e = self.n_embed;
        let mut x = Mat::zeros(t, e);
        for (ti, &tok) in tokens.iter().enumerate() {
            let tok = tok as usize;
            for d in 0..e {
                x.set(ti, d, self.tok_emb.get(tok, d) + self.pos_emb.get(ti, d));
            }
        }
        x
    }

    fn forward(&self, tokens: &[u32]) -> (Mat, GptCache) {
        let mut x = self.embed(tokens);
        let mut block_caches = Vec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            let (y, cache) = block.forward(&x);
            x = y;
            block_caches.push(cache);
        }
        let (lnf_y, lnf_cache) = self.ln_f.forward(&x);
        let logits = self.lm_head.forward(&lnf_y);
        (
            logits,
            GptCache {
                block_caches,
                lnf_cache,
                lnf_y,
            },
        )
    }

    /// Accumulates raw (summed) param grads from one sequence.
    fn backward(&mut self, tokens: &[u32], cache: &GptCache, d_logits: &Mat) {
        let d_lnf_y = self.lm_head.backward(&cache.lnf_y, d_logits);
        let mut d_x = self.ln_f.backward(&cache.lnf_cache, &d_lnf_y);
        for (block, bc) in self.blocks.iter_mut().zip(cache.block_caches.iter()).rev() {
            d_x = block.backward(bc, &d_x);
        }
        let e = self.n_embed;
        for (ti, &tok) in tokens.iter().enumerate() {
            let tok = tok as usize;
            for d in 0..e {
                let g = d_x.get(ti, d);
                self.d_tok_emb.data[tok * e + d] += g;
                self.d_pos_emb.data[ti * e + d] += g;
            }
        }
    }

    fn zero_grad(&mut self) {
        self.d_tok_emb.fill_zero();
        self.d_pos_emb.fill_zero();
        for block in &mut self.blocks {
            block.zero_grad();
        }
        self.ln_f.zero_grad();
        self.lm_head.zero_grad();
    }

    fn scale_grads(&mut self, s: f64) {
        let scale = |v: &mut [f64]| v.iter_mut().for_each(|x| *x *= s);
        scale(&mut self.d_tok_emb.data);
        scale(&mut self.d_pos_emb.data);
        for b in &mut self.blocks {
            scale(&mut b.ln1.dgamma);
            scale(&mut b.ln1.dbeta);
            scale(&mut b.attn.wq.dw.data);
            scale(&mut b.attn.wq.db);
            scale(&mut b.attn.wk.dw.data);
            scale(&mut b.attn.wk.db);
            scale(&mut b.attn.wv.dw.data);
            scale(&mut b.attn.wv.db);
            scale(&mut b.attn.wo.dw.data);
            scale(&mut b.attn.wo.db);
            scale(&mut b.ln2.dgamma);
            scale(&mut b.ln2.dbeta);
            scale(&mut b.mlp.fc1.dw.data);
            scale(&mut b.mlp.fc1.db);
            scale(&mut b.mlp.fc2.dw.data);
            scale(&mut b.mlp.fc2.db);
        }
        scale(&mut self.ln_f.dgamma);
        scale(&mut self.ln_f.dbeta);
        scale(&mut self.lm_head.dw.data);
        scale(&mut self.lm_head.db);
    }

    /// One AdamW update over every parameter block in a fixed order. Weight
    /// decay applies to 2D weight matrices only (not biases / LayerNorm /
    /// embeddings), matching standard GPT practice.
    fn adam_step(&mut self, opt: &mut AdamW, lr: f64) {
        opt.begin_step();
        opt.step_block(&mut self.tok_emb.data, &self.d_tok_emb.data, lr, false);
        opt.step_block(&mut self.pos_emb.data, &self.d_pos_emb.data, lr, false);
        for b in &mut self.blocks {
            opt.step_block(&mut b.ln1.gamma, &b.ln1.dgamma, lr, false);
            opt.step_block(&mut b.ln1.beta, &b.ln1.dbeta, lr, false);
            opt.step_block(&mut b.attn.wq.w.data, &b.attn.wq.dw.data, lr, true);
            opt.step_block(&mut b.attn.wq.b, &b.attn.wq.db, lr, false);
            opt.step_block(&mut b.attn.wk.w.data, &b.attn.wk.dw.data, lr, true);
            opt.step_block(&mut b.attn.wk.b, &b.attn.wk.db, lr, false);
            opt.step_block(&mut b.attn.wv.w.data, &b.attn.wv.dw.data, lr, true);
            opt.step_block(&mut b.attn.wv.b, &b.attn.wv.db, lr, false);
            opt.step_block(&mut b.attn.wo.w.data, &b.attn.wo.dw.data, lr, true);
            opt.step_block(&mut b.attn.wo.b, &b.attn.wo.db, lr, false);
            opt.step_block(&mut b.ln2.gamma, &b.ln2.dgamma, lr, false);
            opt.step_block(&mut b.ln2.beta, &b.ln2.dbeta, lr, false);
            opt.step_block(&mut b.mlp.fc1.w.data, &b.mlp.fc1.dw.data, lr, true);
            opt.step_block(&mut b.mlp.fc1.b, &b.mlp.fc1.db, lr, false);
            opt.step_block(&mut b.mlp.fc2.w.data, &b.mlp.fc2.dw.data, lr, true);
            opt.step_block(&mut b.mlp.fc2.b, &b.mlp.fc2.db, lr, false);
        }
        opt.step_block(&mut self.ln_f.gamma, &self.ln_f.dgamma, lr, false);
        opt.step_block(&mut self.ln_f.beta, &self.ln_f.dbeta, lr, false);
        opt.step_block(&mut self.lm_head.w.data, &self.lm_head.dw.data, lr, true);
        opt.step_block(&mut self.lm_head.b, &self.lm_head.db, lr, false);
    }

    /// Train one full pass over `examples`, accumulating grads then stepping.
    fn train_step(
        &mut self,
        examples: &[&TrainingExample],
        opt: &mut AdamW,
        lr: f64,
    ) -> EvalMetrics {
        self.zero_grad();
        let mut loss_sum = 0.0;
        let mut correct = 0usize;
        let mut samples = 0usize;
        for example in examples {
            let (logits, cache) = self.forward(&example.tokens);
            let (l, c, corr, d_logits) = self.loss_and_grad(&logits, example);
            loss_sum += l;
            samples += c;
            correct += corr;
            if c > 0 {
                self.backward(&example.tokens, &cache, &d_logits);
            }
        }
        if samples > 0 {
            self.scale_grads(1.0 / samples as f64);
            self.adam_step(opt, lr);
        }
        finalize(loss_sum, correct, samples)
    }

    fn evaluate(&self, examples: &[&TrainingExample]) -> EvalMetrics {
        let mut loss_sum = 0.0;
        let mut correct = 0usize;
        let mut samples = 0usize;
        for example in examples {
            let (logits, _cache) = self.forward(&example.tokens);
            let (l, c, corr, _d) = self.loss_and_grad(&logits, example);
            loss_sum += l;
            samples += c;
            correct += corr;
        }
        finalize(loss_sum, correct, samples)
    }

    /// Cross-entropy over masked next-token targets. Returns
    /// (loss_sum, count, correct, d_logits) where d_logits is the raw
    /// (softmax - onehot) gradient summed over predicted positions.
    fn loss_and_grad(&self, logits: &Mat, example: &TrainingExample) -> (f64, usize, usize, Mat) {
        let t = example.tokens.len();
        let v = self.vocab_size;
        let mut d_logits = Mat::zeros(t, v);
        let mut loss_sum = 0.0;
        let mut count = 0usize;
        let mut correct = 0usize;
        for pos in 0..t.saturating_sub(1) {
            let target = example.tokens[pos + 1] as usize;
            if example.loss_mask.get(pos + 1).copied().unwrap_or(0) == 0 || target >= v {
                continue;
            }
            let probs = softmax(logits.row(pos));
            if argmax(&probs) == target {
                correct += 1;
            }
            loss_sum += -probs[target].max(1.0e-12).ln();
            for (vi, p) in probs.iter().enumerate() {
                d_logits.set(pos, vi, p - if vi == target { 1.0 } else { 0.0 });
            }
            count += 1;
        }
        (loss_sum, count, correct, d_logits)
    }

    fn generate(&self, seed: &[u32], max_new_tokens: usize) -> Vec<u32> {
        let mut out = seed.to_vec();
        for _ in 0..max_new_tokens {
            let window = if out.len() > self.context_length {
                &out[out.len() - self.context_length..]
            } else {
                &out[..]
            };
            let (logits, _cache) = self.forward(window);
            let last = window.len() - 1;
            let next = argmax(&softmax(logits.row(last))) as u32;
            out.push(next);
        }
        out
    }

    /// Deterministic hash over all parameters in fixed order.
    fn weights_sha256(&self) -> String {
        let mut flat: Vec<f64> = Vec::new();
        flat.extend_from_slice(&self.tok_emb.data);
        flat.extend_from_slice(&self.pos_emb.data);
        for b in &self.blocks {
            flat.extend_from_slice(&b.ln1.gamma);
            flat.extend_from_slice(&b.ln1.beta);
            for lin in [&b.attn.wq, &b.attn.wk, &b.attn.wv, &b.attn.wo] {
                flat.extend_from_slice(&lin.w.data);
                flat.extend_from_slice(&lin.b);
            }
            flat.extend_from_slice(&b.ln2.gamma);
            flat.extend_from_slice(&b.ln2.beta);
            for lin in [&b.mlp.fc1, &b.mlp.fc2] {
                flat.extend_from_slice(&lin.w.data);
                flat.extend_from_slice(&lin.b);
            }
        }
        flat.extend_from_slice(&self.ln_f.gamma);
        flat.extend_from_slice(&self.ln_f.beta);
        flat.extend_from_slice(&self.lm_head.w.data);
        flat.extend_from_slice(&self.lm_head.b);
        let mut bytes = Vec::with_capacity(flat.len() * 8);
        for value in &flat {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        pack::hex_sha256(&bytes)
    }

    fn parameter_count(&self) -> usize {
        let mut n = self.tok_emb.data.len() + self.pos_emb.data.len();
        for b in &self.blocks {
            n += b.ln1.gamma.len() * 2;
            for lin in [&b.attn.wq, &b.attn.wk, &b.attn.wv, &b.attn.wo] {
                n += lin.w.data.len() + lin.b.len();
            }
            n += b.ln2.gamma.len() * 2;
            for lin in [&b.mlp.fc1, &b.mlp.fc2] {
                n += lin.w.data.len() + lin.b.len();
            }
        }
        n += self.ln_f.gamma.len() * 2 + self.lm_head.w.data.len() + self.lm_head.b.len();
        n
    }
}

fn finalize(loss_sum: f64, correct: usize, samples: usize) -> EvalMetrics {
    if samples == 0 {
        return EvalMetrics::default();
    }
    EvalMetrics {
        loss: loss_sum / samples as f64,
        accuracy: correct as f64 / samples as f64,
        samples,
    }
}

fn softmax(logits: &[f64]) -> Vec<f64> {
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut values: Vec<f64> = logits.iter().map(|v| (v - max).exp()).collect();
    let sum = values.iter().sum::<f64>().max(1.0e-12);
    values.iter_mut().for_each(|v| *v /= sum);
    values
}

fn argmax(values: &[f64]) -> usize {
    values
        .iter()
        .copied()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(i, _)| i)
        .unwrap_or(0)
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

fn examples_from_pack(pack: &LoadedPack, context_length: usize) -> Result<Vec<TrainingExample>> {
    pack.records
        .iter()
        .map(|record| example_from_record(record, pack, context_length))
        .collect()
}

fn example_from_record(
    record: &PackedRecord,
    pack: &LoadedPack,
    context_length: usize,
) -> Result<TrainingExample> {
    let start = record.token_start;
    let end = start + record.token_len;
    if end > pack.tokens.len() || end > pack.loss_mask.len() {
        anyhow::bail!("packed record {} is out of token range", record.id);
    }
    let mut tokens = pack.tokens[start..end].to_vec();
    let mut loss_mask = pack.loss_mask[start..end].to_vec();
    // Clamp to the model's context window (keep the most recent tokens).
    if tokens.len() > context_length {
        let cut = tokens.len() - context_length;
        tokens = tokens[cut..].to_vec();
        loss_mask = loss_mask[cut..].to_vec();
    }
    Ok(TrainingExample {
        id: record.id.clone(),
        split: record.split.clone(),
        tokens,
        loss_mask,
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

pub fn run(paths: &RunPaths, exp: &Experiment) -> Result<NativeGptOutcome> {
    let cfg = GptConfig::from_experiment(exp)?;
    let pack = pack::load_pack(&exp.dataset.path)?;
    let examples = examples_from_pack(&pack, cfg.context_length)?;
    if examples.is_empty() {
        anyhow::bail!("refineforge_native_gpt requires at least one packed example");
    }
    let vocab_size = pack.manifest.tokenizer.vocab_size.max(
        pack.tokens
            .iter()
            .copied()
            .max()
            .map(|m| m as usize + 1)
            .unwrap_or(2),
    );
    let (train, dev) = split_examples(&examples, &cfg.eval_split);
    let mut model = GptModel::new(&cfg, vocab_size);
    let mut opt = AdamW::new(cfg.weight_decay);

    let mut progress_file = std::fs::File::create(&paths.progress_file)
        .with_context(|| format!("creating {}", paths.progress_file.display()))?;
    let mut log_file = std::fs::File::create(&paths.log_file)
        .with_context(|| format!("creating {}", paths.log_file.display()))?;
    write_train_metadata(paths, exp, &pack, &cfg, &model, &train, &dev)?;

    writeln!(
        log_file,
        "refineforge_native_gpt start examples={} train={} dev={} steps={} n_embed={} n_head={} n_layers={} ctx={} vocab={} params={}",
        examples.len(),
        train.len(),
        dev.len(),
        cfg.steps,
        cfg.n_embed,
        cfg.n_head,
        cfg.n_layers,
        cfg.context_length,
        vocab_size,
        model.parameter_count()
    )?;

    let save_steps = exp.checkpoint.save_steps.unwrap_or(cfg.steps as u64).max(1) as usize;
    let mut records = 0usize;
    for step in 1..=cfg.steps {
        let train_metrics = model.train_step(&train, &mut opt, cfg.learning_rate);
        let dev_metrics = model.evaluate(&dev);
        let raw = format!(
            "step={} train_loss={:.8} dev_loss={:.8} target_token_accuracy={:.8} learning_rate={:.8}",
            step, train_metrics.loss, dev_metrics.loss, dev_metrics.accuracy, cfg.learning_rate
        );
        writeln!(log_file, "{raw}")?;
        let mut metrics = BTreeMap::new();
        metrics.insert("train_loss".to_string(), train_metrics.loss);
        metrics.insert("dev_loss".to_string(), dev_metrics.loss);
        metrics.insert("target_token_accuracy".to_string(), dev_metrics.accuracy);
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
                (train_metrics, dev_metrics),
            )?;
        }
    }
    write_generation_smoke(paths, &model, &examples)?;
    writeln!(log_file, "refineforge_native_gpt completed normally")?;
    Ok(NativeGptOutcome {
        progress_records: records,
    })
}

#[derive(Debug, Serialize)]
struct GptCheckpoint {
    schema_version: &'static str,
    backend_kind: &'static str,
    step: usize,
    vocab_size: usize,
    parameter_count: usize,
    train_examples: usize,
    dev_examples: usize,
    train_loss: f64,
    dev_loss: f64,
    target_token_accuracy: f64,
    train_target_tokens: usize,
    dev_target_tokens: usize,
    weights_sha256: String,
    architecture: serde_json::Value,
}

fn architecture_json(cfg: &GptConfig, vocab_size: usize) -> serde_json::Value {
    serde_json::json!({
        "kind": "decoder_only_transformer",
        "vocab_size": vocab_size,
        "n_embed": cfg.n_embed,
        "n_head": cfg.n_head,
        "n_layers": cfg.n_layers,
        "context_length": cfg.context_length,
        "mlp_hidden": cfg.hidden(),
        "token_embedding": {"kind": "trainable", "trainable": true},
        "position_embedding": {"kind": "trainable", "trainable": true},
        "attention": {"kind": "multi_head_causal_self_attention", "trainable": true, "mask": "causal"},
        "mlp_block": {"kind": "linear_gelu_linear", "trainable": true},
        "norm": {"kind": "layernorm", "placement": "pre_norm", "trainable": true},
        "output_head": {"kind": "linear_lm_head", "trainable": true},
        "optimizer": {"kind": "adamw", "weight_decay": cfg.weight_decay},
        "precision": "f64"
    })
}

fn write_checkpoint(
    paths: &RunPaths,
    model: &GptModel,
    cfg: &GptConfig,
    train: &[&TrainingExample],
    dev: &[&TrainingExample],
    step: usize,
    metrics: (EvalMetrics, EvalMetrics),
) -> Result<()> {
    let checkpoint_dir = paths.checkpoint_dir.join(format!("step-{step}"));
    std::fs::create_dir_all(&checkpoint_dir)?;
    let (train_metrics, dev_metrics) = metrics;
    let checkpoint = GptCheckpoint {
        schema_version: "refineforge-native-gpt-checkpoint-v1",
        backend_kind: "refineforge_native_gpt",
        step,
        vocab_size: model.vocab_size,
        parameter_count: model.parameter_count(),
        train_examples: train.len(),
        dev_examples: dev.len(),
        train_loss: train_metrics.loss,
        dev_loss: dev_metrics.loss,
        target_token_accuracy: dev_metrics.accuracy,
        train_target_tokens: train_metrics.samples,
        dev_target_tokens: dev_metrics.samples,
        weights_sha256: model.weights_sha256(),
        architecture: architecture_json(cfg, model.vocab_size),
    };
    std::fs::write(
        checkpoint_dir.join("gpt-checkpoint.json"),
        serde_json::to_vec_pretty(&checkpoint)?,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_train_metadata(
    paths: &RunPaths,
    exp: &Experiment,
    pack: &LoadedPack,
    cfg: &GptConfig,
    model: &GptModel,
    train: &[&TrainingExample],
    dev: &[&TrainingExample],
) -> Result<()> {
    let metadata = serde_json::json!({
        "schema_version": "refineforge-native-gpt-train-metadata-v1",
        "backend_kind": "refineforge_native_gpt",
        "experiment_id": exp.id,
        "pack_sha256": pack.manifest.pack_sha256,
        "pack_root": pack.root.display().to_string(),
        "tokenizer_sha256": pack.manifest.tokenizer.sha256,
        "vocab_size": model.vocab_size,
        "parameter_count": model.parameter_count(),
        "architecture": architecture_json(cfg, model.vocab_size),
        "seed": cfg.seed,
        "steps": cfg.steps,
        "train_examples": train.len(),
        "dev_examples": dev.len(),
        "train_example_ids": train.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        "dev_example_ids": dev.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        "created_at": Utc::now()
    });
    std::fs::write(
        paths.run_dir.join("train-metadata.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    Ok(())
}

fn write_generation_smoke(
    paths: &RunPaths,
    model: &GptModel,
    examples: &[TrainingExample],
) -> Result<()> {
    let seed = examples
        .first()
        .map(|example| {
            let keep = example.tokens.len().clamp(1, 4);
            example.tokens[..keep].to_vec()
        })
        .unwrap_or_else(|| vec![0]);
    let generated = model.generate(&seed, 8);
    let smoke = serde_json::json!({
        "schema_version": "refineforge-generation-smoke-v1",
        "backend_kind": "refineforge_native_gpt",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(id: &str, split: &str, tokens: Vec<u32>) -> TrainingExample {
        let loss_mask = vec![1u8; tokens.len()];
        TrainingExample {
            id: id.into(),
            split: split.into(),
            tokens,
            loss_mask,
        }
    }

    fn tiny_cfg(seed: u64) -> GptConfig {
        GptConfig {
            steps: 40,
            learning_rate: 0.02,
            weight_decay: 0.0,
            n_embed: 16,
            n_head: 2,
            n_layers: 2,
            context_length: 8,
            seed,
            eval_split: "heldout".into(),
        }
    }

    fn fixed_examples() -> Vec<TrainingExample> {
        // A tiny, learnable pattern: repeated token bigrams.
        vec![
            ex("a", "train", vec![2, 3, 2, 3, 2, 3]),
            ex("b", "train", vec![4, 5, 4, 5, 4, 5]),
            ex("c", "train", vec![2, 3, 2, 3]),
        ]
    }

    #[test]
    fn training_decreases_loss_and_learns() {
        let cfg = tiny_cfg(7);
        let examples = fixed_examples();
        let refs: Vec<&TrainingExample> = examples.iter().collect();
        let mut model = GptModel::new(&cfg, 8);
        let mut opt = AdamW::new(cfg.weight_decay);
        let start = model.evaluate(&refs).loss;
        let mut last = start;
        for _ in 0..cfg.steps {
            last = model.train_step(&refs, &mut opt, cfg.learning_rate).loss;
        }
        assert!(
            last < start * 0.5,
            "loss should drop substantially: start={start} end={last}"
        );
        // The model should learn the deterministic bigram pattern well.
        let acc = model.evaluate(&refs).accuracy;
        assert!(acc > 0.8, "should learn the toy pattern, accuracy={acc}");
    }

    #[test]
    fn training_is_deterministic() {
        let cfg = tiny_cfg(11);
        let examples = fixed_examples();
        let refs: Vec<&TrainingExample> = examples.iter().collect();
        let train = |cfg: &GptConfig| {
            let mut model = GptModel::new(cfg, 8);
            let mut opt = AdamW::new(cfg.weight_decay);
            for _ in 0..cfg.steps {
                model.train_step(&refs, &mut opt, cfg.learning_rate);
            }
            model.weights_sha256()
        };
        assert_eq!(train(&cfg), train(&cfg), "same seed => identical weights");
    }

    #[test]
    fn end_to_end_gradient_check_through_loss() {
        // Perturb a handful of real parameters and confirm the analytic grad
        // (after one backward over a sequence) matches finite differences of
        // the summed cross-entropy loss.
        let cfg = tiny_cfg(3);
        let example = ex("g", "train", vec![2, 3, 4, 5, 2]);
        let mut model = GptModel::new(&cfg, 8);
        model.zero_grad();
        let (logits, cache) = model.forward(&example.tokens);
        let (_l, count, _c, d_logits) = model.loss_and_grad(&logits, &example);
        assert!(count > 0);
        model.backward(&example.tokens, &cache, &d_logits);

        let loss_of = |m: &GptModel| -> f64 {
            let (logits, _c) = m.forward(&example.tokens);
            m.loss_and_grad(&logits, &example).0
        };
        let eps = 1.0e-6;
        // lm_head weight [3][2]
        let probe = |m: &mut GptModel, delta: f64| {
            m.lm_head.w.data[3 * cfg.n_embed + 2] += delta;
        };
        let base = model.lm_head.dw.get(3, 2);
        probe(&mut model, eps);
        let lp = loss_of(&model);
        probe(&mut model, -2.0 * eps);
        let lm = loss_of(&model);
        probe(&mut model, eps);
        let num = (lp - lm) / (2.0 * eps);
        assert!(
            (num - base).abs() < 1.0e-3,
            "lm_head grad num={num} analytic={base}"
        );
    }
}
