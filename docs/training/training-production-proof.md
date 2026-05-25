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

## Approval Automation

`refine training-approval` automates the mechanical parts of training approval
without crossing the human trust boundary.

Draft a request after the Training Agent has produced a report where every
production-proof requirement except human approval is passed:

```bash
refine training-approval draft \
  --evidence-dir production-proof/evidence/live-heldout-smoke-2026-05-25 \
  --agent-report production-proof/evidence/live-heldout-smoke-2026-05-25/train-agent-report.stdout.json \
  --policy training/approval-policy.yaml \
  --operator "Galo Training Operator" \
  --json
```

The draft command validates the agent report, required evidence files,
checkpoint hashes, conversion hash, promotion manifest, operator allow-list,
and regression metric floors. It writes:

- `approvals/training.draft.json`
- `approvals/training.review-request.json`

It never writes `approvals/training.json`.

After a real human has reviewed the evidence, finalize the approval explicitly:

```bash
refine training-approval approve \
  --evidence-dir production-proof/evidence/live-heldout-smoke-2026-05-25 \
  --agent-report production-proof/evidence/live-heldout-smoke-2026-05-25/train-agent-report.stdout.json \
  --policy training/approval-policy.yaml \
  --operator "Galo Training Operator" \
  --i-reviewed-this-evidence \
  --json
```

The approve command reruns the same validation and writes
`approvals/training.json` only when the explicit review flag is present.
Prompts, agents, CI, and memory records still cannot upgrade training trust
without that named human approval file.

Start new projects from
[`training/approval-policy.example.yaml`](../../training/approval-policy.example.yaml)
and copy it to `training/approval-policy.yaml` for local policy changes.
