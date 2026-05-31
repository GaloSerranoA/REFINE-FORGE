//! Hand-written CUDA compute for the native GPT GPU path (Phase 3).
//!
//! - Milestone 1: a `vector_add` kernel proving the toolchain.
//! - Milestone 2 (this file): the matmul forms a transformer needs —
//!   `A·B` (`matmul_nn`), `A·Bᵀ` (`matmul_nt`), and `Aᵀ·B` (`matmul_tn`) —
//!   plus GELU. Each GPU kernel has a CPU `f32` reference (the correctness
//!   oracle) and a GPU/CPU parity test that runs on real hardware.
//!
//! The CUDA path is behind the `cuda` cargo feature, so the default workspace
//! build (CI, no GPU) compiles a CPU-only crate with no CUDA dependency. The
//! GPU parity tests are `#[cfg(feature = "cuda")]` and run only on a machine
//! with an NVIDIA GPU — honest hardware-gating, not disabled tests.
//!
//! Honesty note: the GPU path is `f32` and uses FMA contraction, so GPU/CPU
//! parity is checked within an `f32` tolerance, never bit-exactly. The CPU
//! `f64` trainer (`refineforge-trainer`) remains the deterministic reference.

// ─── CPU references (the correctness oracle every GPU kernel matches) ───

/// Element-wise vector addition.
pub fn vector_add_cpu(a: &[f32], b: &[f32]) -> Vec<f32> {
    assert_eq!(a.len(), b.len(), "vector_add length mismatch");
    a.iter().zip(b.iter()).map(|(x, y)| x + y).collect()
}

/// `C = A · B` where A is `m×k`, B is `k×n`, C is `m×n` (row-major).
pub fn matmul_nn_cpu(a: &[f32], m: usize, k: usize, b: &[f32], n: usize) -> Vec<f32> {
    assert_eq!(a.len(), m * k);
    assert_eq!(b.len(), k * n);
    let mut c = vec![0.0f32; m * n];
    for (i, crow) in c.chunks_mut(n).enumerate() {
        for (l, &av) in a[i * k..i * k + k].iter().enumerate() {
            let brow = &b[l * n..l * n + n];
            for (cj, &bv) in crow.iter_mut().zip(brow.iter()) {
                *cj += av * bv;
            }
        }
    }
    c
}

/// `C = A · Bᵀ` where A is `m×k`, B is `n×k`, C is `m×n`.
pub fn matmul_nt_cpu(a: &[f32], m: usize, k: usize, b: &[f32], n: usize) -> Vec<f32> {
    assert_eq!(a.len(), m * k);
    assert_eq!(b.len(), n * k);
    let mut c = vec![0.0f32; m * n];
    for i in 0..m {
        for j in 0..n {
            let mut acc = 0.0f32;
            for l in 0..k {
                acc += a[i * k + l] * b[j * k + l];
            }
            c[i * n + j] = acc;
        }
    }
    c
}

/// `C = Aᵀ · B` where A is `k×m`, B is `k×n`, C is `m×n`.
pub fn matmul_tn_cpu(a: &[f32], k: usize, m: usize, b: &[f32], n: usize) -> Vec<f32> {
    assert_eq!(a.len(), k * m);
    assert_eq!(b.len(), k * n);
    let mut c = vec![0.0f32; m * n];
    for i in 0..m {
        for j in 0..n {
            let mut acc = 0.0f32;
            for l in 0..k {
                acc += a[l * m + i] * b[l * n + j];
            }
            c[i * n + j] = acc;
        }
    }
    c
}

/// Tanh-approximation GELU (matches the GPU kernel and the trainer's CPU GELU).
pub fn gelu_cpu(x: &[f32]) -> Vec<f32> {
    const C: f32 = 0.797_885; // sqrt(2/pi), f32 precision
    x.iter()
        .map(|&v| {
            let inner = C * (v + 0.044_715 * v * v * v);
            0.5 * v * (1.0 + inner.tanh())
        })
        .collect()
}

/// Row-wise softmax: `x` is `rows×cols`, softmax taken over each row.
pub fn softmax_cpu(x: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    assert_eq!(x.len(), rows * cols);
    let mut y = vec![0.0f32; rows * cols];
    for r in 0..rows {
        let row = &x[r * cols..r * cols + cols];
        let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mut sum = 0.0f32;
        for (j, &v) in row.iter().enumerate() {
            let e = (v - max).exp();
            y[r * cols + j] = e;
            sum += e;
        }
        for yj in &mut y[r * cols..r * cols + cols] {
            *yj /= sum;
        }
    }
    y
}

/// Backward of row-wise softmax given the forward output `y` and upstream `dy`.
pub fn softmax_backward_cpu(y: &[f32], dy: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    assert_eq!(y.len(), rows * cols);
    assert_eq!(dy.len(), rows * cols);
    let mut dx = vec![0.0f32; rows * cols];
    for r in 0..rows {
        let yr = &y[r * cols..r * cols + cols];
        let dyr = &dy[r * cols..r * cols + cols];
        let dot: f32 = yr.iter().zip(dyr).map(|(a, b)| a * b).sum();
        for (j, dxj) in dx[r * cols..r * cols + cols].iter_mut().enumerate() {
            *dxj = yr[j] * (dyr[j] - dot);
        }
    }
    dx
}

/// LayerNorm forward over each row (`rows×cols`). Returns `(y, mean, rstd)`;
/// the per-row `mean`/`rstd` feed the backward pass.
pub fn layernorm_forward_cpu(
    x: &[f32],
    gamma: &[f32],
    beta: &[f32],
    rows: usize,
    cols: usize,
    eps: f32,
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    assert_eq!(x.len(), rows * cols);
    assert_eq!(gamma.len(), cols);
    assert_eq!(beta.len(), cols);
    let mut y = vec![0.0f32; rows * cols];
    let mut mean = vec![0.0f32; rows];
    let mut rstd = vec![0.0f32; rows];
    for r in 0..rows {
        let row = &x[r * cols..r * cols + cols];
        let m = row.iter().sum::<f32>() / cols as f32;
        let var = row.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / cols as f32;
        let rs = 1.0 / (var + eps).sqrt();
        mean[r] = m;
        rstd[r] = rs;
        for (j, yj) in y[r * cols..r * cols + cols].iter_mut().enumerate() {
            let xhat = (row[j] - m) * rs;
            *yj = gamma[j] * xhat + beta[j];
        }
    }
    (y, mean, rstd)
}

/// LayerNorm backward. Returns `(dx, dgamma, dbeta)` (`dgamma`/`dbeta` summed
/// over rows).
#[allow(clippy::too_many_arguments)]
pub fn layernorm_backward_cpu(
    x: &[f32],
    gamma: &[f32],
    dy: &[f32],
    mean: &[f32],
    rstd: &[f32],
    rows: usize,
    cols: usize,
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut dx = vec![0.0f32; rows * cols];
    let mut dgamma = vec![0.0f32; cols];
    let mut dbeta = vec![0.0f32; cols];
    for r in 0..rows {
        let (m, rs) = (mean[r], rstd[r]);
        let mut mean_dxhat = 0.0f32;
        let mut mean_dxhat_xhat = 0.0f32;
        for j in 0..cols {
            let xhat = (x[r * cols + j] - m) * rs;
            let dxhat = dy[r * cols + j] * gamma[j];
            mean_dxhat += dxhat;
            mean_dxhat_xhat += dxhat * xhat;
        }
        mean_dxhat /= cols as f32;
        mean_dxhat_xhat /= cols as f32;
        for j in 0..cols {
            let xhat = (x[r * cols + j] - m) * rs;
            let dxhat = dy[r * cols + j] * gamma[j];
            dx[r * cols + j] = rs * (dxhat - mean_dxhat - xhat * mean_dxhat_xhat);
            dgamma[j] += dy[r * cols + j] * xhat;
            dbeta[j] += dy[r * cols + j];
        }
    }
    (dx, dgamma, dbeta)
}

