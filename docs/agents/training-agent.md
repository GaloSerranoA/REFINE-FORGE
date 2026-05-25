# ML Training Agent

The training agent owns dataset, native smoke-training, external trainer run,
checkpoint, evaluation, and promotion evidence for HELYX training workflows.

## Required CLI Surface

```bash
refine agent train --mode inspect --target helyx --out agent-reports/train
refine agent train --mode check --target helyx --out agent-reports/train
refine agent train --mode execute --target helyx --out agent-reports/train
refine agent train --mode execute --target training/configs/your-run.yaml --allow-expensive --out agent-reports/train-live
```

## Source Of Truth

Use the generated `train.json` report. Checkpoint promotion requires training
run reports and acceptance evidence, not just a successful command. The runtime
envelope caps training evidence at `measured-only` until checkpoint, eval,
regression, compute ledger, conversion manifest, promotion manifest, lineage,
and named human approval evidence all validate. Execute mode runs dataset audit plus
`refine-train run --dry-run` by default; live backend execution requires
`--allow-expensive`.

## Allowed Work

- Inspect training configs and dataset fixtures.
- Run dataset audits.
- Run or dry-run `refine-train` workflows with explicit evidence directories,
  including `backend.kind=refineforge_native` smoke training.
- Pack SFT data with target-only loss masks and deterministic multipack
  reports.
- Run `backend.kind=refineforge_native_causal_lm` smoke training.
- Generate production-proof eval/regression/ledger/conversion/promotion
  evidence from successful run reports.
- Record checkpoint metadata and acceptance comparisons.
- Validate `REFINEFORGE_TRAINING_EVIDENCE_DIR` or the individual
  `REFINEFORGE_TRAINING_*` evidence paths before any `human-reviewed` trust
  upgrade.

Model-quality production proof is governed by
`docs/training/training-production-proof.md`.

## Forbidden Claims

- Do not claim model improvement without benchmark evidence.
- Do not claim production checkpoint readiness from training loss alone.
- Do not treat `refineforge_native` or `refineforge_native_causal_lm` smoke
  checkpoints as production LLM weights.
- Do not claim HELYX reasoning correctness from dataset or checkpoint metadata.
- Do not treat environment variable presence as evidence; the named files must
  exist, parse, pass their status checks, and be hashable.
