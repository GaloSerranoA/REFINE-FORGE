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

/// Device-resident LayerNorm with trainable `gamma`/`beta` and AdamW state.
pub struct LayerNorm {
    gamma: DeviceTensor,
    beta: DeviceTensor,
    dgamma: DeviceTensor,
    dbeta: DeviceTensor,
    mg: DeviceTensor,
    vg: DeviceTensor,
    mb: DeviceTensor,
    vb: DeviceTensor,
    eps: f32,
}

impl LayerNorm {
    pub fn new(k: &GpuKernels, dim: usize) -> Result<Self> {
        Ok(Self {
            gamma: k.to_device(&vec![1.0f32; dim], 1, dim)?,
            beta: k.zeros_device(1, dim)?,
            dgamma: k.zeros_device(1, dim)?,
            dbeta: k.zeros_device(1, dim)?,
            mg: k.zeros_device(1, dim)?,
            vg: k.zeros_device(1, dim)?,
            mb: k.zeros_device(1, dim)?,
            vb: k.zeros_device(1, dim)?,
            eps: 1.0e-5,
        })
    }

    /// Forward → `(y, mean, rstd)`; `mean`/`rstd` feed the backward pass.
    pub fn forward(
        &self,
        k: &GpuKernels,
        x: &DeviceTensor,
    ) -> Result<(DeviceTensor, DeviceTensor, DeviceTensor)> {
        k.dev_layernorm_forward(x, &self.gamma, &self.beta, self.eps)
    }

    pub fn backward(
        &mut self,
        k: &GpuKernels,
        x: &DeviceTensor,
        dy: &DeviceTensor,
        mean: &DeviceTensor,
        rstd: &DeviceTensor,
    ) -> Result<DeviceTensor> {
        let (dx, dg, db) = k.dev_layernorm_backward(x, &self.gamma, dy, mean, rstd)?;
        self.dgamma = dg;
        self.dbeta = db;
        Ok(dx)
    }

    pub fn adamw_step(&mut self, k: &GpuKernels, t: u32, lr: f32) -> Result<()> {
        k.dev_adamw(
            &mut self.gamma,
            &self.dgamma,
            &mut self.mg,
            &mut self.vg,
            t,
            lr,
            0.9,
            0.999,
            1.0e-8,
            0.0,
        )?;
        k.dev_adamw(
            &mut self.beta,
            &self.dbeta,
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
}

/// Position-wise MLP: `Linear(embed→hidden) → GELU → Linear(hidden→embed)`.
pub struct Mlp {
    fc1: Linear,
    fc2: Linear,
}

impl Mlp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        k: &GpuKernels,
        w1: &[f32],
        b1: &[f32],
        w2: &[f32],
        b2: &[f32],
        embed: usize,
        hidden: usize,
    ) -> Result<Self> {
        Ok(Self {
            fc1: Linear::new(k, w1, b1, embed, hidden)?,
            fc2: Linear::new(k, w2, b2, hidden, embed)?,
        })
    }

    /// Forward → `(y, h1, act)`; `h1` (pre-GELU) and `act` feed the backward.
    pub fn forward(
        &self,
        k: &GpuKernels,
        x: &DeviceTensor,
    ) -> Result<(DeviceTensor, DeviceTensor, DeviceTensor)> {
        let h1 = self.fc1.forward(k, x)?;
        let act = k.dev_gelu(&h1)?;
        let y = self.fc2.forward(k, &act)?;
        Ok((y, h1, act))
    }

    pub fn backward(
        &mut self,
        k: &GpuKernels,
        x: &DeviceTensor,
        h1: &DeviceTensor,
        act: &DeviceTensor,
        dy: &DeviceTensor,
    ) -> Result<DeviceTensor> {
        let d_act = self.fc2.backward(k, act, dy)?;
        let d_h1 = k.dev_gelu_backward(h1, &d_act)?;
        self.fc1.backward(k, x, &d_h1)
    }

    pub fn adamw_step(&mut self, k: &GpuKernels, t: u32, lr: f32) -> Result<()> {
        self.fc1.adamw_step(k, t, lr)?;
        self.fc2.adamw_step(k, t, lr)?;
        Ok(())
    }
}

