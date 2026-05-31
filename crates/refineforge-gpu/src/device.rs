//! Device-resident layers (Milestone 4). Tensors stay on the GPU across
//! forward, backward, and the optimizer step — the pattern that lets the 3060
//! actually accelerate training. This module is only compiled with
//! `--features cuda`.
//!
//! Milestone 4 delivers a device-resident `Linear` (the dense building block):
//! forward `y = x·Wᵀ + b`, backward (`dx`, `dW`, `db`), and a device-resident
//! AdamW step, all parity-checked against a CPU `f32` reference. Composing this
//! with the M3 norm/attention kernels into a full block/model is the next step.

use crate::gpu::{DeviceTensor, GpuKernels};
use anyhow::Result;

/// A dense layer whose weights, gradients, and AdamW state all live on the GPU.
/// `w` is `out×in`, `bias` is `1×out`.
pub struct Linear {
    pub w: DeviceTensor,
    pub bias: DeviceTensor,
    dw: DeviceTensor,
    db: DeviceTensor,
    mw: DeviceTensor,
    vw: DeviceTensor,
    mb: DeviceTensor,
    vb: DeviceTensor,
    in_dim: usize,
    out_dim: usize,
}

impl Linear {
    pub fn new(
        k: &GpuKernels,
        w: &[f32],
        bias: &[f32],
        in_dim: usize,
        out_dim: usize,
    ) -> Result<Self> {
        anyhow::ensure!(
            w.len() == out_dim * in_dim && bias.len() == out_dim,
            "Linear dim mismatch"
        );
        Ok(Self {
            w: k.to_device(w, out_dim, in_dim)?,
            bias: k.to_device(bias, 1, out_dim)?,
            dw: k.zeros_device(out_dim, in_dim)?,
            db: k.zeros_device(1, out_dim)?,
            mw: k.zeros_device(out_dim, in_dim)?,
            vw: k.zeros_device(out_dim, in_dim)?,
            mb: k.zeros_device(1, out_dim)?,
            vb: k.zeros_device(1, out_dim)?,
            in_dim,
            out_dim,
        })
    }

    /// `y = x·Wᵀ + b`, where `x` is `T×in` and `y` is `T×out`.
    pub fn forward(&self, k: &GpuKernels, x: &DeviceTensor) -> Result<DeviceTensor> {
        anyhow::ensure!(x.cols == self.in_dim, "Linear forward input dim mismatch");
        let mut y = k.dev_matmul_nt(x, &self.w)?;
        k.dev_bias_add(&mut y, &self.bias)?;
        Ok(y)
    }

    /// Backward: returns `dx` (`T×in`) and stores `dW`/`db` for the optimizer.
    pub fn backward(
        &mut self,
        k: &GpuKernels,
        x: &DeviceTensor,
        dy: &DeviceTensor,
    ) -> Result<DeviceTensor> {
        // dx = dy · W   (T×out · out×in)
        let dx = k.dev_matmul_nn(dy, &self.w)?;
        // dW = dyᵀ · x  (out×T · T×in)
        self.dw = k.dev_matmul_tn(dy, x)?;
        // db = Σ_rows dy
        self.db = k.dev_col_sum(dy)?;
        Ok(dx)
    }

    /// One device-resident AdamW step over `w` and `bias`.
    pub fn adamw_step(&mut self, k: &GpuKernels, t: u32, lr: f32) -> Result<()> {
        k.dev_adamw(
            &mut self.w,
            &self.dw,
            &mut self.mw,
            &mut self.vw,
            t,
            lr,
            0.9,
            0.999,
            1.0e-8,
            0.0,
        )?;
        k.dev_adamw(
            &mut self.bias,
            &self.db,
            &mut self.mb,
            &mut self.vb,
            t,
            lr,
            0.9,
            0.999,
            1.0e-8,
            0.0,
        )?;
        Ok(())
    }

    pub fn dims(&self) -> (usize, usize) {
        (self.in_dim, self.out_dim)
    }
}

// ─── CPU f32 reference (the parity oracle) ───

/// CPU `y = x·Wᵀ + b`.
pub fn linear_forward_cpu(
    x: &[f32],
    t: usize,
    in_dim: usize,
    w: &[f32],
    out_dim: usize,
    bias: &[f32],
) -> Vec<f32> {
    let mut y = crate::matmul_nt_cpu(x, t, in_dim, w, out_dim);
    for r in 0..t {
        for c in 0..out_dim {
            y[r * out_dim + c] += bias[c];
        }
    }
    y
}

