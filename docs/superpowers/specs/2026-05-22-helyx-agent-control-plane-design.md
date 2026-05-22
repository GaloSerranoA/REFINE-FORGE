# HELYX Agent Control Plane Design

Date: 2026-05-22
Status: Design approved in chat; awaiting review of this saved spec before implementation planning.

## Scope

This spec defines the Refine-Forge agent layer for HELYX development. The
selected direction is both:

1. CLI-first agents that run deterministic Refine-Forge commands and emit
   evidence.
2. Role prompts that instruct AI coding agents to operate through those CLI
   commands and evidence contracts rather than bypassing them.

The first implementation target is a local open-source control plane. It does
not create hosted SaaS agents, remote worker fleets, or autonomous production
credentials. It gives the operator four executable specialist surfaces that
can inspect, check, repair, and report on HELYX-facing work.

## Current Context

Refine-Forge already has the four engineering tracks needed by HELYX:

- Lean / verification: claim YAMLs, Lean checks, structured Rust scan, claim
  linting, refinement docs, and bundle export/verify.
- Release / infrastructure / DevOps: release readiness, CI gates, verifier
  container, SBOM/provenance, docs truth audit, and signed-bundle flow.
- ML / training: `refine-train`, dataset audits, backend orchestration, run
  reports, checkpoint metadata, and local-finetune promotion boundaries.
- GPU / kernels: `refine-bitexact`, kernel manifests, input fixtures,
  expected SHA-256 baselines, and deterministic run reports.

The next layer should not duplicate those surfaces. It should orchestrate them
through a first-class `refine agent` command family and a shared evidence
format.

## Goals

- Make each specialist role executable through a stable CLI command.
- Make every agent output machine-readable JSON and operator-readable
  Markdown evidence.
- Keep the Lean agent as the highest-priority trust gate.
- Prevent any agent from upgrading a claim beyond the evidence it actually
  generated.
- Give external AI coding agents role prompts that require use of the CLI and
  evidence reports as the source of truth.
- Support a combined HELYX readiness dashboard across proof, release,
  training, and kernel surfaces.
- Preserve existing CLI commands where possible by wrapping them instead of
  replacing them.
- Keep blocked prerequisites explicit, including missing Docker, Nix, cosign,
  GitHub auth, GPU hardware, datasets, backends, or human review.

## Non-Goals

- No hosted multi-agent service.
- No automatic remote cloud provisioning.
- No background daemon or long-running worker fleet.
- No hidden credential use.
- No claim that ML metrics prove correctness.
- No claim that bit-exact kernel output proves CUDA semantic correctness.
- No automatic human-review substitution. `review.human_operator: null`
  remains null until a real operator fills it.
- No weakening of existing Lean, release, training, or bit-exact gates.

## Architecture

The agent layer has three parts:

```text
operator or AI coding agent
   |
   v
refine agent <role> <mode>
   |
   | wraps existing Refine-Forge commands
   v
role-specific evidence
   |-- JSON report
   |-- Markdown summary
   |-- command log
   |-- changed-file list
   |-- blocker list
   v
combined HELYX readiness dashboard
```

The CLI layer is the authority. Role prompts are advisory instructions for
Codex, Claude, or another coding agent, but those prompts must tell the agent
to use the CLI reports rather than inventing status.

## Agent Command Surface

Add a new command family under the existing `refine` binary:

```bash
refine agent lean
refine agent devops
refine agent train
refine agent kernel
refine agent run-all
```

Each role command accepts the same core options:

```bash
--mode inspect|check|repair|execute
--target <claim-id|release-version|experiment-id|kernel-gate|helyx>
--out agent-reports/<run-id>
--json
```

`inspect` gathers status without running expensive gates. `check` runs
verification commands but does not edit files. `repair` may propose or apply
bounded fixes owned by that role. `execute` may run the full role workflow,
including expensive local gates, but still fails closed when prerequisites are
missing.

