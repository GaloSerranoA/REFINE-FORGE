# Proof Inventory

> Snapshot date: 2026-05-22
> Updated: 2026-05-29 — added the **Content grade** column (A/B/C, defined in
> [proof-audit.md](proof-audit.md)) and recorded the CRS de-vacuuming
> remediation (CLAIM-CRS-001…005 strengthened from vacuous to substantive).
> Purpose: classify what each Lean-backed claim proves and what it does not prove.

**Content grade** (full rubric and per-theorem reasoning in
[proof-audit.md](proof-audit.md)):

- **A — substantive:** quantifies over the model's objects and rules out a class
  of misbehavior, or relates two independent quantities. Could be false under a
  different definition.
- **B — definitional / elementary:** pins a definition or states an elementary
  one-step fact; true, but the proof is a single unfold or one library lemma.
- **C — vacuous:** goal reduces to a tautology (`x = x`, `P → P`) that holds
  under *every* model. **No instances remain in this snapshot** (the CRS claims
  were Tier C before 2026-05-29; see Remediation note below).

The content grade is **orthogonal to `status: proven`** — every theorem below is
proven in Lean; the grade says whether the statement carries information.

| Claim | Lean file | Theorem(s) | Proof shape | Content grade | Scope | Impl. link | Decision |
|---|---|---|---|---|---|---|---|
| CLAIM-CRS-001 | `Consciousness/Claims.lean` | `workspace_broadcast_complete` (+ `run_ready`, `run_counts`, `broadcast_complete_falsifiable`) | induction over the `run` ignition process | **A** | `model-only` | none | keep model-only; theorem now ranges over the ignition process and the predicate is falsifiable |
| CLAIM-CRS-002 | `Consciousness/Claims.lean` | `workspace_capacity_bound` (+ `accept_saturates_at_capacity`, `over_capacity_exists`) | case split on `accept` + `omega` | **A** | `model-only` | none | keep model-only; proves the admission op preserves the bound and saturates |
| CLAIM-CRS-003 | `Consciousness/Claims.lean` | `narrative_append_only` (+ `narrative_append_increments_length`, `reset_violates_append_only`) | history-preservation witness + `simp` | **A** | `model-only` | none | best future CRS refinement candidate; proves nothing prior is dropped/reordered |
| CLAIM-CRS-004 | `Consciousness/Claims.lean` | `ethical_gate_non_bypass` (+ `rogue_executor_can_bypass`) | routes `execute` through `gate`; companion exhibits a bypass | **A** | `model-only` | none | keep model-only; do **not** market as a complete ethics implementation proof |
| CLAIM-CRS-005 | `Consciousness/Claims.lean` | `phi_proxy_deterministic` (+ `phi_proxy_monotone_in_nodes`, `phi_proxy_monotone_in_edges`) | closed-form `rfl` (B) + monotonicity `omega` (A) | **B** (named) + **A** (monotonicity) | `model-only` | none | keep model-only; do **not** market as IIT Phi or consciousness |
| HELYX-AUDIT-001 | `Helyx/Audit.lean` | `append_increments_length` | `cases` + `simp` (definitional) | **B** | `model-only` | case-study refinement doc only | keep model-only until cross-repo HELYX refinement evidence is machine-checkable and reviewed |
| EXAMPLE-001 | `Example.lean` | `add_comm_demo` | standard-library wrapper (`Nat.add_comm`) | **B** (re-export) | `tutorial` | none | tutorial example, not production implementation assurance |
| EXAMPLE-002 | `Counter.lean` | `incr_monotone`, `incr_strictly_increases` | `simp` (elementary) | **B**, **B** | `tutorial` | repo-local example refinement doc | tutorial refinement example; refinement doc discloses the `Nat` vs `u64` boundary |
| EXAMPLE-003 | `CapabilityRevocation.lean` | `revoked_authorizes_nothing`, `fresh_capability_authorizes_held_right`, `revoke_is_idempotent` | structural model proof | **A**, **B**, **A** | `tutorial-production-shaped` | repo-local example refinement doc | first human-review candidate; review remains pending because `human_operator` is null |
| REFINEFORGE-TRUST-001 | `AgentTrust.lean` | `enforce_never_exceeds_ceiling`, `enforce_keeps_when_within_ceiling`, `enforce_idempotent` | `by_cases`/`rw`/T1∘T2 composition | **A**, **B**, **A** | `model+refined` | **real (dogfood)**: `crates/refineforge-cli/src/agent/common.rs` — `TrustLevel`, `trust_rank`, `enforce_trust_ceiling` | **First human-reviewed claim** (Galo Serrano Abad, 2026-05-29). `refine agent lean --target REFINEFORGE-TRUST-001` reports **human-reviewed**; all six production-proof requirements pass |

## Remediation note (2026-05-29)

CLAIM-CRS-001…005 were **Tier C (vacuous)** in the 2026-05-22 snapshot: their
proofs reduced to `x = x` / `P → P` (e.g. `ethical_gate_non_bypass` collapsed two
independent decisions into one variable; `phi_proxy_deterministic` proved
`x = x`). They were rewritten on 2026-05-29 to quantify over the model's own
operations (an ignition `run`, an `accept` admission, an `append` log, a `gate`
the executor routes through, a monotone metric) and each now ships a companion
theorem proving the relevant predicate is falsifiable. Verification:
`lake build` green; `refine lean check-all` reports all nine claims
`Verified  sorries=0 admits=0 axioms=0`. Full before/after in
[proof-audit.md](proof-audit.md) §Remediation.

## Notes

- `status: proven` means the referenced Lean theorem builds without `sorry`,
  `admit`, or project-local axioms. It does not, by itself, mean the Rust
  implementation is verified.
- CRS claims intentionally remain `scope: model-only`. Their Lean proofs are now
  substantive **model** checks (Tier A), but they are still not
  implementation-refinement proofs — no `rust_source` is cited and
  `review.human_operator` is `null`.
- HELYX-AUDIT-001 is a cross-repo case-study slice. The refinement document
  records manual assertions about HELYX source alignment; those assertions are
  not yet machine-checked by Refine-Forge.
- One claim now carries a human review signature: **REFINEFORGE-TRUST-001**
  (`review.human_operator: Galo Serrano Abad`, 2026-05-29) → `human-reviewed`.
  All other claims still record `review.human_operator: null`.
- **CRS/EXAMPLE bundles refreshed (2026-05-29):** after the CRS remediation the
  `artifacts/CLAIM-CRS-*` and `artifacts/EXAMPLE-00*` bundles were re-exported
  (`refine bundle export`) and re-verified, so they embed the current Lean. Do
  not hand-edit sealed bundles; regenerate via `refine bundle export`.
- **REFINEFORGE-TRUST-001 (dogfood, 2026-05-29):** the first claim whose Lean
  model is linked to *real* Refine-Forge Rust (the agent trust-ceiling
  enforcement in `common.rs`), and the **first `human-reviewed` claim**
  (Galo Serrano Abad). `refine agent lean --target REFINEFORGE-TRUST-001`
  reports `human-reviewed`; all six production-proof requirements pass. See
  `docs/refinement/REFINEFORGE-TRUST-001.md` §6.