/// Backward of [`gelu_cpu`]: `dx[i] = dy[i] * gelu'(x[i])`.
pub fn gelu_backward_cpu(x: &[f32], dy: &[f32]) -> Vec<f32> {
    const C: f32 = 0.797_885;
    x.iter()
        .zip(dy.iter())
        .map(|(&v, &g)| {
            let inner = C * (v + 0.044_715 * v * v * v);
            let tanh = inner.tanh();
            let dinner = C * (1.0 + 3.0 * 0.044_715 * v * v);
            let grad = 0.5 * (1.0 + tanh) + 0.5 * v * (1.0 - tanh * tanh) * dinner;
            g * grad
        })
        .collect()
}

/// Decoupled AdamW update over a flat parameter block, in place. `m`/`v` are
/// the optimizer state; `t` is the 1-based step for bias correction.
#[allow(clippy::too_many_arguments)]
pub fn adamw_update_cpu(
    param: &mut [f32],
    grad: &[f32],
    m: &mut [f32],
    v: &mut [f32],
    t: u32,
    lr: f32,
    beta1: f32,
    beta2: f32,
    eps: f32,
    weight_decay: f32,
) {
    let bc1 = 1.0 - beta1.powi(t as i32);
    let bc2 = 1.0 - beta2.powi(t as i32);
    for i in 0..param.len() {
        let g = grad[i];
        m[i] = beta1 * m[i] + (1.0 - beta1) * g;
        v[i] = beta2 * v[i] + (1.0 - beta2) * g * g;
        let mhat = m[i] / bc1;
        let vhat = v[i] / bc2;
        if weight_decay != 0.0 {
            param[i] -= lr * weight_decay * param[i];
        }
        param[i] -= lr * mhat / (vhat.sqrt() + eps);
    }
}

/// CUDA C source for all kernels, compiled once at runtime with nvrtc.
#[cfg(feature = "cuda")]
pub const KERNEL_SOURCE: &str = r#"
extern "C" __global__ void vector_add(const float* a, const float* b, float* out, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) out[i] = a[i] + b[i];
}

// C[m,n] = A[m,k] * B[k,n]
extern "C" __global__ void matmul_nn(const float* a, const float* b, float* c, int m, int n, int k) {
    int col = blockIdx.x * blockDim.x + threadIdx.x;
    int row = blockIdx.y * blockDim.y + threadIdx.y;
    if (row < m && col < n) {
        float acc = 0.0f;
        for (int l = 0; l < k; ++l) acc += a[row * k + l] * b[l * n + col];
        c[row * n + col] = acc;
    }
}

// C[m,n] = A[m,k] * B[n,k]^T
extern "C" __global__ void matmul_nt(const float* a, const float* b, float* c, int m, int n, int k) {
    int col = blockIdx.x * blockDim.x + threadIdx.x;
    int row = blockIdx.y * blockDim.y + threadIdx.y;
    if (row < m && col < n) {
        float acc = 0.0f;
        for (int l = 0; l < k; ++l) acc += a[row * k + l] * b[col * k + l];
        c[row * n + col] = acc;
    }
}

// C[m,n] = A[k,m]^T * B[k,n]
extern "C" __global__ void matmul_tn(const float* a, const float* b, float* c, int m, int n, int k) {
    int col = blockIdx.x * blockDim.x + threadIdx.x;
    int row = blockIdx.y * blockDim.y + threadIdx.y;
    if (row < m && col < n) {
        float acc = 0.0f;
        for (int l = 0; l < k; ++l) acc += a[l * m + row] * b[l * n + col];
        c[row * n + col] = acc;
    }
}

extern "C" __global__ void gelu_forward(const float* x, float* y, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        float v = x[i];
        float inner = 0.79788456f * (v + 0.044715f * v * v * v);
        y[i] = 0.5f * v * (1.0f + tanhf(inner));
    }
}

extern "C" __global__ void gelu_backward(const float* x, const float* dy, float* dx, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        float v = x[i];
        float inner = 0.79788456f * (v + 0.044715f * v * v * v);
        float t = tanhf(inner);
        float dinner = 0.79788456f * (1.0f + 3.0f * 0.044715f * v * v);
        float grad = 0.5f * (1.0f + t) + 0.5f * v * (1.0f - t * t) * dinner;
        dx[i] = dy[i] * grad;
    }
}

// One thread per row. softmax over each row of x[rows,cols].
extern "C" __global__ void softmax_forward(const float* x, float* y, int rows, int cols) {
    int r = blockIdx.x * blockDim.x + threadIdx.x;
    if (r < rows) {
        const float* xr = x + r * cols;
        float* yr = y + r * cols;
        float maxv = -1e30f;
        for (int j = 0; j < cols; ++j) maxv = fmaxf(maxv, xr[j]);
        float sum = 0.0f;
        for (int j = 0; j < cols; ++j) { float e = expf(xr[j] - maxv); yr[j] = e; sum += e; }
        for (int j = 0; j < cols; ++j) yr[j] /= sum;
    }
}

extern "C" __global__ void softmax_backward(const float* y, const float* dy, float* dx, int rows, int cols) {
    int r = blockIdx.x * blockDim.x + threadIdx.x;
    if (r < rows) {
        const float* yr = y + r * cols;
        const float* dyr = dy + r * cols;
        float* dxr = dx + r * cols;
        float dot = 0.0f;
        for (int j = 0; j < cols; ++j) dot += yr[j] * dyr[j];
        for (int j = 0; j < cols; ++j) dxr[j] = yr[j] * (dyr[j] - dot);
    }
}

extern "C" __global__ void layernorm_forward(const float* x, const float* gamma, const float* beta,
                                             float* y, float* mean_out, float* rstd_out,
                                             int rows, int cols, float eps) {
    int r = blockIdx.x * blockDim.x + threadIdx.x;
    if (r < rows) {
        const float* xr = x + r * cols;
        float m = 0.0f;
        for (int j = 0; j < cols; ++j) m += xr[j];
        m /= (float)cols;
        float var = 0.0f;
        for (int j = 0; j < cols; ++j) { float d = xr[j] - m; var += d * d; }
        var /= (float)cols;
        float rs = rsqrtf(var + eps);
        mean_out[r] = m; rstd_out[r] = rs;
        float* yr = y + r * cols;
        for (int j = 0; j < cols; ++j) { float xhat = (xr[j] - m) * rs; yr[j] = gamma[j] * xhat + beta[j]; }
    }
}

// dgamma/dbeta accumulate across rows via atomicAdd; they must be zeroed first.
extern "C" __global__ void layernorm_backward(const float* x, const float* gamma, const float* dy,
                                              const float* mean, const float* rstd,
                                              float* dx, float* dgamma, float* dbeta,
                                              int rows, int cols) {
    int r = blockIdx.x * blockDim.x + threadIdx.x;
    if (r < rows) {
        const float* xr = x + r * cols;
        const float* dyr = dy + r * cols;
        float* dxr = dx + r * cols;
        float m = mean[r], rs = rstd[r];
        float mean_dxhat = 0.0f, mean_dxhat_xhat = 0.0f;
        for (int j = 0; j < cols; ++j) {
            float xhat = (xr[j] - m) * rs;
            float dxhat = dyr[j] * gamma[j];
            mean_dxhat += dxhat; mean_dxhat_xhat += dxhat * xhat;
        }
        mean_dxhat /= (float)cols; mean_dxhat_xhat /= (float)cols;
        for (int j = 0; j < cols; ++j) {
            float xhat = (xr[j] - m) * rs;
            float dxhat = dyr[j] * gamma[j];
            dxr[j] = rs * (dxhat - mean_dxhat - xhat * mean_dxhat_xhat);
            atomicAdd(&dgamma[j], dyr[j] * xhat);
            atomicAdd(&dbeta[j], dyr[j]);
        }
    }
}