## Agent Responsibilities

### Lean Agent

Command: `refine agent lean`

Responsibilities:

- Run `refine lean check-all`.
- Run `refine scan check-all`.
- Run `refine lint check-all`.
- Read and summarize `docs/verification/proof-inventory.md`.
- Classify each claim as blocked, model-only, model-linked, or
  human-review-pending.
- Detect proof/refinement mismatches before release or training promotion.
- Emit proof-readiness evidence.

Cannot claim:

- Rust binary correctness.
- Implementation refinement unless the claim YAML, structured scan, and
  refinement doc all support the link.
- Human review when `human_operator` is null.

### DevOps Agent

Command: `refine agent devops`

Responsibilities:

- Run local release readiness.
- Run docs truth audit.
- Check verifier-container readiness when Docker is available.
- Check SBOM and provenance generation.
- Report signed-bundle readiness and live-signing blockers.
- Summarize CI-pending surfaces.

Cannot claim:

- Live Sigstore success without a real signed bundle.
- Docker, Nix, hosted CI, or GitHub OIDC success when the tool or remote run
  is unavailable.
- Release readiness beyond the local or CI evidence actually generated.

### Training Agent

Command: `refine agent train`

Responsibilities:

- Audit dataset manifests and split metadata.
- Run `refine-train` audit and training workflows.
- Validate HELYX, Axolotl, or custom backend configs.
- Capture training command logs, output artifacts, checkpoint metadata, and
  evaluation results.
- Promote or reject local-finetune candidates based on configured acceptance
  criteria.

Cannot claim:

- Model improvement without benchmark evidence.
- Production checkpoint readiness without checkpoint metadata and acceptance
  comparison.
- Correctness of HELYX reasoning from training loss alone.

### Kernel Agent

Command: `refine agent kernel`

Responsibilities:

- Run `refine-bitexact lint`.
- Run `refine-bitexact run` and `run-all`.
- Validate kernel manifests, input fixtures, output paths, and expected
  SHA-256 baselines.
- Detect nondeterministic outputs and missing baselines.
- Prepare HELYX kernel handoff reports.

Cannot claim:

- CUDA semantic correctness from bit-exactness alone.
- GPU portability unless the report includes the hardware and driver
  evidence.
- Kernel performance claims without benchmark evidence.

## Evidence Contract

Every agent writes the same top-level report shape:

```json
{
  "schema_version": "agent-report-v1",
  "agent": "lean",
  "mode": "check",
  "target": "helyx",
  "started_at": "2026-05-22T00:00:00Z",
  "finished_at": "2026-05-22T00:00:00Z",
  "status": "passed",
  "trust_level": "model-only",
  "commands": [],
  "changed_files": [],
  "artifacts": [],
  "blockers": [],
  "warnings": [],
  "summary": "short operator-readable result"
}
```

Valid `status` values:

- `passed`
- `failed`
- `blocked`
- `partial`

Valid `trust_level` values:

- `blocked`
- `measured-only`
- `model-only`
- `model-linked`
- `release-ready-local`
- `release-ready-ci`
- `human-reviewed`

The JSON report is the stable integration contract. The Markdown summary is
for the operator and CI job summaries.

## Combined HELYX Readiness Dashboard

`refine agent run-all --target helyx --out agent-reports/helyx-readiness`
produces:

```text
agent-reports/helyx-readiness/
|-- summary.json
|-- summary.md
|-- lean.json
|-- lean.md
|-- devops.json
|-- devops.md
|-- train.json
|-- train.md
|-- kernel.json
\-- kernel.md
```

The dashboard reports each surface independently. It does not collapse the
whole project to a single optimistic "ready" label. If training passes but
Lean is model-only, the dashboard must say so. If release readiness is local
only because cosign or hosted CI is unavailable, the dashboard must say so.

## File Layout

Add the agent implementation under the CLI crate:

```text
crates/refineforge-cli/src/agent/
|-- mod.rs
|-- common.rs
|-- lean.rs
|-- devops.rs
|-- train.rs
\-- kernel.rs
```

Add role prompts and operator docs:

```text
docs/agents/
|-- README.md
|-- lean-agent.md
|-- devops-agent.md
|-- training-agent.md
\-- kernel-agent.md
```

Add the schema:

```text
schemas/
\-- agent-report.schema.json
```

Add run outputs:

```text
agent-reports/
```

`agent-reports/` is a local run-output directory and should be gitignored.
Selected evidence can be copied into `release/evidence/` when it needs to be
committed as release or review evidence.

## Implementation Phases

### Phase 0: Shared Agent Contract

- Add shared report types.
- Add `refine agent --help`.
- Add JSON and Markdown report writers.
- Add `schemas/agent-report.schema.json`.
- Add `agent-reports/` to `.gitignore`.
- Add unit tests for report serialization and trust-level validation.

### Phase 1: Lean Agent

- Wrap Lean check, scan, and lint commands.
- Summarize proof inventory.
- Emit proof-readiness reports.
- Add tests for passed, failed, and blocked Lean-agent outcomes.

### Phase 2: DevOps Agent

- Wrap release readiness.
- Surface docs truth audit and local infrastructure blockers.
- Emit release-readiness reports.
- Add tests for local-only, blocked, and CI-pending statuses.

### Phase 3: Training Agent

- Wrap `refine-train` audit/run/report surfaces.
- Normalize checkpoint and dataset evidence into agent reports.
- Add tests using the existing training smoke fixture.

### Phase 4: Kernel Agent

- Wrap `refine-bitexact` lint/run/run-all.
- Normalize deterministic, nondeterministic, and missing-baseline outcomes.
- Add tests using existing stub deterministic and nondeterministic scripts.

### Phase 5: Role Prompt Docs

- Add the four role prompt docs under `docs/agents/`.
- Require each prompt to use the CLI agent report as source of truth.
- Document forbidden claims for each role.

### Phase 6: Run-All Dashboard

- Run all four agents and combine reports.
- Produce `summary.json` and `summary.md`.
- Fail closed when any role is failed or blocked.
- Preserve partial results when one role cannot run.

## Testing Strategy

- Unit tests for shared report serialization and Markdown rendering.
- CLI tests for `refine agent --help` and each role command.
- Golden-report tests for representative pass, fail, blocked, and partial
  outcomes.
- Integration tests that use existing smoke fixtures rather than live
  services.
- No test should require GitHub auth, Docker, Nix, cosign, a GPU, or a real
  training backend unless explicitly marked as ignored or CI-only.

## Error Handling

Agents fail closed:

- Missing tools become blockers, not success.
- Failed wrapped commands become failed agent reports with command logs.
- Unavailable optional tools become warnings only when the mode or role marks
  them optional.
- A role may produce `partial` only when some evidence was generated and the
  report names exactly what did not run.

## Security And Trust Boundaries

- Agent prompts are not trusted evidence.
- JSON reports and command logs are the evidence surface.
- Human review remains explicit in claim YAMLs and review packets.
- No agent may write credentials into reports.
- No agent may claim live remote signing, hosted CI, or GPU hardware evidence
  from a local stub.
- The Lean agent is the final trust classifier for proof/refinement status.

## Acceptance Criteria

The design is implemented when:

- `refine agent --help` lists all role commands.
- Each role command can run in `inspect` and `check` mode locally.
- Each role writes JSON and Markdown reports matching
  `agent-report.schema.json`.
- `refine agent run-all --target helyx` writes a combined dashboard.
- The four role prompt docs exist and require CLI evidence as the source of
  truth.
- Tests cover pass, fail, blocked, and partial outcomes.
- Existing verification, release, training, and bit-exact commands keep their
  current behavior.
