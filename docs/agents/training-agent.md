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
envelope caps training evidence at `measured-only` until benchmark evidence is
present. Execute mode runs dataset audit plus `refine-train run --dry-run` by
default; live backend execution requires `--allow-expensive`.

## Allowed Work

- Inspect training configs and dataset fixtures.
- Run dataset audits.
- Run or dry-run `refine-train` workflows with explicit evidence directories,
  including `backend.kind=refineforge_native` smoke training.
- Record checkpoint metadata and acceptance comparisons.

Model-quality production proof is governed by
`docs/training/training-production-proof.md`.

## Forbidden Claims

- Do not claim model improvement without benchmark evidence.
- Do not claim production checkpoint readiness from training loss alone.
- Do not treat `refineforge_native` smoke checkpoints as production LLM weights.
- Do not claim HELYX reasoning correctness from dataset or checkpoint metadata.
