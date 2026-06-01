//! GPU GPT backend (`refineforge_native_gpt_cuda`), only with `--features cuda`.
//!
//! Trains the device-resident `refineforge-gpu` `GptModel` through the live
//! training pipeline and emits the **same evidence contract** as the CPU
//! `native_gpt` backend (`progress.jsonl`, `checkpoints/step-N/gpt-checkpoint.json`
//! with `weights_sha256`, `train-metadata.json`, `generation-smoke.json`), so
//! `report.rs` / `evidence.rs` consume it unchanged and a successful run produces
//! a `final_outcome: "success"` report — closing the trust-ladder approval gate.
//!
//! Pack loading, config parsing, and the train/dev split are reused from the
//! parent module. The GPU path is `f32` / statistical-not-bit-exact, so
//! `weights_sha256` is a per-run integrity hash (the CPU `f64` path remains the
//! deterministic reference).

use anyhow::{Context, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;

use super::{examples_from_pack, split_examples, GptConfig, NativeGptOutcome, TrainingExample};
use crate::experiment::Experiment;
use crate::pack::{self, LoadedPack};
use crate::progress::ProgressRecord;
use crate::runner::RunPaths;

use refineforge_gpu::device::GptModel;
use refineforge_gpu::gpu::GpuKernels;

pub fn run(paths: &RunPaths, exp: &Experiment) -> Result<NativeGptOutcome> {
    let cfg = GptConfig::from_experiment(exp)?;
    let pack = pack::load_pack(&exp.dataset.path)?;
    let examples = examples_from_pack(&pack, cfg.context_length)?;
    anyhow::ensure!(
        !examples.is_empty(),
        "refineforge_native_gpt_cuda requires at least one packed example"
    );
    let vocab_size = pack.manifest.tokenizer.vocab_size.max(
        pack.tokens
            .iter()
            .copied()
            .max()
            .map(|m| m as usize + 1)
            .unwrap_or(2),
    );
    let (train, dev) = split_examples(&examples, &cfg.eval_split);

    let k = GpuKernels::new_auto()
        .context("opening a CUDA device for refineforge_native_gpt_cuda (set REFINEFORGE_CUDA_DEVICE to pick one)")?;
    let mut model = GptModel::new(
        &k,
        vocab_size,
        cfg.n_embed,
        cfg.n_head,
        cfg.n_layers,
        cfg.hidden(),
        cfg.context_length,
        cfg.seed,
    )?;
    model.set_weight_decay(cfg.weight_decay as f32);
    let parameter_count = model.parameter_count();
    let architecture = architecture_json(&cfg, vocab_size, &k.device_name());

    let mut progress_file = std::fs::File::create(&paths.progress_file)
        .with_context(|| format!("creating {}", paths.progress_file.display()))?;
    let mut log_file = std::fs::File::create(&paths.log_file)
        .with_context(|| format!("creating {}", paths.log_file.display()))?;
    writeln!(
        log_file,
        "refineforge_native_gpt_cuda start device=[{}] examples={} train={} dev={} steps={} n_embed={} n_head={} n_layers={} ctx={} vocab={} params={}",
        k.device_summary(), examples.len(), train.len(), dev.len(), cfg.steps,
        cfg.n_embed, cfg.n_head, cfg.n_layers, cfg.context_length, vocab_size, parameter_count
    )?;
    write_train_metadata(
        paths,
        exp,
        &pack,
        vocab_size,
        parameter_count,
        &architecture,
        &cfg,
        &train,
        &dev,
    )?;

    let save_steps = exp.checkpoint.save_steps.unwrap_or(cfg.steps as u64).max(1) as usize;
    let lr = cfg.learning_rate as f32;
    // Optional KL self-distillation (anti-forgetting, per "Why Fine-Tuning
    // Encourages Hallucinations"): after a warm-up the student is snapshotted into a
    // frozen teacher, and the remaining steps add λ·KL(teacher ‖ student) to the
    // loss. Disabled when self_distill_lambda == 0 (the default).
    let self_distill_lambda = super::hyper_f64(exp, "self_distill_lambda", 0.0)? as f32;
    let distill_tau = super::hyper_f64(exp, "distill_temperature", 1.0)? as f32;
    let warmup_steps =
        super::hyper_usize(exp, "self_distill_warmup_steps", (cfg.steps / 3).max(1))?;
    let mut teacher: Option<GptModel> = None;
    let mut global = 0u32;
    let mut records = 0usize;
    // One "step" is a pass over the training set (SGD, one sequence per train_step).
    for step in 1..=cfg.steps {
        // snapshot the frozen teacher once, after warm-up, when distillation is on
        if self_distill_lambda > 0.0 && teacher.is_none() && step > warmup_steps {
            let mut t = GptModel::new(
                &k,
                vocab_size,
                cfg.n_embed,
                cfg.n_head,
                cfg.n_layers,
                cfg.hidden(),
                cfg.context_length,
                cfg.seed,
            )?;
            t.load_weights(&k, &model.weight_values(&k)?)?;
            teacher = Some(t);
            writeln!(
                log_file,
                "self-distillation: snapshotted frozen teacher at step {step} (lambda={self_distill_lambda}, tau={distill_tau})"
            )?;
        }
        let (mut train_loss, mut n) = (0.0f64, 0u32);
        for ex in &train {
            global += 1;
            let toks: Vec<i32> = ex.tokens.iter().map(|&t| t as i32).collect();
            let mask: Vec<i32> = ex.loss_mask.iter().map(|&m| i32::from(m)).collect();
            let loss = if let Some(teach) = &teacher {
                let (teacher_logits, _c) = teach.forward(&k, &toks)?;
                let (ce, _kl, _acc) = model.train_step_distill(
                    &k,
                    &toks,
                    &mask,
                    &teacher_logits,
                    distill_tau,
                    self_distill_lambda,
                    global,
                    lr,
                )?;
                ce
            } else {
                model.train_step(&k, &toks, &mask, global, lr)?.0
            };
            train_loss += f64::from(loss);
            n += 1;
        }
        train_loss /= f64::from(n.max(1));
        let (dev_loss, dev_acc) = evaluate(&k, &model, &dev)?;
        let raw = format!(
            "step={step} train_loss={train_loss:.8} dev_loss={dev_loss:.8} target_token_accuracy={dev_acc:.8} learning_rate={:.8}",
            cfg.learning_rate
        );
        writeln!(log_file, "{raw}")?;
        let mut metrics = BTreeMap::new();
        metrics.insert("train_loss".to_string(), train_loss);
        metrics.insert("dev_loss".to_string(), dev_loss);
        metrics.insert("target_token_accuracy".to_string(), dev_acc);
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
                vocab_size,
                parameter_count,
                &architecture,
                &train,
                &dev,
                step,
                (train_loss, dev_loss, dev_acc),
                &k,
                &model,
            )?;
        }
    }
    write_generation_smoke(paths, &k, &model, &examples)?;
    writeln!(log_file, "refineforge_native_gpt_cuda completed normally")?;
    Ok(NativeGptOutcome {
        progress_records: records,
    })
}

