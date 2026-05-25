# Three Agent Evidence Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden the Lean, DevOps, and Kernel agents so production-proof fields are derived from real evidence files, not environment-variable presence or unverified text.

**Architecture:** Keep the agent command surface unchanged. Add conventional evidence-directory inputs for the three roles and validate the same artifact classes that the central production-proof verifier requires. Human-reviewed trust remains gated by named non-AI approval JSON.

**Tech Stack:** Rust, `refineforge-cli`, integration tests under `crates/refineforge-cli/tests`, existing `AgentReport` production-proof requirements.

---

### Task 1: Regression Tests

**Files:**
- Modify: `crates/refineforge-cli/tests/agent_cli.rs`

- [ ] Add tests proving DevOps cannot pass release production proof from env var names alone.
- [ ] Add tests proving Kernel cannot pass production proof from env var names alone.
- [ ] Add tests proving Lean can consume an explicit evidence directory and reaches human-reviewed only with real implementation-linked evidence and approval.

### Task 2: DevOps Evidence Validation

**Files:**
- Modify: `crates/refineforge-cli/src/agent/devops.rs`

- [ ] Add `REFINEFORGE_RELEASE_EVIDENCE_DIR`.
- [ ] Validate hosted CI report, Sigstore evidence, verifier digest, SBOM, provenance, Nix lock/check, architecture matrix, and `approvals/release.json`.
- [ ] Reject missing paths, malformed JSON, failed status, placeholder approval, and absent hosted CI evidence.

### Task 3: Kernel Evidence Validation

**Files:**
- Modify: `crates/refineforge-cli/src/agent/kernel.rs`

- [ ] Add `REFINEFORGE_KERNEL_EVIDENCE_DIR`.
- [ ] Validate real CUDA/source evidence, reference output, bit-exact report, hardware matrix, compiler metadata, performance baseline, HELYX handoff, and `approvals/kernel.json`.
- [ ] Reject stub source, missing files, malformed JSON, failed status, and placeholder approval.

### Task 4: Lean Evidence Validation

**Files:**
- Modify: `crates/refineforge-cli/src/agent/lean.rs`

- [ ] Add `REFINEFORGE_LEAN_EVIDENCE_DIR`.
- [ ] Validate claims report, proof inventory, refinement links, bundle hashes, and `approvals/lean.json`.
- [ ] Keep model-only claims blocked and require explicit implementation-linked evidence before human-reviewed production proof.

### Task 5: Docs and Verification

**Files:**
- Modify: `docs/agents/README.md`
- Modify: `CHANGELOG.md`
- Optional modify: `STRUCTURE.md`

- [ ] Document the three evidence-directory environment variables.
- [ ] Run `cargo fmt --all --check`.
- [ ] Run `cargo test -p refineforge-cli --test agent_cli`.
- [ ] Run `cargo clippy -p refineforge-cli --all-targets -- -D warnings`.
- [ ] Commit the scoped change.
