//! Scale the device-resident GPT (Milestone 8) on the full Mathlib SFT pack.
//!
//! Build the pack first (800 train + 100 held-out, from the committed dataset):
//!   cat training/data/mathlib-proof-repair-v1/anthropic-sft.train.jsonl \
//!       training/data/mathlib-proof-repair-v1/anthropic-sft.heldout.jsonl > sft-full.jsonl
//!   cargo run -p refineforge-trainer --release -- \
//!       data pack-sft sft-full.jsonl --out <pack-dir> --max-seq-len 256 --target-only --seed 7
//!
//! Then train + eval on the GPU (requires an NVIDIA GPU + CUDA driver):
//!   cargo run -p refineforge-gpu --features cuda --release --example train_scale -- <pack-dir> [epochs]
//!
//! Trains a larger GPT entirely on the GPU (batch=1, lr warmup + cosine decay)
//! over the pack's `train` split and evaluates held-out (`split != "train"`)
//! accuracy each epoch. The point of this milestone: the GPU runs the many
//! optimizer steps the CPU `f64` backend could not (its scale run stalled at 40
//! steps / ~5 s per step), so a bigger model on real data can actually converge.
//! Still smoke-grade trust — bounded by the same eval/regression/approval gates.

#[cfg(not(feature = "cuda"))]
fn main() {
    eprintln!("train_scale requires `--features cuda` (and an NVIDIA GPU + CUDA driver).");
    std::process::exit(2);
}