/// A pre-norm feed-forward block: `out = x + MLP(LayerNorm(x))`. The
/// feed-forward half of a transformer block, fully device-resident.
pub struct MlpBlock {
    ln: LayerNorm,
    mlp: Mlp,
}

/// Forward activations the [`MlpBlock`] backward needs.
pub struct MlpBlockCache {
    ln_y: DeviceTensor,
    mean: DeviceTensor,
    rstd: DeviceTensor,
    h1: DeviceTensor,
    act: DeviceTensor,
}

impl MlpBlock {
    pub fn new(ln: LayerNorm, mlp: Mlp) -> Self {
        Self { ln, mlp }
    }

    pub fn forward(
        &self,
        k: &GpuKernels,
        x: &DeviceTensor,
    ) -> Result<(DeviceTensor, MlpBlockCache)> {
        let (ln_y, mean, rstd) = self.ln.forward(k, x)?;
        let (mlp_out, h1, act) = self.mlp.forward(k, &ln_y)?;
        let mut out = mlp_out;
        k.dev_add_inplace(&mut out, x)?; // residual: out = MLP(LN(x)) + x
        Ok((
            out,
            MlpBlockCache {
                ln_y,
                mean,
                rstd,
                h1,
                act,
            },
        ))
    }

    /// Backward given `d_out`; returns `dx`.
    pub fn backward(
        &mut self,
        k: &GpuKernels,
        x: &DeviceTensor,
        cache: &MlpBlockCache,
        d_out: &DeviceTensor,
    ) -> Result<DeviceTensor> {
        // out = x + MLP(LN(x)) → d_mlp_out = d_out, and the residual adds d_out to dx.
        let d_ln_y = self
            .mlp
            .backward(k, &cache.ln_y, &cache.h1, &cache.act, d_out)?;
        let mut dx = self.ln.backward(k, x, &d_ln_y, &cache.mean, &cache.rstd)?;
        k.dev_add_inplace(&mut dx, d_out)?;
        Ok(dx)
    }

    pub fn adamw_step(&mut self, k: &GpuKernels, t: u32, lr: f32) -> Result<()> {
        self.ln.adamw_step(k, t, lr)?;
        self.mlp.adamw_step(k, t, lr)?;
        Ok(())
    }
}

/// Multi-head causal self-attention, device-resident. Q/K/V/O are dense
/// projections; per head the scores `Q_h·K_hᵀ` are scaled, causally masked,
/// softmaxed, and applied to `V_h`. Composed entirely from the device
/// primitives (matmul, slice/set-cols, causal mask, softmax).
pub struct Attention {
    q: Linear,
    k: Linear,
    v: Linear,
    o: Linear,
    n_head: usize,
    head_dim: usize,
    scale: f32,
}

/// Forward activations the attention backward needs.
pub struct AttnCache {
    qd: DeviceTensor,
    kd: DeviceTensor,
    vd: DeviceTensor,
    attn: Vec<DeviceTensor>, // per head, T×T
    ctx: DeviceTensor,
}

