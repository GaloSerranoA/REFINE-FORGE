# GPU Kernel Bit-Exact Track Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Refine-Forge Section 4 GPU/kernel gate with HELYX-compatible kernel metadata, baseline hashes, input manifests, strict linting, run-all CI orchestration, and documentation.

**Architecture:** Keep `refineforge-bitexact` as the independent evidence gate. Extend the existing `KernelExperiment` schema compatibly, add focused modules for input manifests, linting, and run-all summaries, and keep real HELYX kernel code out of this repo.

**Tech Stack:** Rust, Cargo, `serde`, `serde_yaml`, `serde_json`, `sha2`, `clap`, PowerShell/bash stubs, existing Refine-Forge release evidence flow.

---

## Files

- Modify `crates/refineforge-bitexact/src/experiment.rs`: add optional contract fields and validation helpers.
- Create `crates/refineforge-bitexact/src/manifest.rs`: hash configured input files.
- Create `crates/refineforge-bitexact/src/lint.rs`: strict profile linter.
- Create `crates/refineforge-bitexact/src/run_all.rs`: deterministic config discovery and aggregate summary.
- Modify `crates/refineforge-bitexact/src/report.rs`: include baseline/input evidence and baseline mismatch outcome.
- Modify `crates/refineforge-bitexact/src/runner.rs`: write per-run JSONL and preflight input manifest.
- Modify `crates/refineforge-bitexact/src/main.rs`: add `lint` and `run-all` commands.
- Modify `crates/refineforge-bitexact/src/lib.rs`: export new modules.
- Create `kernels/configs/helyx-bitexact-smoke.yaml`: HELYX-compatible strict contract fixture.
- Modify `kernels/README.md`, `docs/bit-exact-reproducibility.md`, `README.md`, `ARCHITECTURE.md`, `STRUCTURE.md`, `CHANGELOG.md`: docs truth update.

## Task 1: Schema Contract Fields

- [ ] Write failing tests in `experiment.rs` for loading `template_version`, `producer`, `kernel_id`, `profile`, `expected_sha256`, `input_files`, and sorted `tags`.
- [ ] Run `cargo test -p refineforge-bitexact experiment` and confirm failure on missing fields.
- [ ] Implement optional fields and `KernelProfile`.
- [ ] Validate `expected_sha256` as 64 lowercase hex chars when present.
- [ ] Run `cargo test -p refineforge-bitexact experiment`.
- [ ] Commit: `feat(bitexact): add kernel contract fields`.

## Task 2: Input Manifest Evidence

- [ ] Write failing tests for hashing two input files in deterministic path order and failing on a missing input.
- [ ] Run `cargo test -p refineforge-bitexact manifest` and confirm missing module failure.
- [ ] Implement `manifest.rs` with `InputArtifact` and `build_input_manifest`.
- [ ] Wire `runner::run_all` to preflight inputs and pass the manifest to reports.
- [ ] Run `cargo test -p refineforge-bitexact manifest`.
- [ ] Commit: `feat(bitexact): record kernel input manifests`.

## Task 3: Baseline Hash Enforcement

- [ ] Write failing report tests for stable output that mismatches `expected_sha256`.
- [ ] Run `cargo test -p refineforge-bitexact report` and confirm failure.
- [ ] Extend `Report` with `expected_sha256`, `observed_sha256`, and `input_manifest`.
- [ ] Update `Report::build` to fail on expected-hash mismatch.
- [ ] Run `cargo test -p refineforge-bitexact report`.
- [ ] Commit: `feat(bitexact): enforce expected output baselines`.

## Task 4: Strict Linter

- [ ] Write failing lint tests for `helyx_cuda` missing metadata/env and for a valid HELYX fixture.
- [ ] Run `cargo test -p refineforge-bitexact lint` and confirm missing module failure.
- [ ] Implement `LintStatus`, `LintIssue`, `LintReport`, and `lint_experiment`.
- [ ] Add `refine-bitexact lint <config> [--json]`.
- [ ] Run `cargo test -p refineforge-bitexact lint`.
- [ ] Commit: `feat(bitexact): add strict kernel contract linter`.

## Task 5: Run-All Command

- [ ] Write failing tests for deterministic YAML discovery, example filtering, and fail aggregation.
- [ ] Run `cargo test -p refineforge-bitexact run_all` and confirm missing module failure.
- [ ] Implement `run_all.rs` with `discover_configs`, `RunAllOptions`, `RunAllEntry`, and `run_directory`.
- [ ] Add `refine-bitexact run-all <config_dir> [--include-examples] [--summary-json <path>]`.
- [ ] Run `cargo test -p refineforge-bitexact run_all`.
- [ ] Commit: `feat(bitexact): add run-all kernel gate orchestration`.

## Task 6: HELYX Fixture And Docs

- [ ] Compute the deterministic stub SHA-256.
- [ ] Add `kernels/configs/helyx-bitexact-smoke.yaml` with `profile: helyx_cuda`.
- [ ] Update docs to describe HELYX owns `helyx-kernels`; Refine-Forge owns the contract gate.
- [ ] Run `refine-bitexact lint kernels/configs/helyx-bitexact-smoke.yaml`.
- [ ] Run `refine-bitexact run kernels/configs/helyx-bitexact-smoke.yaml`.
- [ ] Remove generated `kernels/runs/` output.
- [ ] Commit: `docs(bitexact): document HELYX kernel gate handoff`.

## Task 7: Final Verification And Merge

- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo test -p refineforge-bitexact`.
- [ ] Run `cargo test -p refineforge-cli`.
- [ ] Run `cargo run -p refineforge-bitexact -- lint kernels/configs/helyx-bitexact-smoke.yaml`.
- [ ] Run `cargo run -p refineforge-bitexact -- run kernels/configs/helyx-bitexact-smoke.yaml`.
- [ ] Run `cargo run -p refineforge-bitexact -- run-all kernels/configs --include-examples --summary-json kernels/runs/run-all-summary.json` and confirm nonzero because `example-nondeterministic` fails while summary exists.
- [ ] Clean generated `kernels/runs/`.
- [ ] Run `cargo run -p refineforge-cli --bin refine -- release ready --version 0.2.2 --allow-dirty --skip-docker --skip-signature --evidence-dir release/evidence/gpu-kernel-local-0.2.2`.
- [ ] Clean generated release evidence and artifacts.
- [ ] Run `git diff --check`.
- [ ] Fast-forward merge to `master`, rerun `cargo test -p refineforge-bitexact`, rerun HELYX lint/run smoke, clean outputs, remove worktree and branch.