#[cfg(feature = "cuda")]
fn main() -> anyhow::Result<()> {
    use refineforge_gpu::device::GptModel;
    use refineforge_gpu::gpu::GpuKernels;
    use std::time::Instant;

    let mut args = std::env::args().skip(1);
    let pack_dir = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: train_scale <pack-dir> [epochs]"))?;
    let epochs: u32 = match args.next() {
        Some(s) => s.parse()?,
        None => 12,
    };
    // Defaults are the measured-best regularization (best held-out loss, peak
    // overfitting delayed to epoch ~10); pass explicit args to ablate.
    let weight_decay: f32 = match args.next() {
        Some(s) => s.parse()?,
        None => 0.1,
    };
    let label_smoothing: f32 = match args.next() {
        Some(s) => s.parse()?,
        None => 0.1,
    };
    // Dropout defaults OFF: measured to not improve held-out on this 800-record
    // data (the accuracy ceiling is data-bound) and it costs ~15% compute. The
    // feature is available — pass a 5th arg to enable it.
    let dropout: f32 = match args.next() {
        Some(s) => s.parse()?,
        None => 0.0,
    };
    let pack = std::path::Path::new(&pack_dir);

    // ─── load the pack ───
    let tokens: Vec<i32> = std::fs::read(pack.join("tokens.bin"))?
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i32)
        .collect();
    let loss_mask: Vec<i32> = std::fs::read(pack.join("loss-mask.bin"))?
        .into_iter()
        .map(i32::from)
        .collect();
    anyhow::ensure!(
        tokens.len() == loss_mask.len(),
        "tokens / loss-mask length mismatch"
    );
    let records: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pack.join("records.json"))?)?;
    let records = records
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("records.json not an array"))?;
    let seqs: Vec<(usize, usize, bool)> = records
        .iter()
        .map(|r| {
            (
                r["token_start"].as_u64().unwrap_or(0) as usize,
                r["token_len"].as_u64().unwrap_or(0) as usize,
                r["split"].as_str().unwrap_or("train") == "train",
            )
        })
        .filter(|&(_, len, _)| len >= 2)
        .collect();
    // vocab from the tokenizer manifest (LM head must cover the whole vocab).
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pack.join("pack-manifest.json"))?)?;
    let vocab = manifest["tokenizer"]["vocab_size"].as_u64().unwrap_or(0) as usize;
    let data_max = tokens.iter().copied().max().unwrap_or(0) as usize + 1;
    let vocab = vocab.max(data_max);
    let context = seqs.iter().map(|&(_, l, _)| l).max().unwrap_or(0);
    let train: Vec<usize> = (0..seqs.len()).filter(|&i| seqs[i].2).collect();
    let dev: Vec<usize> = (0..seqs.len()).filter(|&i| !seqs[i].2).collect();

    // ─── a larger GPT (still bounded by 6 GB VRAM) ───
    let (embed, n_head, n_layers, hidden) = (256usize, 8usize, 4usize, 1024usize);
    let base_lr = 6.0e-4f32;
    println!(
        "pack: {} train / {} dev, vocab={vocab}, context={context}, {} tokens",
        train.len(),
        dev.len(),
        tokens.len()
    );
    println!(
        "model: embed={embed} heads={n_head} layers={n_layers} hidden={hidden} base_lr={base_lr} wd={weight_decay} ls={label_smoothing} do={dropout} · {epochs} epochs, batch=1"
    );

    let k = GpuKernels::new(0)?;
    let mut model = GptModel::new(&k, vocab, embed, n_head, n_layers, hidden, context, 7)?;
    model.set_weight_decay(weight_decay);
    model.set_label_smoothing(label_smoothing);
    model.set_dropout(dropout);

    let total_steps = epochs as usize * train.len();
    let warmup = (total_steps / 20).max(1);
    let lr_at = |step: usize| -> f32 {
        if step < warmup {
            base_lr * step as f32 / warmup as f32
        } else {
            let p = (step - warmup) as f32 / (total_steps - warmup).max(1) as f32;
            base_lr * (0.1 + 0.9 * 0.5 * (1.0 + (std::f32::consts::PI * p).cos()))
        }
    };

    let eval = |model: &GptModel, idxs: &[usize]| -> anyhow::Result<(f32, f32)> {
        let (mut l, mut a, mut n) = (0.0f32, 0.0f32, 0u32);
        for &ri in idxs {
            let (s, len, _) = seqs[ri];
            let (loss, acc) = model.evaluate(&k, &tokens[s..s + len], &loss_mask[s..s + len])?;
            l += loss;
            a += acc;
            n += 1;
        }
        Ok((l / n as f32, a / n as f32))
    };

    let (d0_loss, d0_acc) = eval(&model, &dev)?;
    println!("epoch  0 (init)   dev_loss={d0_loss:.4} dev_acc={d0_acc:.4}");

    // Track the early-stopping (best held-out) checkpoint — on this data the model
    // overfits well before the last epoch, so the best dev point is what matters.
    let (mut best_acc, mut best_acc_epoch) = (d0_acc, 0u32);
    let (mut best_loss, mut best_loss_epoch) = (d0_loss, 0u32);
    let mut order = train.clone();
    let mut step = 0usize;
    let t0 = Instant::now();
    for epoch in 1..=epochs {
        // deterministic per-epoch shuffle (SplitMix64 Fisher–Yates).
        let mut s = 0x1234_5678u64.wrapping_add(epoch as u64);
        for i in (1..order.len()).rev() {
            s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = s;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            let j = ((z ^ (z >> 31)) % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        let (mut tl, mut ta, mut n) = (0.0f32, 0.0f32, 0u32);
        for &ri in &order {
            step += 1;
            let (st, len, _) = seqs[ri];
            let (loss, acc) = model.train_step(
                &k,
                &tokens[st..st + len],
                &loss_mask[st..st + len],
                step as u32,
                lr_at(step),
            )?;
            tl += loss;
            ta += acc;
            n += 1;
        }
        let (dl, da) = eval(&model, &dev)?;
        if da > best_acc {
            best_acc = da;
            best_acc_epoch = epoch;
        }
        if dl < best_loss {
            best_loss = dl;
            best_loss_epoch = epoch;
        }
        println!(
            "epoch {epoch:>2}/{epochs}  train_loss={:.4} train_acc={:.4}  |  dev_loss={dl:.4} dev_acc={da:.4}",
            tl / n as f32,
            ta / n as f32
        );
    }
    let secs = t0.elapsed().as_secs_f64();
    let (dl, da) = eval(&model, &dev)?;
    println!(
        "\n{step} steps in {secs:.1}s ({:.1} ms/step) on the GPU",
        secs * 1000.0 / step as f64
    );
    println!(
        "FINAL held-out: dev_loss={dl:.4}  dev_acc={da:.4}  ({} sequences)",
        dev.len()
    );
    println!(
        "BEST  held-out: dev_acc={best_acc:.4} (epoch {best_acc_epoch}), dev_loss={best_loss:.4} (epoch {best_loss_epoch}) — early-stopping checkpoint"
    );
    println!("(CPU native-gpt scale baseline: dev_loss≈6.40, target_acc≈0.056 after 40 steps)");
    Ok(())
}
