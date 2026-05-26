//! refineforge-bitexact — bit-exact reproducibility gate.
//!
//! Given a GPU kernel (or any computation that should be
//! deterministic), this crate runs it N times and compares the
//! SHA-256 of the outputs. If any hash disagrees, the gate fails.
//!
//! The orchestration is generic; CUDA-specific concerns
//! (atomicAdd ordering, cuBLAS algorithm selection, cuDNN, mixed-
//! precision, etc.) are documented in `docs/bit-exact-reproducibility.md`.
//! This crate does NOT enforce determinism — it only DETECTS its
//! absence. Achieving determinism is the CUDA engineer's job (env
//! vars, deterministic algorithms, kernel rewrites). This crate
//! tells them whether they've succeeded.

pub mod experiment;
pub mod hash;
pub mod lint;
pub mod manifest;
pub mod mentor;
pub mod report;
pub mod run_all;
pub mod runner;
