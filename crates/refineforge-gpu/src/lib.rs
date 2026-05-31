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
"#;

#[cfg(feature = "cuda")]
pub mod gpu {
    //! Real CUDA implementation (only compiled with `--features cuda`).
    //!
    //! [`GpuKernels`] compiles the kernel module once (nvrtc) and reuses it for
    //! every launch, holding the context, default stream, and loaded module.
    use anyhow::{Context, Result};
    use cudarc::driver::{CudaContext, CudaModule, CudaStream, LaunchConfig, PushKernelArg};
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
    }
}

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
    }
}
