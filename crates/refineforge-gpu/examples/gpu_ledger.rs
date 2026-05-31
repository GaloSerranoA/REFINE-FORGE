//! Emit the **GPU compute ledger** (Milestone 10) — a self-contained evidence
//! pack for the hand-written CUDA GPT path: the device, the kernel-source hash,
//! the parity-gate roster, the held-out eval, and the explicit
//! f32 / non-bit-exact reproducibility declaration.
//!
//! Usage (requires an NVIDIA GPU + CUDA driver):
//!   cargo run -p refineforge-gpu --features cuda --release --example gpu_ledger [-- <out.json>]
//!
//! With no argument the JSON ledger is printed to stdout; with a path it is
//! written there. The device name + compute capability are read from the live
//! GPU; the kernel-source SHA-256 and kernel count are computed from
//! `KERNEL_SOURCE`; the eval numbers are the measured M8/M9 results.

#[cfg(not(feature = "cuda"))]
fn main() {
    eprintln!("gpu_ledger requires `--features cuda` (and an NVIDIA GPU + CUDA driver).");
    std::process::exit(2);
}

#[cfg(feature = "cuda")]
fn main() -> anyhow::Result<()> {
    use refineforge_gpu::gpu::GpuKernels;
    use sha2::{Digest, Sha256};

    let out = std::env::args().nth(1);
    let k = GpuKernels::new(0)?;
    let (cc_major, cc_minor) = k.compute_capability();

    let src = refineforge_gpu::KERNEL_SOURCE;
    let source_sha256: String = Sha256::digest(src.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let kernel_count = src.matches("__global__").count();

    // The parity gates that verify every CUDA kernel against its CPU oracle.
    let parity_gates = [
        "gpu_vector_add_matches_cpu",
        "gpu_matmul_nn_matches_cpu",
        "gpu_matmul_nt_matches_cpu",
        "gpu_matmul_tn_matches_cpu",
        "gpu_gelu_matches_cpu",
        "gpu_gelu_backward_matches_cpu",
        "gpu_softmax_matches_cpu",
        "gpu_softmax_backward_matches_cpu",
        "gpu_layernorm_forward_matches_cpu",
        "gpu_layernorm_backward_matches_cpu",
        "gpu_adamw_matches_cpu_over_several_steps",
        "gpu_linear_forward_backward_matches_cpu",
        "gpu_layernorm_layer_matches_cpu",
        "gpu_mlp_layer_matches_cpu",
        "gpu_attention_forward_matches_cpu",
        "gpu_attention_backward_gradient_check",
        "gpu_embedding_matches_cpu",
        "gpu_cross_entropy_matches_cpu",
        "gpu_packed_forward_matches_sequential",
        "gpu_linear_train_step_learns_a_linear_target",
        "gpu_mlp_block_trains_end_to_end",
        "gpu_block_trains_end_to_end",
        "gpu_gpt_model_trains_end_to_end",
        "gpu_packed_model_trains_end_to_end",
    ];

    let ledger = serde_json::json!({
        "schema_version": "refineforge-gpu-compute-ledger-v1",
        "component": "refineforge-gpu (hand-written CUDA GPT path)",
        "device": {
            "name": k.device_name(),
            "compute_capability": format!("{cc_major}.{cc_minor}"),
        },
        "kernels": {
            "source_sha256": source_sha256,
            "kernel_count": kernel_count,
            "language": "CUDA C, compiled at runtime via NVRTC (cudarc)",
            "feature_gated": "behind the optional `cuda` feature; default build is CPU-only",
        },
        "reproducibility": {
            "precision": "f32",
            "mode": "statistical-not-bit-exact",
            "note": "GPU reductions (matmul/softmax/layernorm) are not bit-exact across runs. \
                     The CPU f64 native-gpt path remains the deterministic, bit-exact reference; \
                     every GPU kernel is parity-checked against a CPU f32 oracle within tolerance, \
                     never claimed bit-exact.",
        },
        "verification": {
            "parity_gates": parity_gates,
            "gate_count": parity_gates.len(),
            "cuda_test_total": 32,
            "note": "all gates run on the live GPU under `--features cuda`; CI builds CPU-only.",
        },
        "eval": {
            "dataset": "mathlib-proof-repair anthropic-sft — 800 train / 100 held-out, vocab 7505",
            "model": "embed=256, heads=8, layers=4, hidden=1024, ctx=256",
            "cpu_native_gpt_baseline": {
                "held_out_acc": 0.056, "held_out_loss": 6.40, "steps": 40,
                "note": "compute-bound on CPU f64 (~5 s/step); never converged",
            },
            "gpu_best": {
                "held_out_acc": 0.253, "held_out_loss": 4.55,
                "config": "wd=0.1, ls=0.1; early-stop epoch 4-10", "steps": 16000, "ms_per_step": 37,
            },
            "gpu_peak_acc": 0.256,
            "held_out_acc_gain_vs_cpu": "~4.6x",
            "ceiling_note": "held-out accuracy is data-bound at ~25% on 800 records (train memorizes to ~99%); \
                             regularization improves loss/calibration but not peak accuracy. More data is the lever.",
        },
        "milestones": {
            "M1": "toolchain + vector-add parity",
            "M2": "matmul nn/nt/tn (~28x vs scalar CPU)",
            "M3": "gelu / softmax / layernorm / adamw",
            "M4": "device-resident Linear + AdamW",
            "M5": "multi-head causal attention + transformer Block",
            "M6": "full GptModel + embeddings + cross-entropy",
            "M7": "packed mini-batch (segmented attention; 0.95x — not a throughput win)",
            "M8": "scale on full Mathlib data (~4.6x CPU held-out)",
            "M9": "regularization (weight decay + label smoothing)",
            "M10": "this compute ledger",
        },
    });

    let json = serde_json::to_string_pretty(&ledger)?;
    match out {
        Some(path) => {
            std::fs::write(&path, &json)?;
            println!("wrote GPU compute ledger to {path}");
        }
        None => println!("{json}"),
    }
    Ok(())
}
