//! Emit production-proof **trust-ladder evidence** for a GPU-trained native GPT
//! (Milestone 13, "GPU emits evidence" direction). The GPU model trains on an SFT
//! pack and writes a complete run directory — `config.yaml`, `progress.jsonl`,
//! `checkpoints/step-N/gpt-checkpoint.json` (with a `weights_sha256`),
//! `train-metadata.json`, and `generation-smoke.json` — matching the schemas the
//! trainer's `report.rs` / `evidence.rs` consume. No trainer-crate dependency: the
//! GPU crate produces the evidence directory, and `refineforge-trainer report
//! <run-dir>` then builds `report.json` from it (the verification step).
//!
//! Usage (requires an NVIDIA GPU + CUDA driver):
//!   cargo run -p refineforge-gpu --features cuda --release --example gpu_evidence -- <pack-dir> <out-run-dir> [epochs]
//!
//! The `backend.kind` stays the valid `refineforge_native_gpt` (the architecture
//! is the native GPT); the GPU `f32` compute is recorded honestly in the
//! `architecture` block (precision / device / compute) and the metadata's
//! reproducibility note. The GPU path is statistical-not-bit-exact, so
//! `weights_sha256` is a per-run integrity hash, not a cross-run-reproducible one.

#[cfg(not(feature = "cuda"))]
fn main() {
    eprintln!("gpu_evidence requires `--features cuda` (and an NVIDIA GPU + CUDA driver).");
    std::process::exit(2);
}

