# Human-Reviewed Closure Runbook

This runbook explains how the four Refine-Forge agents reach
`human-reviewed` without inflating trust. A local passing agent report is not
enough; each role needs real evidence and a named human approval file.

## 1. DevOps / Release

Prerequisites:

- Repository is pushed to GitHub on `main` or `master`.
- `flake.lock` is committed.
- GitHub Actions has OIDC permission for the signing job.

Run the CI workflow and download the artifact named
`refineforge-devops-production-evidence`. It must include:

- `release/hosted-ci.json`
- `release/cosign-verify.json`
- `release/sbom.cyclonedx.json`
- `release/provenance.intoto.json`
- `release/flake.lock`
- `release/nix-check.log`
- `release/architecture-matrix.json`
- `release/verifier-container-digest.txt`

After a human reviews the artifact, add:

```json
{
  "schema_version": "refineforge-human-approval-v1",
  "human_operator": "Galo Release Operator",
  "role": "release",
  "decision": "approved",
  "approved_at": "2026-05-25T00:00:00Z",
  "evidence_summary": "release production evidence reviewed"
}
```

Save it as `approvals/release.json` inside that evidence directory, then run:

```bash
REFINEFORGE_RELEASE_EVIDENCE_DIR=production-proof/evidence/devops \
refine agent devops --mode check --target <version> --out agent-reports/devops-reviewed --json
```

## 2. Lean / Verification

Prerequisites:

- The target claim is not `model-only`.
- The claim has `rust_source` entries for the implementation symbols.
- A refinement document exists under `docs/refinement/<CLAIM-ID>.md`.
- `refine lean check`, `refine scan check`, and `refine lint check` pass.
- `refine bundle export <CLAIM-ID>` has produced bundle hashes.

Create a Lean evidence directory with:

- `lean/refinement-doc.md`
- `lean/rust-symbol-scan.json`
- `lean/lean-proof-report.json`
- `lean/exported-bundle-hashes.json`
- `approvals/lean.review-request.json`

Draft the approval without creating final human approval:

```bash
refine approval draft \
  --review-request production-proof/evidence/lean/approvals/lean.review-request.json \
  --policy approval-policy.yaml \
  --operator "Galo Lean Operator" \
  --json
```

After a real human reviews the evidence, finalize explicitly:

```bash
refine approval approve \
  --role lean \
  --evidence-dir production-proof/evidence/lean \
  --policy approval-policy.yaml \
  --operator "Galo Lean Operator" \
  --i-reviewed-this-evidence \
  --json
```

Then run:

```bash
REFINEFORGE_LEAN_EVIDENCE_DIR=production-proof/evidence/lean \
refine agent lean --mode check --target <CLAIM-ID> --out agent-reports/lean-reviewed --json
```

The agent still blocks if the live claim scope is `model-only`; evidence files
cannot override claim YAML.

## 3. Training

Run the live training/eval path, then draft and approve through the shared
approval helper:

```bash
refine approval draft \
  --role training \
  --evidence-dir production-proof/evidence/<training-run> \
  --agent-report production-proof/evidence/<training-run>/train-agent-report.stdout.json \
  --policy approval-policy.yaml \
  --operator "Galo Training Operator" \
  --json

refine approval approve \
  --role training \
  --evidence-dir production-proof/evidence/<training-run> \
  --agent-report production-proof/evidence/<training-run>/train-agent-report.stdout.json \
  --policy approval-policy.yaml \
  --operator "Galo Training Operator" \
  --i-reviewed-this-evidence \
  --json
```

Then run:

```bash
REFINEFORGE_TRAINING_EVIDENCE_DIR=production-proof/evidence/<training-run> \
refine agent train --mode execute --target <config> --allow-expensive --out agent-reports/train-reviewed --json
```

## 4. Kernel / CUDA

Prerequisites:

- Real kernel source exists under `kernels/src/` or in the evidence pack.
- The kernel config cites the real source and is not `source.kind: stub`.
- A GPU runner executes `refine-bitexact run`.
- Hardware, compiler/runtime, performance, and HELYX handoff reports are
  generated from the same run.

Create a kernel evidence directory with:

- `kernels/src/<kernel>.cu`
- `kernels/reference-output.json`
- `kernels/bitexact-report.json`
- `kernels/hardware-matrix.json`
- `kernels/compiler-metadata.json`
- `kernels/performance-baseline.json`
- `kernels/helyx-handoff.json`
- `approvals/kernel.review-request.json`

Draft the approval without creating final human approval:

```bash
refine approval draft \
  --review-request production-proof/evidence/kernel/approvals/kernel.review-request.json \
  --policy approval-policy.yaml \
  --operator "Galo Kernel Operator" \
  --json
```

After a real human reviews the evidence, finalize explicitly:

```bash
refine approval approve \
  --role kernel \
  --evidence-dir production-proof/evidence/kernel \
  --policy approval-policy.yaml \
  --operator "Galo Kernel Operator" \
  --i-reviewed-this-evidence \
  --json
```

Then run:

```bash
REFINEFORGE_KERNEL_EVIDENCE_DIR=production-proof/evidence/kernel \
refine agent kernel --mode execute --target kernels/configs/<kernel>.yaml --out agent-reports/kernel-reviewed --json
```

## 5. Final Combined Proof

Build `production-proof/evidence/<release>/evidence.json` so it points at the
accepted DevOps, Lean, Training, and Kernel artifacts, then run:

```bash
refine production-proof verify \
  --target helyx \
  --evidence-dir production-proof/evidence/<release> \
  --out production-proof/reports/<release> \
  --json
```

Only this final report should be treated as the combined production-proof
closure signal.
