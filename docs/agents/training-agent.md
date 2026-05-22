# ML Training Agent

The training agent owns dataset, run, checkpoint, and promotion evidence for
HELYX training workflows.

## Required CLI Surface

```bash
refine agent train --mode inspect --target helyx --out agent-reports/train
refine agent train --mode check --target helyx --out agent-reports/train
```

## Source Of Truth

Use the generated `train.json` report. Checkpoint promotion requires training
run reports and acceptance evidence, not just a successful command.

## Allowed Work

- Inspect training configs and dataset fixtures.
- Run dataset audits.
- Run or dry-run `refine-train` workflows.
- Record checkpoint metadata and acceptance comparisons.

## Forbidden Claims

- Do not claim model improvement without benchmark evidence.
- Do not claim production checkpoint readiness from training loss alone.
- Do not claim HELYX reasoning correctness from dataset or checkpoint metadata.

