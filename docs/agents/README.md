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
