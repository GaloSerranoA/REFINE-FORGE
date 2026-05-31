//! Hand-written CUDA compute for the native GPT GPU path (Phase 3, Milestone 1).
//!
//! Milestone 1 is a deliberate toolchain de-risk: a single `vector_add` CUDA
//! kernel launched from Rust via `cudarc` + runtime `nvrtc`, with a CPU
//! reference and a parity test. The point is to prove the
//! Windows + CUDA 13.2 + MSVC + cudarc path works on the RTX 3060 before
//! writing matmul/attention/layernorm kernels.
//!
//! The CUDA path is behind the `cuda` cargo feature so the default workspace
//! build (CI, no GPU) compiles a CPU-only crate with no CUDA dependency. The
//! parity test below only exists under `--features cuda` and runs on real
//! hardware — honest hardware-gating, not a disabled test.

/// CPU reference for element-wise vector addition. This is the correctness
/// oracle every GPU kernel is parity-checked against.
pub fn vector_add_cpu(a: &[f32], b: &[f32]) -> Vec<f32> {
    assert_eq!(a.len(), b.len(), "vector_add length mismatch");
    a.iter().zip(b.iter()).map(|(x, y)| x + y).collect()
}

/// CUDA C source for the Milestone-1 vector-add kernel, compiled at runtime
/// with nvrtc. Kept tiny on purpose.
#[cfg(feature = "cuda")]
pub const VECTOR_ADD_KERNEL: &str = r#"
extern "C" __global__ void vector_add(const float* a, const float* b, float* out, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        out[i] = a[i] + b[i];
    }
}
"#;

#[cfg(feature = "cuda")]
pub mod gpu {
    //! Real CUDA implementation (only compiled with `--features cuda`).
    use anyhow::{Context, Result};
    use cudarc::driver::{CudaContext, LaunchConfig, PushKernelArg};
    use cudarc::nvrtc::compile_ptx;

    /// Run `vector_add` on the GPU at device `ordinal`, compiling the kernel
    /// with nvrtc and copying through device memory. Returns the device result.
    pub fn vector_add(ordinal: usize, a: &[f32], b: &[f32]) -> Result<Vec<f32>> {
        anyhow::ensure!(a.len() == b.len(), "vector_add length mismatch");
        let n = a.len();
        let ctx = CudaContext::new(ordinal).context("opening CUDA context")?;
        let stream = ctx.default_stream();
        let ptx = compile_ptx(super::VECTOR_ADD_KERNEL).context("nvrtc compile vector_add")?;
        let module = ctx.load_module(ptx).context("loading vector_add module")?;
        let func = module
            .load_function("vector_add")
            .context("vector_add not found in module")?;
        let a_dev = stream.clone_htod(a).context("copy a to device")?;
        let b_dev = stream.clone_htod(b).context("copy b to device")?;
        let mut out_dev = stream
            .alloc_zeros::<f32>(n)
            .context("alloc device output")?;
        let n_arg = n as i32;
        let mut builder = stream.launch_builder(&func);
        // Argument order matches the kernel signature:
        // (const float* a, const float* b, float* out, int n).
        builder.arg(&a_dev);
        builder.arg(&b_dev);
        builder.arg(&mut out_dev);
        builder.arg(&n_arg);
        // SAFETY: the four arguments match the kernel's parameter types/arity
        // and the device buffers each hold `n` f32 elements.
        unsafe {
            builder
                .launch(LaunchConfig::for_num_elems(n as u32))
                .context("launching vector_add")?;
        }
        stream.clone_dtoh(&out_dev).context("copy result to host")
    }

    /// Probe: open device 0 and report its name, to verify the toolchain.
    pub fn device_summary() -> Result<String> {
        let ctx = CudaContext::new(0).context("opening CUDA device 0")?;
        let name = ctx.name().unwrap_or_else(|_| "unknown".to_string());
        Ok(format!("CUDA device 0 = {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_vector_add_is_elementwise() {
        let a = [1.0_f32, 2.0, 3.0];
        let b = [10.0_f32, 20.0, 30.0];
        assert_eq!(vector_add_cpu(&a, &b), vec![11.0, 22.0, 33.0]);
    }

    // Real-hardware parity test: only compiled with `--features cuda`, and only
    // meaningful on a machine with an NVIDIA GPU. It is the GPU analog of the
    // CPU gradient checks — a kernel that disagrees with the CPU reference fails.
    #[cfg(feature = "cuda")]
    #[test]
    fn gpu_vector_add_matches_cpu_reference() {
        let n = 4096;
        let a: Vec<f32> = (0..n).map(|i| i as f32 * 0.5).collect();
        let b: Vec<f32> = (0..n).map(|i| (n - i) as f32 * 0.25).collect();
        let cpu = vector_add_cpu(&a, &b);
        let gpu = gpu::vector_add(0, &a, &b).expect("GPU vector_add");
        assert_eq!(gpu.len(), cpu.len());
        // Element-wise f32 add is exact on both sides, so require exact parity.
        for (i, (g, c)) in gpu.iter().zip(cpu.iter()).enumerate() {
            assert_eq!(g, c, "mismatch at {i}: gpu={g} cpu={c}");
        }
    }
}
