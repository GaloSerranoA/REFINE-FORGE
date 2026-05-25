# Refine-Forge Agent Runtime

The Refine-Forge agent runtime is the enterprise control plane for the four
HELYX-facing specialist agents. It borrows useful ideas from general agent
runtimes: role playbooks, permission boundaries, tool surfaces, memory-aware
operation, and trajectory-style evidence. It does not import an external agent
loop as the source of trust.

The source of truth is always the CLI report written by `refine agent`.
Prompts, memory, skills, and autonomous orchestration are advisory only.

## Runtime Envelope

Every agent report contains a `runtime` object:

- `runtime_version`: the runtime contract version.
- `authority`: declares that CLI evidence is authoritative, prompts are
  advisory, memory is non-authoritative, and `human-reviewed` trust requires
  explicit human evidence.
- `trust_ceiling`: the maximum trust level this command surface can emit.
- `action_intents`: typed role actions the agent is allowed to perform.
- `evidence_receipts`: deterministic SHA-256 receipts for artifacts, commands,
  and tool checks.
- `policy_decisions`: enforced trust and authority policies.
- `typed_blockers`: blocker strings classified into machine-readable categories.

## Trust Ceilings

The runtime caps any over-claim before the report is written:

- Lean: at most `model-linked` unless the production-proof envelope reaches
  `human-reviewed`; current claim scope may still lower it to `model-only`.
- DevOps: at most `release-ready-local` unless hosted CI/OIDC evidence and
  human release approval make the production-proof envelope `human-reviewed`.
- Training: at most `measured-only` unless checkpoint, eval, regression,
  compute, conversion, promotion, and human approval evidence all pass.
- Kernel: at most `measured-only` unless real source/reference, bit-exact run,
  hardware, compiler, performance, HELYX handoff, and human approval evidence
  all pass.
- Run-all: the lowest trust ceiling and lowest emitted role trust drive the
  dashboard.

## Evidence Receipts

Receipts are deterministic and sorted by id. Artifact receipts hash files or
directory trees with stable path ordering. Command receipts hash the command
argv, status, exit code, and captured output tails without including runtime
duration. Tool-check receipts hash the declared tool gate state.

Receipts are evidence indexes, not trust upgrades. A receipt proves what was
observed, not that a stronger claim is true.

## Hermes-Style Integration Boundary

A Hermes-style planner, memory loop, scheduler, or messaging gateway may sit
above Refine-Forge and call commands such as:

```bash
refine agent run-all --mode inspect --target helyx --out agent-reports/helyx
refine agent lean --mode check --target helyx --out agent-reports/lean
refine agent train --mode execute --target helyx --out agent-reports/train
```

That layer may propose tasks, summarize evidence, remember operator
preferences, and route work. It may not replace `trust_level`, mutate
`runtime.policy_decisions`, mark `human-reviewed`, or hide blockers/warnings.

Central memory follows the same rule. A HELYX or COGN8TY memory backend may
retrieve context for agents, but it remains advisory unless a CLI report,
claim file, refinement document, signed artifact, or human-review record turns
that context into evidence. See `docs/agents/central-memory-integration.md`.

## Enterprise Operating Rule

Agents are alive when they can emit a valid runtime envelope and evidence
receipts. Agents are trusted only to the level supported by their current
report.
