# Plan 1 - Verification (Lean 4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans` to implement this plan task-by-task. Do not use delegated agents unless the operator explicitly requests them in the current turn.

**Goal:** Turn Refine-Forge's Lean verification track from working machinery plus model-only examples into an honestly scoped, reviewer-ready verification surface.

**Architecture:** Keep the existing claim registry, no-`sorry` gate, bundle sealing, and escalation engine. Improve the assurance layer by classifying each Lean theorem, strengthening only the claims that can be tied to Rust invariants, and keeping model-only claims explicitly scoped instead of inflating them into implementation proofs.

**Tech Stack:** Lean 4, Rust, YAML claim files, `refineforge-cli`, `refineforge-escalation`, Markdown refinement docs.

---

## Execution Status

Executed locally on 2026-05-22. Local documentation, inventory, CRS
model-only scope hardening, derive support-boundary documentation, and review
packet artifacts were created. Human review remains blocked because no real
human identity and approval were provided during execution; claim YAML review
fields correctly remain `human_operator: null`.

## 1. Operating Rules

- Do not weaken the no-`sorry` policy.
- Do not claim a Rust implementation is proved unless the claim has a refinement document, implementation citations, and human review.
- Do not introduce a new claim status such as `model-illustrative` as a wording-only change. A new status is a schema change and requires loader, linter, docs, escalation, and migration work.
- Treat `status: proven` as "the referenced Lean theorem is proven." Treat `scope:` and the refinement document as the source of truth for what the theorem proves about Rust.
- Keep stale run counts out of the plan. Full verification commands are acceptance gates, not facts to repeat without a dated artifact.

## 2. Current Snapshot

This snapshot was checked against the repo on 2026-05-22 while revising this plan. Re-run the commands in Section 6 before closing the work.

| Component | Current state | Evidence |
|---|---|---|
| no-`sorry` gate | Implemented; keep as a release gate | `crates/refineforge-cli/src/sorry_gate.rs` |
| Claims registry | Implemented | 9 YAML files under `claims/`; loader types in `crates/refineforge-cli/src/claim.rs` and `crates/refineforge-escalation/src/loaders.rs` |
| Bundle + SHA-256 seal | Implemented; outside this plan unless proof metadata changes require bundle output changes | `crates/refineforge-cli/src/bundle.rs` |
| Escalation engine | Implemented; use it for review/operator changes | `crates/refineforge-escalation/src/engine.rs`, `docs/escalation-criteria.md` |
| CRS Lean proofs | Proven in Lean, but currently model-only and mostly definition-level | `lean/Refineforge/Consciousness/Claims.lean` contains direct `rfl` / `exact h` proofs |
| HELYX audit proof | Model-only case-study proof; not a full HELYX cross-repo proof | `claims/helyx-audit-001.yaml`, `docs/refinement/HELYX-AUDIT-001.md` |
| Claim human review | Not exercised | every claim currently has `review.human_operator: null` |
| `refineforge-derive` | Implemented as a v1 documentation-aid macro, not a proof generator | `crates/refineforge-derive/src/lib.rs` defines `#[proc_macro_derive(LeanModel)]`; `crates/example-counter/src/counter.rs` derives it; `crates/example-counter/tests/counter.rs` tests generated output |

**Reference bar already in repo:** `lean/Refineforge/CapabilityRevocation.lean` is the internal example of a better proof artifact. It still models a simplified domain, but its theorems are about structured definitions rather than only restating an input hypothesis.

## 3. Real Gaps

**G1 - Assurance semantics are not sharp enough.** The CRS claims are honest about `scope: model-only`, but `status: proven` can still be misread by reviewers as "the Rust implementation is proven." The plan must make the distinction machine-checkable and visible in docs, lint output, and release artifacts.

**G2 - CRS proofs are too shallow for implementation assurance.** The five CRS theorems in `lean/Refineforge/Consciousness/Claims.lean` are mostly direct definitional facts. They are acceptable model-only examples only if the claim wording stays narrow. They are not enough for model+Rust refinement.

**G3 - No human review has happened.** All 9 claims carry `review.human_operator: null`. The linter should continue to accept explicit `null` as honest absence of review, but the project needs at least one real end-to-end review packet.

**G4 - `refineforge-derive` needs a support boundary, not resurrection from skeleton.** The crate now has a real `LeanModel` proc macro and example usage. The remaining question is whether it is supported as a stable feature, experimental, or explicitly a documentation aid.

