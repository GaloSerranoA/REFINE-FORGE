# Plan 6 - Kernel Agent Production Proof Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans` to implement this plan task-by-task. Do not use delegated agents unless the operator explicitly requests them in the current turn.

**Goal:** Raise the CUDA/GPU kernel agent from `measured-only` bit-exact gate evidence to reviewer-approved CUDA correctness and performance production proof.

**Architecture:** Keep Refine-Forge as the evidence gate and make real kernel source, CPU reference outputs, deterministic fixtures, hardware matrix, compiler/runtime metadata, bit-exact runs, tolerance policy, performance baseline, and human review explicit requirements. Stub kernels remain allowed as measured fixtures but cannot become CUDA production proof.

**Tech Stack:** Rust, `refineforge-bitexact`, HELYX `helyx-kernels` compatibility, CUDA toolchain metadata, SHA-256 fixtures, benchmark reports, JSON schemas.

---

## 1. Current Level

Current live level:

| Agent | Status | Trust |
|---|---|---|
| Kernel | `passed` | `measured-only` |

Meaning: bit-exact metadata and deterministic run evidence are alive. It does not prove real CUDA source, hardware correctness, portability, or performance.

## 2. Production-Proof Target

The Kernel agent reaches enterprise production proof only when:

- Real kernel source exists and is cited by the config.
- CPU reference implementation or golden output is committed.
- Bit-exact gate runs on deterministic fixtures.
- Hardware matrix records GPU model, driver, CUDA toolkit, OS, architecture, and rustc version.
- Output hash or tolerance policy is enforced.
- Performance baseline records latency/throughput and regression threshold.
- HELYX kernel handoff records source, config, and evidence bundle hashes.
- Human reviewer approves kernel correctness and performance evidence.

## 3. File Map

- Modify: `crates/refineforge-cli/src/agent/kernel.rs`
  - Add production-proof requirements and hardware evidence ingestion.
- Modify: `crates/refineforge-bitexact/src/`
  - Add source/hardware/performance fields to reports.
- Modify: `crates/refineforge-cli/tests/agent_cli.rs`
  - Add kernel trust-boundary regression tests.
- Create: `kernels/hardware-matrix.example.json`
- Create: `docs/kernels/kernel-production-proof.md`
- Modify: `docs/agents/kernel-agent.md`
- Modify: `schemas/agent-report.schema.json` if Plan 3 production-proof envelope is not yet present.

## 4. Work Breakdown

### Task 1 - Freeze Kernel Trust Boundary

**Files:**
- Test: `crates/refineforge-cli/tests/agent_cli.rs`
- Modify: `crates/refineforge-cli/src/agent/kernel.rs`

- [ ] **Step 1: Add regression test**

Add:

```rust
#[test]
fn agent_kernel_stub_fixture_cannot_claim_cuda_correctness() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("kernel-prod");
    let output = run_refine(
        &["agent", "kernel", "--mode", "execute", "--target", "helyx"],
        &out,
    );
    assert_success(&output);
    let report = read_json(&out.join("kernel.json"));
    assert_eq!(report["trust_level"], "measured-only");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(report["production_proof"]["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b.as_str().unwrap().contains("CUDA")));
}
```

- [ ] **Step 2: Add production-proof requirements**

In `kernel.rs`, emit these requirement ids:

```text
kernel.real_source
kernel.cpu_reference
kernel.bitexact_fixture
kernel.hardware_matrix
kernel.compiler_runtime_metadata
kernel.performance_baseline
kernel.helyx_handoff
kernel.human_kernel_approval
```

The current `kernels/src/` empty state must block `kernel.real_source`.

- [ ] **Step 3: Run focused test**

Run:

```powershell
cargo test -p refineforge-cli --test agent_cli agent_kernel_stub_fixture_cannot_claim_cuda_correctness
```

Expected: pass after production-proof envelope exists and Kernel blockers are emitted.

### Task 2 - Add Hardware Matrix Contract

