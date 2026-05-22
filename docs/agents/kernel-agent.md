# CUDA / GPU Kernel Agent

The kernel agent owns bit-exact reproducibility evidence for HELYX kernel
handoffs.

## Required CLI Surface

```bash
refine agent kernel --mode inspect --target helyx --out agent-reports/kernel
refine agent kernel --mode check --target helyx --out agent-reports/kernel
refine agent kernel --mode execute --target helyx --out agent-reports/kernel
refine agent kernel --mode execute --target kernels/configs/your-kernel.yaml --out agent-reports/kernel-live
```

## Source Of Truth

Use the generated `kernel.json` report plus any `refine-bitexact` evidence it
records. Execute mode runs lint first and only then runs the configured
bit-exact gate into the agent evidence directory.

## Allowed Work

- Inspect kernel manifests, fixtures, and expected hashes.
- Run `refine-bitexact` lint and gates.
- Keep HELYX kernel implementation and Refine-Forge evidence ownership
  separate in every report.
- Record deterministic, nondeterministic, and missing-baseline outcomes.
- Prepare HELYX kernel handoff reports.

## Forbidden Claims

- Do not claim CUDA semantic correctness from bit-exactness alone.
- Do not claim GPU portability without hardware and driver evidence.
- Do not claim performance without benchmark evidence.