extern "C" __global__ void adamw_update(float* param, const float* grad, float* m, float* v,
                                        float lr, float beta1, float beta2, float eps,
                                        float weight_decay, float bc1, float bc2, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        float g = grad[i];
        float mi = beta1 * m[i] + (1.0f - beta1) * g;
        float vi = beta2 * v[i] + (1.0f - beta2) * g * g;
        m[i] = mi; v[i] = vi;
        float mhat = mi / bc1;
        float vhat = vi / bc2;
        float p = param[i];
        if (weight_decay != 0.0f) p -= lr * weight_decay * p;
        p -= lr * mhat / (sqrtf(vhat) + eps);
        param[i] = p;
    }
}

// y[r,c] += b[c]  (broadcast a row-vector bias over every row)
extern "C" __global__ void bias_add(float* y, const float* b, int rows, int cols) {
    int c = blockIdx.x * blockDim.x + threadIdx.x;
    int r = blockIdx.y * blockDim.y + threadIdx.y;
    if (r < rows && c < cols) y[r * cols + c] += b[c];
}

// db[c] = sum_r dy[r,c]  (column reduction -> bias gradient)
extern "C" __global__ void col_sum(const float* dy, float* db, int rows, int cols) {
    int c = blockIdx.x * blockDim.x + threadIdx.x;
    if (c < cols) {
        float s = 0.0f;
        for (int r = 0; r < rows; ++r) s += dy[r * cols + c];
        db[c] = s;
    }
}

// out[i] += a[i]  (in-place residual add)
extern "C" __global__ void add_inplace(float* out, const float* a, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) out[i] += a[i];
}

// dst[r, 0..w) = src[r, col0..col0+w)   (extract a column block, e.g. one head)
extern "C" __global__ void slice_cols(const float* src, float* dst, int rows, int total, int col0, int w) {
    int c = blockIdx.x * blockDim.x + threadIdx.x;
    int r = blockIdx.y * blockDim.y + threadIdx.y;
    if (r < rows && c < w) dst[r * w + c] = src[r * total + col0 + c];
}

// out[r, col0..col0+w) = src[r, 0..w)   (write a column block back, e.g. one head)
extern "C" __global__ void set_cols(float* out, const float* src, int rows, int total, int col0, int w) {
    int c = blockIdx.x * blockDim.x + threadIdx.x;
    int r = blockIdx.y * blockDim.y + threadIdx.y;
    if (r < rows && c < w) out[r * total + col0 + c] = src[r * w + c];
}

// In place over a T×T attention-score matrix: scale, then causal-mask (j>i -> -inf).
extern "C" __global__ void scale_causal_mask(float* s, int tt, float scale) {
    int j = blockIdx.x * blockDim.x + threadIdx.x;
    int i = blockIdx.y * blockDim.y + threadIdx.y;
    if (i < tt && j < tt) {
        if (j > i) s[i * tt + j] = -1e30f;
        else s[i * tt + j] *= scale;
    }
}

// Backward of scale_causal_mask: scale the kept entries, zero the masked ones.
extern "C" __global__ void scale_causal_mask_grad(float* s, int tt, float scale) {
    int j = blockIdx.x * blockDim.x + threadIdx.x;
    int i = blockIdx.y * blockDim.y + threadIdx.y;
    if (i < tt && j < tt) {
        if (j > i) s[i * tt + j] = 0.0f;
        else s[i * tt + j] *= scale;
    }
}
"#;

#[cfg(feature = "cuda")]
pub mod gpu {
    //! Real CUDA implementation (only compiled with `--features cuda`).
    //!
    //! [`GpuKernels`] compiles the kernel module once (nvrtc) and reuses it for
    //! every launch, holding the context, default stream, and loaded module.
    use anyhow::{Context, Result};
    use cudarc::driver::{
        CudaContext, CudaModule, CudaSlice, CudaStream, LaunchConfig, PushKernelArg,
    };
    use cudarc::nvrtc::compile_ptx;
    use std::sync::Arc;

    pub struct GpuKernels {
        ctx: Arc<CudaContext>,
        stream: Arc<CudaStream>,
        module: Arc<CudaModule>,
    }

    fn cfg_2d(rows: usize, cols: usize) -> LaunchConfig {
        let (bx, by) = (16u32, 16u32);
        LaunchConfig {
            grid_dim: ((cols as u32).div_ceil(bx), (rows as u32).div_ceil(by), 1),
            block_dim: (bx, by, 1),
            shared_mem_bytes: 0,
        }
    }

    /// A 2-D `f32` tensor that lives in GPU memory between ops (no host
    /// round-trips) — the building block for device-resident layers.
    pub struct DeviceTensor {
        pub(crate) buf: cudarc::driver::CudaSlice<f32>,
        pub rows: usize,
        pub cols: usize,
    }

    impl DeviceTensor {
        pub fn len(&self) -> usize {
            self.rows * self.cols
        }
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
    }

    impl GpuKernels {
        /// Open device `ordinal` and compile the kernel module once.
        pub fn new(ordinal: usize) -> Result<Self> {
            let ctx = CudaContext::new(ordinal).context("opening CUDA context")?;
            let stream = ctx.default_stream();
            let ptx = compile_ptx(super::KERNEL_SOURCE).context("nvrtc compile kernels")?;
            let module = ctx.load_module(ptx).context("loading kernel module")?;
            Ok(Self {
                ctx,
                stream,
                module,
            })
        }

        /// Name of the underlying CUDA device.
        pub fn device_name(&self) -> String {
            self.ctx.name().unwrap_or_else(|_| "unknown".to_string())
        }