## 4. Work Breakdown

### Task 1 - Build a Proof Inventory

**Files:**
- Create: `docs/verification/proof-inventory.md`
- Read: `lean/Refineforge/Consciousness/Claims.lean`
- Read: `lean/HELYX/AuditChain.lean`
- Read: `lean/Refineforge/CapabilityRevocation.lean`
- Read: `claims/*.yaml`

- [ ] **Step 1: Create the inventory file with one row per claim theorem**

Use this exact table structure:

```markdown
# Proof Inventory

> Snapshot date: 2026-05-22
> Purpose: classify what each Lean-backed claim proves and what it does not prove.

| Claim | Lean file | Theorem(s) | Proof shape | Current scope | Implementation link | Decision |
|---|---|---|---|---|---|---|
| CLAIM-CRS-001 | `lean/Refineforge/Consciousness/Claims.lean` | `workspace_broadcast_complete` | direct definition (`rfl`) | `model-only` | none | keep model-only unless refined |
| CLAIM-CRS-002 | `lean/Refineforge/Consciousness/Claims.lean` | `workspace_capacity_bound` | hypothesis passthrough (`exact h`) | `model-only` | none | keep model-only unless refined |
| CLAIM-CRS-003 | `lean/Refineforge/Consciousness/Claims.lean` | `narrative_append_only` | direct definition (`rfl`) | `model-only` | none | candidate for refinement |
| CLAIM-CRS-004 | `lean/Refineforge/Consciousness/Claims.lean` | `ethical_gate_non_bypass` | hypothesis passthrough (`exact h`) | `model-only` | none | likely model-only |
| CLAIM-CRS-005 | `lean/Refineforge/Consciousness/Claims.lean` | `phi_proxy_deterministic` | direct definition (`rfl`) | `model-only` | none | likely model-only |
| HELYX-AUDIT-001 | `lean/HELYX/AuditChain.lean` | audit-chain length theorem(s) | model theorem | `model-only` | case-study doc only | keep model-only unless cross-repo refinement is added |
| EXAMPLE-003 | `lean/Refineforge/CapabilityRevocation.lean` | `revoked_authorizes_nothing`, `fresh_capability_authorizes_held_right`, `revoke_is_idempotent` | structural model proof | `tutorial-production-shaped` | tutorial refinement doc | first human-review candidate |
```

- [ ] **Step 2: Verify no hidden `sorry`, `admit`, or `axiom` exists in Lean files**

Run the current CLI policy gate through Lean verification:

```powershell
cargo run -p refineforge-cli --bin refine -- lean check-all
```

Expected: command exits successfully with no policy violation. The older
`refine sorry-gate` command name is not part of the current CLI surface.

- [ ] **Step 3: Commit only if requested by the operator**

Do not commit automatically in this workspace. The operator decides when this plan becomes a commit.

### Task 2 - Make Claim Assurance Semantics Explicit

**Files:**
- Modify: `docs/methodology.md`
- Modify: `docs/refinement-template.md`
- Modify: `docs/escalation-criteria.md`
- Modify: `crates/refineforge-cli/src/lint.rs`
- Test: `crates/refineforge-cli/src/lint.rs`

- [ ] **Step 1: Document the three assurance levels**

Add this language to `docs/methodology.md` and mirror the reviewer checklist in `docs/refinement-template.md`:

```markdown
## Claim Assurance Levels

`status: proven` means the referenced Lean theorem builds without `sorry`, `admit`, or project-local axioms. It does not, by itself, mean the Rust implementation is verified.

The implementation assurance comes from `scope:`:

- `model-only` - Lean proves a mathematical model. The claim must not cite Rust implementation files as evidence.
- `tutorial` / `tutorial-production-shaped` - Lean plus Rust demonstrate the refinement workflow for examples. These are educational or pattern claims unless reviewed otherwise.
- `model+refined` - Lean theorem, refinement document, Rust implementation citations, and human review all agree on the same invariant.

A claim may move from `model-only` to implementation-refined scope only after the escalation process records the review packet and the claim's `review.human_operator` is populated by a real human.
```

- [ ] **Step 2: Keep the linter strict for CRS model-only claims**

Ensure `crates/refineforge-cli/src/lint.rs` enforces:

- CRS claims stay `scope: model-only` until a reviewed implementation refinement exists.
- CRS model-only claims include an honest-scope disclosure in description or review notes.
- CRS model-only claims do not cite Rust implementation files as proof evidence.
- `review.human_operator` is present and either `null` or a real human identity string.

