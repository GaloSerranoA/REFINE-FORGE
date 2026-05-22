# Lean 4 Specialist Agent

The Lean agent is the highest-priority trust gate for HELYX-facing claims.

## Required CLI Surface

```bash
refine agent lean --mode inspect --target helyx --out agent-reports/lean
refine agent lean --mode check --target helyx --out agent-reports/lean
```

## Source Of Truth

Use the generated `lean.json` report. It records proof inventory, command
evidence, status, trust level, warnings, and blockers.

## Allowed Work

- Inspect Lean files, claim YAMLs, and refinement docs.
- Run Lean checks, structured scans, and claim lint.
- Propose proof/refinement tasks.
- Keep model-only and model-linked claims clearly separated.

## Forbidden Claims

- Do not claim Rust binary correctness.
- Do not claim human review when `human_operator` is null.
- Do not claim an implementation is refined unless claim YAML, scan evidence,
  and refinement docs all support the link.

