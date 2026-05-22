# HELYX Agent Control Plane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `refine agent` CLI control plane for the four HELYX-facing specialist agents.

**Architecture:** Add a focused `crates/refineforge-cli/src/agent/` module with shared report types and role runners. The role runners wrap existing Refine-Forge surfaces and write JSON plus Markdown evidence under `agent-reports/`, while docs under `docs/agents/` define the AI role prompts that must use the CLI evidence as source of truth.

**Tech Stack:** Rust 2021, clap, serde/serde_json, chrono, anyhow, existing Refine-Forge CLI modules, integration tests via `CARGO_BIN_EXE_refine`.

**Execution Status:** Implemented inline on 2026-05-22. Verification covered focused TDD red/green tests, `refine agent run-all --mode check`, `cargo test -p refineforge-cli`, `cargo check --workspace --all-targets`, scoped `rustfmt --check`, and `git diff --check`.

---

### Task 1: Shared Agent Contract

**Files:**
- Create: `crates/refineforge-cli/src/agent/common.rs`
- Create: `crates/refineforge-cli/src/agent/mod.rs`
- Modify: `crates/refineforge-cli/src/lib.rs`
- Modify: `crates/refineforge-cli/src/main.rs`
- Modify: `.gitignore`
- Create: `schemas/agent-report.schema.json`
- Test: `crates/refineforge-cli/tests/agent_cli.rs`

- [ ] **Step 1: Write failing CLI/report test**

Add an integration test that runs `refine agent lean --mode inspect --target helyx --out <tempdir> --json` and asserts that `lean.json`, `lean.md`, and schema fields exist.

- [ ] **Step 2: Run test to verify RED**

Run: `cargo test -p refineforge-cli agent_lean_inspect_writes_report`

Expected: fail because `agent` is not a recognized subcommand.

- [ ] **Step 3: Implement shared report types and report writing**

Define `AgentKind`, `AgentMode`, `AgentStatus`, `TrustLevel`, `CommandRecord`, and `AgentReport`. Add `write_reports(out_dir, stem, report)` to write `<stem>.json` and `<stem>.md`.

- [ ] **Step 4: Add `refine agent` clap surface**

Add `Cmd::Agent { cmd: AgentCmd }`, `AgentCmd::{Lean, Devops, Train, Kernel, RunAll}`, shared options, and dispatch into `agent::run_role` / `agent::run_all`.

- [ ] **Step 5: Verify GREEN**

Run: `cargo test -p refineforge-cli agent_lean_inspect_writes_report`

Expected: pass.

### Task 2: Lean And DevOps Agents

**Files:**
- Create: `crates/refineforge-cli/src/agent/lean.rs`
- Create: `crates/refineforge-cli/src/agent/devops.rs`
- Modify: `crates/refineforge-cli/src/agent/mod.rs`
- Test: `crates/refineforge-cli/tests/agent_cli.rs`

- [ ] **Step 1: Write failing tests for check mode**

Add tests for `refine agent lean --mode inspect` and `refine agent devops --mode inspect`, asserting their JSON reports contain the expected agent names, trust levels, and doc-derived artifacts.

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test -p refineforge-cli agent_`

Expected: fail until role modules return real reports.

- [ ] **Step 3: Implement Lean role**

`inspect` reads `docs/verification/proof-inventory.md`. `check` runs `runner::check_all`, `scan::scan_all`, and `lint::lint_all`, records command names, and fails closed on errors.

- [ ] **Step 4: Implement DevOps role**

`inspect` reads release inventory/audit docs. `check` calls `release::ready` with `allow_dirty=true`, `skip_docker=true`, `skip_signature=true`, and evidence under `<out>/release`.

- [ ] **Step 5: Verify GREEN**

Run: `cargo test -p refineforge-cli agent_`

Expected: pass for the initial agent CLI tests.

### Task 3: Training And Kernel Agents

**Files:**
- Create: `crates/refineforge-cli/src/agent/train.rs`
- Create: `crates/refineforge-cli/src/agent/kernel.rs`
- Modify: `crates/refineforge-cli/src/agent/mod.rs`
- Test: `crates/refineforge-cli/tests/agent_cli.rs`

- [ ] **Step 1: Write failing report tests**

Add inspect-mode tests for train and kernel agents, plus a run-all inspect test that writes `summary.json`, `summary.md`, and each role report.

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test -p refineforge-cli agent_`

Expected: fail until train/kernel/run-all exist.

- [ ] **Step 3: Implement training role**

`inspect` records available config/data paths. `check` invokes `refine-train data audit training/data/lean-proof-repair-smoke.jsonl` through `REFINEFORGE_REFINE_TRAIN_BIN` or `refine-train`.

- [ ] **Step 4: Implement kernel role**

`inspect` records kernel config paths. `check` invokes `refine-bitexact lint kernels/configs/helyx-bitexact-smoke.yaml --json --output <out>/bitexact-lint.json` through `REFINEFORGE_REFINE_BITEXACT_BIN` or `refine-bitexact`.

- [ ] **Step 5: Implement run-all dashboard**

Run all four roles, write per-role reports and `summary.json`/`summary.md`, preserve partial results, and mark summary `failed` when any required role fails or blocks.

- [ ] **Step 6: Verify GREEN**

Run: `cargo test -p refineforge-cli agent_`

Expected: all agent CLI tests pass.

### Task 4: Docs And Final Gates

**Files:**
- Create: `docs/agents/README.md`
- Create: `docs/agents/lean-agent.md`
- Create: `docs/agents/devops-agent.md`
- Create: `docs/agents/training-agent.md`
- Create: `docs/agents/kernel-agent.md`
- Modify: `README.md`
- Modify: `STRUCTURE.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Add role prompt docs**

Each role doc must state allowed commands, required evidence, forbidden claims, and the rule that CLI reports are source of truth.

- [ ] **Step 2: Update top-level docs**

Link the agent docs and schema from README/STRUCTURE/CHANGELOG without claiming live hosted agents.

- [ ] **Step 3: Run verification**

Run:

```bash
cargo test -p refineforge-cli agent_
cargo test -p refineforge-cli
cargo check --workspace --all-targets
git diff --check
```

Expected: tests and check pass; any remaining warnings are existing warning-class output, not failed gates.

- [ ] **Step 4: Commit**

Commit the completed implementation on the current branch with:

```bash
git add -A
git commit -m "feat: add helyx agent control plane"
```