impl Attention {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kern: &GpuKernels,
        embed: usize,
        n_head: usize,
        wq: &[f32],
        wk: &[f32],
        wv: &[f32],
        wo: &[f32],
    ) -> Result<Self> {
        anyhow::ensure!(embed.is_multiple_of(n_head), "embed must divide n_head");
        let zero = vec![0.0f32; embed];
        Ok(Self {
            q: Linear::new(kern, wq, &zero, embed, embed)?,
            k: Linear::new(kern, wk, &zero, embed, embed)?,
            v: Linear::new(kern, wv, &zero, embed, embed)?,
            o: Linear::new(kern, wo, &zero, embed, embed)?,
            n_head,
            head_dim: embed / n_head,
            scale: 1.0 / (embed as f32 / n_head as f32).sqrt(),
        })
    }

    pub fn forward(
        &self,
        kern: &GpuKernels,
        x: &DeviceTensor,
    ) -> Result<(DeviceTensor, AttnCache)> {
        let (t, e, hd) = (x.rows, x.cols, self.head_dim);
        let qd = self.q.forward(kern, x)?;
        let kd = self.k.forward(kern, x)?;
        let vd = self.v.forward(kern, x)?;
        let mut ctx = kern.zeros_device(t, e)?;
        let mut attn_per_head = Vec::with_capacity(self.n_head);
        for h in 0..self.n_head {
            let off = h * hd;
            let q_h = kern.dev_slice_cols(&qd, off, hd)?;
            let k_h = kern.dev_slice_cols(&kd, off, hd)?;
            let v_h = kern.dev_slice_cols(&vd, off, hd)?;
            let mut scores = kern.dev_matmul_nt(&q_h, &k_h)?; // T×T = Q_h·K_hᵀ
            kern.dev_scale_causal_mask(&mut scores, self.scale)?;
            let attn = kern.dev_softmax(&scores)?;
            let ctx_h = kern.dev_matmul_nn(&attn, &v_h)?; // T×hd
            kern.dev_set_cols(&mut ctx, &ctx_h, off)?;
            attn_per_head.push(attn);
        }
        let out = self.o.forward(kern, &ctx)?;
        Ok((
            out,
            AttnCache {
                qd,
                kd,
                vd,
                attn: attn_per_head,
                ctx,
            },
        ))
    }

    pub fn backward(
        &mut self,
        kern: &GpuKernels,
        x: &DeviceTensor,
        cache: &AttnCache,
        d_out: &DeviceTensor,
    ) -> Result<DeviceTensor> {
        let (t, e, hd) = (x.rows, x.cols, self.head_dim);
        let d_ctx = self.o.backward(kern, &cache.ctx, d_out)?;
        let mut dq = kern.zeros_device(t, e)?;
        let mut dk = kern.zeros_device(t, e)?;
        let mut dv = kern.zeros_device(t, e)?;
        for h in 0..self.n_head {
            let off = h * hd;
            let q_h = kern.dev_slice_cols(&cache.qd, off, hd)?;
            let k_h = kern.dev_slice_cols(&cache.kd, off, hd)?;
            let v_h = kern.dev_slice_cols(&cache.vd, off, hd)?;
            let attn = &cache.attn[h];
            let d_ctx_h = kern.dev_slice_cols(&d_ctx, off, hd)?;
            // ctx_h = attn · V_h
            let d_attn = kern.dev_matmul_nt(&d_ctx_h, &v_h)?; // T×T = d_ctx_h·V_hᵀ
            let d_v_h = kern.dev_matmul_tn(attn, &d_ctx_h)?; // T×hd = attnᵀ·d_ctx_h
            kern.dev_set_cols(&mut dv, &d_v_h, off)?;
            // softmax + scale/mask backward
            let mut d_scores = kern.dev_softmax_backward(attn, &d_attn)?; // T×T
            kern.dev_scale_causal_mask_grad(&mut d_scores, self.scale)?;
            // scores = Q_h · K_hᵀ
            let d_q_h = kern.dev_matmul_nn(&d_scores, &k_h)?; // T×hd = d_scores·K_h
            let d_k_h = kern.dev_matmul_tn(&d_scores, &q_h)?; // T×hd = d_scoresᵀ·Q_h
            kern.dev_set_cols(&mut dq, &d_q_h, off)?;
            kern.dev_set_cols(&mut dk, &d_k_h, off)?;
        }
        let mut dx = self.q.backward(kern, x, &dq)?;
        let dx_k = self.k.backward(kern, x, &dk)?;
        let dx_v = self.v.backward(kern, x, &dv)?;
        kern.dev_add_inplace(&mut dx, &dx_k)?;
        kern.dev_add_inplace(&mut dx, &dx_v)?;
        Ok(dx)
    }

    pub fn adamw_step(&mut self, kern: &GpuKernels, t: u32, lr: f32) -> Result<()> {
        self.q.adamw_step(kern, t, lr)?;
        self.k.adamw_step(kern, t, lr)?;
        self.v.adamw_step(kern, t, lr)?;
        self.o.adamw_step(kern, t, lr)?;
        Ok(())
    }
}

