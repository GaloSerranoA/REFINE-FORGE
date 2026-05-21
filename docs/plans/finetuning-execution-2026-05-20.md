# Fine-tuning Plan Execution Status — 2026-05-20

Source plan: `D:\AI-PROJECTS-GALO\PROJECTS\refineforge\docs\plans\finetuning-plan.md`

Update: `docs/plans/finetuning-execution-2026-05-20-corpus-run.md` closes
the first real Mathlib corpus lane after this original status note was written:
1000 Mathlib mutation rows plus 1000 finalized Anthropic SFT rows.

Execution mode: single Codex session, no agents. Root `refineforge` was dirty, so
refineforge code/config work was kept in the sibling worktree
`D:\AI-PROJECTS-GALO\PROJECTS\refineforge-finetuning-exec` on branch
`codex/finetuning-local-strategy`. Knowledge-Foundry is not a git repo, so its
changes were applied in place and verified with pytest plus pack verification.

## Completed Locally

### Phase 1 — KF Probe Set

Landed in `D:\AI-PROJECTS-GALO\PROJECTS\Knowledge-Foundry`:

- `kb_destiller/modes/sft_pair/probes/lean_proof_repair.yaml`
- `kb_destiller/modes/sft_pair/presets/lean_proof_repair.yaml`
- `kb_destiller/modes/sft_pair/gates/patch_well_formed.py`
- `tests/modes/sft_pair/test_lean_proof_repair.py`
- `run-configs/lean-proof-repair-smoke.yaml`

The Lean preset enables patch JSON validation, range sanity, and
`sorry` / `admit` / `axiom` rejection without affecting non-Lean SFT presets.

### Phase 3.5 — Cogn8ty Bridge Contract

Landed in Knowledge-Foundry:

- `kb_destiller/gates/cogn8ty_consistent.py`
- `tests/test_cogn8ty_consistent.py`

The gate consumes the `implied_theorem` field from the patch JSON, calls
`brain_reason` when enabled, drops non-empty `contradictions[]`, drops
`EvidenceConflict`, and treats `NoDomainMatch` alone as a pass-through
coverage gap.

Live endpoint status during this run:

- `http://127.0.0.1:7742` refused connections.
- No global `cogn8ty` command was on PATH.
- No local `target/release/cogn8ty.exe` or `%USERPROFILE%\.cogn8ty\bin\cogn8ty.exe`
  was present.

So the live full-corpus Cogn8ty pass is still blocked on a running brain server
and an actual gate-passed corpus.

### Phase 5 — Training Orchestration Smoke

Landed in refineforge worktree:

- `training/data/lean-proof-repair-smoke.jsonl`
- `training/configs/lean-proof-repair-smoke-stub.yaml`
- `training/README.md` smoke instructions

This is not a model-training result. It proves the local trainer lane can
resolve the proof-repair dataset, spawn a backend, parse progress, capture a
checkpoint, and write a report before a real axolotl run exists.

### Phase 6 — `local-finetune` Strategy

Already landed in this worktree in the prior slice:

- `crates/refineforge-strategies/src/local_finetune.rs`
- CLI/eval/autonomous `--weights-path` wiring
- strategy tests and docs

The runtime is a command-manifest bridge today. Native candle checkpoint loading
still depends on the final checkpoint architecture and tokenizer layout.

## Smoke Evidence

Knowledge-Foundry mock SFT run:

- Command: `.venv\Scripts\python.exe -m kb_destiller.cli run run-configs\lean-proof-repair-smoke.yaml --json`
- Pack: `extracted-kbs\foundry\lean-proof-repair\lean-proof-repair-smoke-v2`
- Accepted beliefs: `2`
- Rejections: `0`
- Gates attempted: `16`
- Chain root: `sha256:8168a54267df90e595499bebb90306422273b8a2ddc5d73d9a18077faabdab0a`

Knowledge-Foundry pack verification:

- Command: `.venv\Scripts\python.exe -m kb_destiller.cli verify extracted-kbs\foundry\lean-proof-repair\lean-proof-repair-smoke-v2`
- Result: schema ok; recomputed audit chain matched.

refineforge trainer smoke:

- Command: `cargo run -p refineforge-trainer -- run training/configs/lean-proof-repair-smoke-stub.yaml --dry-run`
- Result: resolved backend command and run directory.
- Command: `cargo run -p refineforge-trainer -- run training/configs/lean-proof-repair-smoke-stub.yaml`
- Result: final `success`, 1 attempt, 5 progress records, 1 checkpoint.
- Report: `training/runs\2026-05-20-lean-proof-repair-smoke-stub\report.json`

## Still Blocked

These are real blockers, not local scaffolding gaps:

1. **Phase 3.5 live Cogn8ty pass:** the bridge, tests, and corpus exist, but
   port `127.0.0.1:7742` was closed for the live full-corpus pass.
2. **Phase 4 baseline:** no consistency-filtered held-out eval result exists,
   so no honest `claude-opus-4-7` baseline can be produced yet.
3. **Phase 5 real fine-tune:** no axolotl job was launched; there is a full
   dataset, but no local Axolotl/PyTorch runtime, GPU allocation, or checkpoint
   target.
4. **Phase 7 acceptance:** no fine-tuned checkpoint exists, so local-finetune vs
   baseline acceptance cannot run.
5. **Phase 8 release/trust-base signoff:** blocked until real weights exist.

## Local Defaults Chosen

- Corpus target for the first real pass remains the plan minimum: `N = 1000`.
- Smoke base model remains `Qwen/Qwen2.5-Coder-1.5B`.
- Runtime path remains `local-finetune` command manifest until the real
  checkpoint/tokenizer format is known.
- HuggingFace publishing is deferred; local corpus and weights first.
