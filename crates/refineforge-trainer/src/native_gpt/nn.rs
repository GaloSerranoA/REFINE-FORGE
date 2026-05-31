//! From-scratch GPT building blocks: seeded RNG, dense f64 matrices, and every
//! layer (Embedding handled in `mod.rs`, here: Linear, LayerNorm, GELU,
//! multi-head causal self-attention, MLP, Block) with an explicit `forward`
//! and an explicit analytic `backward`. AdamW lives here too.
//!
//! Correctness is not assumed: the unit tests at the bottom finite-difference
//! every backward against the forward. A transformer with wrong gradients is
//! silently worthless, so these checks are the load-bearing honesty gate.
//!
//! Everything is deterministic (seeded init, fixed iteration order, f64) so a
//! run reproduces the same `weights_sha256`.

/// SplitMix64 — a small, fast, deterministic PRNG for reproducible weight init.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1) using the top 53 bits.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    /// Standard normal via Box-Muller (deterministic given the seed).
    pub fn next_gaussian(&mut self) -> f64 {
        let u1 = self.next_f64().max(1.0e-12);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// Dense row-major f64 matrix (`rows` x `cols`).
#[derive(Debug, Clone, PartialEq)]
pub struct Mat {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
}

impl Mat {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    pub fn from_fn(rows: usize, cols: usize, mut f: impl FnMut() -> f64) -> Self {
        let data = (0..rows * cols).map(|_| f()).collect();
        Self { rows, cols, data }
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> f64 {
        self.data[r * self.cols + c]
    }

    #[inline]
    pub fn set(&mut self, r: usize, c: usize, v: f64) {
        self.data[r * self.cols + c] = v;
    }

    #[inline]
    pub fn row(&self, r: usize) -> &[f64] {
        &self.data[r * self.cols..(r + 1) * self.cols]
    }

    pub fn fill_zero(&mut self) {
        self.data.iter_mut().for_each(|v| *v = 0.0);
    }
}

/// A fully-connected layer `y = x · Wᵀ + b`.
/// `w` is `out_dim x in_dim`; `x` is `T x in_dim`; `y` is `T x out_dim`.
#[derive(Debug, Clone)]
pub struct Linear {
    pub w: Mat,
    pub b: Vec<f64>,
    pub dw: Mat,
    pub db: Vec<f64>,
}

impl Linear {
    pub fn new(rng: &mut SplitMix64, in_dim: usize, out_dim: usize, std: f64) -> Self {
        let w = Mat::from_fn(out_dim, in_dim, || rng.next_gaussian() * std);
        Self {
            w,
            b: vec![0.0; out_dim],
            dw: Mat::zeros(out_dim, in_dim),
            db: vec![0.0; out_dim],
        }
    }

    pub fn forward(&self, x: &Mat) -> Mat {
        let t = x.rows;
        let out_dim = self.b.len();
        let in_dim = self.w.cols;
        let mut y = Mat::zeros(t, out_dim);
        for ti in 0..t {
            let xr = x.row(ti);
            for o in 0..out_dim {
                let wr = self.w.row(o);
                let mut acc = self.b[o];
                for i in 0..in_dim {
                    acc += xr[i] * wr[i];
                }
                y.set(ti, o, acc);
            }
        }
        y
    }

    /// Accumulates dw/db; returns dx (`T x in_dim`).
    pub fn backward(&mut self, x: &Mat, dy: &Mat) -> Mat {
        let t = x.rows;
        let out_dim = self.b.len();
        let in_dim = self.w.cols;
        let mut dx = Mat::zeros(t, in_dim);
        for ti in 0..t {
            let xr = x.row(ti);
            for o in 0..out_dim {
                let g = dy.get(ti, o);
                if g == 0.0 {
                    continue;
                }
                self.db[o] += g;
                let wr = self.w.row(o);
                let dwr = &mut self.dw.data[o * in_dim..(o + 1) * in_dim];
                let dxr = &mut dx.data[ti * in_dim..(ti + 1) * in_dim];
                for i in 0..in_dim {
                    dwr[i] += g * xr[i];
                    dxr[i] += g * wr[i];
                }
            }
        }
        dx
    }

    pub fn zero_grad(&mut self) {
        self.dw.fill_zero();
        self.db.iter_mut().for_each(|v| *v = 0.0);
    }
}

/// Per-row LayerNorm over the feature dimension.
#[derive(Debug, Clone)]
pub struct LayerNorm {
    pub gamma: Vec<f64>,
    pub beta: Vec<f64>,
    pub dgamma: Vec<f64>,
    pub dbeta: Vec<f64>,
    eps: f64,
}

/// Cached forward state needed by LayerNorm backward.
pub struct LnCache {
    xhat: Mat,
    rstd: Vec<f64>,
}

impl LayerNorm {
    pub fn new(dim: usize) -> Self {
        Self {
            gamma: vec![1.0; dim],
            beta: vec![0.0; dim],
            dgamma: vec![0.0; dim],
            dbeta: vec![0.0; dim],
            eps: 1.0e-5,
        }
    }

    #[allow(clippy::needless_range_loop)] // index loops read clearer for numeric kernels
    pub fn forward(&self, x: &Mat) -> (Mat, LnCache) {
        let t = x.rows;
        let e = x.cols;
        let mut y = Mat::zeros(t, e);
        let mut xhat = Mat::zeros(t, e);
        let mut rstd = vec![0.0; t];
        for ti in 0..t {
            let xr = x.row(ti);
            let mean = xr.iter().sum::<f64>() / e as f64;
            let var = xr.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / e as f64;
            let rs = 1.0 / (var + self.eps).sqrt();
            rstd[ti] = rs;
            for j in 0..e {
                let xh = (xr[j] - mean) * rs;
                xhat.set(ti, j, xh);
                y.set(ti, j, self.gamma[j] * xh + self.beta[j]);
            }
        }
        (y, LnCache { xhat, rstd })
    }

    /// Accumulates dgamma/dbeta; returns dx.
    pub fn backward(&mut self, cache: &LnCache, dy: &Mat) -> Mat {
        let t = dy.rows;
        let e = dy.cols;
        let mut dx = Mat::zeros(t, e);
        for ti in 0..t {
            let xhat = cache.xhat.row(ti);
            let rs = cache.rstd[ti];
            // dxhat = dy * gamma; accumulate param grads.
            let mut dxhat = vec![0.0; e];
            let mut mean_dxhat = 0.0;
            let mut mean_dxhat_xhat = 0.0;
            for j in 0..e {
                let g = dy.get(ti, j);
                self.dgamma[j] += g * xhat[j];
                self.dbeta[j] += g;
                dxhat[j] = g * self.gamma[j];
                mean_dxhat += dxhat[j];
                mean_dxhat_xhat += dxhat[j] * xhat[j];
            }
            mean_dxhat /= e as f64;
            mean_dxhat_xhat /= e as f64;
            for j in 0..e {
                dx.set(
                    ti,
                    j,
                    rs * (dxhat[j] - mean_dxhat - xhat[j] * mean_dxhat_xhat),
                );
            }
        }
        dx
    }

    pub fn zero_grad(&mut self) {
        self.dgamma.iter_mut().for_each(|v| *v = 0.0);
        self.dbeta.iter_mut().for_each(|v| *v = 0.0);
    }
}

/// Tanh-approximation GELU (the common GPT variant).
pub fn gelu(x: f64) -> f64 {
    const C: f64 = 0.797_884_560_802_865_4; // sqrt(2/pi)
    let inner = C * (x + 0.044715 * x * x * x);
    0.5 * x * (1.0 + inner.tanh())
}

/// d/dx of [`gelu`].
pub fn gelu_grad(x: f64) -> f64 {
    const C: f64 = 0.797_884_560_802_865_4;
    let x3 = x * x * x;
    let inner = C * (x + 0.044715 * x3);
    let tanh = inner.tanh();
    let dinner = C * (1.0 + 3.0 * 0.044715 * x * x);
    0.5 * (1.0 + tanh) + 0.5 * x * (1.0 - tanh * tanh) * dinner
}

/// Multi-head causal self-attention.
#[derive(Debug, Clone)]
pub struct MultiHeadAttention {
    pub wq: Linear,
    pub wk: Linear,
    pub wv: Linear,
    pub wo: Linear,
    pub n_head: usize,
    pub head_dim: usize,
    scale: f64,
}

/// Cached forward state for attention backward.
pub struct AttnCache {
    ln_in: Mat,          // input to q/k/v projections (T x E)
    q: Mat,              // T x E
    k: Mat,              // T x E
    v: Mat,              // T x E
    ctx: Mat,            // T x E (concat heads), input to wo
    attn: Vec<Vec<f64>>, // per (head*T + i) row: softmax weights over j<=i, length i+1
}

impl MultiHeadAttention {
    pub fn new(rng: &mut SplitMix64, embed: usize, n_head: usize, std: f64) -> Self {
        assert!(embed.is_multiple_of(n_head), "embed must divide n_head");
        let head_dim = embed / n_head;
        Self {
            wq: Linear::new(rng, embed, embed, std),
            wk: Linear::new(rng, embed, embed, std),
            wv: Linear::new(rng, embed, embed, std),
            wo: Linear::new(rng, embed, embed, std),
            n_head,
            head_dim,
            scale: 1.0 / (head_dim as f64).sqrt(),
        }
    }

    pub fn forward(&self, x: &Mat) -> (Mat, AttnCache) {
        let t = x.rows;
        let e = x.cols;
        let q = self.wq.forward(x);
        let k = self.wk.forward(x);
        let v = self.wv.forward(x);
        let mut ctx = Mat::zeros(t, e);
        let mut attn_rows: Vec<Vec<f64>> = vec![Vec::new(); self.n_head * t];
        for h in 0..self.n_head {
            let base = h * self.head_dim;
            for i in 0..t {
                // scores over j <= i
                let mut scores = vec![0.0; i + 1];
                let mut max_s = f64::NEG_INFINITY;
                for (j, sc) in scores.iter_mut().enumerate() {
                    let mut dot = 0.0;
                    for d in 0..self.head_dim {
                        dot += q.get(i, base + d) * k.get(j, base + d);
                    }
                    *sc = dot * self.scale;
                    if *sc > max_s {
                        max_s = *sc;
                    }
                }
                let mut sum = 0.0;
                for sc in scores.iter_mut() {
                    *sc = (*sc - max_s).exp();
                    sum += *sc;
                }
                for sc in scores.iter_mut() {
                    *sc /= sum;
                }
                // ctx[i, head] = sum_j attn[j] * v[j, head]
                for d in 0..self.head_dim {
                    let mut acc = 0.0;
                    for (j, &a) in scores.iter().enumerate() {
                        acc += a * v.get(j, base + d);
                    }
                    ctx.set(i, base + d, acc);
                }
                attn_rows[h * t + i] = scores;
            }
        }
        let out = self.wo.forward(&ctx);
        (
            out,
            AttnCache {
                ln_in: x.clone(),
                q,
                k,
                v,
                ctx,
                attn: attn_rows,
            },
        )
    }

    /// Accumulates all param grads; returns dx (`T x E`).
    pub fn backward(&mut self, cache: &AttnCache, d_out: &Mat) -> Mat {
        let t = d_out.rows;
        let e = d_out.cols;
        // out = wo(ctx)
        let d_ctx = self.wo.backward(&cache.ctx, d_out);
        let mut dq = Mat::zeros(t, e);
        let mut dk = Mat::zeros(t, e);
        let mut dv = Mat::zeros(t, e);
        for h in 0..self.n_head {
            let base = h * self.head_dim;
            for i in 0..t {
                let attn = &cache.attn[h * t + i];
                // d_attn[j] = sum_d d_ctx[i] * v[j]; and dv[j] += attn[j]*d_ctx[i]
                let mut d_attn = vec![0.0; i + 1];
                for (j, da) in d_attn.iter_mut().enumerate() {
                    let mut acc = 0.0;
                    for d in 0..self.head_dim {
                        let dc = d_ctx.get(i, base + d);
                        acc += dc * cache.v.get(j, base + d);
                        dv.data[j * e + base + d] += attn[j] * dc;
                    }
                    *da = acc;
                }
                // softmax backward over j<=i
                let s: f64 = (0..=i).map(|j| attn[j] * d_attn[j]).sum();
                for j in 0..=i {
                    let d_score = attn[j] * (d_attn[j] - s);
                    if d_score == 0.0 {
                        continue;
                    }
                    let sd = d_score * self.scale;
                    for d in 0..self.head_dim {
                        dq.data[i * e + base + d] += sd * cache.k.get(j, base + d);
                        dk.data[j * e + base + d] += sd * cache.q.get(i, base + d);
                    }
                }
            }
        }
        // backprop through q/k/v projections; sum dx contributions.
        let dx_q = self.wq.backward(&cache.ln_in, &dq);
        let dx_k = self.wk.backward(&cache.ln_in, &dk);
        let dx_v = self.wv.backward(&cache.ln_in, &dv);
        let mut dx = Mat::zeros(t, e);
        for idx in 0..dx.data.len() {
            dx.data[idx] = dx_q.data[idx] + dx_k.data[idx] + dx_v.data[idx];
        }
        dx
    }

    pub fn zero_grad(&mut self) {
        self.wq.zero_grad();
        self.wk.zero_grad();
        self.wv.zero_grad();
        self.wo.zero_grad();
    }
}

/// Position-wise MLP: Linear(E→4E) → GELU → Linear(4E→E).
#[derive(Debug, Clone)]
pub struct Mlp {
    pub fc1: Linear,
    pub fc2: Linear,
}

pub struct MlpCache {
    ln_in: Mat, // input to fc1 (T x E)
    h1: Mat,    // pre-activation (T x 4E)
    act: Mat,   // post-GELU (T x 4E)
}

impl Mlp {
    pub fn new(rng: &mut SplitMix64, embed: usize, hidden: usize, std: f64) -> Self {
        Self {
            fc1: Linear::new(rng, embed, hidden, std),
            fc2: Linear::new(rng, hidden, embed, std),
        }
    }

    pub fn forward(&self, x: &Mat) -> (Mat, MlpCache) {
        let h1 = self.fc1.forward(x);
        let mut act = Mat::zeros(h1.rows, h1.cols);
        for idx in 0..h1.data.len() {
            act.data[idx] = gelu(h1.data[idx]);
        }
        let y = self.fc2.forward(&act);
        (
            y,
            MlpCache {
                ln_in: x.clone(),
                h1,
                act,
            },
        )
    }

    pub fn backward(&mut self, cache: &MlpCache, dy: &Mat) -> Mat {
        let d_act = self.fc2.backward(&cache.act, dy);
        let mut dh1 = Mat::zeros(d_act.rows, d_act.cols);
        for idx in 0..dh1.data.len() {
            dh1.data[idx] = d_act.data[idx] * gelu_grad(cache.h1.data[idx]);
        }
        self.fc1.backward(&cache.ln_in, &dh1)
    }

    pub fn zero_grad(&mut self) {
        self.fc1.zero_grad();
        self.fc2.zero_grad();
    }
}

/// One pre-norm transformer block.
#[derive(Debug, Clone)]
pub struct Block {
    pub ln1: LayerNorm,
    pub attn: MultiHeadAttention,
    pub ln2: LayerNorm,
    pub mlp: Mlp,
}

pub struct BlockCache {
    ln1_cache: LnCache,
    attn_cache: AttnCache,
    ln2_cache: LnCache,
    mlp_cache: MlpCache,
}

impl Block {
    pub fn new(rng: &mut SplitMix64, embed: usize, n_head: usize, hidden: usize, std: f64) -> Self {
        Self {
            ln1: LayerNorm::new(embed),
            attn: MultiHeadAttention::new(rng, embed, n_head, std),
            ln2: LayerNorm::new(embed),
            mlp: Mlp::new(rng, embed, hidden, std),
        }
    }

    pub fn forward(&self, x: &Mat) -> (Mat, BlockCache) {
        let (ln1_y, ln1_cache) = self.ln1.forward(x);
        let (attn_out, attn_cache) = self.attn.forward(&ln1_y);
        let mut x1 = x.clone();
        for idx in 0..x1.data.len() {
            x1.data[idx] += attn_out.data[idx];
        }
        let (ln2_y, ln2_cache) = self.ln2.forward(&x1);
        let (mlp_out, mlp_cache) = self.mlp.forward(&ln2_y);
        let mut x2 = x1.clone();
        for idx in 0..x2.data.len() {
            x2.data[idx] += mlp_out.data[idx];
        }
        (
            x2,
            BlockCache {
                ln1_cache,
                attn_cache,
                ln2_cache,
                mlp_cache,
            },
        )
    }

    pub fn backward(&mut self, cache: &BlockCache, dy: &Mat) -> Mat {
        // x2 = x1 + mlp_out
        let d_mlp_out = dy;
        let d_ln2_y = self.mlp.backward(&cache.mlp_cache, d_mlp_out);
        let d_x1_from_ln2 = self.ln2.backward(&cache.ln2_cache, &d_ln2_y);
        let mut d_x1 = dy.clone();
        for idx in 0..d_x1.data.len() {
            d_x1.data[idx] += d_x1_from_ln2.data[idx];
        }
        // x1 = x + attn_out
        let d_attn_out = &d_x1;
        let d_ln1_y = self.attn.backward(&cache.attn_cache, d_attn_out);
        let d_x_from_ln1 = self.ln1.backward(&cache.ln1_cache, &d_ln1_y);
        let mut d_x = d_x1.clone();
        for idx in 0..d_x.data.len() {
            d_x.data[idx] += d_x_from_ln1.data[idx];
        }
        d_x
    }

    pub fn zero_grad(&mut self) {
        self.ln1.zero_grad();
        self.attn.zero_grad();
        self.ln2.zero_grad();
        self.mlp.zero_grad();
    }
}

/// Decoupled AdamW. Walked over every parameter block in a fixed order each
/// step; `m`/`v` grow on the first step and stay index-aligned thereafter.
#[derive(Debug, Clone)]
pub struct AdamW {
    m: Vec<f64>,
    v: Vec<f64>,
    t: u64,
    beta1: f64,
    beta2: f64,
    eps: f64,
    weight_decay: f64,
    cursor: usize,
}

impl AdamW {
    pub fn new(weight_decay: f64) -> Self {
        Self {
            m: Vec::new(),
            v: Vec::new(),
            t: 0,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1.0e-8,
            weight_decay,
            cursor: 0,
        }
    }

    pub fn begin_step(&mut self) {
        self.t += 1;
        self.cursor = 0;
    }

    /// Update one contiguous parameter block in place. `decay` toggles weight
    /// decay (applied to 2D weights only, not biases/LayerNorm/embeddings).
    pub fn step_block(&mut self, params: &mut [f64], grads: &[f64], lr: f64, decay: bool) {
        let bc1 = 1.0 - self.beta1.powi(self.t as i32);
        let bc2 = 1.0 - self.beta2.powi(self.t as i32);
        for i in 0..params.len() {
            let idx = self.cursor + i;
            if idx >= self.m.len() {
                self.m.resize(idx + 1, 0.0);
                self.v.resize(idx + 1, 0.0);
            }
            let g = grads[i];
            self.m[idx] = self.beta1 * self.m[idx] + (1.0 - self.beta1) * g;
            self.v[idx] = self.beta2 * self.v[idx] + (1.0 - self.beta2) * g * g;
            let mhat = self.m[idx] / bc1;
            let vhat = self.v[idx] / bc2;
            if decay && self.weight_decay != 0.0 {
                params[i] -= lr * self.weight_decay * params[i];
            }
            params[i] -= lr * mhat / (vhat.sqrt() + self.eps);
        }
        self.cursor += params.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(seed: u64) -> SplitMix64 {
        SplitMix64::new(seed)
    }

    fn rand_mat(rng: &mut SplitMix64, rows: usize, cols: usize) -> Mat {
        Mat::from_fn(rows, cols, || rng.next_gaussian() * 0.5)
    }

    // Sum of all output entries weighted by a fixed cotangent `g` lets us
    // finite-difference a layer: d(sum g*y)/d(param) ≈ analytic grad·g.
    fn cotangent(rows: usize, cols: usize, rng: &mut SplitMix64) -> Mat {
        rand_mat(rng, rows, cols)
    }

    fn dot(a: &Mat, b: &Mat) -> f64 {
        a.data.iter().zip(b.data.iter()).map(|(x, y)| x * y).sum()
    }

    const EPS: f64 = 1.0e-6;
    const TOL: f64 = 1.0e-4;

    #[test]
    fn rng_is_deterministic() {
        let mut a = seeded(42);
        let mut b = seeded(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn linear_backward_matches_numeric() {
        let mut rng = seeded(1);
        let (t, i, o) = (3, 4, 5);
        let mut lin = Linear::new(&mut rng, i, o, 0.3);
        let x = rand_mat(&mut rng, t, i);
        let g = cotangent(t, o, &mut rng);
        // analytic
        lin.zero_grad();
        let y = lin.forward(&x);
        let dx = lin.backward(&x, &g);
        let _ = y;
        // numeric dx
        for ti in 0..t {
            for ii in 0..i {
                let mut xp = x.clone();
                xp.data[ti * i + ii] += EPS;
                let lp = dot(&lin.forward(&xp), &g);
                let mut xm = x.clone();
                xm.data[ti * i + ii] -= EPS;
                let lm = dot(&lin.forward(&xm), &g);
                let num = (lp - lm) / (2.0 * EPS);
                assert!((num - dx.get(ti, ii)).abs() < TOL, "dx[{ti}][{ii}]");
            }
        }
        // numeric dW
        for o2 in 0..o {
            for i2 in 0..i {
                let mut wp = lin.clone();
                wp.w.data[o2 * i + i2] += EPS;
                let lp = dot(&wp.forward(&x), &g);
                let mut wm = lin.clone();
                wm.w.data[o2 * i + i2] -= EPS;
                let lm = dot(&wm.forward(&x), &g);
                let num = (lp - lm) / (2.0 * EPS);
                assert!((num - lin.dw.get(o2, i2)).abs() < TOL, "dW[{o2}][{i2}]");
            }
        }
    }

    #[test]
    fn layernorm_backward_matches_numeric() {
        let mut rng = seeded(2);
        let (t, e) = (3, 6);
        let mut ln = LayerNorm::new(e);
        ln.gamma
            .iter_mut()
            .for_each(|v| *v = 1.0 + rng.next_gaussian() * 0.1);
        let x = rand_mat(&mut rng, t, e);
        let g = cotangent(t, e, &mut rng);
        ln.zero_grad();
        let (_, cache) = ln.forward(&x);
        let dx = ln.backward(&cache, &g);
        for ti in 0..t {
            for ei in 0..e {
                let mut xp = x.clone();
                xp.data[ti * e + ei] += EPS;
                let lp = dot(&ln.forward(&xp).0, &g);
                let mut xm = x.clone();
                xm.data[ti * e + ei] -= EPS;
                let lm = dot(&ln.forward(&xm).0, &g);
                let num = (lp - lm) / (2.0 * EPS);
                assert!((num - dx.get(ti, ei)).abs() < TOL, "dx[{ti}][{ei}]");
            }
        }
        // dgamma
        for ei in 0..e {
            let mut lp = ln.clone();
            lp.gamma[ei] += EPS;
            let vp = dot(&lp.forward(&x).0, &g);
            let mut lm = ln.clone();
            lm.gamma[ei] -= EPS;
            let vm = dot(&lm.forward(&x).0, &g);
            let num = (vp - vm) / (2.0 * EPS);
            assert!((num - ln.dgamma[ei]).abs() < TOL, "dgamma[{ei}]");
        }
    }

    #[test]
    fn gelu_grad_matches_numeric() {
        for &x in &[-2.0, -0.5, 0.0, 0.3, 1.7] {
            let num = (gelu(x + EPS) - gelu(x - EPS)) / (2.0 * EPS);
            assert!((num - gelu_grad(x)).abs() < TOL, "gelu'({x})");
        }
    }

    #[test]
    fn attention_backward_matches_numeric() {
        let mut rng = seeded(3);
        let (t, e, h) = (4, 8, 2);
        let mut attn = MultiHeadAttention::new(&mut rng, e, h, 0.3);
        let x = rand_mat(&mut rng, t, e);
        let g = cotangent(t, e, &mut rng);
        attn.zero_grad();
        let (_, cache) = attn.forward(&x);
        let dx = attn.backward(&cache, &g);
        // numeric dx
        for ti in 0..t {
            for ei in 0..e {
                let mut xp = x.clone();
                xp.data[ti * e + ei] += EPS;
                let lp = dot(&attn.forward(&xp).0, &g);
                let mut xm = x.clone();
                xm.data[ti * e + ei] -= EPS;
                let lm = dot(&attn.forward(&xm).0, &g);
                let num = (lp - lm) / (2.0 * EPS);
                assert!(
                    (num - dx.get(ti, ei)).abs() < TOL,
                    "dx[{ti}][{ei}] num={num}"
                );
            }
        }
        // numeric dWq (a representative weight matrix)
        for r in 0..e {
            for c in 0..e {
                let mut ap = attn.clone();
                ap.wq.w.data[r * e + c] += EPS;
                let lp = dot(&ap.forward(&x).0, &g);
                let mut am = attn.clone();
                am.wq.w.data[r * e + c] -= EPS;
                let lm = dot(&am.forward(&x).0, &g);
                let num = (lp - lm) / (2.0 * EPS);
                assert!((num - attn.wq.dw.get(r, c)).abs() < TOL, "dWq[{r}][{c}]");
            }
        }
    }

    #[test]
    fn block_backward_matches_numeric() {
        let mut rng = seeded(4);
        let (t, e, h, hidden) = (3, 8, 2, 16);
        let mut block = Block::new(&mut rng, e, h, hidden, 0.3);
        let x = rand_mat(&mut rng, t, e);
        let g = cotangent(t, e, &mut rng);
        block.zero_grad();
        let (_, cache) = block.forward(&x);
        let dx = block.backward(&cache, &g);
        for ti in 0..t {
            for ei in 0..e {
                let mut xp = x.clone();
                xp.data[ti * e + ei] += EPS;
                let lp = dot(&block.forward(&xp).0, &g);
                let mut xm = x.clone();
                xm.data[ti * e + ei] -= EPS;
                let lm = dot(&block.forward(&xm).0, &g);
                let num = (lp - lm) / (2.0 * EPS);
                assert!((num - dx.get(ti, ei)).abs() < TOL, "dx[{ti}][{ei}]");
            }
        }
    }

    #[test]
    fn adamw_decreases_a_quadratic() {
        // Minimise f(p) = sum (p - target)^2 with grad 2(p-target).
        let target = [1.0_f64, -2.0, 3.0];
        let mut p = [0.0_f64; 3];
        let mut opt = AdamW::new(0.0);
        for _ in 0..500 {
            opt.begin_step();
            let g: Vec<f64> = p
                .iter()
                .zip(target)
                .map(|(pi, ti)| 2.0 * (pi - ti))
                .collect();
            opt.step_block(&mut p, &g, 0.05, false);
        }
        for (pi, ti) in p.iter().zip(target) {
            assert!((pi - ti).abs() < 1.0e-2, "p={pi} target={ti}");
        }
    }
}
