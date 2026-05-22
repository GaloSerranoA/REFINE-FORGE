# GPU / Kernel Bit-Exact Track Design

Date: 2026-05-22
Owner: Section 4 — GPU / Kernel Rust Engineer

## Intent

Complete the Refine-Forge Section 4 surface as an enterprise-grade compatibility
gate between external HELYX kernel implementations and Refine-Forge release
evidence.

The boundary is explicit:

| Part | Owns | Project |
|---|---|---|
| HELYX | `helyx-kernels` implementation, CUDA/HIP/Metal source, build flags, kernel launch code | HELYX |
| Refine-Forge | `refineforge-bitexact` gate, kernel experiment contract, deterministic evidence, CI reports, release docs | Refine-Forge |

Refine-Forge must not claim to implement production HELYX kernels. It must prove
whether a kernel command produces identical bytes under a declared runtime
contract and record enough evidence for release/reviewer audit.

## Existing State

Already shipped:

- `crates/refineforge-bitexact` with `run` and `report` commands.
- `KernelExperiment` YAML with `id`, `command`, `runs`, `output`, `env`, and
  `hardware`.
- Deterministic and non-deterministic stub scripts.
- CI smoke job for the two example configs.
- Section 4 methodology docs.

Missing for an enterprise gate:

- No schema/template version on kernel configs.
- No declared kernel producer (`helyx-kernels`) or kernel id.
- No expected output hash baseline, so stable-but-wrong output can pass.
- No input file manifest in reports, so reviewers cannot see which input bytes
  were gated.
- No strict-profile linter for missing CUDA determinism knobs.
- No first-class `run-all` command for CI artifact collection.
- No per-run manifest JSONL to support later report rebuilds and audit scraping.

## Design

### Kernel Experiment Contract

Extend `KernelExperiment` with optional fields:

- `template_version`: e.g. `refineforge-bitexact-v1`.
- `producer`: e.g. `helyx-kernels`.
- `kernel_id`: stable external kernel id, e.g. `helyx.attention.rope_v1`.
- `profile`: one of `generic`, `cuda_strict`, `helyx_cuda`.
- `expected_sha256`: optional baseline hash the output must match.
- `input_files`: optional list of deterministic input files to hash into the
  report.
- `tags`: sorted metadata tags for CI filtering and evidence search.

Validation remains backward compatible for existing examples. Strict checks live
in the new linter so old example configs still load.

### Baseline Hash Gate

The report outcome remains `Pass` only when:

1. Every run exited successfully.
2. Every run produced an output hash.
3. All output hashes are identical.
4. If `expected_sha256` is set, the unique hash equals that expected hash.

This closes the stable-but-wrong hole while preserving the existing bit-exact
semantics.

### Input Manifest

Before execution, hash every configured `input_files` entry with streaming
SHA-256. Store the sorted list in `bitexact-report.json`:

```json
{
  "input_manifest": [
    {
      "path": "kernels/fixtures/input.bin",
      "sha256": "...",
      "size_bytes": 1024
    }
  ]
}
```

Missing inputs are a gate error when the run executes. Dry-run and lint can show
the declared paths without reading GPU outputs.

### Strict Linter

Add `refine-bitexact lint <kernel.yaml>` and library function
`lint::lint_experiment`.

For `profile=generic`, only the base schema validation applies.

For `profile=cuda_strict` and `profile=helyx_cuda`, require:

- `runs >= 5`.
- `expected_sha256` present.
- `producer` present.
- `kernel_id` present.
- `hardware.gpu`, `hardware.cuda`, and `hardware.driver` present.
- `env.CUBLAS_WORKSPACE_CONFIG=:4096:8`.
- `env.CUDA_LAUNCH_BLOCKING=1`.

For `profile=helyx_cuda`, additionally require `producer=helyx-kernels` and a
`kernel_id` starting with `helyx.`.

The CLI exits nonzero when lint status is `fail`, writes machine-readable JSON
when requested, and prints concise human text otherwise.

### Run-All CI Command

Add:

```text
refine-bitexact run-all kernels/configs --include-examples --summary-json <path>
```

Behavior:

- Discover `*.yaml` deterministically by path sort.
- Skip `example-*` configs by default unless `--include-examples` is set.
- Execute each config with the same run semantics as `run`.
- Continue through all configs by default so CI gets a full summary.
- Exit nonzero if any included gate fails.
- Write summary JSON with config path, outcome, report path, and error text.

This replaces ad hoc shell loops and makes CI artifacts stable across OSes.

### HELYX Compatibility Config

Add a checked-in HELYX-compatible smoke config that uses the deterministic stub
but declares the real contract shape:

- `producer: helyx-kernels`
- `kernel_id: helyx.bitexact.stub_v1`
- `profile: helyx_cuda`
- strict CUDA env/hardware metadata
- `expected_sha256` for the deterministic stub output

This is not a real HELYX kernel. It is a contract fixture that proves the
Refine-Forge gate accepts the same metadata shape a HELYX kernel gate will use.

## Error Handling

- Missing input files fail the run before subprocess execution.
- Bad expected hashes fail validation or lint with an exact field name.
- Baseline mismatch produces `Outcome::Fail` and a summary mentioning expected
  vs observed hashes.
- `run-all` records per-config failures and continues to collect remaining
  evidence.

## Testing

Use TDD per slice:

- Experiment loading for new optional fields.
- Report failure when hashes match each other but not `expected_sha256`.
- Input manifest hashing and missing-input error.
- Linter pass/fail cases for `helyx_cuda`.
- `run-all` deterministic ordering and fail aggregation.
- CLI smokes for lint, run, run-all, and report.

## Out Of Scope

- Writing production CUDA/HIP/Metal kernels.
- Calling HELYX repositories or requiring HELYX to be installed.
- Cross-GPU runner provisioning.
- Floating-point tolerance modes. This gate remains byte-exact only.

## Acceptance

- `cargo test -p refineforge-bitexact` passes.
- `refine-bitexact lint kernels/configs/helyx-bitexact-smoke.yaml` passes.
- `refine-bitexact run kernels/configs/helyx-bitexact-smoke.yaml` passes and
  writes a report with producer/kernel/profile/input/baseline evidence.
- `refine-bitexact run-all kernels/configs --include-examples` exits nonzero
  because the intentional non-deterministic example fails, while the summary
  records every config.
- Docs state that HELYX owns kernels and Refine-Forge owns the gate.
