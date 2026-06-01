//! Train the device-resident GPT (Milestone 6) on a real Refine-Forge SFT pack.
//!
//! Usage (requires an NVIDIA GPU + CUDA driver):
//!   cargo run -p refineforge-gpu --features cuda --release --example train_mathlib -- <pack-dir> [epochs]
//!
//! `<pack-dir>` must contain `tokens.bin` (u32 LE), `loss-mask.bin` (u8), and
//! `records.json` (the `refineforge-sft-pack-v1` layout). The whole forward →
//! cross-entropy → backward → AdamW path runs on the GPU, one sequence per step.
//!
//! This is a smoke-scale demonstration that the GPU training path learns on real
//! Mathlib tokens — NOT a production-quality LLM. Trust stays bounded by the same
//! eval/regression/approval gates as the CPU backend.

#[cfg(not(feature = "cuda"))]
fn main() {
    eprintln!("train_mathlib requires `--features cuda` (and an NVIDIA GPU + CUDA driver).");
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
        .ok_or_else(|| anyhow::anyhow!("usage: train_mathlib <pack-dir> [epochs]"))?;
    let epochs: u32 = match args.next() {
        Some(s) => s.parse()?,
        None => 8,
    };
    let pack = std::path::Path::new(&pack_dir);

    // ─── load the pack: tokens.bin (u32 LE), loss-mask.bin (u8), records.json ───
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
        "tokens / loss-mask length mismatch ({} vs {})",
        tokens.len(),
        loss_mask.len()
    );
    let records: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pack.join("records.json"))?)?;
    let records = records
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("records.json is not an array"))?;

    // (token_start, token_len, is_train) for every sequence with a next-token target.
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
    anyhow::ensure!(!seqs.is_empty(), "no usable sequences in the pack");

    let vocab = tokens.iter().copied().max().unwrap_or(0) as usize + 1;
    let context = seqs.iter().map(|&(_, l, _)| l).max().unwrap_or(0);
    let n_train = seqs.iter().filter(|s| s.2).count();

    // Smoke-scale GPT — deliberately small for a 6 GB RTX 3060.
    let (embed, n_head, n_layers, hidden, lr) = (128usize, 4usize, 4usize, 512usize, 3.0e-4f32);
    println!(
        "pack: {} sequences ({} train / {} dev), vocab={}, context={}, {} tokens total",
        seqs.len(),
        n_train,
        seqs.len() - n_train,
        vocab,
        context,
        tokens.len()
    );
    println!(
        "model: embed={embed} heads={n_head} layers={n_layers} hidden={hidden} lr={lr}  · {epochs} epochs, batch=1"
    );

    let k = GpuKernels::new_auto()?;
    eprintln!("GPU: {}", k.device_summary());
    let mut model = GptModel::new(&k, vocab, embed, n_head, n_layers, hidden, context, 7)?;

    // ─── device-resident training loop: one sequence per step ───
    let mut step = 0u32;
    let mut processed_targets = 0usize;
    let t0 = Instant::now();
    for epoch in 1..=epochs {
        let (mut loss_sum, mut acc_sum, mut n) = (0.0f32, 0.0f32, 0u32);
        for &(start, len, is_train) in &seqs {
            if !is_train {
                continue;
            }
            step += 1;
            let seq = &tokens[start..start + len];
            let mask = &loss_mask[start..start + len];
            let (loss, acc) = model.train_step(&k, seq, mask, step, lr)?;
            loss_sum += loss;
            acc_sum += acc;
            n += 1;
            processed_targets += mask.iter().skip(1).filter(|&&m| m != 0).count();
        }
        println!(
            "epoch {epoch:>2}/{epochs}  train_loss={:.4}  target_acc={:.3}",
            loss_sum / n as f32,
            acc_sum / n as f32
        );
    }
    let secs = t0.elapsed().as_secs_f64();
    println!(
        "\n{step} steps in {secs:.2}s  ({:.2} ms/step, {:.0} target-tokens/s) — full fwd+bwd+AdamW on the GPU",
        secs * 1000.0 / step as f64,
        processed_targets as f64 / secs
    );

    // ─── held-out (dev) evaluation, forward-only ───
    let (mut dev_loss, mut dev_acc, mut dev_n) = (0.0f32, 0.0f32, 0u32);
    for &(start, len, is_train) in &seqs {
        if is_train {
            continue;
        }
        let (loss, acc) = model.evaluate(
            &k,
            &tokens[start..start + len],
            &loss_mask[start..start + len],
        )?;
        dev_loss += loss;
        dev_acc += acc;
        dev_n += 1;
    }
    if dev_n > 0 {
        println!(
            "held-out: dev_loss={:.4}  dev_acc={:.3}  over {dev_n} sequences",
            dev_loss / dev_n as f32,
            dev_acc / dev_n as f32
        );
    }
    Ok(())
}