- [ ] **Step 3: Add or update focused lint tests**

Keep tests focused on the behavior above. Minimum coverage:

- missing `review.human_operator` is an error;
- AI/placeholder reviewer names are errors;
- CRS model-only claim with Rust source evidence is an error;
- CRS model-only claim with honest disclosure and no Rust source evidence passes.

- [ ] **Step 4: Run the targeted CLI test gate**

Run:

```powershell
cargo test -p refineforge-cli lint
```

Expected: lint tests pass.

### Task 3 - Choose CRS Claim Outcomes One by One

**Files:**
- Modify as needed: `lean/Refineforge/Consciousness/Claims.lean`
- Modify as needed: `claims/CLAIM-CRS-001.yaml`
- Modify as needed: `claims/CLAIM-CRS-002.yaml`
- Modify as needed: `claims/CLAIM-CRS-003.yaml`
- Modify as needed: `claims/CLAIM-CRS-004.yaml`
- Modify as needed: `claims/CLAIM-CRS-005.yaml`
- Create or modify as needed: `docs/refinement/CLAIM-CRS-001.md`
- Create or modify as needed: `docs/refinement/CLAIM-CRS-002.md`
- Create or modify as needed: `docs/refinement/CLAIM-CRS-003.md`
- Create or modify as needed: `docs/refinement/CLAIM-CRS-004.md`
- Create or modify as needed: `docs/refinement/CLAIM-CRS-005.md`

For each CRS claim, choose exactly one outcome.

**Outcome A - Keep model-only and make the limitation unavoidable.**

Use this when the claim is valuable as a mathematical model but is not tied to a Rust invariant.

Required YAML properties:

```yaml
scope: model-only
status: proven
review:
  human_operator: null
```

Required wording in `description:` or `review.notes`:

```text
Honest scope: this is a model-level invariant. It does not prove the Rust implementation.
```

**Outcome B - Upgrade to model+refined only after real refinement exists.**

Use this only when a refinement document maps the Lean theorem to concrete Rust behavior.

Required artifacts:

- Lean theorem proves a non-trivial model property.
- Refinement doc names the Rust files/functions, the abstraction boundary, and uncovered cases.
- Claim YAML cites both the Lean theorem and the refinement doc.
- Claim YAML cites Rust source only after the refinement doc exists.
- Human review packet exists for the scope upgrade.

**Outcome C - Downgrade if the theorem does not support the wording.**

Use this when the claim's prose says more than the theorem proves and cannot be narrowed honestly.

Allowed statuses are the existing status vocabulary only: `unformalized`, `drafted`, `builds`, `proven`, `broken`. Do not add a new status unless executing a separate schema migration plan.

- [ ] **Step 1: Start with CLAIM-CRS-003**

Reason: append-only shape is the best CRS candidate for a future refinement because it can plausibly map to concrete event-log or narrative-buffer behavior.

Produce one of:

- model-only wording tightened, or
- a refinement doc with exact Rust mapping, or
- status downgrade.

- [ ] **Step 2: Then evaluate CLAIM-CRS-001 and CLAIM-CRS-002**

Reason: broadcast and capacity claims may map to implementation invariants if the corresponding Rust routing/capacity code is stable and easy to cite.

- [ ] **Step 3: Leave CLAIM-CRS-004 and CLAIM-CRS-005 model-only unless a precise Rust invariant is found**

Reason: ethical-gate and phi-proxy claims are easy to overstate. Prefer narrow, honest model-only scope over weak implementation claims.

- [ ] **Step 4: Run claim lint after each claim edit**

Run:

```powershell
cargo run -p refineforge-cli --bin refine -- lint check-all
```

Expected: no errors. Warnings are acceptable only when they accurately describe model-only scope.

### Task 4 - Exercise Human Review Without Faking It

**Files:**
- Read: `docs/escalation-criteria.md`
- Read: `crates/refineforge-escalation/src/action.rs`
- Read: `crates/refineforge-escalation/src/engine.rs`
- Modify as needed: one claim under `claims/`
- Save evidence under: `release/evidence/verification-review-YYYYMMDD/`

- [ ] **Step 1: Pick the first review target**

Use `claims/example-capability-revocation.yaml` first unless the operator directs otherwise. It has the best in-repo Lean reference bar and is safer than reviewing CRS claims before their final scope is settled.

- [ ] **Step 2: Generate or document the `SetReviewOperator` escalation packet**