**Files:**
- Create: `kernels/hardware-matrix.example.json`
- Modify: `crates/refineforge-bitexact/src/`
- Test: `crates/refineforge-cli/tests/agent_cli.rs`

- [ ] **Step 1: Create example matrix**

Create:

```json
{
  "schema_version": "kernel-hardware-matrix-v1",
  "runs": [
    {
      "gpu_name": "local-smoke-placeholder",
      "gpu_arch": "not-cuda",
      "driver_version": null,
      "cuda_toolkit": null,
      "os": "local",
      "cpu_arch": "local",
      "rustc": null,
      "status": "not-production-evidence"
    }
  ]
}
```

- [ ] **Step 2: Require production matrix for CUDA proof**

The Kernel agent must accept the example matrix as documentation only and block production proof until a matrix run has:

```json
{
  "gpu_name": "non-empty",
  "gpu_arch": "non-empty",
  "driver_version": "non-empty",
  "cuda_toolkit": "non-empty",
  "status": "passed"
}
```

- [ ] **Step 3: Run kernel agent inspect**

Run:

```powershell
cargo run -p refineforge-cli --bin refine -- --root . agent kernel --mode inspect --target helyx --out agent-reports/kernel-prod --json
```

Expected: command passes, production proof remains blocked because example matrix is not production evidence.

### Task 3 - Add Source and Reference Requirements

**Files:**
- Modify: `kernels/configs/helyx-bitexact-smoke.yaml`
- Modify: `crates/refineforge-bitexact/src/`
- Test: `crates/refineforge-cli/tests/agent_cli.rs`

- [ ] **Step 1: Extend config schema**

Add fields to kernel configs:

```yaml
source:
  kind: stub
  path: null
reference:
  kind: text_fixture
  path: kernels/fixtures/helyx-bitexact-input.txt
production:
  requires_real_cuda: true
```

- [ ] **Step 2: Block stub source**

`source.kind: stub` must pass measured-only lint but block production proof with:

```text
kernel.real_source blocked: source.kind is stub
```

- [ ] **Step 3: Run bitexact tests**

Run:

```powershell
cargo test -p refineforge-bitexact
cargo test -p refineforge-cli --test agent_cli agent_kernel_execute_runs_lint_and_bitexact_gate
```

Expected: measured bit-exact gates still pass; production proof remains blocked.

### Task 4 - Add Kernel Production Checklist

**Files:**
- Create: `docs/kernels/kernel-production-proof.md`
- Modify: `docs/agents/kernel-agent.md`

- [ ] **Step 1: Create checklist**

Create:

```markdown
# Kernel Production Proof Checklist

The Kernel agent may emit `human-reviewed` only when the evidence pack includes:

| Requirement | Evidence |
|---|---|
| Real kernel source | source path and SHA-256 |
| CPU reference | reference implementation or golden output hash |
| Bit-exact run | run report and fixture hash |
| Hardware matrix | GPU, driver, CUDA toolkit, OS, CPU architecture |
| Compiler/runtime metadata | rustc, nvcc, build flags |
| Tolerance policy | exact hash or numeric tolerance justification |
| Performance baseline | latency/throughput and regression threshold |
| HELYX handoff | config, source, and report hashes |
| Human approval | named reviewer, date, decision |
```

- [ ] **Step 2: Link from kernel agent doc**

Add:

```markdown
CUDA correctness production proof is governed by `docs/kernels/kernel-production-proof.md`.
```

## 5. Acceptance Gate

Run:

```powershell
cargo clippy -p refineforge-cli --all-targets -- -D warnings
cargo test -p refineforge-bitexact
cargo test -p refineforge-cli --test agent_cli agent_kernel_execute_runs_lint_and_bitexact_gate
cargo run -p refineforge-cli --bin refine -- --root . agent kernel --mode execute --target helyx --out agent-reports/kernel-prod --json
```

Expected local final state: Kernel passes as `measured-only`, production proof remains blocked until real CUDA source, hardware matrix, performance baseline, HELYX handoff, and human approval evidence exist.
