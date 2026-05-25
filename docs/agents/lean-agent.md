# Lean 4 Specialist Agent

The Lean agent is the highest-priority trust gate for HELYX-facing claims.

## Required CLI Surface

```bash
refine agent lean --mode inspect --target helyx --out agent-reports/lean
refine agent lean --mode check --target helyx --out agent-reports/lean
refine agent lean --mode execute --target helyx --out agent-reports/lean
```

## Source Of Truth

Use the generated `lean.json` report. It records proof inventory, liveness,
runtime authority, action intents, evidence receipts, capabilities, tool
checks, command evidence, status, trust level, warnings, and blockers. The
runtime ceiling can reach `human-reviewed` only when the selected
implementation-linked claims, refinement docs, bundle hashes, and named Lean
approval evidence all pass.

## Allowed Work

- Inspect Lean files, claim YAMLs, and refinement docs.
- Run Lean checks, structured scans, and claim lint.
- Use execute mode as the full local verification gate.
- Derive the reported trust level from claim `scope`, Rust-source presence,
  and refinement-doc evidence; passing gates alone must not upgrade
  `model-only` claims to `model-linked`.
- Propose proof/refinement tasks.
- Keep model-only and model-linked claims clearly separated.

For the production-proof human review gate, use
`docs/verification/lean-production-proof-checklist.md`.

## Forbidden Claims

- Do not claim Rust binary correctness.
- Do not claim human review when `human_operator` is null.
- Do not claim an implementation is refined unless claim YAML, scan evidence,
  and refinement docs all support the link.
