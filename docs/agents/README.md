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

Each command writes JSON and Markdown reports. The JSON report follows
`schemas/agent-report.schema.json`; the Markdown report is for operator and CI
summaries.

## Trust Rule

Role prompts may guide Codex, Claude, or another coding agent, but the source
of truth is always the CLI report:

- `passed` means the configured local evidence passed.
- `failed` means a gate ran and failed.
- `blocked` means a prerequisite was missing.
- `partial` means some evidence exists and the report names what did not run.

No agent may upgrade a claim beyond its report, and no prompt may replace human
review.