/// A full pre-norm transformer block:
/// `x1 = x + Attn(LN1(x)); out = x1 + MLP(LN2(x1))`.
pub struct Block {
    ln1: LayerNorm,
    attn: Attention,
    ln2: LayerNorm,
    mlp: Mlp,
}

/// Forward activations the [`Block`] backward needs.
pub struct BlockCache {
    ln1_y: DeviceTensor,
    m1: DeviceTensor,
    r1: DeviceTensor,
    attn_cache: AttnCache,
    x1: DeviceTensor,
    ln2_y: DeviceTensor,
    m2: DeviceTensor,
    r2: DeviceTensor,
    h1: DeviceTensor,
    act: DeviceTensor,
}

impl Block {
    pub fn new(ln1: LayerNorm, attn: Attention, ln2: LayerNorm, mlp: Mlp) -> Self {
        Self {
            ln1,
            attn,
            ln2,
            mlp,
        }
    }

    pub fn forward(&self, k: &GpuKernels, x: &DeviceTensor) -> Result<(DeviceTensor, BlockCache)> {
        let (ln1_y, m1, r1) = self.ln1.forward(k, x)?;
        let (attn_out, attn_cache) = self.attn.forward(k, &ln1_y)?;
        let mut x1 = attn_out;
        k.dev_add_inplace(&mut x1, x)?; // x1 = x + Attn(LN1(x))
        let (ln2_y, m2, r2) = self.ln2.forward(k, &x1)?;
        let (mlp_out, h1, act) = self.mlp.forward(k, &ln2_y)?;
        let mut out = mlp_out;
        k.dev_add_inplace(&mut out, &x1)?; // out = x1 + MLP(LN2(x1))
        Ok((
            out,
            BlockCache {
                ln1_y,
                m1,
                r1,
                attn_cache,
                x1,
                ln2_y,
                m2,
                r2,
                h1,
                act,
            },
        ))
    }

    pub fn backward(
        &mut self,
        k: &GpuKernels,
        x: &DeviceTensor,
        c: &BlockCache,
        d_out: &DeviceTensor,
    ) -> Result<DeviceTensor> {
        // out = x1 + MLP(LN2(x1))
        let d_ln2_y = self.mlp.backward(k, &c.ln2_y, &c.h1, &c.act, d_out)?;
        let mut d_x1 = self.ln2.backward(k, &c.x1, &d_ln2_y, &c.m2, &c.r2)?;
        k.dev_add_inplace(&mut d_x1, d_out)?; // residual through x1
                                              // x1 = x + Attn(LN1(x))
        let d_ln1_y = self.attn.backward(k, &c.ln1_y, &c.attn_cache, &d_x1)?;
        let mut dx = self.ln1.backward(k, x, &d_ln1_y, &c.m1, &c.r1)?;
        k.dev_add_inplace(&mut dx, &d_x1)?; // residual through x
        Ok(dx)
    }

    pub fn adamw_step(&mut self, k: &GpuKernels, t: u32, lr: f32) -> Result<()> {
        self.ln1.adamw_step(k, t, lr)?;
        self.attn.adamw_step(k, t, lr)?;
        self.ln2.adamw_step(k, t, lr)?;
        self.mlp.adamw_step(k, t, lr)?;
        Ok(())
    }
}

// ─── CPU references for the composed layers (parity oracle) ───