        /// Element-wise `a + b` on the GPU.
        pub fn vector_add(&self, a: &[f32], b: &[f32]) -> Result<Vec<f32>> {
            anyhow::ensure!(a.len() == b.len(), "vector_add length mismatch");
            let n = a.len();
            let a_dev = self.stream.clone_htod(a).context("htod a")?;
            let b_dev = self.stream.clone_htod(b).context("htod b")?;
            let mut out = self.stream.alloc_zeros::<f32>(n).context("alloc out")?;
            let func = self.module.load_function("vector_add")?;
            let n_arg = n as i32;
            let mut builder = self.stream.launch_builder(&func);
            builder.arg(&a_dev);
            builder.arg(&b_dev);
            builder.arg(&mut out);
            builder.arg(&n_arg);
            // SAFETY: 4 args match (const float*, const float*, float*, int).
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(n as u32))
                    .context("launch vector_add")?;
            }
            self.stream.clone_dtoh(&out).context("dtoh")
        }

        /// `C = A · B`, A `m×k`, B `k×n`.
        pub fn matmul_nn(
            &self,
            a: &[f32],
            m: usize,
            k: usize,
            b: &[f32],
            n: usize,
        ) -> Result<Vec<f32>> {
            anyhow::ensure!(
                a.len() == m * k && b.len() == k * n,
                "matmul_nn dim mismatch"
            );
            self.matmul("matmul_nn", a, b, m, n, k)
        }

        /// `C = A · Bᵀ`, A `m×k`, B `n×k`.
        pub fn matmul_nt(
            &self,
            a: &[f32],
            m: usize,
            k: usize,
            b: &[f32],
            n: usize,
        ) -> Result<Vec<f32>> {
            anyhow::ensure!(
                a.len() == m * k && b.len() == n * k,
                "matmul_nt dim mismatch"
            );
            self.matmul("matmul_nt", a, b, m, n, k)
        }

        /// `C = Aᵀ · B`, A `k×m`, B `k×n`.
        pub fn matmul_tn(
            &self,
            a: &[f32],
            k: usize,
            m: usize,
            b: &[f32],
            n: usize,
        ) -> Result<Vec<f32>> {
            anyhow::ensure!(
                a.len() == k * m && b.len() == k * n,
                "matmul_tn dim mismatch"
            );
            self.matmul("matmul_tn", a, b, m, n, k)
        }

        fn matmul(
            &self,
            func: &str,
            a: &[f32],
            b: &[f32],
            m: usize,
            n: usize,
            k: usize,
        ) -> Result<Vec<f32>> {
            let a_dev = self.stream.clone_htod(a).context("htod a")?;
            let b_dev = self.stream.clone_htod(b).context("htod b")?;
            let mut c_dev = self.stream.alloc_zeros::<f32>(m * n).context("alloc c")?;
            let function = self
                .module
                .load_function(func)
                .with_context(|| format!("load {func}"))?;
            let (mi, ni, ki) = (m as i32, n as i32, k as i32);
            let mut builder = self.stream.launch_builder(&function);
            builder.arg(&a_dev);
            builder.arg(&b_dev);
            builder.arg(&mut c_dev);
            builder.arg(&mi);
            builder.arg(&ni);
            builder.arg(&ki);
            // SAFETY: 6 args match (const float*, const float*, float*, int, int, int)
            // and the device buffers hold m*k, k*n (or transposed), and m*n elements.
            unsafe {
                builder
                    .launch(cfg_2d(m, n))
                    .with_context(|| format!("launch {func}"))?;
            }
            self.stream.clone_dtoh(&c_dev).context("dtoh c")
        }

        /// Element-wise GELU on the GPU.
        pub fn gelu(&self, x: &[f32]) -> Result<Vec<f32>> {
            let n = x.len();
            let x_dev = self.stream.clone_htod(x).context("htod x")?;
            let mut y_dev = self.stream.alloc_zeros::<f32>(n).context("alloc y")?;
            let func = self.module.load_function("gelu_forward")?;
            let n_arg = n as i32;
            let mut builder = self.stream.launch_builder(&func);
            builder.arg(&x_dev);
            builder.arg(&mut y_dev);
            builder.arg(&n_arg);
            // SAFETY: 3 args match (const float*, float*, int); buffers hold n elements.
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(n as u32))
                    .context("launch gelu")?;
            }
            self.stream.clone_dtoh(&y_dev).context("dtoh y")
        }

        /// Backward of [`GpuKernels::gelu`].
        pub fn gelu_backward(&self, x: &[f32], dy: &[f32]) -> Result<Vec<f32>> {
            anyhow::ensure!(x.len() == dy.len(), "gelu_backward length mismatch");
            let n = x.len();
            let x_dev = self.stream.clone_htod(x).context("htod x")?;
            let dy_dev = self.stream.clone_htod(dy).context("htod dy")?;
            let mut dx_dev = self.stream.alloc_zeros::<f32>(n).context("alloc dx")?;
            let func = self.module.load_function("gelu_backward")?;
            let n_arg = n as i32;
            let mut builder = self.stream.launch_builder(&func);
            builder.arg(&x_dev);
            builder.arg(&dy_dev);
            builder.arg(&mut dx_dev);
            builder.arg(&n_arg);
            // SAFETY: 4 args match (const float*, const float*, float*, int).
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(n as u32))
                    .context("launch gelu_backward")?;
            }
            self.stream.clone_dtoh(&dx_dev).context("dtoh dx")
        }

        /// Row-wise softmax over `x` (`rows×cols`).
        pub fn softmax(&self, x: &[f32], rows: usize, cols: usize) -> Result<Vec<f32>> {
            anyhow::ensure!(x.len() == rows * cols, "softmax dim mismatch");
            let x_dev = self.stream.clone_htod(x).context("htod x")?;
            let mut y_dev = self
                .stream
                .alloc_zeros::<f32>(rows * cols)
                .context("alloc y")?;
            let func = self.module.load_function("softmax_forward")?;
            let (ri, ci) = (rows as i32, cols as i32);
            let mut builder = self.stream.launch_builder(&func);
            builder.arg(&x_dev);
            builder.arg(&mut y_dev);
            builder.arg(&ri);
            builder.arg(&ci);
            // SAFETY: 4 args match (const float*, float*, int, int); one thread per row.
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(rows as u32))
                    .context("launch softmax")?;
            }
            self.stream.clone_dtoh(&y_dev).context("dtoh y")
        }

        /// Backward of row-wise softmax given forward output `y` and `dy`.
        pub fn softmax_backward(
            &self,
            y: &[f32],
            dy: &[f32],
            rows: usize,
            cols: usize,
        ) -> Result<Vec<f32>> {
            anyhow::ensure!(
                y.len() == rows * cols && dy.len() == rows * cols,
                "softmax_backward dim mismatch"
            );
            let y_dev = self.stream.clone_htod(y).context("htod y")?;
            let dy_dev = self.stream.clone_htod(dy).context("htod dy")?;
            let mut dx_dev = self
                .stream
                .alloc_zeros::<f32>(rows * cols)
                .context("alloc dx")?;
            let func = self.module.load_function("softmax_backward")?;
            let (ri, ci) = (rows as i32, cols as i32);
            let mut builder = self.stream.launch_builder(&func);
            builder.arg(&y_dev);
            builder.arg(&dy_dev);
            builder.arg(&mut dx_dev);
            builder.arg(&ri);
            builder.arg(&ci);
            // SAFETY: 5 args match (const float*, const float*, float*, int, int).
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(rows as u32))
                    .context("launch softmax_backward")?;
            }
            self.stream.clone_dtoh(&dx_dev).context("dtoh dx")
        }

        /// LayerNorm forward; returns `(y, mean, rstd)` with per-row mean/rstd.
        pub fn layernorm_forward(
            &self,
            x: &[f32],
            gamma: &[f32],
            beta: &[f32],
            rows: usize,
            cols: usize,
            eps: f32,
        ) -> Result<(Vec<f32>, Vec<f32>, Vec<f32>)> {
            anyhow::ensure!(
                x.len() == rows * cols && gamma.len() == cols && beta.len() == cols,
                "layernorm_forward dim mismatch"
            );
            let x_dev = self.stream.clone_htod(x).context("htod x")?;
            let g_dev = self.stream.clone_htod(gamma).context("htod gamma")?;
            let b_dev = self.stream.clone_htod(beta).context("htod beta")?;
            let mut y_dev = self
                .stream
                .alloc_zeros::<f32>(rows * cols)
                .context("alloc y")?;
            let mut mean_dev = self.stream.alloc_zeros::<f32>(rows).context("alloc mean")?;
            let mut rstd_dev = self.stream.alloc_zeros::<f32>(rows).context("alloc rstd")?;
            let func = self.module.load_function("layernorm_forward")?;
            let (ri, ci) = (rows as i32, cols as i32);
            let mut builder = self.stream.launch_builder(&func);
            builder.arg(&x_dev);
            builder.arg(&g_dev);
            builder.arg(&b_dev);
            builder.arg(&mut y_dev);
            builder.arg(&mut mean_dev);
            builder.arg(&mut rstd_dev);
            builder.arg(&ri);
            builder.arg(&ci);
            builder.arg(&eps);
            // SAFETY: 9 args match the layernorm_forward signature; one thread per row.
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(rows as u32))
                    .context("launch layernorm_forward")?;
            }
            Ok((
                self.stream.clone_dtoh(&y_dev).context("dtoh y")?,
                self.stream.clone_dtoh(&mean_dev).context("dtoh mean")?,
                self.stream.clone_dtoh(&rstd_dev).context("dtoh rstd")?,
            ))
        }

        /// LayerNorm backward; returns `(dx, dgamma, dbeta)`.
        #[allow(clippy::too_many_arguments)]
        pub fn layernorm_backward(
            &self,
            x: &[f32],
            gamma: &[f32],
            dy: &[f32],
            mean: &[f32],
            rstd: &[f32],
            rows: usize,
            cols: usize,
        ) -> Result<(Vec<f32>, Vec<f32>, Vec<f32>)> {
            let x_dev = self.stream.clone_htod(x).context("htod x")?;
            let g_dev = self.stream.clone_htod(gamma).context("htod gamma")?;
            let dy_dev = self.stream.clone_htod(dy).context("htod dy")?;
            let mean_dev = self.stream.clone_htod(mean).context("htod mean")?;
            let rstd_dev = self.stream.clone_htod(rstd).context("htod rstd")?;
            let mut dx_dev = self
                .stream
                .alloc_zeros::<f32>(rows * cols)
                .context("alloc dx")?;
            // alloc_zeros is required: the kernel atomicAdd-accumulates into these.
            let mut dgamma_dev = self
                .stream
                .alloc_zeros::<f32>(cols)
                .context("alloc dgamma")?;
            let mut dbeta_dev = self
                .stream
                .alloc_zeros::<f32>(cols)
                .context("alloc dbeta")?;
            let func = self.module.load_function("layernorm_backward")?;
            let (ri, ci) = (rows as i32, cols as i32);
            let mut builder = self.stream.launch_builder(&func);
            builder.arg(&x_dev);
            builder.arg(&g_dev);
            builder.arg(&dy_dev);
            builder.arg(&mean_dev);
            builder.arg(&rstd_dev);
            builder.arg(&mut dx_dev);
            builder.arg(&mut dgamma_dev);
            builder.arg(&mut dbeta_dev);
            builder.arg(&ri);
            builder.arg(&ci);
            // SAFETY: 10 args match the layernorm_backward signature; one thread per row.
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(rows as u32))
                    .context("launch layernorm_backward")?;
            }
            Ok((
                self.stream.clone_dtoh(&dx_dev).context("dtoh dx")?,
                self.stream.clone_dtoh(&dgamma_dev).context("dtoh dgamma")?,
                self.stream.clone_dtoh(&dbeta_dev).context("dtoh dbeta")?,
            ))
        }

        /// Decoupled AdamW update over a flat parameter block, in place.
        #[allow(clippy::too_many_arguments)]
        pub fn adamw_update(
            &self,
            param: &mut [f32],
            grad: &[f32],
            m: &mut [f32],
            v: &mut [f32],
            t: u32,
            lr: f32,
            beta1: f32,
            beta2: f32,
            eps: f32,
            weight_decay: f32,
        ) -> Result<()> {
            let n = param.len();
            anyhow::ensure!(
                grad.len() == n && m.len() == n && v.len() == n,
                "adamw length mismatch"
            );
            let mut p_dev = self.stream.clone_htod(&*param).context("htod param")?;
            let g_dev = self.stream.clone_htod(grad).context("htod grad")?;
            let mut m_dev = self.stream.clone_htod(&*m).context("htod m")?;
            let mut v_dev = self.stream.clone_htod(&*v).context("htod v")?;
            let bc1 = 1.0f32 - beta1.powi(t as i32);
            let bc2 = 1.0f32 - beta2.powi(t as i32);
            let func = self.module.load_function("adamw_update")?;
            let n_arg = n as i32;
            let mut builder = self.stream.launch_builder(&func);
            builder.arg(&mut p_dev);
            builder.arg(&g_dev);
            builder.arg(&mut m_dev);
            builder.arg(&mut v_dev);
            builder.arg(&lr);
            builder.arg(&beta1);
            builder.arg(&beta2);
            builder.arg(&eps);
            builder.arg(&weight_decay);
            builder.arg(&bc1);
            builder.arg(&bc2);
            builder.arg(&n_arg);
            // SAFETY: 12 args match the adamw_update signature; buffers hold n elements.
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(n as u32))
                    .context("launch adamw_update")?;
            }
            param.copy_from_slice(&self.stream.clone_dtoh(&p_dev).context("dtoh param")?);
            m.copy_from_slice(&self.stream.clone_dtoh(&m_dev).context("dtoh m")?);
            v.copy_from_slice(&self.stream.clone_dtoh(&v_dev).context("dtoh v")?);
            Ok(())
        }

        // ─── Device-resident ops (no host round-trips between calls) ───

        /// Upload a host slice to a new device tensor.
        pub fn to_device(&self, host: &[f32], rows: usize, cols: usize) -> Result<DeviceTensor> {
            anyhow::ensure!(host.len() == rows * cols, "to_device dim mismatch");
            Ok(DeviceTensor {
                buf: self.stream.clone_htod(host).context("htod")?,
                rows,
                cols,
            })
        }

        /// A zeroed device tensor.
        pub fn zeros_device(&self, rows: usize, cols: usize) -> Result<DeviceTensor> {
            Ok(DeviceTensor {
                buf: self
                    .stream
                    .alloc_zeros::<f32>(rows * cols)
                    .context("alloc")?,
                rows,
                cols,
            })
        }

        /// Download a device tensor to host.
        pub fn to_host(&self, t: &DeviceTensor) -> Result<Vec<f32>> {
            self.stream.clone_dtoh(&t.buf).context("dtoh")
        }

        fn dev_mm(
            &self,
            func: &str,
            a: &CudaSlice<f32>,
            b: &CudaSlice<f32>,
            m: usize,
            n: usize,
            k: usize,
        ) -> Result<CudaSlice<f32>> {
            let mut c = self.stream.alloc_zeros::<f32>(m * n).context("alloc c")?;
            let function = self
                .module
                .load_function(func)
                .with_context(|| format!("load {func}"))?;
            let (mi, ni, ki) = (m as i32, n as i32, k as i32);
            let mut builder = self.stream.launch_builder(&function);
            builder.arg(a);
            builder.arg(b);
            builder.arg(&mut c);
            builder.arg(&mi);
            builder.arg(&ni);
            builder.arg(&ki);
            // SAFETY: 6 args match the matmul kernel signature.
            unsafe {
                builder
                    .launch(cfg_2d(m, n))
                    .with_context(|| format!("launch {func}"))?;
            }
            Ok(c)
        }

        /// `C = A · B` (A `m×k`, B `k×n`), device-resident.
        pub fn dev_matmul_nn(&self, a: &DeviceTensor, b: &DeviceTensor) -> Result<DeviceTensor> {
            anyhow::ensure!(a.cols == b.rows, "dev_matmul_nn inner dim mismatch");
            Ok(DeviceTensor {
                buf: self.dev_mm("matmul_nn", &a.buf, &b.buf, a.rows, b.cols, a.cols)?,
                rows: a.rows,
                cols: b.cols,
            })
        }

        /// `C = A · Bᵀ` (A `m×k`, B `n×k`), device-resident.
        pub fn dev_matmul_nt(&self, a: &DeviceTensor, b: &DeviceTensor) -> Result<DeviceTensor> {
            anyhow::ensure!(a.cols == b.cols, "dev_matmul_nt inner dim mismatch");
            Ok(DeviceTensor {
                buf: self.dev_mm("matmul_nt", &a.buf, &b.buf, a.rows, b.rows, a.cols)?,
                rows: a.rows,
                cols: b.rows,
            })
        }

        /// `C = Aᵀ · B` (A `k×m`, B `k×n`), device-resident.
        pub fn dev_matmul_tn(&self, a: &DeviceTensor, b: &DeviceTensor) -> Result<DeviceTensor> {
            anyhow::ensure!(a.rows == b.rows, "dev_matmul_tn inner dim mismatch");
            Ok(DeviceTensor {
                buf: self.dev_mm("matmul_tn", &a.buf, &b.buf, a.cols, b.cols, a.rows)?,
                rows: a.cols,
                cols: b.cols,
            })
        }

        /// `y[r,c] += bias[c]` in place (bias is `1×cols`).
        pub fn dev_bias_add(&self, y: &mut DeviceTensor, bias: &DeviceTensor) -> Result<()> {
            anyhow::ensure!(bias.len() == y.cols, "dev_bias_add dim mismatch");
            let function = self.module.load_function("bias_add")?;
            let (ri, ci) = (y.rows as i32, y.cols as i32);
            let mut builder = self.stream.launch_builder(&function);
            builder.arg(&mut y.buf);
            builder.arg(&bias.buf);
            builder.arg(&ri);
            builder.arg(&ci);
            // SAFETY: 4 args match bias_add(float*, const float*, int, int).
            unsafe {
                builder
                    .launch(cfg_2d(y.rows, y.cols))
                    .context("launch bias_add")?;
            }
            Ok(())
        }

        /// Column sum `db[c] = Σ_r dy[r,c]` → a `1×cols` device tensor.
        pub fn dev_col_sum(&self, dy: &DeviceTensor) -> Result<DeviceTensor> {
            let mut db = self
                .stream
                .alloc_zeros::<f32>(dy.cols)
                .context("alloc db")?;
            let function = self.module.load_function("col_sum")?;
            let (ri, ci) = (dy.rows as i32, dy.cols as i32);
            let mut builder = self.stream.launch_builder(&function);
            builder.arg(&dy.buf);
            builder.arg(&mut db);
            builder.arg(&ri);
            builder.arg(&ci);
            // SAFETY: 4 args match col_sum(const float*, float*, int, int).
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(dy.cols as u32))
                    .context("launch col_sum")?;
            }
            Ok(DeviceTensor {
                buf: db,
                rows: 1,
                cols: dy.cols,
            })
        }

        /// `out[i] += a[i]` in place (residual add).
        pub fn dev_add_inplace(&self, out: &mut DeviceTensor, a: &DeviceTensor) -> Result<()> {
            anyhow::ensure!(out.len() == a.len(), "dev_add_inplace length mismatch");
            let n = out.len();
            let function = self.module.load_function("add_inplace")?;
            let n_arg = n as i32;
            let mut builder = self.stream.launch_builder(&function);
            builder.arg(&mut out.buf);
            builder.arg(&a.buf);
            builder.arg(&n_arg);
            // SAFETY: 3 args match add_inplace(float*, const float*, int).
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(n as u32))
                    .context("launch add_inplace")?;
            }
            Ok(())
        }

        /// Device-resident AdamW update over a parameter tensor in place.
        #[allow(clippy::too_many_arguments)]
        pub fn dev_adamw(
            &self,
            param: &mut DeviceTensor,
            grad: &DeviceTensor,
            m: &mut DeviceTensor,
            v: &mut DeviceTensor,
            t: u32,
            lr: f32,
            beta1: f32,
            beta2: f32,
            eps: f32,
            weight_decay: f32,
        ) -> Result<()> {
            let n = param.len();
            anyhow::ensure!(
                grad.len() == n && m.len() == n && v.len() == n,
                "dev_adamw length mismatch"
            );
            let bc1 = 1.0f32 - beta1.powi(t as i32);
            let bc2 = 1.0f32 - beta2.powi(t as i32);
            let function = self.module.load_function("adamw_update")?;
            let n_arg = n as i32;
            let mut builder = self.stream.launch_builder(&function);
            builder.arg(&mut param.buf);
            builder.arg(&grad.buf);
            builder.arg(&mut m.buf);
            builder.arg(&mut v.buf);
            builder.arg(&lr);
            builder.arg(&beta1);
            builder.arg(&beta2);
            builder.arg(&eps);
            builder.arg(&weight_decay);
            builder.arg(&bc1);
            builder.arg(&bc2);
            builder.arg(&n_arg);
            // SAFETY: 12 args match the adamw_update kernel signature.
            unsafe {
                builder
                    .launch(LaunchConfig::for_num_elems(n as u32))
                    .context("launch dev_adamw")?;
            }
            Ok(())
        }

        // ─── Device-resident elementwise / norm / attention-helper ops ───

        /// GELU forward (device-resident).
        pub fn dev_gelu(&self, x: &DeviceTensor) -> Result<DeviceTensor> {
            let mut y = self.zeros_device(x.rows, x.cols)?;
            let func = self.module.load_function("gelu_forward")?;
            let n = x.len() as i32;
            let mut b = self.stream.launch_builder(&func);
            b.arg(&x.buf);
            b.arg(&mut y.buf);
            b.arg(&n);
            // SAFETY: 3 args match gelu_forward(const float*, float*, int).
            unsafe {
                b.launch(LaunchConfig::for_num_elems(x.len() as u32))
                    .context("launch dev_gelu")?;
            }
            Ok(y)
        }

        /// GELU backward (device-resident).
        pub fn dev_gelu_backward(
            &self,
            x: &DeviceTensor,
            dy: &DeviceTensor,
        ) -> Result<DeviceTensor> {
            anyhow::ensure!(x.len() == dy.len(), "dev_gelu_backward length mismatch");
            let mut dx = self.zeros_device(x.rows, x.cols)?;
            let func = self.module.load_function("gelu_backward")?;
            let n = x.len() as i32;
            let mut b = self.stream.launch_builder(&func);
            b.arg(&x.buf);
            b.arg(&dy.buf);
            b.arg(&mut dx.buf);
            b.arg(&n);
            // SAFETY: 4 args match gelu_backward(const float*, const float*, float*, int).
            unsafe {
                b.launch(LaunchConfig::for_num_elems(x.len() as u32))
                    .context("launch dev_gelu_backward")?;
            }
            Ok(dx)
        }

        /// Row-wise softmax (device-resident).
        pub fn dev_softmax(&self, x: &DeviceTensor) -> Result<DeviceTensor> {
            let mut y = self.zeros_device(x.rows, x.cols)?;
            let func = self.module.load_function("softmax_forward")?;
            let (ri, ci) = (x.rows as i32, x.cols as i32);
            let mut b = self.stream.launch_builder(&func);
            b.arg(&x.buf);
            b.arg(&mut y.buf);
            b.arg(&ri);
            b.arg(&ci);
            // SAFETY: 4 args match softmax_forward; one thread per row.
            unsafe {
                b.launch(LaunchConfig::for_num_elems(x.rows as u32))
                    .context("launch dev_softmax")?;
            }
            Ok(y)
        }

        /// Row-wise softmax backward (device-resident).
        pub fn dev_softmax_backward(
            &self,
            y: &DeviceTensor,
            dy: &DeviceTensor,
        ) -> Result<DeviceTensor> {
            anyhow::ensure!(y.len() == dy.len(), "dev_softmax_backward dim mismatch");
            let mut dx = self.zeros_device(y.rows, y.cols)?;
            let func = self.module.load_function("softmax_backward")?;
            let (ri, ci) = (y.rows as i32, y.cols as i32);
            let mut b = self.stream.launch_builder(&func);
            b.arg(&y.buf);
            b.arg(&dy.buf);
            b.arg(&mut dx.buf);
            b.arg(&ri);
            b.arg(&ci);
            // SAFETY: 5 args match softmax_backward; one thread per row.
            unsafe {
                b.launch(LaunchConfig::for_num_elems(y.rows as u32))
                    .context("launch dev_softmax_backward")?;
            }
            Ok(dx)
        }

        /// LayerNorm forward (device-resident) → `(y, mean, rstd)`.
        pub fn dev_layernorm_forward(
            &self,
            x: &DeviceTensor,
            gamma: &DeviceTensor,
            beta: &DeviceTensor,
            eps: f32,
        ) -> Result<(DeviceTensor, DeviceTensor, DeviceTensor)> {
            anyhow::ensure!(
                gamma.len() == x.cols && beta.len() == x.cols,
                "layernorm dim mismatch"
            );
            let mut y = self.zeros_device(x.rows, x.cols)?;
            let mut mean = self.zeros_device(x.rows, 1)?;
            let mut rstd = self.zeros_device(x.rows, 1)?;
            let func = self.module.load_function("layernorm_forward")?;
            let (ri, ci) = (x.rows as i32, x.cols as i32);
            let mut b = self.stream.launch_builder(&func);
            b.arg(&x.buf);
            b.arg(&gamma.buf);
            b.arg(&beta.buf);
            b.arg(&mut y.buf);
            b.arg(&mut mean.buf);
            b.arg(&mut rstd.buf);
            b.arg(&ri);
            b.arg(&ci);
            b.arg(&eps);
            // SAFETY: 9 args match layernorm_forward; one thread per row.
            unsafe {
                b.launch(LaunchConfig::for_num_elems(x.rows as u32))
                    .context("launch dev_layernorm_forward")?;
            }
            Ok((y, mean, rstd))
        }

        /// LayerNorm backward (device-resident) → `(dx, dgamma, dbeta)`.
        pub fn dev_layernorm_backward(
            &self,
            x: &DeviceTensor,
            gamma: &DeviceTensor,
            dy: &DeviceTensor,
            mean: &DeviceTensor,
            rstd: &DeviceTensor,
        ) -> Result<(DeviceTensor, DeviceTensor, DeviceTensor)> {
            let mut dx = self.zeros_device(x.rows, x.cols)?;
            // zeroed: the kernel atomicAdd-accumulates these across rows.
            let mut dgamma = self.zeros_device(1, x.cols)?;
            let mut dbeta = self.zeros_device(1, x.cols)?;
            let func = self.module.load_function("layernorm_backward")?;
            let (ri, ci) = (x.rows as i32, x.cols as i32);
            let mut b = self.stream.launch_builder(&func);
            b.arg(&x.buf);
            b.arg(&gamma.buf);
            b.arg(&dy.buf);
            b.arg(&mean.buf);
            b.arg(&rstd.buf);
            b.arg(&mut dx.buf);
            b.arg(&mut dgamma.buf);
            b.arg(&mut dbeta.buf);
            b.arg(&ri);
            b.arg(&ci);
            // SAFETY: 10 args match layernorm_backward; one thread per row.
            unsafe {
                b.launch(LaunchConfig::for_num_elems(x.rows as u32))
                    .context("launch dev_layernorm_backward")?;
            }
            Ok((dx, dgamma, dbeta))
        }

        /// Extract columns `[col0, col0+w)` of `src` into a new `rows×w` tensor.
        pub fn dev_slice_cols(
            &self,
            src: &DeviceTensor,
            col0: usize,
            w: usize,
        ) -> Result<DeviceTensor> {
            anyhow::ensure!(col0 + w <= src.cols, "slice_cols out of range");
            let mut dst = self.zeros_device(src.rows, w)?;
            let func = self.module.load_function("slice_cols")?;
            let (ri, ti, c0, wi) = (src.rows as i32, src.cols as i32, col0 as i32, w as i32);
            let mut b = self.stream.launch_builder(&func);
            b.arg(&src.buf);
            b.arg(&mut dst.buf);
            b.arg(&ri);
            b.arg(&ti);
            b.arg(&c0);
            b.arg(&wi);
            // SAFETY: 6 args match slice_cols.
            unsafe {
                b.launch(cfg_2d(src.rows, w)).context("launch slice_cols")?;
            }
            Ok(dst)
        }

        /// Write `src` (`rows×w`) into columns `[col0, col0+w)` of `out` in place.
        pub fn dev_set_cols(
            &self,
            out: &mut DeviceTensor,
            src: &DeviceTensor,
            col0: usize,
        ) -> Result<()> {
            anyhow::ensure!(
                src.rows == out.rows && col0 + src.cols <= out.cols,
                "set_cols out of range"
            );
            let w = src.cols;
            let func = self.module.load_function("set_cols")?;
            let (ri, ti, c0, wi) = (out.rows as i32, out.cols as i32, col0 as i32, w as i32);
            let mut b = self.stream.launch_builder(&func);
            b.arg(&mut out.buf);
            b.arg(&src.buf);
            b.arg(&ri);
            b.arg(&ti);
            b.arg(&c0);
            b.arg(&wi);
            // SAFETY: 6 args match set_cols.
            unsafe {
                b.launch(cfg_2d(out.rows, w)).context("launch set_cols")?;
            }
            Ok(())
        }

        /// In-place scale + causal mask over a `T×T` score matrix.
        pub fn dev_scale_causal_mask(&self, s: &mut DeviceTensor, scale: f32) -> Result<()> {
            anyhow::ensure!(
                s.rows == s.cols,
                "scale_causal_mask expects a square matrix"
            );
            let func = self.module.load_function("scale_causal_mask")?;
            let ti = s.rows as i32;
            let mut b = self.stream.launch_builder(&func);
            b.arg(&mut s.buf);
            b.arg(&ti);
            b.arg(&scale);
            // SAFETY: 3 args match scale_causal_mask(float*, int, float).
            unsafe {
                b.launch(cfg_2d(s.rows, s.cols))
                    .context("launch scale_causal_mask")?;
            }
            Ok(())
        }

        /// Backward of [`GpuKernels::dev_scale_causal_mask`].
        pub fn dev_scale_causal_mask_grad(&self, s: &mut DeviceTensor, scale: f32) -> Result<()> {
            anyhow::ensure!(
                s.rows == s.cols,
                "scale_causal_mask_grad expects a square matrix"
            );
            let func = self.module.load_function("scale_causal_mask_grad")?;
            let ti = s.rows as i32;
            let mut b = self.stream.launch_builder(&func);
            b.arg(&mut s.buf);
            b.arg(&ti);
            b.arg(&scale);
            // SAFETY: 3 args match scale_causal_mask_grad(float*, int, float).
            unsafe {
                b.launch(cfg_2d(s.rows, s.cols))
                    .context("launch scale_causal_mask_grad")?;
            }
            Ok(())
        }
    }
}