The packet must state:

- claim id;
- old value: `review.human_operator: null`;
- proposed human operator value;
- reviewer checklist status;
- exact Lean/refinement files reviewed;
- reason the operator is qualified to sign.

- [ ] **Step 3: Require a real human identity**

Do not write `codex`, `claude`, `ai`, `automated-review`, or any other placeholder. If the operator does not provide a real human identity, leave `human_operator: null` and mark this task blocked.

- [ ] **Step 4: Update the claim only after the operator approves the packet**

The resulting review block must include:

```yaml
review:
  human_operator: "<real human identity>"
  reviewed_on: "YYYY-MM-DD"
  notes: "<short reviewer decision and scope>"
```

- [ ] **Step 5: Re-run lint**

Run:

```powershell
cargo run -p refineforge-cli --bin refine -- lint check-all
```

Expected: no errors and no fake-reviewer warnings.

### Task 5 - Audit `refineforge-derive` Support Status

**Files:**
- Read: `crates/refineforge-derive/src/lib.rs`
- Read: `crates/example-counter/src/counter.rs`
- Read: `crates/example-counter/tests/counter.rs`
- Modify as needed: `crates/refineforge-derive/README.md`
- Modify as needed: `docs/refinement/EXAMPLE-003.md`
- Modify as needed: `README.md`

- [ ] **Step 1: Record the real current state**

The current state is not "skeleton." It is:

- `#[proc_macro_derive(LeanModel)]` exists;
- it supports a limited Rust struct subset;
- it emits a Lean structure declaration as a string constant;
- `example-counter` uses it;
- tests cover generated output;
- it does not generate proofs or refinement arguments.

- [ ] **Step 2: Choose the support label**

Use one of these exact labels:

- `supported-documentation-aid` - stable enough for examples, not a proof generator;
- `experimental` - available in-tree but not promised as stable;
- `supported-feature` - stable public feature with docs, examples, and compatibility expectations.

Recommended for the current repo: `supported-documentation-aid`.

- [ ] **Step 3: Document the boundary**

Add this sentence to the derive docs:

```markdown
`LeanModel` generates a Lean structure declaration for review and refinement documentation. It does not prove the Rust implementation correct, does not generate theorems, and does not replace a human-reviewed refinement document.
```

- [ ] **Step 4: Run the example-counter tests**

Run:

```powershell
cargo test -p example-counter
```

Expected: tests covering `LeanModel` still pass.

### Task 6 - Final Verification Gates

**Files:**
- No planned edits; this is a verification task.

- [ ] **Step 1: Run formatting/check hygiene**

Run:

```powershell
cargo fmt --all --check
git diff --check
```

Expected: both pass.

- [ ] **Step 2: Run the Lean policy gate**

Run:

```powershell
cargo run -p refineforge-cli --bin refine -- lean check-all
```

Expected: pass.

- [ ] **Step 3: Run claim lint**

Run:

```powershell
cargo run -p refineforge-cli --bin refine -- lint check-all
```

Expected: no errors. Any warnings must be model-only honesty warnings, not missing metadata.

- [ ] **Step 4: Run targeted Rust tests**

Run:

```powershell
cargo test -p refineforge-cli
cargo test -p refineforge-escalation
cargo test -p example-counter
```

Expected: all targeted tests pass.

- [ ] **Step 5: Run broad workspace check if local environment allows it**

Run:

```powershell
cargo check --workspace --all-targets
```

Expected: pass. If it fails on unrelated pre-existing warnings/errors, record the exact blocker in the closeout instead of weakening this plan.

## 5. Definition of Done

- `docs/verification/proof-inventory.md` exists and classifies every Lean-backed claim.
- CRS and HELYX model-only claims do not imply implementation proof.
- Any claim that cites Rust implementation evidence has a refinement doc and review path.
- At least one claim has a real human review record, or the work is explicitly blocked on missing human identity/approval.
- `refineforge-derive` is documented as a limited `LeanModel` generator with a clear support boundary.
- The final verification gates in Section 6 have been run and their exact results are recorded in the closeout.

## 6. Non-Goals

- Do not try to prove the full Consciousness-rs implementation in this plan.
- Do not claim full HELYX verification from the case-study claim.
- Do not add Mathlib unless a specific proof requires it and the trust-base escalation is approved.
- Do not replace human review with AI-generated approval text.
- Do not turn plan work into release/Sigstore work; that belongs to Plan 2.
