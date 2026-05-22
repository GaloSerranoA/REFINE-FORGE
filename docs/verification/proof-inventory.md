# Proof Inventory

> Snapshot date: 2026-05-22
> Purpose: classify what each Lean-backed claim proves and what it does not prove.

| Claim | Lean file | Theorem(s) | Proof shape | Current scope | Implementation link | Decision |
|---|---|---|---|---|---|---|
| CLAIM-CRS-001 | `lean/Refineforge/Consciousness/Claims.lean` | `workspace_broadcast_complete` | direct definition (`rfl`) | `model-only` | none | keep model-only unless a human-reviewed refinement links the model to Rust |
| CLAIM-CRS-002 | `lean/Refineforge/Consciousness/Claims.lean` | `workspace_capacity_bound` | hypothesis passthrough (`exact h`) | `model-only` | none | keep model-only unless a human-reviewed refinement links the capacity invariant to Rust |
| CLAIM-CRS-003 | `lean/Refineforge/Consciousness/Claims.lean` | `narrative_append_only` | direct definition (`rfl`) | `model-only` | none | best future CRS refinement candidate, but currently model-only |
| CLAIM-CRS-004 | `lean/Refineforge/Consciousness/Claims.lean` | `ethical_gate_non_bypass` | hypothesis passthrough (`exact h`) | `model-only` | none | keep model-only; do not market as a complete ethics implementation proof |
| CLAIM-CRS-005 | `lean/Refineforge/Consciousness/Claims.lean` | `phi_proxy_deterministic` | direct definition (`rfl`) | `model-only` | none | keep model-only; do not market as IIT Phi or consciousness proof |
| HELYX-AUDIT-001 | `lean/Refineforge/Helyx/Audit.lean` | `append_increments_length` | structural model proof (`cases` + `simp`) | `model-only` | case-study refinement doc only | keep model-only until cross-repo HELYX refinement evidence is machine-checkable and reviewed |
| EXAMPLE-001 | `lean/Refineforge/Example.lean` | `add_comm_demo` | standard-library theorem wrapper (`Nat.add_comm`) | `tutorial` | none | tutorial example, not production implementation assurance |
| EXAMPLE-002 | `lean/Refineforge/Counter.lean` | `incr_monotone`, `incr_strictly_increases` | tutorial model proof (`simp`) | `tutorial` | repo-local example refinement doc | tutorial refinement example; refinement doc discloses the `Nat` vs `u64` boundary |
| EXAMPLE-003 | `lean/Refineforge/CapabilityRevocation.lean` | `revoked_authorizes_nothing`, `fresh_capability_authorizes_held_right`, `revoke_is_idempotent` | structural model proof | `tutorial-production-shaped` | repo-local example refinement doc | first human-review candidate; review remains pending because `human_operator` is null |

## Notes

- `status: proven` means the referenced Lean theorem builds without `sorry`, `admit`, or project-local axioms. It does not, by itself, mean the Rust implementation is verified.
- CRS claims intentionally remain `scope: model-only` in this snapshot. Their Lean proofs are useful as narrow model checks, but they are not implementation-refinement proofs.
- HELYX-AUDIT-001 is a cross-repo case-study slice. The refinement document records manual assertions about HELYX source alignment; those assertions are not yet machine-checked by Refine-Forge.
- No claim in this snapshot has a human review signature. Every claim still records `review.human_operator: null`.