/// Device-resident layers built on [`gpu::GpuKernels`] (only with `--features cuda`).
#[cfg(feature = "cuda")]
pub mod device;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_vector_add_is_elementwise() {
        assert_eq!(vector_add_cpu(&[1.0, 2.0], &[10.0, 20.0]), vec![11.0, 22.0]);
    }

    #[test]
    fn cpu_matmul_forms_are_consistent() {
        // A 2x3, B 3x2.
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]; // 2x3
        let b = [7.0, 8.0, 9.0, 10.0, 11.0, 12.0]; // 3x2
        let nn = matmul_nn_cpu(&a, 2, 3, &b, 2);
        assert_eq!(nn, vec![58.0, 64.0, 139.0, 154.0]);
        // A·Bᵀ with B stored 2x3 equals A·B with B stored 3x2 transposed.
        let bt = [7.0, 9.0, 11.0, 8.0, 10.0, 12.0]; // (3x2)^T = 2x3
        assert_eq!(matmul_nt_cpu(&a, 2, 3, &bt, 2), nn);
        // Aᵀ·B with A stored 3x2 equals (2x3)·B with A transposed.
        let at = [1.0, 4.0, 2.0, 5.0, 3.0, 6.0]; // (2x3)^T = 3x2
        assert_eq!(matmul_tn_cpu(&at, 3, 2, &b, 2), nn);
    }

    #[test]
    fn cpu_softmax_rows_sum_to_one() {
        let y = softmax_cpu(&[1.0, 2.0, 3.0, 0.0, 5.0, 0.0], 2, 3);
        assert!((y[0..3].iter().sum::<f32>() - 1.0).abs() < 1.0e-6);
        assert!((y[3..6].iter().sum::<f32>() - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn cpu_layernorm_zero_centers_rows() {
        let (y, _m, _r) = layernorm_forward_cpu(
            &[1.0, 2.0, 3.0, 4.0],
            &[1.0, 1.0],
            &[0.0, 0.0],
            2,
            2,
            1.0e-5,
        );
        // gamma=1, beta=0 => each row is zero-mean.
        assert!((y[0] + y[1]).abs() < 1.0e-4);
        assert!((y[2] + y[3]).abs() < 1.0e-4);
    }

    #[test]
    fn cpu_adamw_moves_params_toward_lower_grad() {
        let mut p = [1.0f32, -2.0, 3.0];
        let (mut m, mut v) = (vec![0.0f32; 3], vec![0.0f32; 3]);
        let before = p;
        adamw_update_cpu(
            &mut p,
            &[1.0, 1.0, 1.0],
            &mut m,
            &mut v,
            1,
            0.1,
            0.9,
            0.999,
            1.0e-8,
            0.0,
        );
        // positive gradient => parameters decrease.
        for (a, b) in p.iter().zip(before.iter()) {
            assert!(a < b);
        }
    }

    // ─── GPU parity tests (hardware-gated behind --features cuda) ───
    #[cfg(feature = "cuda")]
    mod cuda {
        use super::*;

        // Deterministic pseudo-random fill so tests vary without an RNG dep.
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

        fn assert_close(gpu: &[f32], cpu: &[f32], tol: f32) {
            assert_eq!(gpu.len(), cpu.len());
            for (i, (g, c)) in gpu.iter().zip(cpu.iter()).enumerate() {
                let diff = (g - c).abs();
                assert!(
                    diff <= tol * (1.0 + c.abs()),
                    "mismatch at {i}: gpu={g} cpu={c} diff={diff}"
                );
            }
        }

        #[test]
        fn gpu_vector_add_matches_cpu() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let a = fill(4096, 1);
            let b = fill(4096, 2);
            assert_close(&k.vector_add(&a, &b).unwrap(), &vector_add_cpu(&a, &b), 0.0);
        }

        #[test]
        fn gpu_matmul_nn_matches_cpu() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let (m, kk, n) = (37, 53, 41);
            let a = fill(m * kk, 3);
            let b = fill(kk * n, 4);
            assert_close(
                &k.matmul_nn(&a, m, kk, &b, n).unwrap(),
                &matmul_nn_cpu(&a, m, kk, &b, n),
                1.0e-3,
            );
        }

        #[test]
        fn gpu_matmul_nt_matches_cpu() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let (m, kk, n) = (33, 48, 29);
            let a = fill(m * kk, 5);
            let b = fill(n * kk, 6);
            assert_close(
                &k.matmul_nt(&a, m, kk, &b, n).unwrap(),
                &matmul_nt_cpu(&a, m, kk, &b, n),
                1.0e-3,
            );
        }

        #[test]
        fn gpu_matmul_tn_matches_cpu() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let (kk, m, n) = (50, 31, 27);
            let a = fill(kk * m, 7);
            let b = fill(kk * n, 8);
            assert_close(
                &k.matmul_tn(&a, kk, m, &b, n).unwrap(),
                &matmul_tn_cpu(&a, kk, m, &b, n),
                1.0e-3,
            );
        }

        #[test]
        fn gpu_matmul_nt_equals_nn_on_transposed_input() {
            // Cross-check: A·Bᵀ via matmul_nt == A·B via matmul_nn when B is the
            // transpose. Guards the index math, not just the CPU oracle.
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let (m, kk, n) = (16, 24, 20);
            let a = fill(m * kk, 9);
            let b_kn = fill(kk * n, 10); // k×n
            let mut b_nk = vec![0.0f32; n * kk]; // n×k = (k×n)^T
            for r in 0..kk {
                for col in 0..n {
                    b_nk[col * kk + r] = b_kn[r * n + col];
                }
            }
            let nn = k.matmul_nn(&a, m, kk, &b_kn, n).unwrap();
            let nt = k.matmul_nt(&a, m, kk, &b_nk, n).unwrap();
            assert_close(&nt, &nn, 1.0e-4);
        }

        #[test]
        fn gpu_gelu_matches_cpu() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let x = fill(2048, 11);
            assert_close(&k.gelu(&x).unwrap(), &gelu_cpu(&x), 1.0e-3);
        }

        #[test]
        fn gpu_device_name_is_reported() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            assert!(!k.device_name().is_empty());
        }

        #[test]
        fn gpu_softmax_matches_cpu() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let (rows, cols) = (40, 17);
            let x = fill(rows * cols, 20);
            assert_close(
                &k.softmax(&x, rows, cols).unwrap(),
                &softmax_cpu(&x, rows, cols),
                1.0e-4,
            );
        }

        #[test]
        fn gpu_softmax_backward_matches_cpu() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let (rows, cols) = (33, 21);
            let x = fill(rows * cols, 21);
            let y = softmax_cpu(&x, rows, cols);
            let dy = fill(rows * cols, 22);
            assert_close(
                &k.softmax_backward(&y, &dy, rows, cols).unwrap(),
                &softmax_backward_cpu(&y, &dy, rows, cols),
                1.0e-4,
            );
        }

        #[test]
        fn gpu_layernorm_forward_matches_cpu() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let (rows, cols) = (24, 32);
            let x = fill(rows * cols, 23);
            let gamma: Vec<f32> = fill(cols, 24).iter().map(|v| 1.0 + v * 0.1).collect();
            let beta = fill(cols, 25);
            let (gy, gm, gr) = k
                .layernorm_forward(&x, &gamma, &beta, rows, cols, 1.0e-5)
                .unwrap();
            let (cy, cm, cr) = layernorm_forward_cpu(&x, &gamma, &beta, rows, cols, 1.0e-5);
            assert_close(&gy, &cy, 1.0e-3);
            assert_close(&gm, &cm, 1.0e-4);
            assert_close(&gr, &cr, 1.0e-3);
        }

        #[test]
        fn gpu_layernorm_backward_matches_cpu() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let (rows, cols) = (28, 30);
            let x = fill(rows * cols, 26);
            let gamma: Vec<f32> = fill(cols, 27).iter().map(|v| 1.0 + v * 0.1).collect();
            let beta = vec![0.0f32; cols];
            let (_y, mean, rstd) = layernorm_forward_cpu(&x, &gamma, &beta, rows, cols, 1.0e-5);
            let dy = fill(rows * cols, 28);
            let (gdx, gdg, gdb) = k
                .layernorm_backward(&x, &gamma, &dy, &mean, &rstd, rows, cols)
                .unwrap();
            let (cdx, cdg, cdb) = layernorm_backward_cpu(&x, &gamma, &dy, &mean, &rstd, rows, cols);
            assert_close(&gdx, &cdx, 1.0e-3);
            assert_close(&gdg, &cdg, 2.0e-3); // dgamma/dbeta use atomicAdd (order-dependent f32)
            assert_close(&gdb, &cdb, 2.0e-3);
        }

        #[test]
        fn gpu_gelu_backward_matches_cpu() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let x = fill(2048, 29);
            let dy = fill(2048, 30);
            assert_close(
                &k.gelu_backward(&x, &dy).unwrap(),
                &gelu_backward_cpu(&x, &dy),
                1.0e-3,
            );
        }

        #[test]
        fn gpu_adamw_matches_cpu_over_several_steps() {
            let k = gpu::GpuKernels::new(0).expect("gpu init");
            let n = 1024;
            let grad = fill(n, 32);
            let (mut gp, mut cp) = (fill(n, 31), fill(n, 31));
            let (mut gm, mut gv) = (vec![0.0f32; n], vec![0.0f32; n]);
            let (mut cm, mut cv) = (vec![0.0f32; n], vec![0.0f32; n]);
            for t in 1..=3u32 {
                k.adamw_update(
                    &mut gp, &grad, &mut gm, &mut gv, t, 0.01, 0.9, 0.999, 1.0e-8, 0.01,
                )
                .unwrap();
                adamw_update_cpu(
                    &mut cp, &grad, &mut cm, &mut cv, t, 0.01, 0.9, 0.999, 1.0e-8, 0.01,
                );
            }
            assert_close(&gp, &cp, 1.0e-4);
            assert_close(&gm, &cm, 1.0e-4);
            assert_close(&gv, &cv, 1.0e-4);
        }
    }
}