/// CPU Linear backward → `(dx, dW, db)`.
pub fn linear_backward_cpu(
    x: &[f32],
    t: usize,
    in_dim: usize,
    w: &[f32],
    out_dim: usize,
    dy: &[f32],
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let dx = crate::matmul_nn_cpu(dy, t, out_dim, w, in_dim);
    let dw = crate::matmul_tn_cpu(dy, t, out_dim, x, in_dim);
    let mut db = vec![0.0f32; out_dim];
    for r in 0..t {
        for c in 0..out_dim {
            db[c] += dy[r * out_dim + c];
        }
    }
    (dx, dw, db)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(n: usize, seed: u64) -> Vec<f32> {
        let mut s = seed.wrapping_add(1);
        (0..n)
            .map(|_| {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((s >> 33) as f32 / u32::MAX as f32) - 0.5
            })
            .collect()
    }

    fn assert_close(a: &[f32], b: &[f32], tol: f32) {
        assert_eq!(a.len(), b.len());
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            assert!(
                (x - y).abs() <= tol * (1.0 + y.abs()),
                "mismatch at {i}: {x} vs {y}"
            );
        }
    }

    #[test]
    fn gpu_linear_forward_backward_matches_cpu() {
        let k = GpuKernels::new(0).expect("gpu init");
        let (t, in_dim, out_dim) = (24, 40, 32);
        let x = fill(t * in_dim, 1);
        let w = fill(out_dim * in_dim, 2);
        let bias = fill(out_dim, 3);
        let mut lin = Linear::new(&k, &w, &bias, in_dim, out_dim).unwrap();
        let x_dev = k.to_device(&x, t, in_dim).unwrap();

        let y = k.to_host(&lin.forward(&k, &x_dev).unwrap()).unwrap();
        assert_close(
            &y,
            &linear_forward_cpu(&x, t, in_dim, &w, out_dim, &bias),
            1.0e-3,
        );

        let dy = fill(t * out_dim, 4);
        let dy_dev = k.to_device(&dy, t, out_dim).unwrap();
        let dx = k
            .to_host(&lin.backward(&k, &x_dev, &dy_dev).unwrap())
            .unwrap();
        let dw = k.to_host(&lin.dw).unwrap();
        let db = k.to_host(&lin.db).unwrap();
        let (dx_cpu, dw_cpu, db_cpu) = linear_backward_cpu(&x, t, in_dim, &w, out_dim, &dy);
        assert_close(&dx, &dx_cpu, 1.0e-3);
        assert_close(&dw, &dw_cpu, 1.0e-3);
        assert_close(&db, &db_cpu, 1.0e-3);
    }

    #[test]
    fn gpu_linear_train_step_learns_a_linear_target() {
        // Fit a known linear function end-to-end on the GPU: forward, MSE,
        // backward, and AdamW all device-resident. Loss must drop sharply.
        let k = GpuKernels::new(0).expect("gpu init");
        let (t, in_dim, out_dim) = (32, 16, 8);
        let x = fill(t * in_dim, 5);
        let w_true = fill(out_dim * in_dim, 7);
        let b_true = fill(out_dim, 8);
        let target = linear_forward_cpu(&x, t, in_dim, &w_true, out_dim, &b_true);

        let w0: Vec<f32> = fill(out_dim * in_dim, 9).iter().map(|v| v * 0.1).collect();
        let b0 = vec![0.0f32; out_dim];
        let mut lin = Linear::new(&k, &w0, &b0, in_dim, out_dim).unwrap();
        let x_dev = k.to_device(&x, t, in_dim).unwrap();

        let n = (t * out_dim) as f32;
        let mse = |pred: &[f32]| {
            pred.iter()
                .zip(&target)
                .map(|(p, q)| (p - q) * (p - q))
                .sum::<f32>()
                / n
        };
        let mut first = 0.0f32;
        let mut last = 0.0f32;
        for step in 1..=80u32 {
            let pred = k.to_host(&lin.forward(&k, &x_dev).unwrap()).unwrap();
            let loss = mse(&pred);
            let dy: Vec<f32> = pred
                .iter()
                .zip(&target)
                .map(|(p, q)| 2.0 * (p - q) / n)
                .collect();
            let dy_dev = k.to_device(&dy, t, out_dim).unwrap();
            lin.backward(&k, &x_dev, &dy_dev).unwrap();
            lin.adamw_step(&k, step, 0.05).unwrap();
            if step == 1 {
                first = loss;
            }
            last = loss;
        }
        assert!(
            last < first * 0.05,
            "GPU training should fit the linear target: first={first} last={last}"
        );
    }
}
