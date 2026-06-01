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
    /// AdamW weight decay applied to `w` only (0 = none). Biases are never decayed.
    weight_decay: f32,
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
            weight_decay: 0.0,
        })
    }

    /// Set the AdamW weight decay for this layer's weight matrix `w`.
    pub fn set_weight_decay(&mut self, wd: f32) {
        self.weight_decay = wd;
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
            self.weight_decay,
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

    pub fn set_weight_decay(&mut self, wd: f32) {
        self.fc1.set_weight_decay(wd);
        self.fc2.set_weight_decay(wd);
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
        self.forward_impl(kern, x, None)
    }

    /// Packed-batch forward: attention masked per segment via `seg_ids[row]`.
    pub fn forward_seg(
        &self,
        kern: &GpuKernels,
        x: &DeviceTensor,
        seg_ids: &[i32],
    ) -> Result<(DeviceTensor, AttnCache)> {
        self.forward_impl(kern, x, Some(seg_ids))
    }

    fn forward_impl(
        &self,
        kern: &GpuKernels,
        x: &DeviceTensor,
        seg: Option<&[i32]>,
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
            match seg {
                Some(s) => kern.dev_scale_segmented_causal_mask(&mut scores, s, self.scale)?,
                None => kern.dev_scale_causal_mask(&mut scores, self.scale)?,
            }
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
        self.backward_impl(kern, x, cache, d_out, None)
    }

    /// Packed-batch backward: attention grad masked per segment via `seg_ids[row]`.
    pub fn backward_seg(
        &mut self,
        kern: &GpuKernels,
        x: &DeviceTensor,
        cache: &AttnCache,
        d_out: &DeviceTensor,
        seg_ids: &[i32],
    ) -> Result<DeviceTensor> {
        self.backward_impl(kern, x, cache, d_out, Some(seg_ids))
    }

    fn backward_impl(
        &mut self,
        kern: &GpuKernels,
        x: &DeviceTensor,
        cache: &AttnCache,
        d_out: &DeviceTensor,
        seg: Option<&[i32]>,
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
            match seg {
                Some(s) => {
                    kern.dev_scale_segmented_causal_mask_grad(&mut d_scores, s, self.scale)?
                }
                None => kern.dev_scale_causal_mask_grad(&mut d_scores, self.scale)?,
            }
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

    pub fn set_weight_decay(&mut self, wd: f32) {
        self.q.set_weight_decay(wd);
        self.k.set_weight_decay(wd);
        self.v.set_weight_decay(wd);
        self.o.set_weight_decay(wd);
    }
}

/// A full pre-norm transformer block:
/// `x1 = x + Attn(LN1(x)); out = x1 + MLP(LN2(x1))`.
pub struct Block {
    ln1: LayerNorm,
    attn: Attention,
    ln2: LayerNorm,
    mlp: Mlp,
    /// Residual dropout probability (0 = off). Set via [`Block::set_dropout`].
    dropout_p: f32,
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
    /// Keep-masks for the attention / MLP residual dropout (None when off).
    drop_a: Option<DeviceTensor>,
    drop_m: Option<DeviceTensor>,
}

impl Block {
    pub fn new(ln1: LayerNorm, attn: Attention, ln2: LayerNorm, mlp: Mlp) -> Self {
        Self {
            ln1,
            attn,
            ln2,
            mlp,
            dropout_p: 0.0,
        }
    }

    /// Set residual dropout probability (applied to the attention + MLP outputs).
    pub fn set_dropout(&mut self, p: f32) {
        self.dropout_p = p;
    }

    /// Apply residual dropout to `t` when enabled and `seed != 0` (training);
    /// returns the (possibly unchanged) tensor and the keep-mask for backward.
    fn apply_dropout(
        &self,
        k: &GpuKernels,
        t: DeviceTensor,
        seed: u64,
    ) -> Result<(DeviceTensor, Option<DeviceTensor>)> {
        if self.dropout_p > 0.0 && seed != 0 {
            let (y, mask) = k.dev_dropout(&t, self.dropout_p, seed)?;
            Ok((y, Some(mask)))
        } else {
            Ok((t, None))
        }
    }

    pub fn forward(&self, k: &GpuKernels, x: &DeviceTensor) -> Result<(DeviceTensor, BlockCache)> {
        self.forward_impl(k, x, None, 0)
    }

    /// Packed-batch forward: attention masked per segment via `seg_ids[row]`.
    pub fn forward_seg(
        &self,
        k: &GpuKernels,
        x: &DeviceTensor,
        seg_ids: &[i32],
    ) -> Result<(DeviceTensor, BlockCache)> {
        self.forward_impl(k, x, Some(seg_ids), 0)
    }

    /// `dropout_seed == 0` disables residual dropout (eval); a nonzero seed makes
    /// the per-sublayer masks reproducible while differing across training steps.
    fn forward_impl(
        &self,
        k: &GpuKernels,
        x: &DeviceTensor,
        seg: Option<&[i32]>,
        dropout_seed: u64,
    ) -> Result<(DeviceTensor, BlockCache)> {
        let (ln1_y, m1, r1) = self.ln1.forward(k, x)?;
        let (attn_out, attn_cache) = self.attn.forward_impl(k, &ln1_y, seg)?;
        let (attn_out, drop_a) = self.apply_dropout(k, attn_out, dropout_seed)?;
        let mut x1 = attn_out;
        k.dev_add_inplace(&mut x1, x)?; // x1 = x + dropout(Attn(LN1(x)))
        let (ln2_y, m2, r2) = self.ln2.forward(k, &x1)?;
        let (mlp_out, h1, act) = self.mlp.forward(k, &ln2_y)?;
        let mlp_seed = if dropout_seed == 0 {
            0
        } else {
            dropout_seed.wrapping_add(0x9E37_79B9_7F4A_7C15)
        };
        let (mlp_out, drop_m) = self.apply_dropout(k, mlp_out, mlp_seed)?;
        let mut out = mlp_out;
        k.dev_add_inplace(&mut out, &x1)?; // out = x1 + dropout(MLP(LN2(x1)))
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
                drop_a,
                drop_m,
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
        self.backward_impl(k, x, c, d_out, None)
    }

    /// Packed-batch backward: attention grad masked per segment via `seg_ids[row]`.
    pub fn backward_seg(
        &mut self,
        k: &GpuKernels,
        x: &DeviceTensor,
        c: &BlockCache,
        d_out: &DeviceTensor,
        seg_ids: &[i32],
    ) -> Result<DeviceTensor> {
        self.backward_impl(k, x, c, d_out, Some(seg_ids))
    }

    fn backward_impl(
        &mut self,
        k: &GpuKernels,
        x: &DeviceTensor,
        c: &BlockCache,
        d_out: &DeviceTensor,
        seg: Option<&[i32]>,
    ) -> Result<DeviceTensor> {
        // out = x1 + dropout(MLP(LN2(x1)))
        let d_ln2_y = if let Some(mask) = &c.drop_m {
            let d_mlp = k.dev_dropout_backward(d_out, mask)?;
            self.mlp.backward(k, &c.ln2_y, &c.h1, &c.act, &d_mlp)?
        } else {
            self.mlp.backward(k, &c.ln2_y, &c.h1, &c.act, d_out)?
        };
        let mut d_x1 = self.ln2.backward(k, &c.x1, &d_ln2_y, &c.m2, &c.r2)?;
        k.dev_add_inplace(&mut d_x1, d_out)?; // residual through x1
                                              // x1 = x + dropout(Attn(LN1(x)))
        let d_ln1_y = if let Some(mask) = &c.drop_a {
            let d_attn = k.dev_dropout_backward(&d_x1, mask)?;
            self.attn
                .backward_impl(k, &c.ln1_y, &c.attn_cache, &d_attn, seg)?
        } else {
            self.attn
                .backward_impl(k, &c.ln1_y, &c.attn_cache, &d_x1, seg)?
        };
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

    /// Weight decay on attention + MLP weight matrices (not the LayerNorms).
    pub fn set_weight_decay(&mut self, wd: f32) {
        self.attn.set_weight_decay(wd);
        self.mlp.set_weight_decay(wd);
    }
}

/// Deterministic seeded `Normal(0, std)` init (SplitMix64 + Box-Muller).
fn seeded_normal(n: usize, seed: u64, std: f32) -> Vec<f32> {
    let mut s = seed;
    let mut next = || {
        s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f32 / ((1u64 << 53) as f32)
    };
    (0..n)
        .map(|_| {
            let u1 = next().max(1.0e-7);
            let u2 = next();
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos() * std
        })
        .collect()
}

/// A device-resident decoder-only transformer (GPT): trainable token + position
/// embeddings → N pre-norm [`Block`]s → final LayerNorm → LM head. Forward,
/// backward, cross-entropy, and AdamW all run on the GPU.
pub struct GptModel {
    tok_emb: DeviceTensor,
    pos_emb: DeviceTensor,
    d_tok: DeviceTensor,
    d_pos: DeviceTensor,
    mt: DeviceTensor,
    vt: DeviceTensor,
    mp: DeviceTensor,
    vp: DeviceTensor,
    blocks: Vec<Block>,
    ln_f: LayerNorm,
    lm_head: Linear,
    vocab: usize,
    context: usize,
    /// Label-smoothing ε used by the training cross-entropy (0 = none).
    label_smoothing: f32,
    /// Residual dropout probability used during training (0 = none).
    dropout_p: f32,
}

/// Forward activations the [`GptModel`] backward needs.
pub struct ModelCache {
    xs: Vec<DeviceTensor>,
    caches: Vec<BlockCache>,
    lnf_y: DeviceTensor,
    m: DeviceTensor,
    r: DeviceTensor,
}

impl GptModel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        k: &GpuKernels,
        vocab: usize,
        embed: usize,
        n_head: usize,
        n_layers: usize,
        hidden: usize,
        context: usize,
        seed: u64,
    ) -> Result<Self> {
        let std = 0.02f32;
        let blocks = (0..n_layers)
            .map(|i| -> Result<Block> {
                let s = seed.wrapping_add(i as u64 * 1000 + 1);
                let attn = Attention::new(
                    k,
                    embed,
                    n_head,
                    &seeded_normal(embed * embed, s + 1, std),
                    &seeded_normal(embed * embed, s + 2, std),
                    &seeded_normal(embed * embed, s + 3, std),
                    &seeded_normal(embed * embed, s + 4, std),
                )?;
                let mlp = Mlp::new(
                    k,
                    &seeded_normal(hidden * embed, s + 5, std),
                    &vec![0.0; hidden],
                    &seeded_normal(embed * hidden, s + 6, std),
                    &vec![0.0; embed],
                    embed,
                    hidden,
                )?;
                Ok(Block::new(
                    LayerNorm::new(k, embed)?,
                    attn,
                    LayerNorm::new(k, embed)?,
                    mlp,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            tok_emb: k.to_device(
                &seeded_normal(vocab * embed, seed.wrapping_mul(13) + 1, std),
                vocab,
                embed,
            )?,
            pos_emb: k.to_device(
                &seeded_normal(context * embed, seed.wrapping_mul(13) + 2, std),
                context,
                embed,
            )?,
            d_tok: k.zeros_device(vocab, embed)?,
            d_pos: k.zeros_device(context, embed)?,
            mt: k.zeros_device(vocab, embed)?,
            vt: k.zeros_device(vocab, embed)?,
            mp: k.zeros_device(context, embed)?,
            vp: k.zeros_device(context, embed)?,
            blocks,
            ln_f: LayerNorm::new(k, embed)?,
            lm_head: Linear::new(
                k,
                &seeded_normal(vocab * embed, seed.wrapping_mul(13) + 3, std),
                &vec![0.0; vocab],
                embed,
                vocab,
            )?,
            vocab,
            context,
            label_smoothing: 0.0,
            dropout_p: 0.0,
        })
    }

    pub fn forward(&self, k: &GpuKernels, tokens: &[i32]) -> Result<(DeviceTensor, ModelCache)> {
        self.forward_seeded(k, tokens, 0)
    }

    fn forward_seeded(
        &self,
        k: &GpuKernels,
        tokens: &[i32],
        dropout_seed: u64,
    ) -> Result<(DeviceTensor, ModelCache)> {
        anyhow::ensure!(tokens.len() <= self.context, "sequence longer than context");
        let x0 = k.dev_embedding_forward(&self.tok_emb, &self.pos_emb, tokens)?;
        let mut xs = vec![x0];
        let mut caches = Vec::with_capacity(self.blocks.len());
        for (i, block) in self.blocks.iter().enumerate() {
            let (y, c) =
                block.forward_impl(k, xs.last().unwrap(), None, block_seed(dropout_seed, i))?;
            caches.push(c);
            xs.push(y);
        }
        let (lnf_y, m, r) = self.ln_f.forward(k, xs.last().unwrap())?;
        let logits = self.lm_head.forward(k, &lnf_y)?;
        Ok((
            logits,
            ModelCache {
                xs,
                caches,
                lnf_y,
                m,
                r,
            },
        ))
    }

    /// Packed mini-batch forward over several sequences in one buffer.
    /// `seg_ids[row]` is the sequence index (for segmented causal attention) and
    /// `pos_ids[row]` the within-sequence position (for the position embedding).
    pub fn forward_packed(
        &self,
        k: &GpuKernels,
        tokens: &[i32],
        seg_ids: &[i32],
        pos_ids: &[i32],
    ) -> Result<(DeviceTensor, ModelCache)> {
        self.forward_packed_seeded(k, tokens, seg_ids, pos_ids, 0)
    }

    fn forward_packed_seeded(
        &self,
        k: &GpuKernels,
        tokens: &[i32],
        seg_ids: &[i32],
        pos_ids: &[i32],
        dropout_seed: u64,
    ) -> Result<(DeviceTensor, ModelCache)> {
        let t = tokens.len();
        anyhow::ensure!(
            seg_ids.len() == t && pos_ids.len() == t,
            "packed layout length mismatch"
        );
        anyhow::ensure!(
            pos_ids.iter().all(|&p| (p as usize) < self.context),
            "packed position exceeds context"
        );
        let x0 = k.dev_embedding_forward_packed(&self.tok_emb, &self.pos_emb, tokens, pos_ids)?;
        let mut xs = vec![x0];
        let mut caches = Vec::with_capacity(self.blocks.len());
        for (i, block) in self.blocks.iter().enumerate() {
            let (y, c) = block.forward_impl(
                k,
                xs.last().unwrap(),
                Some(seg_ids),
                block_seed(dropout_seed, i),
            )?;
            caches.push(c);
            xs.push(y);
        }
        let (lnf_y, m, r) = self.ln_f.forward(k, xs.last().unwrap())?;
        let logits = self.lm_head.forward(k, &lnf_y)?;
        Ok((
            logits,
            ModelCache {
                xs,
                caches,
                lnf_y,
                m,
                r,
            },
        ))
    }

    pub fn backward(
        &mut self,
        k: &GpuKernels,
        tokens: &[i32],
        cache: &ModelCache,
        d_logits: &DeviceTensor,
    ) -> Result<()> {
        let d_lnf_y = self.lm_head.backward(k, &cache.lnf_y, d_logits)?;
        let n = self.blocks.len();
        let mut d_x = self
            .ln_f
            .backward(k, &cache.xs[n], &d_lnf_y, &cache.m, &cache.r)?;
        for i in (0..n).rev() {
            d_x = self.blocks[i].backward(k, &cache.xs[i], &cache.caches[i], &d_x)?;
        }
        let (d_tok, d_pos) = k.dev_embedding_backward(&d_x, tokens, self.vocab, self.context)?;
        self.d_tok = d_tok;
        self.d_pos = d_pos;
        Ok(())
    }

    /// Packed mini-batch backward (segmented attention via `seg_ids`, packed
    /// position grads via `pos_ids`).
    pub fn backward_packed(
        &mut self,
        k: &GpuKernels,
        tokens: &[i32],
        seg_ids: &[i32],
        pos_ids: &[i32],
        cache: &ModelCache,
        d_logits: &DeviceTensor,
    ) -> Result<()> {
        let d_lnf_y = self.lm_head.backward(k, &cache.lnf_y, d_logits)?;
        let n = self.blocks.len();
        let mut d_x = self
            .ln_f
            .backward(k, &cache.xs[n], &d_lnf_y, &cache.m, &cache.r)?;
        for i in (0..n).rev() {
            d_x = self.blocks[i].backward_seg(k, &cache.xs[i], &cache.caches[i], &d_x, seg_ids)?;
        }
        let (d_tok, d_pos) =
            k.dev_embedding_backward_packed(&d_x, tokens, pos_ids, self.vocab, self.context)?;
        self.d_tok = d_tok;
        self.d_pos = d_pos;
        Ok(())
    }

    pub fn adamw_step(&mut self, k: &GpuKernels, t: u32, lr: f32) -> Result<()> {
        k.dev_adamw(
            &mut self.tok_emb,
            &self.d_tok,
            &mut self.mt,
            &mut self.vt,
            t,
            lr,
            0.9,
            0.999,
            1.0e-8,
            0.0,
        )?;
        k.dev_adamw(
            &mut self.pos_emb,
            &self.d_pos,
            &mut self.mp,
            &mut self.vp,
            t,
            lr,
            0.9,
            0.999,
            1.0e-8,
            0.0,
        )?;
        for block in &mut self.blocks {
            block.adamw_step(k, t, lr)?;
        }
        self.ln_f.adamw_step(k, t, lr)?;
        self.lm_head.adamw_step(k, t, lr)?;
        Ok(())
    }

    /// Set AdamW weight decay on the matmul weights (attention / MLP / LM head).
    /// Token + position embeddings, LayerNorm, and biases are left undecayed,
    /// which is standard practice for transformer training.
    pub fn set_weight_decay(&mut self, wd: f32) {
        for block in &mut self.blocks {
            block.set_weight_decay(wd);
        }
        self.lm_head.set_weight_decay(wd);
    }

    /// Set the label-smoothing ε for the training cross-entropy (0 = none).
    /// Held-out [`evaluate`](Self::evaluate) always uses the unsmoothed loss.
    pub fn set_label_smoothing(&mut self, eps: f32) {
        self.label_smoothing = eps;
    }

    /// Set the residual dropout probability applied inside every block during
    /// training. Held-out eval and the public `forward`/`forward_packed` never
    /// apply dropout (they pass a zero seed).
    pub fn set_dropout(&mut self, p: f32) {
        self.dropout_p = p;
        for block in &mut self.blocks {
            block.set_dropout(p);
        }
    }

    /// Count active next-token targets (mask set, in-vocab) in a sequence.
    fn count_targets(&self, tokens: &[i32], loss_mask: &[i32]) -> usize {
        (0..tokens.len().saturating_sub(1))
            .filter(|&i| {
                loss_mask[i + 1] != 0 && tokens[i + 1] >= 0 && (tokens[i + 1] as usize) < self.vocab
            })
            .count()
    }

    /// One training step on one sequence → `(mean_loss, accuracy)`.
    pub fn train_step(
        &mut self,
        k: &GpuKernels,
        tokens: &[i32],
        loss_mask: &[i32],
        t: u32,
        lr: f32,
    ) -> Result<(f32, f32)> {
        let (logits, cache) = self.forward_seeded(k, tokens, t as u64)?;
        let count = self.count_targets(tokens, loss_mask);
        let inv = if count > 0 { 1.0 / count as f32 } else { 0.0 };
        let (d_logits, losses, correct) =
            k.dev_cross_entropy(&logits, tokens, loss_mask, inv, self.label_smoothing)?;
        self.backward(k, tokens, &cache, &d_logits)?;
        self.adamw_step(k, t, lr)?;
        Ok(mean_loss_acc(&losses, &correct, count))
    }

    /// Forward-only loss/accuracy on one sequence (no weight update) — held-out eval.
    pub fn evaluate(
        &self,
        k: &GpuKernels,
        tokens: &[i32],
        loss_mask: &[i32],
    ) -> Result<(f32, f32)> {
        let (logits, _cache) = self.forward(k, tokens)?;
        let count = self.count_targets(tokens, loss_mask);
        let inv = if count > 0 { 1.0 / count as f32 } else { 0.0 };
        let (_d, losses, correct) = k.dev_cross_entropy(&logits, tokens, loss_mask, inv, 0.0)?;
        Ok(mean_loss_acc(&losses, &correct, count))
    }

    /// One packed mini-batch training step → `(mean_loss, accuracy)` over all
    /// supervised targets in the batch. `seg_ids`/`pos_ids` come from
    /// [`packed_layout`]; `loss_mask` is the packed per-token supervision mask.
    #[allow(clippy::too_many_arguments)]
    pub fn train_step_packed(
        &mut self,
        k: &GpuKernels,
        tokens: &[i32],
        seg_ids: &[i32],
        pos_ids: &[i32],
        loss_mask: &[i32],
        t: u32,
        lr: f32,
    ) -> Result<(f32, f32)> {
        let (logits, cache) = self.forward_packed_seeded(k, tokens, seg_ids, pos_ids, t as u64)?;
        let count = self.count_targets(tokens, loss_mask);
        let inv = if count > 0 { 1.0 / count as f32 } else { 0.0 };
        let (d_logits, losses, correct) =
            k.dev_cross_entropy(&logits, tokens, loss_mask, inv, self.label_smoothing)?;
        self.backward_packed(k, tokens, seg_ids, pos_ids, &cache, &d_logits)?;
        self.adamw_step(k, t, lr)?;
        Ok(mean_loss_acc(&losses, &correct, count))
    }

    /// Forward-only packed eval (no weight update) → `(mean_loss, accuracy)`.
    pub fn evaluate_packed(
        &self,
        k: &GpuKernels,
        tokens: &[i32],
        seg_ids: &[i32],
        pos_ids: &[i32],
        loss_mask: &[i32],
    ) -> Result<(f32, f32)> {
        let (logits, _cache) = self.forward_packed(k, tokens, seg_ids, pos_ids)?;
        let count = self.count_targets(tokens, loss_mask);
        let inv = if count > 0 { 1.0 / count as f32 } else { 0.0 };
        let (_d, losses, correct) = k.dev_cross_entropy(&logits, tokens, loss_mask, inv, 0.0)?;
        Ok(mean_loss_acc(&losses, &correct, count))
    }
}

/// Per-block dropout seed derived from a base seed; `base == 0` (eval) keeps the
/// result 0 so dropout stays disabled. Each block gets a distinct nonzero seed.
fn block_seed(base: u64, block_index: usize) -> u64 {
    if base == 0 {
        0
    } else {
        base.wrapping_add((block_index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }
}

/// Build `(seg_ids, pos_ids)` for a packed batch from per-sequence lengths:
/// `seg_ids[row]` = sequence index, `pos_ids[row]` = within-sequence position.
pub fn packed_layout(lengths: &[usize]) -> (Vec<i32>, Vec<i32>) {
    let total: usize = lengths.iter().sum();
    let mut seg = Vec::with_capacity(total);
    let mut pos = Vec::with_capacity(total);
    for (s, &len) in lengths.iter().enumerate() {
        for p in 0..len {
            seg.push(s as i32);
            pos.push(p as i32);
        }
    }
    (seg, pos)
}

/// Mean loss + accuracy from per-row cross-entropy outputs over `count` targets.
fn mean_loss_acc(losses: &[f32], correct: &[i32], count: usize) -> (f32, f32) {
    if count == 0 {
        return (0.0, 0.0);
    }
    (
        losses.iter().sum::<f32>() / count as f32,
        correct.iter().sum::<i32>() as f32 / count as f32,
    )
}

// ─── CPU references for the composed layers (parity oracle) ───

/// CPU embedding forward (mirrors `embedding_forward`).
#[cfg(test)]
fn embedding_forward_cpu(tok_emb: &[f32], pos_emb: &[f32], tokens: &[i32], e: usize) -> Vec<f32> {
    let t = tokens.len();
    let mut x = vec![0.0f32; t * e];
    for i in 0..t {
        for d in 0..e {
            x[i * e + d] = tok_emb[tokens[i] as usize * e + d] + pos_emb[i * e + d];
        }
    }
    x
}

/// CPU softmax cross-entropy (mirrors `softmax_cross_entropy`).
#[cfg(test)]
fn cross_entropy_cpu(
    logits: &[f32],
    tokens: &[i32],
    loss_mask: &[i32],
    inv: f32,
    smoothing: f32,
    t: usize,
    v: usize,
) -> (Vec<f32>, Vec<f32>, Vec<i32>) {
    let mut d = vec![0.0f32; t * v];
    let mut loss = vec![0.0f32; t];
    let mut correct = vec![0i32; t];
    let eps_v = smoothing / v as f32;
    let q_target = (1.0 - smoothing) + eps_v;
    for i in 0..t.saturating_sub(1) {
        let target = tokens[i + 1];
        if loss_mask[i + 1] == 0 || target < 0 || target as usize >= v {
            continue;
        }
        let target = target as usize;
        let probs = crate::softmax_cpu(&logits[i * v..i * v + v], 1, v);
        for j in 0..v {
            let q = if j == target { q_target } else { eps_v };
            d[i * v + j] = inv * (probs[j] - q);
        }
        loss[i] = -probs[target].max(1.0e-12).ln();
        let am = probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|x| x.0)
            .unwrap();
        correct[i] = i32::from(am == target);
    }
    (d, loss, correct)
}

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

    #[test]
    fn gpu_embedding_matches_cpu() {
        let k = GpuKernels::new(0).expect("gpu init");
        let (vocab, e, t) = (20usize, 16usize, 10usize);
        let tok_emb = fill(vocab * e, 1);
        let pos_emb = fill(t * e, 2);
        let tokens: Vec<i32> = (0..t).map(|i| ((i * 7 + 3) % vocab) as i32).collect();
        let te = k.to_device(&tok_emb, vocab, e).unwrap();
        let pe = k.to_device(&pos_emb, t, e).unwrap();
        let x = k
            .to_host(&k.dev_embedding_forward(&te, &pe, &tokens).unwrap())
            .unwrap();
        assert_close(
            &x,
            &embedding_forward_cpu(&tok_emb, &pos_emb, &tokens, e),
            1.0e-5,
        );

        let dx = fill(t * e, 3);
        let dx_dev = k.to_device(&dx, t, e).unwrap();
        let (d_tok, d_pos) = k
            .dev_embedding_backward(&dx_dev, &tokens, vocab, t)
            .unwrap();
        let mut dt_cpu = vec![0.0f32; vocab * e];
        let mut dp_cpu = vec![0.0f32; t * e];
        for i in 0..t {
            for d in 0..e {
                dt_cpu[tokens[i] as usize * e + d] += dx[i * e + d];
                dp_cpu[i * e + d] += dx[i * e + d];
            }
        }
        assert_close(&k.to_host(&d_tok).unwrap(), &dt_cpu, 1.0e-3);
        assert_close(&k.to_host(&d_pos).unwrap(), &dp_cpu, 1.0e-5);
    }

    #[test]
    fn gpu_cross_entropy_matches_cpu() {
        let k = GpuKernels::new(0).expect("gpu init");
        let (t, v) = (12usize, 30usize);
        let logits = fill(t * v, 1);
        let tokens: Vec<i32> = (0..t).map(|i| ((i * 5 + 2) % v) as i32).collect();
        let loss_mask: Vec<i32> = (0..t).map(|i| i32::from(i % 3 != 0)).collect();
        let count = (1..t).filter(|&i| loss_mask[i] != 0).count();
        let inv = 1.0 / count as f32;
        let logits_dev = k.to_device(&logits, t, v).unwrap();
        // parity at both no smoothing and label-smoothing ε = 0.1
        for smoothing in [0.0f32, 0.1] {
            let (d_dev, loss, correct) = k
                .dev_cross_entropy(&logits_dev, &tokens, &loss_mask, inv, smoothing)
                .unwrap();
            let (d_cpu, loss_cpu, correct_cpu) =
                cross_entropy_cpu(&logits, &tokens, &loss_mask, inv, smoothing, t, v);
            assert_close(&k.to_host(&d_dev).unwrap(), &d_cpu, 1.0e-4);
            assert_close(&loss, &loss_cpu, 1.0e-4);
            assert_eq!(correct, correct_cpu);
        }
    }

    #[test]
    fn gpu_gpt_model_trains_end_to_end() {
        // A full GPT (embeddings → 2 blocks → final LN → LM head + cross-entropy)
        // trained entirely on the GPU learns a periodic next-token pattern.
        let k = GpuKernels::new(0).expect("gpu init");
        let (vocab, embed, n_head, n_layers, hidden, context) = (24, 32, 4, 2, 64, 12);
        let mut model =
            GptModel::new(&k, vocab, embed, n_head, n_layers, hidden, context, 7).unwrap();
        let tokens: Vec<i32> = (0..context).map(|i| ((i % 5) + 2) as i32).collect();
        let loss_mask: Vec<i32> = vec![1; context];
        let mut first = 0.0f32;
        let mut last = 0.0f32;
        let mut last_acc = 0.0f32;
        for step in 1..=80u32 {
            let (loss, acc) = model
                .train_step(&k, &tokens, &loss_mask, step, 0.01)
                .unwrap();
            if step == 1 {
                first = loss;
            }
            last = loss;
            last_acc = acc;
        }
        assert!(
            last < first * 0.3,
            "GPT model should learn the pattern: first={first} last={last}"
        );
        assert!(
            last_acc > 0.7,
            "next-token accuracy should rise: {last_acc}"
        );
    }

    #[test]
    fn gpu_model_trains_with_dropout() {
        // With residual dropout enabled, the masks must flow correctly through
        // forward AND backward — the model still learns the pattern (looser bound
        // since dropout injects noise), and held-out (dropout-off) eval works.
        let k = GpuKernels::new(0).expect("gpu init");
        let (vocab, embed, n_head, n_layers, hidden, context) = (24, 32, 4, 2, 64, 12);
        let mut model =
            GptModel::new(&k, vocab, embed, n_head, n_layers, hidden, context, 7).unwrap();
        model.set_dropout(0.1);
        let tokens: Vec<i32> = (0..context).map(|i| ((i % 5) + 2) as i32).collect();
        let loss_mask: Vec<i32> = vec![1; context];
        let mut first = 0.0f32;
        let mut last = 0.0f32;
        for step in 1..=120u32 {
            let (loss, _acc) = model
                .train_step(&k, &tokens, &loss_mask, step, 0.01)
                .unwrap();
            if step == 1 {
                first = loss;
            }
            last = loss;
        }
        assert!(
            last < first * 0.6,
            "model should still learn with dropout: first={first} last={last}"
        );
        // eval runs with dropout off (zero seed) and produces a finite loss
        let (eval_loss, eval_acc) = model.evaluate(&k, &tokens, &loss_mask).unwrap();
        assert!(
            eval_loss.is_finite() && eval_acc >= 0.0,
            "eval must run dropout-off"
        );
    }

    #[test]
    fn gpu_packed_forward_matches_sequential() {
        // Packing several sequences into one buffer (segmented attention + packed
        // positions) must produce the SAME per-token logits as forwarding each
        // sequence on its own — segments are independent. This is the M7 gate.
        let k = GpuKernels::new(0).expect("gpu init");
        let (vocab, embed, n_head, n_layers, hidden, context) = (20, 32, 4, 2, 64, 16);
        let model = GptModel::new(&k, vocab, embed, n_head, n_layers, hidden, context, 11).unwrap();
        let seqs: Vec<Vec<i32>> = vec![vec![3, 7, 1, 9, 4], vec![2, 8, 5], vec![6, 0, 1, 1, 7, 3]];

        // standalone forwards (one sequence at a time)
        let want: Vec<Vec<f32>> = seqs
            .iter()
            .map(|s| k.to_host(&model.forward(&k, s).unwrap().0).unwrap())
            .collect();

        // packed forward (all sequences in one buffer)
        let tokens: Vec<i32> = seqs.iter().flatten().copied().collect();
        let lengths: Vec<usize> = seqs.iter().map(Vec::len).collect();
        let (seg, pos) = packed_layout(&lengths);
        let got = k
            .to_host(&model.forward_packed(&k, &tokens, &seg, &pos).unwrap().0)
            .unwrap();

        let mut off = 0;
        for (si, s) in seqs.iter().enumerate() {
            let rows = s.len();
            assert_close(&got[off * vocab..(off + rows) * vocab], &want[si], 3.0e-3);
            off += rows;
        }
    }

    #[test]
    fn gpu_packed_model_trains_end_to_end() {
        // A packed mini-batch (segmented attention + packed positions in BOTH
        // forward and backward) trains end-to-end: loss drops, accuracy rises.
        let k = GpuKernels::new(0).expect("gpu init");
        let (vocab, embed, n_head, n_layers, hidden, context) = (24, 32, 4, 2, 64, 10);
        let mut model =
            GptModel::new(&k, vocab, embed, n_head, n_layers, hidden, context, 5).unwrap();
        // four phase-shifted period-5 sequences, prompt-first masks
        let (mut tokens, mut loss_mask, mut lengths) = (Vec::new(), Vec::new(), Vec::new());
        for sidx in 0..4usize {
            for i in 0..context {
                tokens.push((((i + sidx) % 5) + 2) as i32);
                loss_mask.push(i32::from(i >= 2)); // first 2 tokens = prompt
            }
            lengths.push(context);
        }
        let (seg, pos) = packed_layout(&lengths);
        let mut first = 0.0f32;
        let mut last = 0.0f32;
        let mut last_acc = 0.0f32;
        for step in 1..=80u32 {
            let (loss, acc) = model
                .train_step_packed(&k, &tokens, &seg, &pos, &loss_mask, step, 0.01)
                .unwrap();
            if step == 1 {
                first = loss;
            }
            last = loss;
            last_acc = acc;
        }
        assert!(
            last < first * 0.4,
            "packed batch should learn: first={first} last={last}"
        );
        assert!(
            last_acc > 0.7,
            "packed next-token accuracy should rise: {last_acc}"
        );
    }
}
