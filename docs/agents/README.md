# Refine-Forge Agents

Refine-Forge agents are CLI-first specialist surfaces for HELYX development.
They are not trusted because a prompt says so; they are trusted only through
the evidence written by `refine agent`.

Use:

```bash
refine agent lean --mode inspect --target helyx --out agent-reports/helyx
refine agent devops --mode inspect --target helyx --out agent-reports/helyx
refine agent train --mode inspect --target helyx --out agent-reports/helyx
refine agent kernel --mode inspect --target helyx --out agent-reports/helyx
refine agent run-all --mode inspect --target helyx --out agent-reports/helyx
```

Modes:

- `inspect` records role artifacts, liveness, declared capabilities, and tool
  gates without running heavy checks.
- `check` runs the role's local readiness gates.
- `repair` is evidence-only for now: it runs the same gates and records
  blockers/warnings without mutating source, data, or kernels.
- `execute` runs the role execution surface. Training defaults to
  `refine-train run --dry-run`; use `--allow-expensive` only when live
  backend compute is intentional. Kernel execute runs the configured
  `refine-bitexact` gate into the agent evidence directory. DevOps uses
  `--allow-expensive` to request Docker/signature gates instead of local skips.

Each command writes JSON and Markdown reports. The JSON report follows
`schemas/agent-report.schema.json`; the Markdown report is for operator and CI
summaries.

Every report must include:

- `liveness.state = "alive"` with the command surface that emitted it.
- `runtime` with authority rules, trust ceiling, action intents, deterministic
  evidence receipts, policy decisions, and typed blockers.
- `capabilities[]` for the role's available, tool-gated, and evidence-only
  abilities.
- `tool_checks[]` for local or external prerequisites such as Docker, cosign,
  `helyx-train`, or `helyx-kernels`.

## Trust Rule

Role prompts may guide Codex, Claude, or another coding agent, but the source
of truth is always the CLI report:

- `passed` means the configured local evidence passed.
- `failed` means a gate ran and failed.
- `blocked` means a prerequisite was missing.
- `partial` means some evidence exists and the report names what did not run.

No agent may upgrade a claim beyond its report, and no prompt may replace human
review.

## Role Evidence Directories

The agents accept role-specific evidence directories for production-proof
validation. These directories reduce ad hoc environment variables, but they do
not lower the trust bar:

- `REFINEFORGE_LEAN_EVIDENCE_DIR`
  - `lean/claims-report.json`
  - `lean/proof-inventory.md`
  - `lean/refinement-links.json`
  - `lean/bundle-hashes.json`
  - `approvals/lean.json`
- `REFINEFORGE_RELEASE_EVIDENCE_DIR`
  - `release/hosted-ci.json` or `REFINEFORGE_HOSTED_CI_EVIDENCE` as a GitHub
    Actions run URL
  - `release/cosign-verify.json`
  - `release/sbom.cyclonedx.json`
  - `release/provenance.intoto.json`
  - `release/flake.lock`
  - `release/nix-check.log`
  - `release/architecture-matrix.json`
  - `approvals/release.json`
- `REFINEFORGE_KERNEL_EVIDENCE_DIR`
  - `kernels/src/hvector_add.cu` or another real `*.cu`, `*.cuh`, or `*.rs`
    source file
  - `kernels/hardware-matrix.json`
  - `kernels/compiler-metadata.json`
  - `kernels/performance-baseline.json`
  - `kernels/helyx-handoff.json`
  - `approvals/kernel.json`

Human approval files must use `schema_version:
refineforge-human-approval-v1`, the correct role, `decision: approved`, a
non-empty `approved_at`, a non-AI `human_operator`, and an
`evidence_summary`. Bare environment-variable presence, fake file paths, and
placeholder operators are blocked.

See `docs/agents/runtime.md` for the enterprise runtime contract and the
Hermes-style integration boundary.

See `docs/agents/central-memory-integration.md` for the HELYX/COGN8TY memory
compatibility boundary, and `docs/agents/knowledge-source-audit.md` for the
local PDF and Rust training-source analysis.

For production closure across all four agents, build a self-contained evidence
pack and run:

```bash
refine production-proof verify --target helyx --evidence-dir <dir> --out <report-dir>
```

See `docs/agents/production-proof-evidence.md` and
`schemas/production-proof-evidence.schema.json` for the required hosted CI,
OIDC signing, Nix, approval, checkpoint/eval/promotion, and CUDA
source/hardware/performance evidence.

## Production-Proof Plans

The four role-specific enterprise closure plans are:

- `docs/plans/plan-3-lean-agent-production-proof.md`
- `docs/plans/plan-4-devops-agent-production-proof.md`
- `docs/plans/plan-5-training-agent-production-proof.md`
- `docs/plans/plan-6-kernel-agent-production-proof.md`
