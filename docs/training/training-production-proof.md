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
| Promotion manifest | model id, checkpoint hash, rollback command |
| Human approval | named reviewer, date, decision |