/// Mean dev loss + target-token accuracy (forward-only, dropout-off).
fn evaluate(k: &GpuKernels, model: &GptModel, dev: &[&TrainingExample]) -> Result<(f64, f64)> {
    let (mut l, mut a, mut n) = (0.0f64, 0.0f64, 0u32);
    for ex in dev {
        let toks: Vec<i32> = ex.tokens.iter().map(|&t| t as i32).collect();
        let mask: Vec<i32> = ex.loss_mask.iter().map(|&m| i32::from(m)).collect();
        let (loss, acc) = model.evaluate(k, &toks, &mask)?;
        l += f64::from(loss);
        a += f64::from(acc);
        n += 1;
    }
    Ok((l / f64::from(n.max(1)), a / f64::from(n.max(1))))
}

fn weights_sha256(k: &GpuKernels, model: &GptModel) -> Result<String> {
    let mut hasher = Sha256::new();
    for v in model.weight_values(k)? {
        hasher.update(v.to_le_bytes());
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn architecture_json(cfg: &GptConfig, vocab_size: usize, device: &str) -> serde_json::Value {
    serde_json::json!({
        "n_embed": cfg.n_embed,
        "n_head": cfg.n_head,
        "n_layers": cfg.n_layers,
        "context_length": cfg.context_length,
        "vocab_size": vocab_size,
        "optimizer": {"kind": "adamw", "weight_decay": cfg.weight_decay},
        "precision": "f32",
        "compute": "refineforge-gpu (hand-written CUDA)",
        "device": device,
    })
}

/// Supervised next-token targets in a set (mask set on the predicted token).
fn target_tokens(set: &[&TrainingExample]) -> usize {
    set.iter()
        .map(|ex| ex.loss_mask.iter().skip(1).filter(|&&m| m != 0).count())
        .sum()
}

#[allow(clippy::too_many_arguments)]
fn write_checkpoint(
    paths: &RunPaths,
    vocab_size: usize,
    parameter_count: usize,
    architecture: &serde_json::Value,
    train: &[&TrainingExample],
    dev: &[&TrainingExample],
    step: usize,
    metrics: (f64, f64, f64),
    k: &GpuKernels,
    model: &GptModel,
) -> Result<()> {
    let checkpoint_dir = paths.checkpoint_dir.join(format!("step-{step}"));
    std::fs::create_dir_all(&checkpoint_dir)?;
    let (train_loss, dev_loss, dev_acc) = metrics;
    let checkpoint = serde_json::json!({
        "schema_version": "refineforge-native-gpt-checkpoint-v1",
        "backend_kind": "refineforge_native_gpt_cuda",
        "step": step,
        "vocab_size": vocab_size,
        "parameter_count": parameter_count,
        "train_examples": train.len(),
        "dev_examples": dev.len(),
        "train_loss": train_loss,
        "dev_loss": dev_loss,
        "target_token_accuracy": dev_acc,
        "train_target_tokens": target_tokens(train),
        "dev_target_tokens": target_tokens(dev),
        "weights_sha256": weights_sha256(k, model)?,
        "architecture": architecture,
    });
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
    vocab_size: usize,
    parameter_count: usize,
    architecture: &serde_json::Value,
    cfg: &GptConfig,
    train: &[&TrainingExample],
    dev: &[&TrainingExample],
) -> Result<()> {
    let metadata = serde_json::json!({
        "schema_version": "refineforge-native-gpt-train-metadata-v1",
        "backend_kind": "refineforge_native_gpt_cuda",
        "experiment_id": exp.id,
        "pack_sha256": pack.manifest.pack_sha256,
        "tokenizer_sha256": pack.manifest.tokenizer.sha256,
        "vocab_size": vocab_size,
        "parameter_count": parameter_count,
        "architecture": architecture,
        "seed": cfg.seed,
        "steps": cfg.steps,
        "train_examples": train.len(),
        "dev_examples": dev.len(),
        "train_example_ids": train.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        "dev_example_ids": dev.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        "created_at": Utc::now(),
        "reproducibility": "f32 / statistical-not-bit-exact; weights_sha256 is a per-run integrity hash (the CPU f64 native_gpt path is the deterministic reference)",
    });
    std::fs::write(
        paths.run_dir.join("train-metadata.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    Ok(())
}

fn write_generation_smoke(
    paths: &RunPaths,
    k: &GpuKernels,
    model: &GptModel,
    examples: &[TrainingExample],
) -> Result<()> {
    let seed: Vec<i32> = examples
        .first()
        .map(|ex| {
            let keep = ex.tokens.len().clamp(1, 4);
            ex.tokens[..keep].iter().map(|&t| t as i32).collect()
        })
        .unwrap_or_else(|| vec![0]);
    let generated = model.generate(k, &seed, 8)?;
    let bytes =
        |ts: &[i32]| -> Vec<u8> { ts.iter().flat_map(|&t| (t as u32).to_le_bytes()).collect() };
    let psha: String = Sha256::digest(bytes(&seed))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let osha: String = Sha256::digest(bytes(&generated))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let smoke = serde_json::json!({
        "schema_version": "refineforge-generation-smoke-v1",
        "backend_kind": "refineforge_native_gpt_cuda",
        "prompt_token_count": seed.len(),
        "generated_token_count": generated.len().saturating_sub(seed.len()),
        "prompt_sha256": psha,
        "output_sha256": osha,
        "tokens": generated,
    });
    std::fs::write(
        paths.run_dir.join("generation-smoke.json"),
        serde_json::to_vec_pretty(&smoke)?,
    )?;
    Ok(())
}