/// CPU multi-head causal self-attention forward (mirrors the GPU composition).
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn attention_forward_cpu(
    x: &[f32],
    t: usize,
    e: usize,
    n_head: usize,
    wq: &[f32],
    wk: &[f32],
    wv: &[f32],
    wo: &[f32],
) -> Vec<f32> {
    let zero = vec![0.0f32; e];
    let hd = e / n_head;
    let scale = 1.0f32 / (hd as f32).sqrt();
    let q = linear_forward_cpu(x, t, e, wq, e, &zero);
    let kk = linear_forward_cpu(x, t, e, wk, e, &zero);
    let vv = linear_forward_cpu(x, t, e, wv, e, &zero);
    let mut ctx = vec![0.0f32; t * e];
    for h in 0..n_head {
        let off = h * hd;
        let mut q_h = vec![0.0f32; t * hd];
        let mut k_h = vec![0.0f32; t * hd];
        let mut v_h = vec![0.0f32; t * hd];
        for i in 0..t {
            for d in 0..hd {
                q_h[i * hd + d] = q[i * e + off + d];
                k_h[i * hd + d] = kk[i * e + off + d];
                v_h[i * hd + d] = vv[i * e + off + d];
            }
        }
        let mut scores = crate::matmul_nt_cpu(&q_h, t, hd, &k_h, t); // T×T
        for i in 0..t {
            for j in 0..t {
                if j > i {
                    scores[i * t + j] = -1.0e30;
                } else {
                    scores[i * t + j] *= scale;
                }
            }
        }
        let attn = crate::softmax_cpu(&scores, t, t);
        let ctx_h = crate::matmul_nn_cpu(&attn, t, t, &v_h, hd); // T×hd
        for i in 0..t {
            for d in 0..hd {
                ctx[i * e + off + d] = ctx_h[i * hd + d];
            }
        }
    }
    linear_forward_cpu(&ctx, t, e, wo, e, &zero)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn mlp_forward_cpu(
    x: &[f32],
    t: usize,
    embed: usize,
    hidden: usize,
    w1: &[f32],
    b1: &[f32],
    w2: &[f32],
    b2: &[f32],
) -> Vec<f32> {
    let h1 = linear_forward_cpu(x, t, embed, w1, hidden, b1);
    let act = crate::gelu_cpu(&h1);
    linear_forward_cpu(&act, t, hidden, w2, embed, b2)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn mlp_backward_cpu(
    x: &[f32],
    t: usize,
    embed: usize,
    hidden: usize,
    w1: &[f32],
    b1: &[f32],
    w2: &[f32],
    dy: &[f32],
) -> Vec<f32> {
    let h1 = linear_forward_cpu(x, t, embed, w1, hidden, b1);
    let act = crate::gelu_cpu(&h1);
    let (d_act, _dw2, _db2) = linear_backward_cpu(&act, t, hidden, w2, embed, dy);
    let d_h1 = crate::gelu_backward_cpu(&h1, &d_act);
    let (dx, _dw1, _db1) = linear_backward_cpu(x, t, embed, w1, hidden, &d_h1);
    dx
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

    #[test]
    fn gpu_layernorm_layer_matches_cpu() {
        let k = GpuKernels::new(0).expect("gpu init");
        let (t, dim) = (16, 24);
        let x = fill(t * dim, 1);
        let mut ln = LayerNorm::new(&k, dim).unwrap();
        let x_dev = k.to_device(&x, t, dim).unwrap();
        let (y_dev, mean_dev, rstd_dev) = ln.forward(&k, &x_dev).unwrap();
        let gamma = vec![1.0f32; dim];
        let beta = vec![0.0f32; dim];
        let (y_cpu, mean_cpu, rstd_cpu) =
            crate::layernorm_forward_cpu(&x, &gamma, &beta, t, dim, 1.0e-5);
        assert_close(&k.to_host(&y_dev).unwrap(), &y_cpu, 1.0e-3);

        let dy = fill(t * dim, 2);
        let dy_dev = k.to_device(&dy, t, dim).unwrap();
        let dx = k
            .to_host(
                &ln.backward(&k, &x_dev, &dy_dev, &mean_dev, &rstd_dev)
                    .unwrap(),
            )
            .unwrap();
        let (dx_cpu, _dg, _db) =
            crate::layernorm_backward_cpu(&x, &gamma, &dy, &mean_cpu, &rstd_cpu, t, dim);
        assert_close(&dx, &dx_cpu, 1.0e-3);
    }

    #[test]
    fn gpu_mlp_layer_matches_cpu() {
        let k = GpuKernels::new(0).expect("gpu init");
        let (t, embed, hidden) = (16, 24, 48);
        let x = fill(t * embed, 1);
        let w1 = fill(hidden * embed, 2);
        let b1 = fill(hidden, 3);
        let w2 = fill(embed * hidden, 4);
        let b2 = fill(embed, 5);
        let mut mlp = Mlp::new(&k, &w1, &b1, &w2, &b2, embed, hidden).unwrap();
        let x_dev = k.to_device(&x, t, embed).unwrap();
        let (y_dev, h1, act) = mlp.forward(&k, &x_dev).unwrap();
        assert_close(
            &k.to_host(&y_dev).unwrap(),
            &mlp_forward_cpu(&x, t, embed, hidden, &w1, &b1, &w2, &b2),
            2.0e-3,
        );

        let dy = fill(t * embed, 6);
        let dy_dev = k.to_device(&dy, t, embed).unwrap();
        let dx = k
            .to_host(&mlp.backward(&k, &x_dev, &h1, &act, &dy_dev).unwrap())
            .unwrap();
        assert_close(
            &dx,
            &mlp_backward_cpu(&x, t, embed, hidden, &w1, &b1, &w2, &dy),
            2.0e-3,
        );
    }

    #[test]
    fn gpu_mlp_block_trains_end_to_end() {
        // A pre-norm feed-forward block (LayerNorm → Linear → GELU → Linear →
        // residual), trained entirely on the GPU. Loss must drop sharply.
        let k = GpuKernels::new(0).expect("gpu init");
        let (t, embed, hidden) = (16, 16, 32);
        let x = fill(t * embed, 1);
        let target = fill(t * embed, 2);
        let w1: Vec<f32> = fill(hidden * embed, 3).iter().map(|v| v * 0.1).collect();
        let b1 = vec![0.0f32; hidden];
        let w2: Vec<f32> = fill(embed * hidden, 4).iter().map(|v| v * 0.1).collect();
        let b2 = vec![0.0f32; embed];
        let ln = LayerNorm::new(&k, embed).unwrap();
        let mlp = Mlp::new(&k, &w1, &b1, &w2, &b2, embed, hidden).unwrap();
        let mut block = MlpBlock::new(ln, mlp);
        let x_dev = k.to_device(&x, t, embed).unwrap();

        let n = (t * embed) as f32;
        let mse = |p: &[f32]| {
            p.iter()
                .zip(&target)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                / n
        };
        let mut first = 0.0f32;
        let mut last = 0.0f32;
        for step in 1..=150u32 {
            let (out_dev, cache) = block.forward(&k, &x_dev).unwrap();
            let pred = k.to_host(&out_dev).unwrap();
            let loss = mse(&pred);
            let dout: Vec<f32> = pred
                .iter()
                .zip(&target)
                .map(|(a, b)| 2.0 * (a - b) / n)
                .collect();
            let dout_dev = k.to_device(&dout, t, embed).unwrap();
            block.backward(&k, &x_dev, &cache, &dout_dev).unwrap();
            block.adamw_step(&k, step, 0.02).unwrap();
            if step == 1 {
                first = loss;
            }
            last = loss;
        }
        assert!(
            last < first * 0.3,
            "GPU MLP block should fit the target: first={first} last={last}"
        );
    }

    #[test]
    fn gpu_attention_forward_matches_cpu() {
        let k = GpuKernels::new(0).expect("gpu init");
        let (t, e, h) = (8, 16, 4);
        let x = fill(t * e, 1);
        let (wq, wk, wv, wo) = (
            fill(e * e, 2),
            fill(e * e, 3),
            fill(e * e, 4),
            fill(e * e, 5),
        );
        let attn = Attention::new(&k, e, h, &wq, &wk, &wv, &wo).unwrap();
        let x_dev = k.to_device(&x, t, e).unwrap();
        let (out_dev, _cache) = attn.forward(&k, &x_dev).unwrap();
        assert_close(
            &k.to_host(&out_dev).unwrap(),
            &attention_forward_cpu(&x, t, e, h, &wq, &wk, &wv, &wo),
            2.0e-3,
        );
    }

    #[test]
    fn gpu_attention_backward_gradient_check() {
        // Finite-difference the input gradient through the whole attention
        // (incl. softmax + causal mask) against the analytic GPU backward.
        let k = GpuKernels::new(0).expect("gpu init");
        let (t, e, h) = (5, 8, 2);
        let x = fill(t * e, 1);
        let (wq, wk, wv, wo) = (
            fill(e * e, 2),
            fill(e * e, 3),
            fill(e * e, 4),
            fill(e * e, 5),
        );
        let g = fill(t * e, 6); // cotangent / d_out
        let mut attn = Attention::new(&k, e, h, &wq, &wk, &wv, &wo).unwrap();
        let x_dev = k.to_device(&x, t, e).unwrap();
        let (_out, cache) = attn.forward(&k, &x_dev).unwrap();
        let g_dev = k.to_device(&g, t, e).unwrap();
        let dx = k
            .to_host(&attn.backward(&k, &x_dev, &cache, &g_dev).unwrap())
            .unwrap();

        // loss(x) = Σ out(x) ⊙ g, computed via the CPU forward oracle.
        let loss_of = |xv: &[f32]| -> f32 {
            attention_forward_cpu(xv, t, e, h, &wq, &wk, &wv, &wo)
                .iter()
                .zip(&g)
                .map(|(a, b)| a * b)
                .sum()
        };
        let eps = 2.0e-3f32;
        for idx in 0..(t * e) {
            let mut xp = x.clone();
            xp[idx] += eps;
            let mut xm = x.clone();
            xm[idx] -= eps;
            let num = (loss_of(&xp) - loss_of(&xm)) / (2.0 * eps);
            let denom = dx[idx].abs().max(0.5);
            assert!(
                (num - dx[idx]).abs() / denom < 0.05,
                "dx[{idx}]: numeric={num} analytic={}",
                dx[idx]
            );
        }
    }

    #[test]
    fn gpu_block_trains_end_to_end() {
        // A full pre-norm transformer block (attention + MLP + residuals)
        // trained entirely on the GPU. Requires a correct attention backward.
        let k = GpuKernels::new(0).expect("gpu init");
        let (t, e, h, hidden) = (8, 16, 4, 32);
        let x = fill(t * e, 1);
        let target = fill(t * e, 2);
        let small =
            |seed: u64, n: usize| -> Vec<f32> { fill(n, seed).iter().map(|v| v * 0.1).collect() };
        let attn = Attention::new(
            &k,
            e,
            h,
            &small(3, e * e),
            &small(4, e * e),
            &small(5, e * e),
            &small(6, e * e),
        )
        .unwrap();
        let mlp = Mlp::new(
            &k,
            &small(7, hidden * e),
            &vec![0.0; hidden],
            &small(8, e * hidden),
            &vec![0.0; e],
            e,
            hidden,
        )
        .unwrap();
        let ln1 = LayerNorm::new(&k, e).unwrap();
        let ln2 = LayerNorm::new(&k, e).unwrap();
        let mut block = Block::new(ln1, attn, ln2, mlp);
        let x_dev = k.to_device(&x, t, e).unwrap();

        let n = (t * e) as f32;
        let mse = |p: &[f32]| {
            p.iter()
                .zip(&target)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                / n
        };
        let mut first = 0.0f32;
        let mut last = 0.0f32;
        for step in 1..=200u32 {
            let (out_dev, cache) = block.forward(&k, &x_dev).unwrap();
            let pred = k.to_host(&out_dev).unwrap();
            let loss = mse(&pred);
            let dout: Vec<f32> = pred
                .iter()
                .zip(&target)
                .map(|(a, b)| 2.0 * (a - b) / n)
                .collect();
            let dout_dev = k.to_device(&dout, t, e).unwrap();
            block.backward(&k, &x_dev, &cache, &dout_dev).unwrap();
            block.adamw_step(&k, step, 0.01).unwrap();
            if step == 1 {
                first = loss;
            }
            last = loss;
        }
        assert!(
            last < first * 0.3,
            "full transformer block should fit the target: first={first} last={last}"
        );
    }
}
