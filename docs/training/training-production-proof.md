# Training Production Proof Checklist

The Training agent may emit `human-reviewed` only when the model promotion
record includes:

| Requirement | Evidence |
|---|---|
| Dataset lineage | dataset path, SHA-256, record count |
| Reproducible config | training YAML and config SHA-256 |
| Live run | run report and checkpoint metadata |
| Evaluation | benchmark report with baseline and candidate metrics |
| Regression guard | no required metric regresses below threshold |
| Compute ledger | backend, device, duration, cost/budget |
| Conversion manifest | source format, target format, checkpoint hash, converted artifacts |
| Promotion manifest | model id, checkpoint hash, lineage, conversion hash, rollback command |
| Human approval | named reviewer, date, decision |

## CLI Evidence Contract

`refine agent train --mode execute --allow-expensive` stays
`measured-only` unless all production evidence validates. Evidence may be
provided either through individual environment variables or through one
self-contained directory:

```bash
set REFINEFORGE_TRAINING_EVIDENCE_DIR=production-proof/evidence/human-reviewed-training
refine agent train \
  --mode execute \
  --target training/configs/refineforge-native-proof-repair-smoke.yaml \
  --allow-expensive \
  --out agent-reports/train-reviewed \
  --json
```

With `REFINEFORGE_TRAINING_EVIDENCE_DIR`, the Training agent expects:

| Path | Required validation |
|---|---|
| `training/checkpoint.safetensors` | checkpoint file exists and is hashed |
| `training/eval-report.json` | JSON `status: "passed"`, non-empty non-loss quality metrics, baseline, and candidate |
| `training/regression-report.json` | JSON `status: "passed"`, baseline/candidate references, and metric deltas |
| `training/compute-ledger.json` | JSON `status: "passed"`, backend, device, and duration/budget data |
| `training/conversion-manifest.json` | `status: "passed"`, source/target formats, `checkpoint_sha256` matching the checkpoint artifact, and converted artifact hashes |
| `training/promotion-manifest.json` | `status: "approved"` or `decision: "promote"`, `model_id`, `checkpoint_sha256` matching the checkpoint artifact, rollback, lineage hashes, and conversion manifest hash |
| `approvals/training.json` | `refineforge-human-approval-v1`, role `training`, decision `approved`, named non-AI human operator |

Individual overrides are also accepted:

- `REFINEFORGE_TRAINING_CHECKPOINT`
- `REFINEFORGE_TRAINING_EVAL_REPORT`
- `REFINEFORGE_TRAINING_REGRESSION_REPORT`
- `REFINEFORGE_TRAINING_COMPUTE_LEDGER`
- `REFINEFORGE_TRAINING_PROMOTION_MANIFEST`
- `REFINEFORGE_TRAINING_CONVERSION_MANIFEST`
- `REFINEFORGE_TRAINING_HUMAN_APPROVAL`

The agent hashes every accepted evidence file into the runtime receipts.
Setting an environment variable to a missing path, malformed JSON, failed
report, loss-only eval, hash mismatch, or AI/operator placeholder keeps
production proof blocked.