#[cfg(feature = "cuda")]
fn main() -> anyhow::Result<()> {
    use refineforge_gpu::device::GptModel;
    use refineforge_gpu::gpu::GpuKernels;
    use sha2::{Digest, Sha256};
    use std::io::Write as _;

    let mut args = std::env::args().skip(1);
    let usage = || anyhow::anyhow!("usage: gpu_evidence <pack-dir> <out-run-dir> [epochs]");
    let pack_dir = args.next().ok_or_else(usage)?;
    let out_dir = args.next().ok_or_else(usage)?;
    let epochs: u32 = match args.next() {
        Some(s) => s.parse()?,
        None => 6,
    };
    let pack = std::path::Path::new(&pack_dir);
    let run_dir = std::path::Path::new(&out_dir);
    std::fs::create_dir_all(run_dir.join("checkpoints"))?;

    // ─── load the pack ───
    let tokens: Vec<i32> = std::fs::read(pack.join("tokens.bin"))?
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i32)
        .collect();
    let loss_mask: Vec<i32> = std::fs::read(pack.join("loss-mask.bin"))?
        .into_iter()
        .map(i32::from)
        .collect();
    let records: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pack.join("records.json"))?)?;
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pack.join("pack-manifest.json"))?)?;
    let records = records
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("records.json"))?;
    // (start, len, is_train, id)
    let seqs: Vec<(usize, usize, bool, String)> = records
        .iter()
        .map(|r| {
            (
                r["token_start"].as_u64().unwrap_or(0) as usize,
                r["token_len"].as_u64().unwrap_or(0) as usize,
                r["split"].as_str().unwrap_or("train") == "train",
                r["id"].as_str().unwrap_or("").to_string(),
            )
        })
        .filter(|s| s.1 >= 2)
        .collect();
    let vocab = (manifest["tokenizer"]["vocab_size"].as_u64().unwrap_or(0) as usize)
        .max(tokens.iter().copied().max().unwrap_or(0) as usize + 1);
    let context = seqs.iter().map(|s| s.1).max().unwrap_or(0);
    let train: Vec<&(usize, usize, bool, String)> = seqs.iter().filter(|s| s.2).collect();
    let dev: Vec<&(usize, usize, bool, String)> = seqs.iter().filter(|s| !s.2).collect();
    anyhow::ensure!(!train.is_empty(), "no train sequences");

    let target_tokens = |set: &[&(usize, usize, bool, String)]| -> usize {
        set.iter()
            .map(|s| {
                loss_mask[s.0 + 1..s.0 + s.1]
                    .iter()
                    .filter(|&&m| m != 0)
                    .count()
            })
            .sum()
    };

    // ─── small model for a quick evidence run ───
    let (embed, n_head, n_layers, hidden, seed, lr, wd) =
        (128usize, 4usize, 2usize, 512usize, 7u64, 6.0e-4f32, 0.1f32);
    let k = GpuKernels::new(0)?;
    let mut model = GptModel::new(&k, vocab, embed, n_head, n_layers, hidden, context, seed)?;
    model.set_weight_decay(wd);
    model.set_label_smoothing(0.1);

    let eval =
        |model: &GptModel, set: &[&(usize, usize, bool, String)]| -> anyhow::Result<(f32, f32)> {
            let (mut l, mut a, mut n) = (0.0f32, 0.0f32, 0u32);
            for s in set {
                let (loss, acc) =
                    model.evaluate(&k, &tokens[s.0..s.0 + s.1], &loss_mask[s.0..s.0 + s.1])?;
                l += loss;
                a += acc;
                n += 1;
            }
            Ok((l / n.max(1) as f32, a / n.max(1) as f32))
        };

    // ─── train, logging progress.jsonl (one ProgressRecord per epoch) ───
    let mut progress = std::fs::File::create(run_dir.join("progress.jsonl"))?;
    let mut step = 0u32;
    let (mut train_loss, mut dev_loss, mut dev_acc) = (0.0f32, 0.0f32, 0.0f32);
    for epoch in 1..=epochs {
        let (mut tl, mut n) = (0.0f32, 0u32);
        for s in &train {
            step += 1;
            let (loss, _) = model.train_step(
                &k,
                &tokens[s.0..s.0 + s.1],
                &loss_mask[s.0..s.0 + s.1],
                step,
                lr,
            )?;
            tl += loss;
            n += 1;
        }
        train_loss = tl / n.max(1) as f32;
        let (dl, da) = eval(&model, &dev)?;
        dev_loss = dl;
        dev_acc = da;
        let raw = format!(
            "step={epoch} train_loss={train_loss:.8} dev_loss={dev_loss:.8} target_token_accuracy={dev_acc:.8} learning_rate={lr:.8}"
        );
        let rec = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "raw": raw,
            "metrics": {
                "train_loss": train_loss, "dev_loss": dev_loss,
                "target_token_accuracy": dev_acc, "learning_rate": lr
            },
            "step": epoch,
        });
        writeln!(progress, "{}", serde_json::to_string(&rec)?)?;
        println!("epoch {epoch}/{epochs}  train_loss={train_loss:.4}  dev_loss={dev_loss:.4}  dev_acc={dev_acc:.4}");
    }

    // ─── checkpoint hash + architecture ───
    let weights = model.weight_values(&k)?;
    let mut hasher = Sha256::new();
    for v in &weights {
        hasher.update(v.to_le_bytes());
    }
    let weights_sha256: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let parameter_count = model.parameter_count();
    let architecture = serde_json::json!({
        "n_embed": embed, "n_head": n_head, "n_layers": n_layers,
        "context_length": context, "vocab_size": vocab,
        "optimizer": {"kind": "adamw", "weight_decay": wd, "label_smoothing": 0.1},
        "precision": "f32",
        "compute": "refineforge-gpu (hand-written CUDA)",
        "device": k.device_name(),
    });

    // ─── checkpoints/step-N/gpt-checkpoint.json ───
    let ck_dir = run_dir.join("checkpoints").join(format!("step-{epochs}"));
    std::fs::create_dir_all(&ck_dir)?;
    let checkpoint = serde_json::json!({
        "schema_version": "refineforge-native-gpt-checkpoint-v1",
        "backend_kind": "refineforge_native_gpt",
        "step": epochs,
        "vocab_size": vocab,
        "parameter_count": parameter_count,
        "train_examples": train.len(),
        "dev_examples": dev.len(),
        "train_loss": train_loss,
        "dev_loss": dev_loss,
        "target_token_accuracy": dev_acc,
        "train_target_tokens": target_tokens(&train),
        "dev_target_tokens": target_tokens(&dev),
        "weights_sha256": weights_sha256,
        "architecture": architecture,
    });
    std::fs::write(
        ck_dir.join("gpt-checkpoint.json"),
        serde_json::to_vec_pretty(&checkpoint)?,
    )?;

    // ─── train-metadata.json ───
    let metadata = serde_json::json!({
        "schema_version": "refineforge-native-gpt-train-metadata-v1",
        "backend_kind": "refineforge_native_gpt",
        "experiment_id": "gpu-native-gpt-evidence",
        "pack_sha256": manifest["pack_sha256"],
        "tokenizer_sha256": manifest["tokenizer"]["sha256"],
        "vocab_size": vocab,
        "parameter_count": parameter_count,
        "architecture": architecture,
        "seed": seed,
        "steps": step,
        "train_examples": train.len(),
        "dev_examples": dev.len(),
        "train_example_ids": train.iter().map(|s| s.3.as_str()).collect::<Vec<_>>(),
        "dev_example_ids": dev.iter().map(|s| s.3.as_str()).collect::<Vec<_>>(),
        "created_at": chrono::Utc::now().to_rfc3339(),
        "reproducibility": "f32 / statistical-not-bit-exact; weights_sha256 is a per-run integrity hash, not cross-run reproducible (the CPU f64 native_gpt path is the deterministic reference)",
    });
    std::fs::write(
        run_dir.join("train-metadata.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;

    // ─── generation-smoke.json ───
    let prompt: Vec<i32> = train
        .first()
        .map(|s| {
            let plen = loss_mask[s.0..s.0 + s.1]
                .iter()
                .take_while(|&&m| m == 0)
                .count()
                .clamp(1, 4);
            tokens[s.0..s.0 + plen].to_vec()
        })
        .unwrap_or_else(|| vec![0]);
    let generated = model.generate(&k, &prompt, 8)?;
    let tok_bytes =
        |ts: &[i32]| -> Vec<u8> { ts.iter().flat_map(|&t| (t as u32).to_le_bytes()).collect() };
    let psha: String = Sha256::digest(tok_bytes(&prompt))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let osha: String = Sha256::digest(tok_bytes(&generated))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let smoke = serde_json::json!({
        "schema_version": "refineforge-generation-smoke-v1",
        "backend_kind": "refineforge_native_gpt",
        "prompt_token_count": prompt.len(),
        "generated_token_count": generated.len() - prompt.len(),
        "prompt_sha256": psha,
        "output_sha256": osha,
        "tokens": generated,
    });
    std::fs::write(
        run_dir.join("generation-smoke.json"),
        serde_json::to_vec_pretty(&smoke)?,
    )?;

    // ─── config.yaml (so `refineforge-trainer report <run-dir>` consumes it) ───
    let config = format!(
        "id: gpu-native-gpt-evidence\n\
         description: |\n  GPU-trained native GPT trust-ladder evidence (f32, hand-written CUDA).\n\
         base_model:\n  name: refineforge-native-gpt\n  source: native\n  revision: null\n\
         dataset:\n  path: {pack_dir}\n  format: sft_pack\n  fields: {{}}\n\
         backend:\n  kind: refineforge_native_gpt\n  config_file: null\n  command: null\n  extra_args: []\n  runtime:\n    compute: cuda-f32\n\
         hyperparameters:\n  context_length: {context}\n  eval_split: heldout\n  learning_rate: {lr}\n  n_embed: {embed}\n  n_head: {n_head}\n  n_layers: {n_layers}\n  seed: {seed}\n  steps: {step}\n  weight_decay: {wd}\n\
         checkpoint:\n  dir: checkpoints\n  save_steps: {epochs}\n  keep_last: 1\n\
         monitoring:\n  log_file: train.log\n  progress_format: native\n  metrics_to_track:\n  - train_loss\n  - dev_loss\n  - target_token_accuracy\n  - learning_rate\n\
         retry:\n  max_attempts: 1\n  backoff_seconds: 0\n  resume_from_checkpoint: false\n"
    );
    std::fs::write(run_dir.join("config.yaml"), config)?;

    println!("\nwrote GPU trust-ladder evidence to {}", run_dir.display());
    println!("  config.yaml · progress.jsonl ({epochs} records) · checkpoints/step-{epochs}/gpt-checkpoint.json");
    println!("  train-metadata.json · generation-smoke.json");
    println!(
        "verify with:  refineforge-trainer report {}",
        run_dir.display()
    );
    Ok(())
}
